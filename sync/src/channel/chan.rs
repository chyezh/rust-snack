use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
};

pub struct Channel<T> {
    queue: Mutex<VecDeque<T>>,
    item_ready: Condvar,
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            item_ready: Condvar::new(),
        }
    }
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            item_ready: Condvar::new(),
        }
    }

    pub fn send(&self, value: T) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(value);
        if queue.len() == 1 {
            self.item_ready.notify_one();
        }
    }

    pub fn recv(&self) -> T {
        let mut queue = self
            .item_ready
            .wait_while(self.queue.lock().unwrap(), |q| q.is_empty())
            .unwrap();
        assert!(!queue.is_empty());
        queue.pop_front().unwrap()
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::Channel;
    #[allow(unused_imports)]
    use std::{sync::Arc, thread};

    #[test]
    fn test_channel() {
        let sender = Arc::new(Channel::new());
        let receiver = Arc::clone(&sender);
        thread::scope(|s| {
            s.spawn(|| {
                for i in 0..100 {
                    sender.send(i);
                }
            });
            s.spawn(|| {
                for i in 0..100 {
                    assert_eq!(i, receiver.recv());
                }
            });
        });
    }
}
