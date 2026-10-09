//! Sampling hardware capability forward in time.
//!
//! A trajectory answers two questions, and the risk engine needs both:
//!
//! - forward: how many logical qubits are available in year `Y`?
//! - inverse: in what year does capability first reach `N` logical qubits?
//!
//! Both are distributions, not numbers. Each sample perturbs the fitted curve by its
//! residual spread and by a draw from the schedule-slip prior, so a caller that wants a
//! point estimate has to ask for a quantile explicitly.

use crate::fit::{CapabilityFit, SlipPrior};
use rand::Rng;
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};

/// Default horizon beyond the last observed data point, in years.
///
/// Past this the fit is not evidence, it is an opinion with a regression line attached.
pub const DEFAULT_HORIZON_YEARS: f64 = 25.0;

/// Hard ceiling on any year the model will report.
pub const MAX_REPORTABLE_YEAR: u32 = 2200;

/// A sampled hardware capability trajectory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    /// The fitted capability curve.
    pub fit: CapabilityFit,
    /// The schedule-slip prior.
    pub slip: SlipPrior,
    /// Extrapolation horizon beyond the last fitted year.
    pub horizon_years: f64,
}

/// A warning raised while sampling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrajectoryWarning {
    /// The requested year lies beyond the extrapolation horizon.
    BeyondHorizon {
        /// Years past the last fitted observation.
        years_past: u32,
        /// The horizon in force.
        horizon: u32,
    },
    /// Capability never reaches the requested size inside the reportable range.
    NeverReached {
        /// Logical qubits requested.
        required: u64,
    },
}

impl Trajectory {
    /// Build a trajectory from a fit and a slip prior.
    pub fn new(fit: CapabilityFit, slip: SlipPrior) -> Self {
        Self {
            fit,
            slip,
            horizon_years: DEFAULT_HORIZON_YEARS,
        }
    }

    /// Sample the logical qubits available in `year`.
    ///
    /// Slip is applied by shifting the effective year *backwards*: a milestone that slips
    /// two years means the capability expected in 2029 actually arrives in 2031, which is
    /// the same as evaluating the unslipped curve two years earlier.
    pub fn sample_logical_qubits_at<R: Rng>(&self, year: f64, rng: &mut R) -> f64 {
        let slip = self.sample_slip(rng);
        let residual = Normal::new(0.0, self.fit.residual_sd.max(1e-9))
            .expect("valid normal")
            .sample(rng);
        let effective_year = year - slip;
        10f64.powf(self.fit.slope * effective_year + self.fit.intercept + residual)
    }

    /// Sample a slip in years, clamped at zero: hardware does not arrive early.
    fn sample_slip<R: Rng>(&self, rng: &mut R) -> f64 {
        let d = Normal::new(self.slip.mean_years, self.slip.sd_years.max(1e-9))
            .expect("valid normal");
        d.sample(rng).max(0.0)
    }

    /// Sample the first year in which capability reaches `required` logical qubits.
    ///
    /// Solves the fitted curve analytically rather than searching, then applies slip.
    /// Returns `None` when the curve is flat or falling, or when the answer exceeds the
    /// reportable range.
    pub fn sample_year_reaching<R: Rng>(&self, required: u64, rng: &mut R) -> Option<u32> {
        if required == 0 {
            return Some(self.fit.last_year);
        }
        if self.fit.slope <= 0.0 {
            return None;
        }
        let residual = Normal::new(0.0, self.fit.residual_sd.max(1e-9))
            .expect("valid normal")
            .sample(rng);
        let target = (required as f64).log10();
        let year = (target - self.fit.intercept - residual) / self.fit.slope;
        let with_slip = year + self.sample_slip(rng);
        let y = with_slip.ceil();
        if !y.is_finite() || y > MAX_REPORTABLE_YEAR as f64 {
            return None;
        }
        Some((y.max(self.fit.first_year as f64)) as u32)
    }

    /// Warnings that apply to a reported year.
    pub fn warnings_for(&self, year: u32) -> Vec<TrajectoryWarning> {
        let mut out = Vec::new();
        let past = year as f64 - self.fit.last_year as f64;
        if past > self.horizon_years {
            out.push(TrajectoryWarning::BeyondHorizon {
                years_past: past as u32,
                horizon: self.horizon_years as u32,
            });
        }
        out
    }

    /// Compare the fitted curve against a vendor's announced target.
    ///
    /// A large disagreement in either direction is informative: it says either that the
    /// roadmap is optimistic relative to delivered history, or that history is about to be
    /// outrun. PRAMANA reports it rather than silently preferring one source.
    pub fn disagreement_with_target(&self, target_year: u32, target_logical: u64) -> f64 {
        let predicted = self.fit.median_logical_qubits(target_year as f64);
        if target_logical == 0 {
            return 0.0;
        }
        predicted / target_logical as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::{CapabilityFit, SlipPrior};
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    fn fit() -> CapabilityFit {
        // Roughly a doubling per year, anchored so that 2026 gives ~100 logical qubits.
        CapabilityFit {
            slope: 0.30,
            intercept: 2.0 - 0.30 * 2026.0,
            residual_sd: 0.1,
            points: 4,
            first_year: 2024,
            last_year: 2026,
        }
    }

    fn slip(mean: f64) -> SlipPrior {
        SlipPrior {
            mean_years: mean,
            sd_years: 1.0,
            observed: 5,
            censored: 0,
            warnings: vec![],
        }
    }

    fn rng() -> ChaCha20Rng {
        ChaCha20Rng::seed_from_u64(42)
    }

    #[test]
    fn sampling_is_deterministic_under_a_fixed_seed() {
        // Law 5: same manifest, same result.
        let t = Trajectory::new(fit(), slip(1.0));
        let a = t.sample_logical_qubits_at(2030.0, &mut rng());
        let b = t.sample_logical_qubits_at(2030.0, &mut rng());
        assert_eq!(a, b);
    }

    #[test]
    fn capability_grows_with_year() {
        let t = Trajectory::new(fit(), slip(0.0));
        let mut r = rng();
        let early: f64 = (0..200)
            .map(|_| t.sample_logical_qubits_at(2027.0, &mut r))
            .sum();
        let late: f64 = (0..200)
            .map(|_| t.sample_logical_qubits_at(2032.0, &mut r))
            .sum();
        assert!(late > early * 10.0, "five years should be a large gain");
    }

    #[test]
    fn slip_delays_capability() {
        let optimistic = Trajectory::new(fit(), slip(0.0));
        let pessimistic = Trajectory::new(fit(), slip(4.0));
        let mut r1 = rng();
        let mut r2 = rng();
        let a: f64 = (0..500)
            .map(|_| optimistic.sample_logical_qubits_at(2032.0, &mut r1))
            .sum();
        let b: f64 = (0..500)
            .map(|_| pessimistic.sample_logical_qubits_at(2032.0, &mut r2))
            .sum();
        assert!(b < a, "slip must reduce capability at a fixed year");
    }

    #[test]
    fn slip_never_makes_hardware_arrive_early() {
        let t = Trajectory::new(fit(), slip(0.0));
        let mut r = rng();
        // With mean slip zero and sd one, half the raw draws are negative; they must clamp.
        for _ in 0..500 {
            let s = t.sample_slip(&mut r);
            assert!(s >= 0.0);
        }
    }

    #[test]
    fn inverse_and_forward_are_consistent() {
        let t = Trajectory::new(fit(), slip(0.0));
        let mut r = rng();
        let target = 10_000u64;
        let years: Vec<u32> = (0..200)
            .filter_map(|_| t.sample_year_reaching(target, &mut r))
            .collect();
        assert!(!years.is_empty());
        let median = {
            let mut v = years.clone();
            v.sort_unstable();
            v[v.len() / 2]
        };
        let predicted = t.fit.median_logical_qubits(median as f64);
        assert!(
            predicted > target as f64 / 4.0 && predicted < target as f64 * 4.0,
            "inverse solve gave {median}, at which the curve predicts {predicted:.0} against a target of {target}"
        );
    }

    #[test]
    fn a_flat_trajectory_never_reaches_a_target() {
        let mut f = fit();
        f.slope = 0.0;
        let t = Trajectory::new(f, slip(0.0));
        assert!(t.sample_year_reaching(1000, &mut rng()).is_none());
    }

    #[test]
    fn extrapolation_beyond_the_horizon_is_flagged() {
        let t = Trajectory::new(fit(), slip(0.0));
        assert!(t.warnings_for(2030).is_empty());
        let w = t.warnings_for(2080);
        assert!(matches!(w[0], TrajectoryWarning::BeyondHorizon { .. }));
    }

    #[test]
    fn disagreement_with_a_roadmap_target_is_reported_not_resolved() {
        let t = Trajectory::new(fit(), slip(0.0));
        // IBM's announced 2000 logical qubits in 2033, against the fitted curve.
        let ratio = t.disagreement_with_target(2033, 2000);
        assert!(ratio > 0.0);
        // The fitted curve here is aggressive, so it should outrun the roadmap.
        assert!(
            ratio > 1.0,
            "a doubling-per-year fit should exceed IBM's 2033 target; got {ratio:.2}"
        );
    }

    #[test]
    fn large_targets_do_not_produce_absurd_years() {
        let t = Trajectory::new(fit(), slip(0.0));
        let mut r = rng();
        if let Some(y) = t.sample_year_reaching(u64::MAX / 2, &mut r) {
            assert!(y <= MAX_REPORTABLE_YEAR);
        }
    }
}
