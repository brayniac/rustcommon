use super::*;
use core::sync::atomic::AtomicU64;

use crate::atomic::Histogram as AtomicHistogram;

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
    tick_at: AtomicInstant,

    // the historical histogram snapshots
    snapshots: Box<[AtomicHistogram<'a>]>,

    // the live histogram, this is free-running
    live: AtomicHistogram<'a>,
}

impl Histogram<'_> {
    /// Create a new histogram that stores values across a sliding window and
    /// allows concurrent modification.
    ///
    /// # Parameters:
    /// * `a` sets bin width in the linear portion, the bin width is `2^a`
    /// * `b` sets the number of divisions in the logarithmic portion to `2^b`.
    /// * `n` sets the max value as `2^n`. Note: when `n` is 64, the max value
    ///   is `u64::MAX`
    /// * `resolution` is the duration of each discrete time slice
    /// * `slices` is the number of discrete time slices
    ///
    /// # Constraints:
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

        let live = AtomicHistogram::new(a, b, n)?;

        let mut snapshots = Vec::with_capacity(common.num_slices());
        snapshots.resize_with(common.num_slices(), || {
            AtomicHistogram::new(a, b, n).unwrap()
        });

        Ok(Self {
            tick_at: (common.tick_origin() + common.resolution()).into(),
            common,
            live,
            snapshots: snapshots.into(),
        })
    }

    /// Moves the window forward, if necessary.
    pub fn snapshot(&self, instant: Instant) {
        loop {
            let tick_at = self.tick_at.load(Ordering::Relaxed);

            // fast path when the window does not need to be advanced
            if instant < tick_at {
                return;
            }

            // otherwise we need to slide the window forward

            // Even if we are behind by multiple ticks, we will only snapshot
            // into the most recent snapshot position. This ensures that we will
            // not change past readings. It also simplifies things and reduces
            // the number of load/store operations.
            //
            // To actually snapshot, let's just move the tick_at forward to
            // unblock other increments. This will slightly smear things into
            // the snapshot that occur after the end boundary, but this tradeoff
            // seems worth it to reduce pause durations.

            let tick_next = tick_at + self.common.resolution();

            // cas and if we lose, loop back, another thread may have won
            if self
                .tick_at
                .compare_exchange(tick_at, tick_next, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }

            // we won the race, let's snapshot

            // calculate the indices for the previous start and end snapshots
            let duration = Duration::from_nanos(
                self.common.resolution().as_nanos() * self.snapshots.len() as u64,
            );
            let end = tick_at - self.common.resolution();
            let start = end - duration;
            let (start, _end) = self.range(start, end);

            // we copy from the live slice into the start slice (since it's the oldest)
            let src = self.live.as_slice();
            let dst = self.snapshots[start].as_slice();

            for (s, d) in src.iter().zip(dst) {
                d.store(s.load(Ordering::Relaxed), Ordering::Relaxed);
            }
        }
    }

    /// Increment the bucket that contains the value by one. This is a
    /// convenience method that uses `Timestamp::now()` as the time associated
    /// with the observation. If you already have a timestamp, please use
    /// `increment_at` instead.
    pub fn add(&self, value: u64, count: u64) -> Result<(), Error> {
        self.add_at(Instant::now(), value, count)
    }

    /// Increment the bucket that contains the value by one. This is a
    /// convenience method that uses `Timestamp::now()` as the time associated
    /// with the observation. If you already have a timestamp, please use
    /// `increment_at` instead.
    pub fn increment(&self, value: u64) -> Result<(), Error> {
        self.add(value, 1)
    }


    pub fn increment_at(&self, instant: Instant, value: u64) -> Result<(), Error> {
        self.add_at(instant, value, 1)
    }

    /// Increment a timestamp-value pair by some count. This is useful if you
    /// already have done the timestamping elsewhere. For example, if tracking
    /// latency measurements, you have the timestamps for the start and end of
    /// the event and it would be wasteful to timestamp again.
    ///
    /// # NOTE
    /// When the increment requires snapshot updates (eg, when )
    pub fn add_at(&self, instant: Instant, value: u64, count: u64) -> Result<(), Error> {
        self.snapshot(instant);

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

        let start: &[AtomicU64] = self.snapshots[start].buckets;
        let end: &[AtomicU64] = self.snapshots[end].buckets;

        let mut buckets: Vec<u64> = start
            .iter()
            .zip(end.iter())
            .map(|(start, end)| {
                end.load(Ordering::Relaxed)
                    .wrapping_sub(start.load(Ordering::Relaxed))
            })
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
        let tick_at = self.tick_at.load(Ordering::Relaxed);

        let end = tick_at - self.common.resolution();
        let start = end - duration;

        let (start, end) = self.range(start, end);

        let start: &[AtomicU64] = self.snapshots[start].buckets;
        let end: &[AtomicU64] = self.snapshots[end].buckets;

        let mut buckets: Vec<u64> = start
            .iter()
            .zip(end.iter())
            .map(|(start, end)| {
                end.load(Ordering::Relaxed)
                    .wrapping_sub(start.load(Ordering::Relaxed))
            })
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
        let now = Instant::now();
        let h = Histogram::new(0, 7, 64, core::time::Duration::from_secs(1), 60).unwrap();
        assert_eq!(h.range(now - Duration::from_secs(60), now), (0, 60));
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(30)),
            (0, 30)
        );
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(59)),
            (0, 1)
        );
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(60)),
            (0, 0)
        );

        // if end is earlier than start, start and end indices should be the same
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(61)),
            (0, 0)
        );

        // we can't report across a range that's longer than our history
        assert_eq!(
            h.range(now - Duration::from_secs(60), now + Duration::from_secs(1)),
            (0, 60)
        );

        let now = Instant::now();
        let h = Histogram::new(0, 7, 64, core::time::Duration::from_secs(1), 60).unwrap();
        assert_eq!(h.range(now - Duration::from_secs(60), now), (0, 60));
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(30)),
            (0, 30)
        );
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(59)),
            (0, 1)
        );
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(60)),
            (0, 0)
        );

        // if end is earlier than start, start and end indices should be the same
        assert_eq!(
            h.range(now - Duration::from_secs(60), now - Duration::from_secs(61)),
            (0, 0)
        );

        // ranges that are too long get truncated
        assert_eq!(
            h.range(now - Duration::from_secs(60), now + Duration::from_secs(1)),
            (0, 60)
        );
    }
}
