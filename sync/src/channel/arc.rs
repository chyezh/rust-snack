use std::ops::Deref;
use std::sync::atomic::fence;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::{ptr::NonNull, sync::atomic::AtomicUsize};

struct ArcData<T> {
    ref_count: AtomicUsize,
    data: T,
}

pub struct Arc<T> {
    ptr: NonNull<ArcData<T>>,
}

unsafe impl<T: Send + Sync> Send for Arc<T> {}
unsafe impl<T: Send + Sync> Sync for Arc<T> {}

impl<T> Arc<T> {
    pub fn new(data: T) -> Arc<T> {
        let ptr = Box::leak(Box::new(ArcData {
            ref_count: AtomicUsize::new(1),
            data,
        }));
        Arc {
            ptr: NonNull::from(ptr),
        }
    }

    pub fn get_mut(arc: &mut Self) -> Option<&mut T> {
        if arc.data().ref_count.load(Relaxed) == 1 {
            fence(Acquire);
            unsafe { Some(&mut arc.ptr.as_mut().data) }
        } else {
            None
        }
    }

    fn data(&self) -> &ArcData<T> {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.data().data
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Arc<T> {
        if self.data().ref_count.fetch_add(1, Relaxed) > usize::MAX / 2 {
            std::process::abort();
        }
        Arc { ptr: self.ptr }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        if self.data().ref_count.fetch_sub(1, Release) == 1 {
            fence(Acquire);
            unsafe { drop(Box::from_raw(self.ptr.as_ptr())) };
        }
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::Arc;
    #[allow(unused_imports)]
    use std::sync::atomic::AtomicUsize;

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
}
