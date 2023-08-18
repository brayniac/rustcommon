use crate::{Bucket, BuildError, Config, Error, Histograms, _Histograms};

pub struct Builder {
    a: u8,
    b: u8,
    n: u8,
}

impl Builder {
    pub fn new(a: u8, b: u8, n: u8) -> Result<Self, BuildError> {
        // we only allow values up to 2^64
        if n > 64 {
            return Err(BuildError::MaxPowerTooHigh);
        }

        // check that the other parameters make sense together
        if a + b >= n {
            return Err(BuildError::MaxPowerTooLow);
        }

        Ok(Self { a, b, n })
    }

    pub fn build(self) -> Result<Histogram, BuildError> {
        let config = Config::new(self.a, self.b, self.n)?;

        Ok(Histogram::from_config(config))
    }
}

/// A simple histogram that can be used to track the distribution of occurances
/// of u64 values. Internally it uses 32bit counters.
pub struct Histogram {
    pub(crate) buckets: Box<[u64]>,
    pub(crate) config: Config,
}

impl _Histograms for Histogram {
    fn config(&self) -> &Config {
        &self.config
    }

    fn total_count(&self) -> u128 {
        self.buckets
            .iter()
            .map(|v| *v as u128)
            .sum()
    }

    fn get_count(&self, index: usize) -> u64 {
        self.buckets[index]
    }
}

impl<'a> Histogram {
    /// Construct a new `AtomicHistogram` from the provided parameters.
    /// * `a` sets bin width in the linear portion, the bin width is `2^a`
    /// * `b` sets the number of divisions in the logarithmic portion to `2^b`.
    /// * `n` sets the max value as `2^n`. Note: when `n` is 64, the max value
    ///   is `u64::MAX`
    ///
    /// # Constraints
    /// * `n` must be less than or equal to 64
    /// * `n` must be greater than `a + b`
    pub fn new(a: u8, b: u8, n: u8) -> Result<Self, BuildError> {
        let config = Config::new(a, b, n)?;

        Ok(Self::from_config(config))
    }

    pub fn increment(&mut self, value: u64) -> Result<(), Error> {
        self.add(value, 1)
    }

    pub fn add(&mut self, value: u64, count: u64) -> Result<(), Error> {
        let index = self.config.value_to_index(value)?;
        self.buckets[index] =
            self.buckets[index].wrapping_add(count);
        Ok(())
    }

    pub(crate) fn from_config(config: Config) -> Self {
        let buckets: Box<[u64]> = vec![0; config.total_bins()].into();
        // let buckets = Box::leak(buckets);

        Self {
            buckets,
            config,
        }
    }

    // /// Construct a `Histogram` from it's parameters and a raw pointer.
    // ///
    // /// # Safety
    // /// The pointer must be valid and outlive the `Histogram`. The allocation
    // /// must be properly aligned and initialized. The length of the slice must
    // /// match the number of bins for a histogram with the provided parameters.
    // pub unsafe fn from_raw(a: u8, b: u8, n: u8, buckets: &'a mut [u64]) -> Result<Self, BuildError> {
    //     let config = Config::new(a, b, n)?;

    //     Ok(Self { buckets, config })
    // }

    // /// Consume the `Histogram` and return a raw pointer to the buckets. It is
    // /// the caller's responsibility to free the memory allocation. If the
    // /// `Histogram` was not created from a raw pointer, then you may use
    // /// `Box::from_raw()` to cleanup the allocation. Otherwise it is your
    // /// responsibility to use the appropriate cleanup for the original
    // /// allocation.
    // pub fn into_raw(self) -> *mut [u64] {
    //     self.buckets
    // }

    pub(crate) fn as_slice(&self) -> &[u64] {
        &self.buckets
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u64] {
        &mut self.buckets
    }
}

// impl Drop for Histogram {
//     fn drop(&mut self) {
//         if !self.config.is_from_raw() {
//             // if we allocated the buckets, we must clean them up
//             let _buckets = unsafe { Box::from_raw(self.buckets) };
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size() {
        assert_eq!(std::mem::size_of::<Histogram>(), 64);
    }

    #[test]
    // Tests percentiles
    fn percentiles() {
        let mut histogram = Histogram::new(0, 7, 64).unwrap();
        for i in 0..=100 {
            println!("increment: {i}");
            let _ = histogram.increment(i);
            assert_eq!(
                histogram.percentile(0.0),
                Ok(Bucket {
                    count: 1,
                    lower: 0,
                    upper: 0
                })
            );
            assert_eq!(
                histogram.percentile(100.0),
                Ok(Bucket {
                    count: 1,
                    lower: i,
                    upper: i
                })
            );
        }
        assert_eq!(histogram.percentile(25.0).map(|b| b.upper), Ok(25));
        assert_eq!(histogram.percentile(50.0).map(|b| b.upper), Ok(50));
        assert_eq!(histogram.percentile(75.0).map(|b| b.upper), Ok(75));
        assert_eq!(histogram.percentile(90.0).map(|b| b.upper), Ok(90));
        assert_eq!(histogram.percentile(99.0).map(|b| b.upper), Ok(99));
        assert_eq!(histogram.percentile(99.9).map(|b| b.upper), Ok(100));

        let percentiles: Vec<(f64, u64)> = histogram
            .percentiles(&[50.0, 90.0, 99.0, 99.9])
            .unwrap()
            .iter()
            .map(|(p, b)| (*p, b.upper))
            .collect();

        assert_eq!(
            percentiles,
            vec![(50.0, 50), (90.0, 90), (99.0, 99), (99.9, 100)]
        );

        let _ = histogram.increment(1024);
        assert_eq!(
            histogram.percentile(99.9),
            Ok(Bucket {
                count: 1,
                lower: 1024,
                upper: 1031
            })
        );
    }
}
