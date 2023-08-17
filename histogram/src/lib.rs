//! This crate contains a collection of histogram datastructures to help count
//! occurances of values and report on their distribution.
//!
//! There are several implementations to choose from, with each targeting
//! a specific use-case.
//!
//! All the implementations share the same bucketing / binning strategy and
//! allow you to store values across a wide range with minimal loss of
//! precision. We do this by using linear buckets for the smaller values in the
//! histogram and transition to logarithmic buckets with linear subdivisions for
//! buckets that contain larger values. The indexing strategy is designed to be
//! efficient, allowing for blazingly fast increments.
//!
//! * `Histogram` - when a very fast histogram is all you need
//! * `AtomicHistogram` - when you need to share a histogram across threads
//! * `SlidingWindowHistogram` - if you care about data points within a bounded
//!    range of time, with old values automatically dropping out
//!

pub mod atomic;
pub mod sliding_window;

mod bucket;
mod config;
mod errors;
mod standard;

pub use clocksource::precise::{Instant, UnixInstant};

pub use bucket::Bucket;
pub use errors::{BuildError, Error};
pub use standard::Histogram;

use core::sync::atomic::Ordering;

use crate::config::Config;

use clocksource::precise::{AtomicInstant, Duration};

/// A private trait that allows us to share logic across `Histogram` and
/// `AtomicHistogram` types.
trait _Histograms {
    fn config(&self) -> &Config;

    fn total_count(&self) -> u128;

    fn get_count(&self, index: usize) -> u64;

    fn get_bucket(&self, index: usize) -> Bucket {
        Bucket {
            count: self.get_count(index),
            lower: self.config().index_to_lower_bound(index),
            upper: self.config().index_to_upper_bound(index),
        }
    }
}

/// A histogram stores counts for values and produces summary statistics about
/// the distribution of values.
pub trait Histograms {
    fn percentile(&self, percentile: f64) -> Result<Bucket, Error> {
        self.percentiles(&[percentile])
            .map(|v| v.first().unwrap().1)
    }

    fn percentiles(&self, percentiles: &[f64]) -> Result<Vec<(f64, Bucket)>, Error>;
}

impl<T: _Histograms> Histograms for T {
    fn percentiles(&self, percentiles: &[f64]) -> Result<Vec<(f64, Bucket)>, Error> {
        // get the total count across all buckets as a u64
        let total: u128 = self.total_count();

        // if the histogram is empty, then we should return an error
        if total == 0_u128 {
            // TODO(brian): this should return an error =)
            return Err(Error::Empty);
        }

        // sort the requested percentiles so we can find them in a single pass
        let mut percentiles = percentiles.to_vec();
        percentiles.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut result = Vec::new();

        let mut have = 0_u128;
        let mut percentile_idx = 0_usize;
        let mut current_idx = 0_usize;
        let mut max_idx = 0_usize;

        // outer loop walks through the requested percentiles
        'outer: loop {
            // if we have all the requested percentiles, return the result
            if percentile_idx >= percentiles.len() {
                return Ok(result);
            }

            // calculate the count we need to have for the requested percentile
            let percentile = percentiles[percentile_idx];
            let needed = (percentile / 100.0 * total as f64).ceil() as u128;

            // if the count is already that high, push to the results and
            // continue onto the next percentile
            if have >= needed {
                result.push((percentile, self.get_bucket(current_idx)));
                percentile_idx += 1;
                continue;
            }

            // the inner loop walks through the buckets
            'inner: loop {
                // if we've run out of buckets, break the outer loop
                if current_idx >= self.config().total_bins() {
                    break 'outer;
                }

                // get the current count for the current bucket
                let current_count = self.get_count(current_idx);

                // track the highest index with a non-zero count
                if current_count > 0 {
                    max_idx = current_idx;
                }

                // increment what we have by the current bucket count
                have += current_count as u128;

                // if this is enough for the requested percentile, push to the
                // results and break the inner loop to move onto the next
                // percentile
                if have >= needed {
                    result.push((percentile, self.get_bucket(current_idx)));
                    percentile_idx += 1;
                    current_idx += 1;
                    break 'inner;
                }

                // increment the current_idx so we continue from the next bucket
                current_idx += 1;
            }
        }

        // fill the remaining percentiles with the highest non-zero bucket's
        // value. this is possible if the histogram has been modified while we
        // are still iterating.
        for percentile in percentiles.iter().skip(result.len()) {
            result.push((*percentile, self.get_bucket(max_idx)));
        }

        Ok(result)
    }
}
