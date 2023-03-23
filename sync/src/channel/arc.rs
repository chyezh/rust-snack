use std::cell::UnsafeCell;
use std::ops::Deref;
use std::sync::atomic::fence;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::{ptr::NonNull, sync::atomic::AtomicUsize};

struct ArcData<T> {
    strong_ref_count: AtomicUsize,
    weak_ref_count: AtomicUsize,
    data: UnsafeCell<Option<T>>,
}

pub struct Arc<T> {
    weak: Weak<T>,
}

unsafe impl<T: Send + Sync> Send for Arc<T> {}
unsafe impl<T: Send + Sync> Sync for Arc<T> {}

impl<T> Arc<T> {
    pub fn new(data: T) -> Arc<T> {
        let ptr = Box::leak(Box::new(ArcData {
            strong_ref_count: AtomicUsize::new(1),
            weak_ref_count: AtomicUsize::new(1),
            data: UnsafeCell::new(Some(data)),
        }));
        Arc {
            weak: Weak {
                ptr: NonNull::from(ptr),
            },
        }
    }

    pub fn get_mut(arc: &mut Self) -> Option<&mut T> {
        if arc.weak.data().weak_ref_count.load(Relaxed) == 1 {
            fence(Acquire);
            let data = unsafe { arc.weak.ptr.as_mut() };
            let option = data.data.get_mut();
            let data = option.as_mut().unwrap();
            Some(data)
        } else {
            None
        }
    }

    pub fn downgrade(arc: &Self) -> Weak<T> {
        arc.weak.clone()
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        let ptr = self.weak.data().data.get();
        unsafe { (*ptr).as_ref().unwrap() }
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Arc<T> {
        let weak = self.weak.clone();
        if weak.data().strong_ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort();
        }
        Arc { weak }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        if self.weak.data().strong_ref_count.fetch_sub(1, Release) == 1 {
            fence(Acquire);
            let ptr = self.weak.data().data.get();
            unsafe {
                (*ptr) = None;
            }
        }
    }
}

pub struct Weak<T> {
    ptr: NonNull<ArcData<T>>,
}

impl<T> Clone for Weak<T> {
    fn clone(&self) -> Self {
        if self.data().weak_ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort();
        }
        Weak { ptr: self.ptr }
    }
}

impl<T> Drop for Weak<T> {
    fn drop(&mut self) {
        if self.data().weak_ref_count.fetch_sub(1, Release) == 1 {
            fence(Acquire);
            unsafe { drop(Box::from_raw(self.ptr.as_ptr())) }
        }
    }
}

unsafe impl<T: Send + Sync> Send for Weak<T> {}
unsafe impl<T: Send + Sync> Sync for Weak<T> {}

impl<T> Weak<T> {
    fn data(&self) -> &ArcData<T> {
        unsafe { self.ptr.as_ref() }
    }

    pub fn upgrade(&self) -> Option<Arc<T>> {
        let mut n = self.data().strong_ref_count.load(Relaxed);
        loop {
            if n == 0 {
                return None;
            }
            assert!(n < usize::MAX);
            if let Err(e) =
                self.data()
                    .strong_ref_count
                    .compare_exchange_weak(n, n + 1, Relaxed, Relaxed)
            {
                n = e;
                continue;
            }
            return Some(Arc { weak: self.clone() });
        }
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::Arc;
    #[allow(unused_imports)]
    use std::sync::atomic::AtomicUsize;
    #[allow(unused_imports)]
    use std::sync::atomic::Ordering::Relaxed;

    #[test]
    fn test_arc() {
        static NUM_DROPS: AtomicUsize = AtomicUsize::new(0);
        struct DetectDrop;

        impl Drop for DetectDrop {
            fn drop(&mut self) {
                NUM_DROPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }

        let x = Arc::new(("hello", DetectDrop));
        let y = x.clone();
        let t = std::thread::spawn(move || {
            assert_eq!(x.0, "hello");
        });
        assert_eq!(y.0, "hello");

        t.join().unwrap();
        assert_eq!(NUM_DROPS.load(std::sync::atomic::Ordering::Relaxed), 0);
        drop(y);
        assert_eq!(NUM_DROPS.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn test_arc2() {
        static NUM_DROPS: AtomicUsize = AtomicUsize::new(0);
        struct DetectDrop;
        impl Drop for DetectDrop {
            fn drop(&mut self) {
                NUM_DROPS.fetch_add(1, Relaxed);
            }
        }
        // Create an Arc with two weak pointers.
        let x = Arc::new(("hello", DetectDrop));
        let y = Arc::downgrade(&x);
        let z = Arc::downgrade(&x);
        let t = std::thread::spawn(move || {
            // Weak pointer should be upgradable at this point.
            let y = y.upgrade().unwrap();
            assert_eq!(y.0, "hello");
        });
        assert_eq!(x.0, "hello");
        t.join().unwrap();
        // The data shouldn't be dropped yet,
        // and the weak pointer should be upgradable.
        assert_eq!(NUM_DROPS.load(Relaxed), 0);
        assert!(z.upgrade().is_some());
        drop(x);
        // Now, the data should be dropped, and the
        // weak pointer should no longer be upgradable.
        assert_eq!(NUM_DROPS.load(Relaxed), 1);
        assert!(z.upgrade().is_none());
    }
}
