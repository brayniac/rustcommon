use core::ops::{Add, AddAssign, Sub, SubAssign};

use super::Duration;

/// An instant represents a moment in time and is taken from the system
/// monotonic clock. Unlike `std::time::Instant` the internal representation
/// uses only nanoseconds in a u64 field to hold the clock reading. This means
/// that they will wrap after ~584 years.
#[repr(transparent)]
#[derive(Copy, Clone, Default, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnixInstant {
    pub(crate) secs: u32,
}

impl UnixInstant {
    pub const EPOCH: UnixInstant = UnixInstant { secs: 0 };

    /// Return a `UnixInstant` that represents the current moment.
    #[cfg(not(target_os = "macos"))]
    pub fn now() -> Self {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME_COARSE, &mut ts);
        }

        let now = ts.tv_sec as u32;

        Self { secs: now }
    }

    /// Return a `UnixInstant` that represents the current moment.
    #[cfg(target_os = "macos")]
    pub fn now() -> Self {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);
        }

        let now = ts.tv_sec as u32;

        Self { secs: now }
    }

    /// Return the elapsed time, in nanoseconds, since the original timestamp.
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    /// Return the elapsed duration, in nanoseconds, from some earlier timestamp
    /// until this timestamp.
    pub fn duration_since(&self, earlier: Self) -> Duration {
        *self - earlier
    }

    pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
        self.secs.checked_sub(earlier.secs).map(|secs| Duration { secs })
    }

    pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
        self.secs.checked_sub(duration.secs).map(|secs| Self { secs })
    }
}

impl Add<Duration> for UnixInstant {
    type Output = UnixInstant;

    fn add(self, rhs: Duration) -> Self::Output {
        UnixInstant {
            secs: self.secs + rhs.secs,
        }
    }
}

impl Sub<UnixInstant> for UnixInstant {
    type Output = Duration;

    fn sub(self, rhs: UnixInstant) -> Self::Output {
        Duration {
            secs: self.secs - rhs.secs,
        }
    }
}

impl AddAssign<Duration> for UnixInstant {
    fn add_assign(&mut self, rhs: Duration) {
        self.secs += rhs.secs;
    }
}

impl Sub<Duration> for UnixInstant {
    type Output = UnixInstant;

    fn sub(self, rhs: Duration) -> Self::Output {
        UnixInstant {
            secs: self.secs - rhs.secs,
        }
    }
}

impl SubAssign<Duration> for UnixInstant {
    fn sub_assign(&mut self, rhs: Duration) {
        self.secs -= rhs.secs;
    }
}
