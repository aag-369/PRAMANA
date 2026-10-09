//! Time quantities.
//!
//! Code cycles and seconds are kept distinct because their ratio (the syndrome
//! extraction cycle time) varies by three orders of magnitude across the architectures
//! PRAMANA models: roughly 1 us for superconducting surface codes, 500 ns for cat
//! qubits, and hundreds of microseconds to milliseconds for neutral atoms once atom
//! movement is included. Silently treating a cycle as a fixed duration is exactly how
//! cross-architecture comparisons go wrong.

use crate::count_newtype;

count_newtype!(
    /// A count of syndrome-extraction rounds (code cycles).
    CodeCycles, u64, "code cycles"
);

/// A duration in seconds.
///
/// Floating point rather than an integer newtype because wall-clock estimates span
/// from microseconds to years and are always approximate.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Seconds(f64);

impl Seconds {
    /// Zero duration.
    pub const ZERO: Self = Self(0.0);

    /// Construct from a value in seconds.
    #[inline]
    pub const fn new(v: f64) -> Self {
        Self(v)
    }

    /// Construct from a value in microseconds.
    #[inline]
    pub fn from_micros(v: f64) -> Self {
        Self(v * 1e-6)
    }

    /// Construct from a value in nanoseconds.
    #[inline]
    pub fn from_nanos(v: f64) -> Self {
        Self(v * 1e-9)
    }

    /// Construct from a value in hours.
    #[inline]
    pub fn from_hours(v: f64) -> Self {
        Self(v * 3600.0)
    }

    /// Construct from a value in days.
    #[inline]
    pub fn from_days(v: f64) -> Self {
        Self(v * 86_400.0)
    }

    /// The raw value in seconds.
    #[inline]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The value expressed in hours.
    #[inline]
    pub fn as_hours(self) -> f64 {
        self.0 / 3600.0
    }

    /// The value expressed in days.
    #[inline]
    pub fn as_days(self) -> f64 {
        self.0 / 86_400.0
    }

    /// The value expressed in years (Julian, 365.25 days).
    #[inline]
    pub fn as_years(self) -> f64 {
        self.0 / (86_400.0 * 365.25)
    }

    /// The larger of two durations. Used pervasively when a wall-clock estimate is the
    /// maximum over several independent limits (reaction, factory, data movement).
    #[inline]
    pub fn max(self, other: Self) -> Self {
        if self.0 >= other.0 {
            self
        } else {
            other
        }
    }

    /// A human-readable rendering that picks a sensible unit.
    pub fn humanise(self) -> String {
        let s = self.0;
        if s < 1e-6 {
            format!("{:.3} ns", s * 1e9)
        } else if s < 1e-3 {
            format!("{:.3} us", s * 1e6)
        } else if s < 1.0 {
            format!("{:.3} ms", s * 1e3)
        } else if s < 3600.0 {
            format!("{:.2} s", s)
        } else if s < 86_400.0 {
            format!("{:.2} hours", self.as_hours())
        } else if s < 86_400.0 * 365.25 {
            format!("{:.2} days", self.as_days())
        } else {
            format!("{:.2} years", self.as_years())
        }
    }
}

impl core::ops::Add for Seconds {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl core::ops::Mul<f64> for Seconds {
    type Output = Self;
    #[inline]
    fn mul(self, k: f64) -> Self {
        Self(self.0 * k)
    }
}

impl core::iter::Sum for Seconds {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |a, b| a + b)
    }
}

impl core::fmt::Display for Seconds {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.humanise())
    }
}

impl CodeCycles {
    /// Convert a number of code cycles into wall-clock time given a cycle duration.
    ///
    /// This is the only sanctioned bridge between [`CodeCycles`] and [`Seconds`], and
    /// it forces the caller to state the cycle time explicitly.
    #[inline]
    pub fn to_seconds(self, cycle_time: Seconds) -> Seconds {
        Seconds::new(self.get() as f64 * cycle_time.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_convert_to_seconds_only_via_explicit_cycle_time() {
        let t = CodeCycles::new(1_000_000).to_seconds(Seconds::from_micros(1.0));
        assert!((t.get() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn cat_qubit_cycle_time_gives_different_answer_for_same_cycles() {
        let cycles = CodeCycles::new(1_000_000);
        let sc = cycles.to_seconds(Seconds::from_micros(1.0));
        let cat = cycles.to_seconds(Seconds::from_nanos(500.0));
        assert!(cat.get() < sc.get(), "500ns cycles are faster than 1us cycles");
        assert!((sc.get() / cat.get() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn humanise_picks_sensible_units() {
        assert!(Seconds::from_days(7.0).humanise().contains("days"));
        assert!(Seconds::from_hours(8.0).humanise().contains("hours"));
        assert!(Seconds::new(1e-7).humanise().contains("ns"));
    }

    #[test]
    fn max_selects_the_binding_limit() {
        let reaction = Seconds::from_hours(8.0);
        let factory = Seconds::from_days(3.0);
        assert_eq!(reaction.max(factory).get(), factory.get());
    }
}
