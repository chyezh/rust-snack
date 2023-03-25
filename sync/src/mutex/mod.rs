use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::AtomicU32,
    sync::atomic::Ordering::{Acquire, Relaxed, Release},
};

use atomic_wait::{wait, wake_one};

const MUTEX_UNLOCKED: u32 = 0; // unlocked
const MUTEX_LOCKED: u32 = 1; // locked, no contention
const MUTEX_CONTENTION: u32 = 2; // locked, other threads waiting

/// A mutual-exclusive lock implementation.
pub struct Mutex<T> {
    // 0 if unlocked, 1 if locked.
    state: AtomicU32,
    value: UnsafeCell<T>,
}

/// Implement Sync if and only if T is Send.
/// Only one thread access the &T at a time,
/// so T is not required to be Sync.
unsafe impl<T> Sync for Mutex<T> where T: Send {}

impl<T> Mutex<T> {
    /// Create a new mutex for given value.
    pub fn new(value: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquire lock guard if mutex is not locked,
    /// otherwise block until the lock is released.
    pub fn lock(&self) -> MutexGuard<T> {
        // Skip atomic-wait if there is no contention.
        if self
            .state
            .compare_exchange(MUTEX_UNLOCKED, MUTEX_LOCKED, Acquire, Relaxed)
            .is_err()
        {
            while self.state.swap(MUTEX_CONTENTION, Acquire) != MUTEX_UNLOCKED {
                // Wait until lock state is no longer MUTEX_CONTENTION.
                wait(&self.state, MUTEX_CONTENTION);
            }
        }
        MutexGuard { mutex: self }
    }
}

/// A guard type can be acquired from Mutex lock method.
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // Safety: There's only one guard of same mutex can be accessed at a time,
        // it's safe to access the inner value by any shared reference.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safety: There's only one guard of same mutex can be accessed at a time,
        // it's safe to access the inner value with mutable reference by mutable reference.
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        // Release the lock and
        if self.mutex.state.swap(MUTEX_UNLOCKED, Release) == MUTEX_CONTENTION {
            // wake any one blocked thread if lock-contention.
            wake_one(&self.mutex.state);
        }
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::Mutex;
    #[allow(unused_imports)]
    use std::thread;

    #[test]
    fn test_mutex() {
        for _ in 1..1000 {
            let x = Mutex::new(Vec::new());
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
