use super::mutex::MutexGuard;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicU32, AtomicUsize};

use atomic_wait::{wait, wake_all, wake_one};

pub struct Condvar {
    counter: AtomicU32,
    num_waiters: AtomicUsize,
}

impl Condvar {
    /// Create a new Condvar.
    pub const fn new() -> Self {
        Self {
            counter: AtomicU32::new(0),
            num_waiters: AtomicUsize::new(0),
        }
    }

    /// Notify one thread waiting for signal.
    pub fn notify_one(&self) {
        if self.num_waiters.load(Relaxed) > 0 {
            self.counter.fetch_add(1, Relaxed);
            wake_one(&self.counter);
        }
    }

    /// Notify one thread waiting for all signal.
    pub fn notify_all(&self) {
        if self.num_waiters.load(Relaxed) > 0 {
            self.counter.fetch_add(1, Relaxed);
            wake_all(&self.counter);
        }
    }

    /// Wait for notifying signal. May waking up spuriously.
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        // Protected by Mutex, so Relaxed is enough in correct use of CondVar.
        self.num_waiters.fetch_add(1, Relaxed);
        let counter_value = self.counter.load(Relaxed);

        // Remember the mutex reference and release it.
        let mutex = guard.mutex;
        drop(guard);

        // Wait for notifying.
        wait(&self.counter, counter_value);

        // No notifying is needed if spurious wake-up happens.
        // It's safe to use relaxed ordering on here.
        self.num_waiters.fetch_sub(1, Relaxed);

        // Lock the mutex after notifying.
        mutex.lock()
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use crate::mutex::Mutex;

    use super::Condvar;

    #[test]
    fn test_condvar() {
        let m = Mutex::new(0);
        let cv = Condvar::new();

        let mut wakeups = 0;
        thread::scope(|s| {
            s.spawn(|| {
                thread::sleep(Duration::from_secs(1));
                *m.lock() = 123;
                cv.notify_one();
            });

            let mut m = m.lock();
            while *m < 100 {
                m = cv.wait(m);
                wakeups += 1;
            }

            assert_eq!(*m, 123);
        });
        assert!(wakeups < 10);
    }
}
