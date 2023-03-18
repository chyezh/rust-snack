use std::cell::UnsafeCell;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};

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
    #[allow(clippy::mut_from_ref)]
    pub fn lock(&self) -> &mut T {
        // Must use acquire-release memory order to sync in multithread.
        while self.locked.swap(true, Acquire) {
            // Enter a spin loop
            std::hint::spin_loop();
        }
        unsafe { &mut *self.value.get() }
    }

    pub fn unlock(&self) {
        self.locked.store(false, Release);
    }
}
