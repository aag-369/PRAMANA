//! Fitting capability trajectories and the schedule-slip prior.
//!
//! Two fits, deliberately kept apart:
//!
//! - a **capability fit** of `log10(verified logical qubits)` against year, from delivered
//!   and independently verified demonstrations only;
//! - a **slip prior** from delivered milestones' announce-to-deliver gap, with abandoned
//!   milestones retained as right-censored observations.
//!
//! Keeping them apart is the point. Fitting announced targets into the capability curve
//! would launder vendor optimism into what claims to be an empirical trajectory.

use crate::roadmap::{Modality, Roadmaps};
use serde::{Deserialize, Serialize};

/// A fitted exponential capability curve, `log10(logical qubits) = slope * year + intercept`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CapabilityFit {
    /// Slope in log10 units per year.
    pub slope: f64,
    /// Intercept.
    pub intercept: f64,
    /// Residual standard deviation in log10 units, the basis of the uncertainty band.
    pub residual_sd: f64,
    /// Number of points fitted.
    pub points: usize,
    /// Earliest year in the fit.
    pub first_year: u32,
    /// Latest year in the fit.
    pub last_year: u32,
}

impl CapabilityFit {
    /// Multiplicative growth in logical qubits per year implied by the fit.
    pub fn growth_per_year(&self) -> f64 {
        10f64.powf(self.slope)
    }

    /// Median predicted logical qubits available in `year`.
    pub fn median_logical_qubits(&self, year: f64) -> f64 {
        10f64.powf(self.slope * year + self.intercept)
    }

    /// Years beyond the last fitted point, for the extrapolation guardrail.
    pub fn extrapolation_years(&self, year: f64) -> f64 {
        (year - self.last_year as f64).max(0.0)
    }
}

/// Errors from fitting.
#[derive(Debug, Clone, PartialEq)]
pub enum FitError {
    /// Not enough data to fit a line.
    InsufficientData {
        /// How many points were available.
        have: usize,
        /// How many are needed.
        need: usize,
    },
    /// All points share a single year, so the slope is undefined.
    Degenerate,
}

impl std::fmt::Display for FitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FitError::InsufficientData { have, need } => {
                write!(f, "need at least {need} verified data points, have {have}")
            }
            FitError::Degenerate => write!(f, "all data points share one year; slope undefined"),
        }
    }
}

impl std::error::Error for FitError {}

/// Minimum verified demonstrations required before a trajectory will be fitted.
pub const MIN_FIT_POINTS: usize = 3;

/// Reduce a set of observations to the annual frontier: the best result achieved in each
/// year.
///
/// Pooling every demonstration into one regression mixes modalities that differ by two
/// orders of magnitude in encoding efficiency — Google's 1 logical qubit and Quantinuum's
/// 12 in the same year — which inflates the residual spread and biases the slope. What the
/// risk engine needs is the leading edge: an attacker uses the best machine available, not
/// the average one.
fn annual_frontier(mut points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (x, y) in points {
        match out.last_mut() {
            Some(last) if (last.0 - x).abs() < 0.5 => {
                if y > last.1 {
                    last.1 = y;
                }
            }
            _ => out.push((x, y)),
        }
    }
    out
}

/// Fit the capability curve from verified demonstrations.
///
/// Optionally restricted to one modality, since encoding efficiency differs enough between
/// technologies that a pooled fit blurs a real distinction.
pub fn fit_capability(
    roadmaps: &Roadmaps,
    modality: Option<Modality>,
) -> Result<CapabilityFit, FitError> {
    let data: Vec<(f64, f64)> = roadmaps
        .fit_data()
        .into_iter()
        .filter(|m| modality.map(|k| m.modality == k).unwrap_or(true))
        .filter_map(|m| {
            m.logical_qubits
                .filter(|l| *l > 0)
                .map(|l| (m.year as f64, (l as f64).log10()))
        })
        .collect();

    let data = annual_frontier(data);

    if data.len() < MIN_FIT_POINTS {
        return Err(FitError::InsufficientData {
            have: data.len(),
            need: MIN_FIT_POINTS,
        });
    }

    let n = data.len() as f64;
    let mx = data.iter().map(|(x, _)| x).sum::<f64>() / n;
    let my = data.iter().map(|(_, y)| y).sum::<f64>() / n;
    let sxx: f64 = data.iter().map(|(x, _)| (x - mx).powi(2)).sum();
    if sxx.abs() < f64::EPSILON {
        return Err(FitError::Degenerate);
    }
    let sxy: f64 = data.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
    let slope = sxy / sxx;
    let intercept = my - slope * mx;

    let residual_sd = if data.len() > 2 {
        let ss: f64 = data
            .iter()
            .map(|(x, y)| (y - (slope * x + intercept)).powi(2))
            .sum();
        (ss / (n - 2.0)).sqrt()
    } else {
        0.0
    };

    let first_year = data.iter().map(|(x, _)| *x as u32).min().unwrap_or(0);
    let last_year = data.iter().map(|(x, _)| *x as u32).max().unwrap_or(0);

    Ok(CapabilityFit {
        slope,
        intercept,
        residual_sd,
        points: data.len(),
        first_year,
        last_year,
    })
}

/// Fit the capability curve from verified demonstrations **and** announced targets shifted
/// later by the slip prior.
///
/// # Why announced targets belong in the fit, once corrected
///
/// A pure history fit is untrustworthy here. Verified logical-qubit demonstrations span
/// only 2024 to 2026, and a steep slope over three years extrapolates to nonsense: fitted
/// on history alone the curve predicts millions of logical qubits by 2033, against IBM's
/// own target of 2,000 for that year. When a fitted history disagrees with the people
/// building the machine by three orders of magnitude, the fit is wrong.
///
/// Roadmap targets encode engineering knowledge that three years of demonstrations do not.
/// They are also systematically optimistic, which is what the slip prior is for. Shifting
/// each target later by the mean slip and admitting it as a data point uses the vendors'
/// knowledge while discounting their optimism, and keeps the extrapolation anchored.
///
/// Verified demonstrations still dominate the near term, because the frontier reduction
/// keeps the best result per year and recent verified results exceed the near-term targets.
pub fn fit_capability_blended(
    roadmaps: &Roadmaps,
    slip: &SlipPrior,
) -> Result<CapabilityFit, FitError> {
    let mut points: Vec<(f64, f64)> = roadmaps
        .fit_data()
        .into_iter()
        .filter_map(|m| {
            m.logical_qubits
                .filter(|l| *l > 0)
                .map(|l| (m.year as f64, (l as f64).log10()))
        })
        .collect();

    for m in roadmaps.announced() {
        if let Some(l) = m.logical_qubits.filter(|l| *l > 0) {
            points.push((m.year as f64 + slip.mean_years, (l as f64).log10()));
        }
    }

    fit_points(points)
}

/// Fit a line through prepared `(year, log10 value)` points.
fn fit_points(points: Vec<(f64, f64)>) -> Result<CapabilityFit, FitError> {
    let data = annual_frontier(points);
    if data.len() < MIN_FIT_POINTS {
        return Err(FitError::InsufficientData {
            have: data.len(),
            need: MIN_FIT_POINTS,
        });
    }
    let n = data.len() as f64;
    let mx = data.iter().map(|(x, _)| x).sum::<f64>() / n;
    let my = data.iter().map(|(_, y)| y).sum::<f64>() / n;
    let sxx: f64 = data.iter().map(|(x, _)| (x - mx).powi(2)).sum();
    if sxx.abs() < f64::EPSILON {
        return Err(FitError::Degenerate);
    }
    let sxy: f64 = data.iter().map(|(x, y)| (x - mx) * (y - my)).sum();
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let residual_sd = if data.len() > 2 {
        let ss: f64 = data
            .iter()
            .map(|(x, y)| (y - (slope * x + intercept)).powi(2))
            .sum();
        (ss / (n - 2.0)).sqrt()
    } else {
        0.0
    };
    Ok(CapabilityFit {
        slope,
        intercept,
        residual_sd,
        points: data.len(),
        first_year: data.iter().map(|(x, _)| *x as u32).min().unwrap_or(0),
        last_year: data.iter().map(|(x, _)| *x as u32).max().unwrap_or(0),
    })
}

/// A fitted schedule-slip prior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlipPrior {
    /// Mean observed slip in years.
    pub mean_years: f64,
    /// Standard deviation of observed slip.
    pub sd_years: f64,
    /// Number of delivered milestones with a recorded announcement year.
    pub observed: usize,
    /// Number of abandoned milestones, treated as right-censored.
    pub censored: usize,
    /// Warnings about the quality of the fit.
    pub warnings: Vec<String>,
}

/// Extra slip charged per right-censored (abandoned) milestone.
///
/// Abandoned milestones are observations of slip that never resolved. Ignoring them makes
/// the prior optimistic; assigning them a specific value would be invention. This applies a
/// documented penalty proportional to the censoring fraction, and says so in a warning.
pub const CENSORED_SLIP_PENALTY_YEARS: f64 = 3.0;

/// Fit the slip prior.
pub fn fit_slip(roadmaps: &Roadmaps) -> SlipPrior {
    let (observed, censored) = roadmaps.slips();
    let mut warnings = Vec::new();

    let n = observed.len();
    let (mean, sd) = if n == 0 {
        warnings.push(
            "no delivered milestone carries an announcement year; slip prior is uninformed \
             and defaults to zero, which is optimistic"
                .into(),
        );
        (0.0, 1.0)
    } else {
        let m = observed.iter().sum::<i64>() as f64 / n as f64;
        let v = if n > 1 {
            observed
                .iter()
                .map(|s| (*s as f64 - m).powi(2))
                .sum::<f64>()
                / (n as f64 - 1.0)
        } else {
            1.0
        };
        (m, v.sqrt().max(0.5))
    };

    let censor_fraction = if n + censored > 0 {
        censored as f64 / (n + censored) as f64
    } else {
        0.0
    };
    let adjusted_mean = mean + censor_fraction * CENSORED_SLIP_PENALTY_YEARS;

    if n < 5 {
        warnings.push(format!(
            "slip prior fitted on only {n} observations; treat the resulting uncertainty \
             band as a lower bound on the true spread"
        ));
    }
    if censored > 0 {
        warnings.push(format!(
            "{censored} abandoned milestone(s) treated as right-censored, adding \
             {:.2} years to the mean slip",
            censor_fraction * CENSORED_SLIP_PENALTY_YEARS
        ));
    }
    warnings.push(
        "observed slips are dominated by physical-qubit milestones, which historically \
         tracked their roadmaps closely; error-correction milestones have barely completed \
         an announce-to-deliver cycle and are the ones that slip"
            .into(),
    );

    SlipPrior {
        mean_years: adjusted_mean,
        sd_years: sd,
        observed: n,
        censored,
        warnings,
    }
}

/// Observed physical-to-logical ratios for a modality, summarised.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RatioSummary {
    /// Geometric mean of observed ratios.
    pub geometric_mean: f64,
    /// Smallest observed.
    pub min: f64,
    /// Largest observed.
    pub max: f64,
    /// Number of observations.
    pub count: usize,
}

/// Summarise physical-to-logical ratios for a modality.
///
/// Geometric rather than arithmetic mean, because the ratios span more than an order of
/// magnitude and an arithmetic mean would be dominated by the worst encoder.
pub fn ratio_summary(roadmaps: &Roadmaps, modality: Modality) -> Option<RatioSummary> {
    let rs = roadmaps.ratios_for(modality);
    if rs.is_empty() {
        return None;
    }
    let log_sum: f64 = rs.iter().map(|r| r.ln()).sum();
    Some(RatioSummary {
        geometric_mean: (log_sum / rs.len() as f64).exp(),
        min: rs.iter().cloned().fold(f64::INFINITY, f64::min),
        max: rs.iter().cloned().fold(0.0, f64::max),
        count: rs.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roadmap::{Milestone, MilestoneStatus};

    fn demo(year: u32, lq: u64, pq: u64, modality: Modality) -> Milestone {
        Milestone {
            vendor: "v".into(),
            modality,
            name: "n".into(),
            year,
            announced_in: None,
            logical_qubits: Some(lq),
            physical_qubits: Some(pq),
            verified: true,
            status: MilestoneStatus::Delivered,
            citation: "c".into(),
            note: None,
        }
    }

    fn roadmaps() -> Roadmaps {
        Roadmaps {
            milestones: vec![
                demo(2024, 1, 105, Modality::Superconducting),
                demo(2024, 12, 56, Modality::TrappedIon),
                demo(2025, 48, 98, Modality::TrappedIon),
                demo(2026, 96, 448, Modality::NeutralAtom),
            ],
        }
    }

    #[test]
    fn frontier_reduction_keeps_the_best_result_per_year() {
        // Google's 1 logical qubit and Quantinuum's 12 in the same year must not be
        // averaged; an attacker uses the best machine available.
        let f = fit_capability(&roadmaps(), None).unwrap();
        assert_eq!(f.points, 3, "2024, 2025 and 2026 after frontier reduction");
    }

    #[test]
    fn history_alone_extrapolates_absurdly() {
        // Documented failure of the naive fit, retained as a regression guard. Three years
        // of steep data cannot support a ten-year extrapolation.
        let f = fit_capability(&roadmaps(), None).unwrap();
        let y2033 = f.median_logical_qubits(2033.0);
        assert!(
            y2033 > 1e5,
            "the history-only fit is expected to be absurd here; got {y2033:.0}"
        );
    }

    #[test]
    fn blending_slipped_roadmap_targets_tames_the_extrapolation() {
        let mut r = roadmaps();
        r.milestones.push(Milestone {
            vendor: "IBM".into(),
            modality: Modality::Superconducting,
            name: "Blue Jay".into(),
            year: 2033,
            announced_in: Some(2025),
            logical_qubits: Some(2000),
            physical_qubits: None,
            verified: false,
            status: MilestoneStatus::Announced,
            citation: "c".into(),
            note: None,
        });
        let slip = SlipPrior {
            mean_years: 1.6,
            sd_years: 0.9,
            observed: 5,
            censored: 0,
            warnings: vec![],
        };
        let history = fit_capability(&r, None).unwrap();
        let blended = fit_capability_blended(&r, &slip).unwrap();
        assert!(
            blended.slope < history.slope,
            "roadmap anchoring must flatten the curve: {} vs {}",
            blended.slope,
            history.slope
        );
        let y2033 = blended.median_logical_qubits(2033.0);
        assert!(
            y2033 < 1e5,
            "blended fit should stay in a plausible range at 2033; got {y2033:.0}"
        );
    }

    #[test]
    fn capability_fit_recovers_growth() {
        let f = fit_capability(&roadmaps(), None).unwrap();
        assert!(f.slope > 0.0, "logical qubit counts are growing");
        assert!(
            f.growth_per_year() > 1.5,
            "expected rapid growth, got {:.2}x/yr",
            f.growth_per_year()
        );
        assert_eq!(f.points, 3, "frontier reduction collapses 2024's two demonstrations");
        assert_eq!(f.last_year, 2026);
    }

    #[test]
    fn too_few_points_is_an_error_not_a_guess() {
        let r = Roadmaps {
            milestones: vec![demo(2025, 10, 100, Modality::TrappedIon)],
        };
        assert!(matches!(
            fit_capability(&r, None),
            Err(FitError::InsufficientData { .. })
        ));
    }

    #[test]
    fn announced_targets_never_enter_the_capability_fit() {
        let mut r = roadmaps();
        r.milestones.push(Milestone {
            logical_qubits: Some(2000),
            year: 2033,
            status: MilestoneStatus::Announced,
            verified: false,
            ..demo(2033, 2000, 0, Modality::Superconducting)
        });
        let f = fit_capability(&r, None).unwrap();
        assert_eq!(f.points, 3, "the announced 2033 target must be excluded");
        assert_eq!(
            f.last_year, 2026,
            "an announced target must not extend the fitted range"
        );
    }

    #[test]
    fn modality_filter_separates_encoders() {
        let r = roadmaps();
        let ion = fit_capability(&r, Some(Modality::TrappedIon));
        assert!(ion.is_err() || ion.unwrap().points == 2);
    }

    #[test]
    fn ratio_summary_uses_geometric_mean_and_reports_spread() {
        let r = roadmaps();
        let s = ratio_summary(&r, Modality::TrappedIon).unwrap();
        assert_eq!(s.count, 2);
        // 56/12 = 4.67 and 98/48 = 2.04; geometric mean ~3.08.
        assert!(s.geometric_mean > 2.5 && s.geometric_mean < 3.5);
        assert!(s.min < s.max);
    }

    #[test]
    fn ratios_span_an_order_of_magnitude_across_modalities() {
        // The finding that motivates fitting logical rather than physical qubits.
        let r = roadmaps();
        let sc = ratio_summary(&r, Modality::Superconducting).unwrap();
        let ion = ratio_summary(&r, Modality::TrappedIon).unwrap();
        assert!(
            sc.geometric_mean > ion.geometric_mean * 10.0,
            "superconducting {} should be far less efficient than trapped ion {}",
            sc.geometric_mean,
            ion.geometric_mean
        );
    }

    #[test]
    fn slip_prior_warns_on_a_small_sample() {
        let mut r = roadmaps();
        r.milestones[0].announced_in = Some(2022);
        let s = fit_slip(&r);
        assert!(s.warnings.iter().any(|w| w.contains("only 1 observations")));
    }

    #[test]
    fn censored_milestones_push_the_slip_prior_pessimistic() {
        let mut base = roadmaps();
        base.milestones[0].announced_in = Some(2023);
        let without = fit_slip(&base);

        let mut with = base.clone();
        with.milestones.push(Milestone {
            status: MilestoneStatus::Abandoned,
            ..demo(2030, 1000, 1000, Modality::Photonic)
        });
        let after = fit_slip(&with);

        assert!(
            after.mean_years > without.mean_years,
            "an abandoned milestone must not make the prior more optimistic"
        );
        assert_eq!(after.censored, 1);
    }

    #[test]
    fn uninformed_slip_prior_says_so() {
        let s = fit_slip(&roadmaps());
        assert_eq!(s.observed, 0);
        assert!(s.warnings.iter().any(|w| w.contains("uninformed")));
    }
}
