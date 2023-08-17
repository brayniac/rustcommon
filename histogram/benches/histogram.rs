use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use criterion::Throughput;
use criterion::{criterion_group, criterion_main, Criterion};
use heatmap2::MovingWindowHistogram;
use heatmap2::{AtomicHistogram, Histogram};
use std::sync::Arc;

#[cfg(target_os = "linux")]
mod perf;
#[cfg(target_os = "linux")]
use perf::FlamegraphProfiler;

#[cfg(target_os = "linux")]
fn custom() -> Criterion {
    Criterion::default().with_profiler(FlamegraphProfiler::new(100))
}

fn histogram(c: &mut Criterion) {
    let mut group = c.benchmark_group("histogram");

    group.throughput(Throughput::Elements(1));

    let mut histogram = Histogram::new(0, 7, 64).unwrap();

    group.bench_function("increment (linear)", |b| b.iter(|| histogram.increment(1)));
    group.bench_function("increment (log)", |b| {
        b.iter(|| histogram.increment(95633239299398))
    });

    let mut histogram = Histogram::new(0, 7, 64).unwrap();
    histogram.increment(u64::MAX);

    group.bench_function("percentile", |b| b.iter(|| histogram.percentile(100.0)));
}

fn atomic_histogram(c: &mut Criterion) {
    let mut group = c.benchmark_group("atomic histogram");

    group.throughput(Throughput::Elements(1));

    let histogram = AtomicHistogram::new(0, 7, 64).unwrap();

    group.bench_function("increment (linear)", |b| b.iter(|| histogram.increment(1)));
    group.bench_function("increment (log)", |b| {
        b.iter(|| histogram.increment(95633239299398))
    });

    // prepare to test contended performance
    let running = Arc::new(AtomicBool::new(true));
    let histogram = Arc::new(histogram);
    let h = histogram.clone();
    let r = running.clone();

    std::thread::spawn(move || {
        while r.load(Ordering::Relaxed) {
            h.increment(1);
        }
    });

    group.bench_function("increment (contended)", |b| {
        b.iter(|| histogram.increment(1))
    });
    running.store(false, Ordering::Relaxed);

    let histogram = AtomicHistogram::new(0, 7, 64).unwrap();
    histogram.increment(u64::MAX);

    group.bench_function("percentile", |b| b.iter(|| histogram.percentile(100.0)));
}

fn moving_window_histogram(c: &mut Criterion) {
    let mut group = c.benchmark_group("moving window histogram");

    group.throughput(Throughput::Elements(1));

    let histogram = MovingWindowHistogram::new(0, 7, 64, Duration::from_micros(100), 1000);
    // let mut now = clocksource::precise::Instant::now();
    group.bench_function("increment (linear)", |b| {
        b.iter(|| {
            histogram.increment_at(clocksource::precise::Instant::now(), 1);
            // now += clocksource::precise::Duration::from_millis(1);
        })
    });
}

#[cfg(not(target_os = "linux"))]
criterion_group!(
    benches,
    histogram,
    atomic_histogram,
    moving_window_histogram
);

#[cfg(target_os = "linux")]
criterion_group! {
    name = benches;
    config = custom();
    targets = histogram, atomic_histogram, moving_window_histogram
}

criterion_main!(benches);
