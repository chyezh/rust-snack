use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::thread;
use sync::mutex::Mutex;

const LOOP_COUNTS: usize = 10;

#[allow(dead_code)]
fn bench_single_thread_mutex(c: &mut Criterion) {
    let m = Mutex::new(0);
    black_box(&m);
    c.bench_function("single thread mutex", |b| {
        b.iter(|| {
            for _ in 0..LOOP_COUNTS {
                *m.lock() += 1;
            }
        })
    });
}

#[allow(dead_code)]
fn bench_multi_thread_mutex(c: &mut Criterion) {
    let m = Mutex::new(0);
    black_box(&m);
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..LOOP_COUNTS {
                    *m.lock() += 1;
                }
            });
        }
        c.bench_function("single thread mutex", |b| b.iter(|| *m.lock() += 1));
    });
}

criterion_group!(mutex, bench_single_thread_mutex, bench_multi_thread_mutex);
criterion_main!(mutex);
