use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};

/// A raw spin lock implementation
pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

/// Implement Sync if and only if T is Send
/// Only one thread at a time access the T protected by reference,
/// so T is not required to be Sync
unsafe impl<T> Sync for SpinLock<T> where T: Send {}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquire the spin lock and access the unique mutable reference of inner T
    pub fn lock(&self) -> SpinLockGuard<T> {
        // Must use acquire-release memory order to sync in multithread.
        while self.locked.swap(true, Acquire) {
            // Enter a spin loop
            std::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }

    #[inline]
    fn unlock(&self) {
        self.locked.store(false, Release);
    }
}

/// A guard type acquired by SpinLock lock method
pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // Safety: There's only one guard of same value existed at a time,
        // it's safe to access the inner value by any shared reference.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safety: There's only one guard of same value existed at a time,
        // it's safe to access the inner value with mutable reference by mutable reference of guard
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        // Unlock the corresponding spin lock when guard is dropped
        self.lock.unlock();
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::SpinLock;
    #[allow(unused_imports)]
    use std::thread;

    #[test]
    fn test_spin_lock() {
        for _ in 1..1000 {
            let x = SpinLock::new(Vec::new());
            thread::scope(|s| {
                s.spawn(|| x.lock().push(1));
                s.spawn(|| {
                    let mut g = x.lock();
                    g.push(2);
                    g.push(2);
                });
            });
            let g = x.lock();
            assert!(g.as_slice() == [1, 2, 2] || g.as_slice() == [2, 2, 1]);
        }
    }
}
