// use crate::common::SlidingWindowHistogramCommon;
use super::*;

impl _SlidingWindow for Histogram<'_> {
    fn common(&self) -> &Common {
        &self.common
    }
}

/// A type of histogram that reports on the distribution of values across a
/// moving window of time. For example, the distribution of values for the past
/// minute.
pub struct Histogram<'a> {
    common: Common,

    // when the next tick begins
    tick_at: Instant,

    // the historical histogram snapshots
    snapshots: Box<[crate::Histogram<'a>]>,

    // the live histogram, this is free-running
    live: crate::Histogram<'a>,
}

impl Histogram<'_> {
    /// Create a new `SlidingWindowHistogram` given the provided parameters.
    ///
    /// Construct a new `SlidingWindowHistogram` from the provided parameters.
    /// * `a` sets bin width in the linear portion, the bin width is `2^a`
    /// * `b` sets the number of divisions in the logarithmic portion to `2^b`.
    /// * `n` sets the max value as `2^n`. Note: when `n` is 64, the max value
    ///   is `u64::MAX`
    /// * `resolution` is the duration of each discrete time slice
    /// * `slices` is the number of discrete time slices
    ///
    /// # Constraints
    /// * `n` must be less than or equal to 64
    /// * `n` must be greater than `a + b`
    /// * `resolution` in nanoseconds must fit within a `u64`
    /// * `resolution` must be at least 1 microsecond
    pub fn new(
        a: u8,
        b: u8,
        n: u8,
        resolution: core::time::Duration,
        slices: usize,
    ) -> Result<Self, BuildError> {
        let common = Common::new(a, b, n, resolution, slices)?;

        let live = crate::Histogram::new(a, b, n)?;

        let mut snapshots = Vec::with_capacity(common.num_slices());
        snapshots.resize_with(common.num_slices(), || {
            crate::Histogram::new(a, b, n).unwrap()
        });

        Ok(Self {
            tick_at: common.tick_origin() + common.resolution(),
            common,
            live,
            snapshots: snapshots.into(),
        })
    }

    /// Increment the bucket that contains the value by one. This is a
    /// convenience method that uses `Timestamp::now()` as the time associated
    /// with the observation. If you already have a timestamp, please use
    /// `increment_at` instead.
    pub fn increment(&mut self, value: u64) -> Result<(), Error> {
        self.add(value, 1)
    }

    /// Increment the bucket that contains the value by one. This is a
    /// convenience method that uses `Timestamp::now()` as the time associated
    /// with the observation. If you already have a timestamp, please use
    /// `increment_at` instead.
    pub fn add(&mut self, value: u64, count: u64) -> Result<(), Error> {
        self.add_at(Instant::now(), value, count)
    }

    pub fn increment_at(&mut self, instant: Instant, value: u64) -> Result<(), Error> {
        self.add_at(instant, value, 1)
    }

    /// Increment a timestamp-value pair by some count. This is useful if you
    /// already have done the timestamping elsewhere. For example, if tracking
    /// latency measurements, you have the timestamps for the start and end of
    /// the event and it would be wasteful to timestamp again.
    ///
    /// # NOTE
    /// When the increment requires snapshot updates (eg, when )
    pub fn add_at(&mut self, instant: Instant, value: u64, count: u64) -> Result<(), Error> {
        let tick_at = self.tick_at;

        // fast path, we just update the live histogram
        if instant < tick_at {
            // if instant < (tick_at - self.resolution) {
            // We *could* attempt to record into prior snapshots. But
            // for simplicity and to avoid changing past readings, we
            // will simply record into the live histogram anyway. We
            // might want to raise this as an error.
            // }

            return self.live.add(value, count);
        }

        // rarer path where we need to snapshot
        //
        // Even if we are behind by multiple ticks, we will only snapshot
        // into the most recent snapshot position. This ensures that we will
        // not change past readings. It also simplifies things and reduces
        // the number of load/store operations.

        let tick_next = self.tick_at + self.common.resolution();

        self.tick_at = tick_next;

        // calculate the indices for the previous start and end snapshots
        let duration =
            Duration::from_nanos(self.common.resolution().as_nanos() * self.snapshots.len() as u64);
        let end = tick_at - self.common.resolution();
        let start = end - duration;
        let (start, _end) = self.range(start, end);

        // we copy from the live slice into the start slice (since it's the oldest)
        let src = self.live.as_slice();
        let dst = self.snapshots[start].as_mut_slice();

        dst.copy_from_slice(src);

        // and finally record into the live histogram
        self.live.add(value, count)
    }
}

impl SlidingWindowHistograms for Histogram<'_> {
    fn percentiles_between(
        &self,
        start: Instant,
        end: Instant,
        percentiles: &[f64],
    ) -> Result<Vec<(f64, Bucket)>, Error> {
        let (start, end) = self.range(start, end);

        let start: &[u64] = self.snapshots[start].buckets;
        let end: &[u64] = self.snapshots[end].buckets;

        let mut buckets: Vec<u64> = start
            .iter()
            .zip(end.iter())
            .map(|(start, end)| (*end).wrapping_sub(*start))
            .collect();

        let (a, b, n) = self.live.config.params();

        let histogram = unsafe {
            crate::Histogram::from_raw(
                a,
                b,
                n,
                &mut buckets,
            )
            .unwrap()
        };

        histogram.percentiles(percentiles)
    }

    fn percentiles_last(
        &self,
        duration: Duration,
        percentiles: &[f64],
    ) -> Result<Vec<(f64, Bucket)>, Error> {
        let tick_at = self.tick_at;

        let end = tick_at - self.common.resolution();
        let start = end - duration;

        let (start, end) = self.range(start, end);

        let start: &[u64] = self.snapshots[start].buckets;
        let end: &[u64] = self.snapshots[end].buckets;

        let mut buckets: Vec<u64> = start
            .iter()
            .zip(end.iter())
            .map(|(start, end)| (*end).wrapping_sub(*start))
            .collect();

        let (a, b, n) = self.live.config.params();

        let histogram = unsafe {
            crate::Histogram::from_raw(
                a,
                b,
                n,
                &mut buckets,
            )
            .unwrap()
        };

        histogram.percentiles(percentiles)
    }
}

impl Histograms for Histogram<'_> {
    fn percentiles(&self, percentiles: &[f64]) -> Result<Vec<(f64, Bucket)>, Error> {
        // the behavior here is to return percentiles across the full window
        let duration =
            Duration::from_nanos(self.common.resolution().as_nanos() * self.snapshots.len() as u64);

        self.percentiles_last(duration, percentiles)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn size() {
        assert_eq!(std::mem::size_of::<Histogram>(), 128);
    }

    #[test]
    fn indexing() {
        let h = Histogram::new(0, 7, 64, core::time::Duration::from_secs(1), 60).unwrap();

        let origin = h.common.tick_origin();

        assert_eq!(h.range(origin, origin + Duration::from_secs(60)), (0, 60));
        assert_eq!(h.range(origin, origin + Duration::from_secs(30)), (0, 30));
        assert_eq!(h.range(origin, origin + Duration::from_secs(1)), (0, 1));
        assert_eq!(h.range(origin, origin), (0, 0));

        // if end is earlier than start, start and end indices should be the same
        assert_eq!(h.range(origin, origin - Duration::from_secs(1)), (0, 0));

        // ranges that are too long get truncated
        assert_eq!(h.range(origin, origin + Duration::from_secs(61)), (0, 60));
    }
}
