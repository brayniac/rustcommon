use crate::*;

pub mod atomic;
pub mod standard;

pub struct Builder {
    common: Common,
}

impl Builder {
    pub fn new(
        a: u8,
        b: u8,
        n: u8,
        resolution: core::time::Duration,
        slices: usize,
    ) -> Result<Self, BuildError> {
        Ok(Self {
            common: Common::new(a, b, n, resolution, slices)?,
        })
    }

    pub fn start_unix(mut self, start: UnixInstant) -> Self {
        if self.common.started < start {
            let delta = start - self.common.started;
            self.common.started += delta;
            self.common.tick_origin += delta;
        } else {
            let delta = self.common.started - start;
            self.common.started -= delta;
            self.common.tick_origin -= delta;
        }
        self
    }

    pub fn start_instant(mut self, start: Instant) -> Self {
        if self.common.tick_origin < start {
            let delta = start - self.common.tick_origin;
            self.common.started += delta;
            self.common.tick_origin += delta;
        } else {
            let delta = self.common.tick_origin - start;
            self.common.started -= delta;
            self.common.tick_origin -= delta;
        }
        self
    }
}

pub trait SlidingWindowHistograms {
    fn distribution_between(
        &self,
        start: Instant,
        end: Instant,
    ) -> Result<Histogram, Error>;

    fn distribution_last(
        &self,
        duration: core::time::Duration,
    ) -> Result<Histogram, Error>;

    fn percentiles_between(
        &self,
        start: Instant,
        end: Instant,
        percentiles: &[f64],
    ) -> Result<Vec<(f64, Bucket)>, Error>;

    fn percentiles_last(
        &self,
        duration: core::time::Duration,
        percentiles: &[f64],
    ) -> Result<Vec<(f64, Bucket)>, Error>;
}

impl<T: _SlidingWindow> SlidingWindowHistograms for T {
    fn distribution_between(&self, start: Instant, end: Instant) -> Result<crate::Histogram, Error> {
        self.distribution_between(start, end)
    }

    fn distribution_last(&self, duration: core::time::Duration) -> Result<crate::Histogram, Error> {
        let tick_at = self.tick_at();

        let end = tick_at - self.common().resolution();
        let start = end - duration;
        self.distribution_between(start, end)
    }

    fn percentiles_between(&self, start: Instant, end: Instant, percentiles: &[f64]) -> Result<Vec<(f64, Bucket)>, Error> {
        let histogram = self.distribution_between(start, end)?;
        histogram.percentiles(percentiles)
    }

    fn percentiles_last(
        &self,
        duration: core::time::Duration,
        percentiles: &[f64],
    ) -> Result<Vec<(f64, Bucket)>, Error> {
        let tick_at = self.tick_at();

        let end = tick_at - self.common().resolution();
        let start = end - duration;

        let histogram = self.distribution_between(start, end)?;
        histogram.percentiles(percentiles)
    }
}

pub(crate) trait _SlidingWindow {
    fn common(&self) -> &Common;

    fn range(&self, start: Instant, end: Instant) -> (usize, usize) {
        // calculate the whole number of ticks in the interval
        let interval_ticks = if end < start {
            0
        } else if end >= start + self.common().span() {
            self.max_idx()
        } else {
            (end - start).as_nanos() / self.common().resolution().as_nanos()
        };

        // calculate the offset from origin (in ticks)
        let offset_ticks = if start <= self.common().tick_origin() {
            0
        } else {
            (start - self.common().tick_origin()).as_nanos() / self.common().resolution().as_nanos()
        };

        let start = offset_ticks as usize % self.common().num_slices();
        let end = (start as u64 + interval_ticks) as usize % self.common().num_slices();

        (start, end)
    }

    fn tick_at(&self) -> Instant;

    fn max_idx(&self) -> u64 {
        self.common().num_slices() as u64 - 1
    }

    fn distribution_between(
        &self,
        start: Instant,
        end: Instant,
    ) -> Result<Histogram, Error>;
}

#[derive(Clone, Copy)]
pub struct Common {
    resolution: Duration,
    span: Duration,
    started: UnixInstant,
    tick_origin: Instant,
    num_slices: usize,
}

impl Common {
    pub fn new(
        a: u8,
        b: u8,
        n: u8,
        resolution: core::time::Duration,
        slices: usize,
    ) -> Result<Self, BuildError> {
        let num_slices = slices;

        let started = UnixInstant::now();
        let now = Instant::now();

        let resolution: u128 = resolution.as_nanos();

        assert!(resolution <= u64::MAX.into());
        assert!(resolution >= 1000);

        let span = Duration::from_nanos(resolution as u64 * slices as u64);
        let resolution = Duration::from_nanos(resolution as u64);

        // used to validate the other parameters
        let _ = config::Config::new(a, b, n)?;

        // we allocate one extra histogram for the snapshots, this prevents
        // percentile calculation reading from a histogram that's being updated
        let num_slices = num_slices + 1;

        Ok(Self {
            resolution,
            span,
            started,
            tick_origin: now - span,
            num_slices,
        })
    }

    pub fn num_slices(&self) -> usize {
        self.num_slices
    }

    pub fn tick_origin(&self) -> Instant {
        self.tick_origin
    }

    pub fn resolution(&self) -> Duration {
        self.resolution
    }

    pub fn span(&self) -> Duration {
        self.span
    }
}
