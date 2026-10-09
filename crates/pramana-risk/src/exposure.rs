//! Exposure scoring.
//!
//! A single 0-100 number, defined explicitly rather than by an opaque rubric, carrying a
//! confidence interval derived from the Monte Carlo standard error. Every term is visible
//! so a user can see why an asset scored what it did.

use crate::mosca::MoscaResolution;
use serde::{Deserialize, Serialize};

/// Business criticality, assigned by a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criticality {
    /// Minimal impact.
    Low,
    /// Moderate impact.
    Medium,
    /// Significant impact.
    High,
    /// Severe impact.
    Critical,
}

impl Criticality {
    /// Weight applied to the exposure score.
    pub fn weight(&self) -> f64 {
        match self {
            Criticality::Low => 0.4,
            Criticality::Medium => 0.7,
            Criticality::High => 0.9,
            Criticality::Critical => 1.0,
        }
    }
}

/// Sensitivity of the data protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClass {
    /// Public data.
    Public,
    /// Internal use.
    Internal,
    /// Confidential.
    Confidential,
    /// Restricted or regulated.
    Restricted,
}

impl DataClass {
    /// Weight applied to the exposure score.
    pub fn weight(&self) -> f64 {
        match self {
            DataClass::Public => 0.2,
            DataClass::Internal => 0.5,
            DataClass::Confidential => 0.8,
            DataClass::Restricted => 1.0,
        }
    }
}

/// A scored exposure, with its terms exposed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExposureScore {
    /// The score, 0-100.
    pub score: f64,
    /// Lower bound of the 95% interval.
    pub lower: f64,
    /// Upper bound of the 95% interval.
    pub upper: f64,
    /// The probability term.
    pub probability_exposed: f64,
    /// The criticality weight applied.
    pub criticality_weight: f64,
    /// The data-class weight applied.
    pub data_class_weight: f64,
    /// The formula, rendered, so the UI can show it on hover.
    pub formula: String,
}

/// Compute the exposure score.
///
/// `score = 100 * P(exposed) * w_criticality * w_data_class`
///
/// The interval comes from the Monte Carlo standard error on the probability, widened to
/// 95% and propagated through the weights. It is a sampling interval only: it does not
/// capture model uncertainty, which is what the architecture sensitivity sweep is for.
pub fn score(
    resolution: &MoscaResolution,
    criticality: Criticality,
    data_class: DataClass,
) -> ExposureScore {
    let wc = criticality.weight();
    let wd = data_class.weight();
    let p = resolution.probability_exposed;
    let scale = 100.0 * wc * wd;
    let se = resolution.probability_standard_error;
    let half = if se.is_finite() { 1.96 * se } else { 1.0 };

    ExposureScore {
        score: scale * p,
        lower: (scale * (p - half)).max(0.0),
        upper: (scale * (p + half)).min(100.0),
        probability_exposed: p,
        criticality_weight: wc,
        data_class_weight: wd,
        formula: format!(
            "100 x P(exposed)={p:.4} x criticality={wc:.2} x data_class={wd:.2} = {:.1}",
            scale * p
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(p: f64, se: f64) -> MoscaResolution {
        MoscaResolution {
            probability_exposed: p,
            probability_standard_error: se,
            deadline_year: 2040.0,
            median_break_year: Some(2038),
            harvest_now_decrypt_later_year: Some(2033),
            already_exposed: p > 0.5,
            warnings: vec![],
        }
    }

    #[test]
    fn score_is_bounded() {
        let s = score(&res(1.0, 0.0), Criticality::Critical, DataClass::Restricted);
        assert_eq!(s.score, 100.0);
        let z = score(&res(0.0, 0.0), Criticality::Low, DataClass::Public);
        assert_eq!(z.score, 0.0);
    }

    #[test]
    fn score_is_monotone_in_every_term() {
        let base = score(&res(0.5, 0.0), Criticality::Medium, DataClass::Internal).score;
        assert!(score(&res(0.9, 0.0), Criticality::Medium, DataClass::Internal).score > base);
        assert!(score(&res(0.5, 0.0), Criticality::Critical, DataClass::Internal).score > base);
        assert!(score(&res(0.5, 0.0), Criticality::Medium, DataClass::Restricted).score > base);
    }

    #[test]
    fn interval_brackets_the_score_and_stays_in_range() {
        let s = score(&res(0.5, 0.01), Criticality::High, DataClass::Confidential);
        assert!(s.lower <= s.score && s.score <= s.upper);
        assert!(s.lower >= 0.0 && s.upper <= 100.0);
    }

    #[test]
    fn an_infinite_standard_error_widens_the_interval_rather_than_hiding_it() {
        let s = score(&res(0.5, f64::INFINITY), Criticality::High, DataClass::Confidential);
        assert_eq!(s.lower, 0.0);
        assert!(s.upper > s.score);
    }

    #[test]
    fn the_formula_is_reported_so_the_score_can_be_audited() {
        let s = score(&res(0.42, 0.01), Criticality::High, DataClass::Restricted);
        assert!(s.formula.contains("P(exposed)"));
        assert!(s.formula.contains("criticality"));
    }
}
