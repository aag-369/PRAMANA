//! Validated error rates and code distances.
//!
//! Both types reject invalid values at construction (Law 9: no silent fallbacks). An
//! error rate outside `[0, 1]` or an even code distance is a modelling bug, and
//! surfacing it at the point of construction is far cheaper than tracing a nonsensical
//! qubit count back through a Monte Carlo.

use thiserror::Error;

/// Errors arising from constructing a dimensioned quantity with an invalid value.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum UnitError {
    /// An error rate was outside the closed interval `[0, 1]`.
    #[error("error rate {0} is outside [0, 1]")]
    ErrorRateOutOfRange(f64),

    /// An error rate was NaN.
    #[error("error rate is NaN")]
    ErrorRateNaN,

    /// A code distance was even. Surface-code distances must be odd so that the
    /// majority-vote decoder has no ties.
    #[error("code distance {0} is even; distances must be odd")]
    EvenDistance(u32),

    /// A code distance was zero.
    #[error("code distance must be at least 1")]
    ZeroDistance,
}

/// A probability in `[0, 1]`, used for physical and logical error rates.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ErrorRate(f64);

impl ErrorRate {
    /// Certainty of success.
    pub const ZERO: Self = Self(0.0);
    /// Certainty of failure.
    pub const ONE: Self = Self(1.0);

    /// Construct a validated error rate.
    pub fn new(v: f64) -> Result<Self, UnitError> {
        if v.is_nan() {
            return Err(UnitError::ErrorRateNaN);
        }
        if !(0.0..=1.0).contains(&v) {
            return Err(UnitError::ErrorRateOutOfRange(v));
        }
        Ok(Self(v))
    }

    /// Construct without validation. Only for compile-time constants known to be valid.
    #[inline]
    pub const fn new_unchecked(v: f64) -> Self {
        Self(v)
    }

    /// The raw probability.
    #[inline]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Probability that at least one of `n` independent events with this rate occurs.
    ///
    /// Uses the exact complement `1 - (1-p)^n` rather than the union bound `n*p`, which
    /// overestimates badly once `n*p` approaches 1 and can produce "probabilities"
    /// above 1 in resource estimates.
    pub fn at_least_once_in(self, n: u128) -> ErrorRate {
        if n == 0 {
            return ErrorRate::ZERO;
        }
        // (1-p)^n computed in log space for numerical stability at large n.
        let ln_survive = (1.0 - self.0).ln() * n as f64;
        let p = 1.0 - ln_survive.exp();
        ErrorRate(p.clamp(0.0, 1.0))
    }

    /// The union bound `n * p`, clamped to 1.
    ///
    /// Retained because several published estimates use it; PRAMANA records which
    /// convention a model used so reproductions are like-for-like.
    pub fn union_bound(self, n: u128) -> ErrorRate {
        ErrorRate((self.0 * n as f64).clamp(0.0, 1.0))
    }
}

impl core::fmt::Display for ErrorRate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0 == 0.0 {
            write!(f, "0")
        } else {
            write!(f, "{:.3e}", self.0)
        }
    }
}

/// A non-zero quantum error-correcting code distance.
///
/// Oddness is a *surface-code* convention, not a universal one: it ensures the
/// majority-vote decoder has no ties. Bivariate bicycle codes routinely have even
/// distance, the `[[144,12,12]]` gross code being the obvious example. `Distance::new`
/// therefore accepts any non-zero distance, and [`Distance::new_odd`] enforces the
/// stricter surface-code requirement where it genuinely applies.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Distance(u32);

impl Distance {
    /// Construct a validated distance. Must be non-zero.
    pub fn new(d: u32) -> Result<Self, UnitError> {
        if d == 0 {
            return Err(UnitError::ZeroDistance);
        }
        Ok(Self(d))
    }

    /// Construct a distance that must additionally be odd.
    ///
    /// Use this in surface-code contexts, where an even distance would leave the
    /// majority-vote decoder with ties.
    pub fn new_odd(d: u32) -> Result<Self, UnitError> {
        if d == 0 {
            return Err(UnitError::ZeroDistance);
        }
        if d % 2 == 0 {
            return Err(UnitError::EvenDistance(d));
        }
        Ok(Self(d))
    }

    /// Round `d` up to the next odd value and construct.
    ///
    /// Distance solvers naturally produce real-valued requirements; this is the
    /// sanctioned way to land on a legal distance, and it always rounds *up* so the
    /// error target is met rather than missed.
    pub fn next_odd_at_least(d: u32) -> Self {
        let d = d.max(1);
        Self(if d % 2 == 0 { d + 1 } else { d })
    }

    /// The raw distance.
    #[inline]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The number of error-correction rounds conventionally run per logical timestep,
    /// equal to the distance for standard surface-code lattice surgery.
    #[inline]
    pub const fn rounds_per_logical_step(self) -> u64 {
        self.0 as u64
    }

    /// Physical qubits in one rotated surface-code patch: `d^2` data + `d^2 - 1` measure.
    #[inline]
    pub const fn rotated_patch_qubits(self) -> u64 {
        let d = self.0 as u64;
        2 * d * d - 1
    }

    /// The next larger legal distance.
    #[inline]
    pub fn next(self) -> Self {
        Self(self.0 + 2)
    }
}

impl core::fmt::Display for Distance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "d={}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_rate_rejects_out_of_range() {
        assert!(ErrorRate::new(-0.1).is_err());
        assert!(ErrorRate::new(1.5).is_err());
        assert!(ErrorRate::new(f64::NAN).is_err());
        assert!(ErrorRate::new(0.001).is_ok());
    }

    #[test]
    fn distance_rejects_zero_but_allows_even() {
        assert!(matches!(Distance::new(0), Err(UnitError::ZeroDistance)));
        assert_eq!(Distance::new(27).unwrap().get(), 27);
        // The [[144,12,12]] gross code has even distance; refusing it would be wrong.
        assert_eq!(Distance::new(12).unwrap().get(), 12);
    }

    #[test]
    fn odd_constructor_enforces_the_surface_code_convention() {
        assert!(matches!(Distance::new_odd(4), Err(UnitError::EvenDistance(4))));
        assert!(matches!(Distance::new_odd(0), Err(UnitError::ZeroDistance)));
        assert_eq!(Distance::new_odd(25).unwrap().get(), 25);
    }

    #[test]
    fn next_odd_rounds_up_never_down() {
        assert_eq!(Distance::next_odd_at_least(4).get(), 5);
        assert_eq!(Distance::next_odd_at_least(5).get(), 5);
        assert_eq!(Distance::next_odd_at_least(0).get(), 1);
    }

    #[test]
    fn rotated_patch_qubit_count_matches_known_values() {
        // d=3 -> 9 data + 8 measure = 17; d=27 -> 1458 - 1 = 1457
        assert_eq!(Distance::new(3).unwrap().rotated_patch_qubits(), 17);
        assert_eq!(Distance::new(27).unwrap().rotated_patch_qubits(), 1457);
    }

    #[test]
    fn union_bound_overestimates_relative_to_exact_complement() {
        let p = ErrorRate::new(1e-9).unwrap();
        let n = 1_000_000_000u128;
        let exact = p.at_least_once_in(n).get();
        let bound = p.union_bound(n).get();
        assert!(bound >= exact, "union bound must not underestimate");
        assert!(exact > 0.6 && exact < 0.7, "1 - e^-1 = 0.632, got {exact}");
    }

    #[test]
    fn union_bound_saturates_instead_of_exceeding_one() {
        let p = ErrorRate::new(0.5).unwrap();
        assert_eq!(p.union_bound(100).get(), 1.0);
    }
}
