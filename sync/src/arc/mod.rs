use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::sync::atomic::fence;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::{ptr::NonNull, sync::atomic::AtomicUsize};

struct ArcInner<T> {
    strong_ref_count: AtomicUsize,
    weak_ref_count: AtomicUsize,
    data: UnsafeCell<ManuallyDrop<T>>,
}

pub struct Arc<T> {
    inner: NonNull<ArcInner<T>>,
}

unsafe impl<T: Sync + Send> Send for Arc<T> {}
unsafe impl<T: Sync + Send> Sync for Arc<T> {}

impl<T> Arc<T> {
    /// Constructs a new `Arc<T>`
    pub fn new(data: T) -> Self {
        let inner = Box::new(ArcInner {
            strong_ref_count: AtomicUsize::new(1),
            weak_ref_count: AtomicUsize::new(1),
            data: UnsafeCell::new(ManuallyDrop::new(data)),
        });
        Arc {
            inner: NonNull::from(unsafe { &*Box::into_raw(inner) }),
        }
    }

    /// Get mutable reference of underlying T if only one Arc exists,
    /// otherwise return None.
    pub fn get_mut(arc: &mut Self) -> Option<&mut T> {
        if arc
            .data()
            .weak_ref_count
            .compare_exchange(1, usize::MAX, Acquire, Relaxed)
            .is_err()
        {
            return None;
        }
        let is_unique = arc.data().strong_ref_count.load(Relaxed) == 1;
        arc.data().weak_ref_count.store(1, Release);
        if !is_unique {
            return None;
        }

        fence(Acquire);
        Some(unsafe { &mut **arc.inner.as_mut().data.get_mut() })
    }

    pub fn downgrade(arc: &Self) -> Weak<T> {
        let mut n = arc.data().weak_ref_count.load(Relaxed);
        loop {
            if n == usize::MAX {
                std::hint::spin_loop();
                n = arc.data().weak_ref_count.load(Relaxed);
                continue;
            }
            if let Err(e) =
                arc.data()
                    .weak_ref_count
                    .compare_exchange_weak(n, n + 1, Acquire, Relaxed)
            {
                n = e;
                continue;
            }
            return Weak { inner: arc.inner };
        }
    }

    fn data(&self) -> &ArcInner<T> {
        // Safety: Arc promise that inner was always valid
        unsafe { self.inner.as_ref() }
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.data().data.get() }
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        if self.data().strong_ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort();
        }
        Arc { inner: self.inner }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        if self.data().strong_ref_count.fetch_sub(1, Release) != 1 {
            return;
        }
        fence(Acquire);

        unsafe { ManuallyDrop::drop(&mut *self.data().data.get()) }

        drop(Weak { inner: self.inner });
    }
}

pub struct Weak<T> {
    inner: NonNull<ArcInner<T>>,
}

unsafe impl<T: Sync + Send> Send for Weak<T> {}
unsafe impl<T: Sync + Send> Sync for Weak<T> {}

impl<T> Weak<T> {
    fn data(&self) -> &ArcInner<T> {
        unsafe { self.inner.as_ref() }
    }

    pub fn upgrade(&self) -> Option<Arc<T>> {
        let mut n = self.data().strong_ref_count.load(Relaxed);
        loop {
            if n == 0 {
                return None;
            }
            if let Err(e) =
                self.data()
                    .strong_ref_count
                    .compare_exchange_weak(n, n + 1, Relaxed, Relaxed)
            {
                n = e;
                continue;
            }
            return Some(Arc { inner: self.inner });
        }
    }
}

impl<T> Clone for Weak<T> {
    fn clone(&self) -> Self {
        if self.data().weak_ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort();
        }
        Weak { inner: self.inner }
    }
}

impl<T> Drop for Weak<T> {
    fn drop(&mut self) {
        if self.data().weak_ref_count.fetch_sub(1, Release) == 1 {
            fence(Acquire);
            unsafe { drop(Box::from_raw(self.inner.as_ptr())) }
        }
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use std::sync::atomic::AtomicUsize;
    #[allow(unused_imports)]
    use std::sync::atomic::Ordering::Relaxed;

    #[test]
    fn test() {
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
