//! Mosca's inequality, resolved against a computed break-year distribution.
//!
//! The classical statement is `X + Y > Z`: if the time to migrate plus the time the data
//! must stay secret exceeds the time until a capable quantum computer exists, the asset is
//! already exposed.
//!
//! # A deliberate departure from the usual notation
//!
//! The literature is inconsistent about which letter is which — some sources use `X` for
//! migration time and `Y` for data shelf life, others reverse them. The inequality is
//! symmetric in the two so the arithmetic is unaffected, but a user-facing label that means
//! the opposite of what a reader expects is a real hazard. This module therefore uses named
//! fields throughout and never exposes the letters.
//!
//! # And a departure from the usual resolution
//!
//! `Z` is normally a single hand-waved date. Here it is a distribution computed per asset,
//! so the inequality resolves to a **probability of exposure** rather than a boolean.

use crate::montecarlo::BreakYearDistribution;
use serde::{Deserialize, Serialize};

/// What kind of harm a break causes, which changes how the inequality applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreatMode {
    /// Key establishment and encryption. Vulnerable to harvest-now-decrypt-later: traffic
    /// captured today can be decrypted whenever the machine arrives, so the full data
    /// retention period counts against the attacker's schedule.
    Confidentiality,
    /// Signatures. A forged signature is only useful once the machine exists, so past
    /// traffic is not retroactively compromised and the secrecy lifetime collapses toward
    /// zero — *except* for long-lived trust anchors, whose validity period reintroduces it.
    Authenticity,
    /// Both apply.
    Both,
}

/// The asset-side inputs to the inequality.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MoscaInputs {
    /// Years needed to migrate this asset to post-quantum cryptography.
    pub migration_years: f64,
    /// Years the data must remain confidential.
    pub secrecy_lifetime_years: f64,
    /// How a break would harm this asset.
    pub threat_mode: ThreatMode,
    /// Year from which the clock runs.
    pub reference_year: u32,
}

impl MoscaInputs {
    /// The effective secrecy lifetime under this threat mode.
    ///
    /// For authenticity-only assets a break is not retroactive, so the data retention
    /// period does not count. Treating a code-signing certificate the same as a TLS session
    /// key would badly distort the prioritisation.
    pub fn effective_secrecy_years(&self) -> f64 {
        match self.threat_mode {
            ThreatMode::Confidentiality | ThreatMode::Both => self.secrecy_lifetime_years,
            ThreatMode::Authenticity => 0.0,
        }
    }

    /// The year by which migration must be complete for the asset never to be exposed.
    pub fn deadline_year(&self) -> f64 {
        self.reference_year as f64 + self.migration_years + self.effective_secrecy_years()
    }
}

/// The resolved inequality.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoscaResolution {
    /// Probability that the break arrives before migration plus secrecy completes.
    pub probability_exposed: f64,
    /// Monte Carlo standard error on that probability.
    pub probability_standard_error: f64,
    /// The computed deadline year.
    pub deadline_year: f64,
    /// Median computed break year, where one exists.
    pub median_break_year: Option<u32>,
    /// The earliest credible break year, at the 5th percentile.
    ///
    /// For confidentiality assets this is the date that actually matters: data captured
    /// today is at risk from here.
    pub harvest_now_decrypt_later_year: Option<u32>,
    /// Whether the asset is already exposed at conventional confidence.
    pub already_exposed: bool,
    /// Warnings.
    pub warnings: Vec<String>,
}

/// Probability threshold above which an asset is called already exposed.
pub const EXPOSED_THRESHOLD: f64 = 0.5;

/// Resolve the inequality against a computed break-year distribution.
pub fn resolve(inputs: &MoscaInputs, dist: &BreakYearDistribution) -> MoscaResolution {
    let deadline = inputs.deadline_year();
    let p = dist.probability_broken_by(deadline.floor() as u32);
    let n = (dist.samples.len() + dist.never) as f64;
    let se = if n > 0.0 {
        (p * (1.0 - p) / n).sqrt()
    } else {
        f64::INFINITY
    };

    let mut warnings = dist.warnings.clone();
    if inputs.threat_mode == ThreatMode::Authenticity && inputs.secrecy_lifetime_years > 0.0 {
        warnings.push(
            "signature asset: the stated secrecy lifetime is not counted, because a forged \
             signature is only useful once the machine exists. If this is a long-lived trust \
             anchor, set the secrecy lifetime to its validity period and mark it Both."
                .into(),
        );
    }
    if inputs.migration_years <= 0.0 {
        warnings.push("migration time is zero or negative; exposure is understated".into());
    }

    MoscaResolution {
        probability_exposed: p,
        probability_standard_error: se,
        deadline_year: deadline,
        median_break_year: dist.median(),
        harvest_now_decrypt_later_year: dist.quantile(0.05),
        already_exposed: p > EXPOSED_THRESHOLD,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::montecarlo::MonteCarloConfig;

    fn dist(years: &[u32], never: usize) -> BreakYearDistribution {
        let mut s = years.to_vec();
        s.sort_unstable();
        BreakYearDistribution {
            samples: s,
            never,
            by_architecture: vec![],
            config: MonteCarloConfig::default(),
            warnings: vec![],
        }
    }

    fn inputs(mig: f64, sec: f64, mode: ThreatMode) -> MoscaInputs {
        MoscaInputs {
            migration_years: mig,
            secrecy_lifetime_years: sec,
            threat_mode: mode,
            reference_year: 2026,
        }
    }

    #[test]
    fn a_long_lived_secret_with_a_slow_migration_is_exposed() {
        let d = dist(&[2035, 2036, 2038, 2040, 2042], 0);
        let r = resolve(&inputs(3.0, 20.0, ThreatMode::Confidentiality), &d);
        assert_eq!(r.deadline_year, 2049.0);
        assert!(r.already_exposed);
        assert_eq!(r.probability_exposed, 1.0);
    }

    #[test]
    fn a_short_lived_secret_with_a_fast_migration_is_not() {
        let d = dist(&[2045, 2050, 2055, 2060, 2065], 0);
        let r = resolve(&inputs(1.0, 1.0, ThreatMode::Confidentiality), &d);
        assert_eq!(r.deadline_year, 2028.0);
        assert!(!r.already_exposed);
        assert_eq!(r.probability_exposed, 0.0);
    }

    #[test]
    fn signature_assets_do_not_carry_their_data_retention_period() {
        // The distinction that changes prioritisation most.
        let d = dist(&[2040, 2041, 2042, 2043, 2044], 0);
        let conf = resolve(&inputs(2.0, 25.0, ThreatMode::Confidentiality), &d);
        let auth = resolve(&inputs(2.0, 25.0, ThreatMode::Authenticity), &d);
        assert!(conf.probability_exposed > auth.probability_exposed);
        assert_eq!(auth.deadline_year, 2028.0);
        assert_eq!(conf.deadline_year, 2053.0);
    }

    #[test]
    fn a_long_lived_trust_anchor_is_flagged_rather_than_silently_discounted() {
        let d = dist(&[2040], 0);
        let r = resolve(&inputs(2.0, 20.0, ThreatMode::Authenticity), &d);
        assert!(r.warnings.iter().any(|w| w.contains("trust anchor")));
    }

    #[test]
    fn harvest_now_decrypt_later_date_uses_the_early_tail() {
        let years: Vec<u32> = (2030..2130).collect();
        let d = dist(&years, 0);
        let r = resolve(&inputs(1.0, 1.0, ThreatMode::Confidentiality), &d);
        let hndl = r.harvest_now_decrypt_later_year.unwrap();
        assert!(
            hndl < r.median_break_year.unwrap(),
            "the HNDL date must come from the early tail, not the median"
        );
    }

    #[test]
    fn never_feasible_samples_reduce_the_exposure_probability() {
        let with_never = resolve(
            &inputs(3.0, 20.0, ThreatMode::Confidentiality),
            &dist(&[2035, 2036], 8),
        );
        let without = resolve(
            &inputs(3.0, 20.0, ThreatMode::Confidentiality),
            &dist(&[2035, 2036], 0),
        );
        assert!(with_never.probability_exposed < without.probability_exposed);
    }

    #[test]
    fn probability_carries_a_standard_error() {
        let d = dist(&[2035, 2040, 2045, 2050], 0);
        let r = resolve(&inputs(3.0, 10.0, ThreatMode::Confidentiality), &d);
        assert!(r.probability_standard_error.is_finite());
        assert!(r.probability_standard_error >= 0.0);
    }
}
