use std::ops::Range;
use core::sync::atomic::AtomicU64;
use crate::{Metric, Value};
use std::sync::OnceLock;
use std::time::Duration;

use histogram::SlidingWindowHistogram as _Histogram;

pub use ::histogram::Error as HistogramError;
pub use ::histogram::{Bucket, Instant, Snapshot, UnixInstant};

// pub use ::histogram::sliding_window::atomic::Iter as HistogramIter;

/// A heatmap holds counts for quantized values across a period of time. It can
/// be used to record observations at points in time and report out percentile
/// metrics or the underlying distribution.
///
/// Common use cases of heatmaps include any per-event measurement such as
/// latency or size. Alternate use cases include summarizing fine-grained
/// observations (sub-secondly rates, or sub-secondly gauge readings) into
/// percentiles across a period of time.
///
/// Heatmaps are lazily initialized, which means that read methods will return
/// a None variant until some write has occured. This also means they occupy
/// very little space until they are initialized.
pub struct Histogram {
    inner: OnceLock<_Histogram>,
    a: u8,
    b: u8,
    n: u8,
    resolution: Duration,
    slices: usize,
}

impl Histogram {
    /// Create a new heatmap with the given parameters.
    ///
    /// - `m` - sets the minimum resolution `M = 2^m`. This is the smallest unit
    /// of quantification, which is also the smallest bucket width. If the input
    /// values are always integers, choosing `m=0` would ensure precise
    /// recording for the smallest values.
    ///
    /// - `r` - sets the minimum resolution range `R = 2^r - 1`. The selected
    /// value must be greater than the minimum resolution `m`. This sets the
    /// maximum value that the minimum resolution should extend to.
    ///
    /// - `n` - sets the maximum value `N = 2^n - 1`. The selected value must be
    /// greater than or equal to the minimum resolution range `r`.
    ///
    /// - `span` - sets the total window of time that the heatmap will cover.
    /// Observations that are older than the span will age out.
    ///
    /// - `resolution` - sets the resolution in the time domain. The times of
    /// observations are quantized into slices of this duration. Entire slices
    /// are aged out of the heatmap as necessary.
    pub const fn new(a: u8, b: u8, n: u8, resolution: Duration, slices: usize) -> Self {
        Self {
            a,
            b,
            n,
            resolution,
            slices,
            inner: OnceLock::new(),
        }
    }

    /// Returns the `Bucket` (if any) where the requested percentile falls
    /// within the value range for the bucket.Percentiles should be expressed as
    /// a value in the range `0.0..=100.0`.
    ///
    /// `None` will be returned if the heatmap has not been written to.
    pub fn percentile(&self, percentile: f64, range: Range<UnixInstant>) -> Option<Result<Bucket, HistogramError>> {
        self.inner
            .get()
            .map(|h| h.snapshot_between(range)?.percentile(percentile))
    }

    /// Increments a time-value pair by one.
    pub fn increment(&self, time: Instant, value: u64) -> Result<(), HistogramError> {
        self.add(time, value, 1)
    }

    /// Increments a time-value pair by some count.
    pub fn add(&self, time: Instant, value: u64, count: u64) -> Result<(), HistogramError> {
        self.get_or_init().add_at(time, value, count)
    }

    // pub fn iter(&self) -> Option<HeatmapIter> {
    //     self.inner.get().map(|heatmap| heatmap.iter())
    // }

    fn get_or_init(&self) -> &_Histogram {
        self.inner.get_or_init(|| {
            _Histogram::new(
                self.a,
                self.b,
                self.n,
                self.resolution,
                self.slices,
            )
            .unwrap()
        })
    }

    pub fn as_slice(&self) -> &[AtomicU64] {
        self.get_or_init().as_slice()
    }

    pub fn snapshot_between(&self, range: Range<UnixInstant>) -> Option<Result<histogram::Snapshot, HistogramError>> {
        self.inner.get().map(|h| h.snapshot_between(range))
    }
}

impl Metric for Histogram {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn value(&self) -> Option<Value> {
        Some(Value::Histogram(self))
    }
}
