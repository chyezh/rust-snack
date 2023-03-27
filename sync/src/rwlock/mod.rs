use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{
        fence, AtomicU32,
        Ordering::{Acquire, Relaxed, Release},
    },
};

use atomic_wait::{wait, wake_one};

const RWLOCK_WLOCKED: u32 = u32::MAX;

pub struct RwLock<T> {
    state: AtomicU32, // counter of reader, RWLOCK_WLOCKED for write lock
    value: UnsafeCell<T>,
}

/// Implement Sync if and only if T is Send.
/// Only one thread access the &mut T at a time,
/// so T is not required to be Sync.
unsafe impl<T> Sync for RwLock<T> where T: Send + Sync {}

impl<T> RwLock<T> {
    /// Create a new rwlock for given value.
    pub fn new(value: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            value: UnsafeCell::new(value),
        }
    }

    /// Read lock for value.
    pub fn read(&self) -> ReadGuard<T> {
        let mut x = self.state.load(Relaxed);
        loop {
            // Block until write lock released.
            if x == RWLOCK_WLOCKED {
                wait(&self.state, RWLOCK_WLOCKED);
                x = self.state.load(Relaxed);
                continue;
            }
            match self.state.compare_exchange_weak(x, x + 1, Relaxed, Relaxed) {
                Ok(_) => {
                    // Lock success, fence with acquire ordering and break.
                    fence(Acquire);
                    break;
                }
                Err(e) => x = e,
            }
        }
        ReadGuard { lock: self }
    }

    /// Write lock fro value
    pub fn write(&self) -> WriteGuard<T> {
        loop {
            // Block until no readers and writer.
            match self
                .state
                .compare_exchange_weak(0, RWLOCK_WLOCKED, Relaxed, Relaxed)
            {
                Ok(_) => {
                    // Lock success, fence with acquire ordering and break.
                    fence(Acquire);
                    break;
                }
                // Block until state modified.
                Err(e) => wait(&self.state, e),
            }
        }
        WriteGuard { lock: self }
    }
}

/// A guard type for read operation of RwLock.
pub struct ReadGuard<'a, T> {
    pub(crate) lock: &'a RwLock<T>,
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // Safety: multi-thread get the immutable reference of inner value is safe.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        // Release the lock
        if self.lock.state.fetch_sub(1, Release) == 1 {
            wake_one(&self.lock.state);
        }
    }
}

/// A guard type for write operation of RwLock.
pub struct WriteGuard<'a, T> {
    pub(crate) lock: &'a RwLock<T>,
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // Safety: There's only one guard of same mutex can be accessed at a time,
        // it's safe to access the inner value by any shared reference.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safety: There's only one guard of same mutex can be accessed at a time,
        // it's safe to access the inner value with mutable reference by mutable reference.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        // Release the lock
        self.lock.state.store(0, Release);
        wake_one(&self.lock.state);
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::RwLock;
    #[allow(unused_imports)]
    use std::thread;

    #[test]
    fn test_mutex() {
        for _ in 1..1000 {
            let x = RwLock::new(Vec::new());
            thread::scope(|s| {
                s.spawn(|| x.write().push(1));
                s.spawn(|| {
                    let mut g = x.write();
                    g.push(2);
                    g.push(2);
                });
                s.spawn(|| {
                    for _ in 0..100_000 {
                        assert!(x.read().len() <= 3);
                    }
                });
                s.spawn(|| {
                    for _ in 0..100_000 {
                        assert!(x.read().len() <= 3);
                    }
                });
            });
            let g = x.write();
            assert!(g.as_slice() == [1, 2, 2] || g.as_slice() == [2, 2, 1]);
        }
    }
}
