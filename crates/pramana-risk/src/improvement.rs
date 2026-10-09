//! Algorithmic improvement over time.
//!
//! A model that assumes today's attack circuits are final is systematically wrong in the
//! unsafe direction: it makes the attacker look further away than they are. The build spec
//! required this term as a design judgement. It is now a measurement.
//!
//! # The data
//!
//! **ECDLP over a 256-bit curve**, Toffoli count:
//!
//! | Year | Source | Toffoli |
//! |---|---|---|
//! | 2017 | Roetteler et al. | 1.29e11 |
//! | 2023 | Litinski | 2.0e8 |
//! | 2026 | Schrottenloher | 5.6e7 |
//!
//! **RSA-2048**, Toffoli count: 2.6e9 in 2019 (Gidney-Ekera) to 6.5e9 in 2025 (Gidney) —
//! which *rose*, because that line of work spent its progress on width instead, taking
//! logical qubits from 6,189 to 1,399.
//!
//! # Two consequences the model must encode
//!
//! **The rate is not uniform across problems.** ECC improved by roughly 2,300x in nine
//! years while RSA's gate count barely moved. Fitting one global rate would be wrong. The
//! causal reason is documented: factoring absorbed two decades of optimisation before the
//! curves received comparable attention, so ECC had far more slack to give up.
//!
//! **The rate must decelerate toward a floor.** An elliptic-curve point is two coordinates
//! over an `n`-bit field, so no construction in the standard representation holds fewer
//! than `2n` data qubits, and the gate count cannot fall below a cubic envelope. An
//! unbounded exponential prior eventually predicts sub-floor costs, which is unphysical.
//! This module therefore decays toward a floor rather than toward zero.

use serde::{Deserialize, Serialize};

/// The published resource-estimate series, compiled in from `data/improvement_series.toml`.
///
/// Embedded at build time so the fitted rates need no filesystem access at runtime, while
/// the observations themselves stay declared data with provenance rather than constants
/// buried in code (Law 1).
const SERIES_TOML: &str = include_str!("../../../data/improvement_series.toml");

/// One observation in a published series.
#[derive(Debug, Clone, Deserialize)]
struct SeriesPoint {
    year: u32,
    value: f64,
    #[allow(dead_code)]
    source: String,
}

/// A published series for one family and metric.
#[derive(Debug, Clone, Deserialize)]
struct Series {
    family: String,
    metric: String,
    #[serde(default)]
    #[allow(dead_code)]
    note: Option<String>,
    #[serde(rename = "point")]
    points: Vec<SeriesPoint>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeriesFile {
    #[serde(rename = "series")]
    series: Vec<Series>,
}

/// Fit a rate from the first and last observation of a named series.
///
/// Returns the rate, the reference year (the latest observation) and a provenance string.
fn fit_from_series(family: &str, metric: &str) -> Option<(f64, u32, String)> {
    let file: SeriesFile = toml::from_str(SERIES_TOML).ok()?;
    let s = file
        .series
        .into_iter()
        .find(|s| s.family == family && s.metric == metric)?;
    if s.points.len() < 2 {
        return None;
    }
    let first = s.points.first()?;
    let last = s.points.last()?;
    let rate = ImprovementModel::fit_rate(first.value, first.year, last.value, last.year)
        .unwrap_or(0.0)
        .max(0.0);
    let basis = format!(
        "{} {} from {:.3e} in {} to {:.3e} in {} ({} observations)",
        family,
        metric,
        first.value,
        first.year,
        last.value,
        last.year,
        s.points.len()
    );
    Some((rate, last.year, basis))
}

/// Which problem family an improvement rate applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemFamily {
    /// Integer factoring.
    Factoring,
    /// Elliptic-curve discrete logarithm.
    Ecdlp,
    /// Finite-field discrete logarithm.
    FiniteFieldDlp,
    /// Symmetric key search.
    Symmetric,
}

/// A bounded exponential improvement model for one quantity.
///
/// `value(t) = floor + (value_now - floor) * exp(-rate * (t - now))`
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImprovementModel {
    /// Family this applies to.
    pub family: ProblemFamily,
    /// Continuous annual improvement rate.
    pub rate_per_year: f64,
    /// Uncertainty on the rate, as a standard deviation.
    pub rate_sd: f64,
    /// Reference year for `value_now`.
    pub reference_year: u32,
    /// Provenance of the fitted rate.
    pub basis: &'static str,
}

impl ImprovementModel {
    /// Fit a rate from two observations of the same quantity, `value_a` in `year_a` and
    /// `value_b` in `year_b`, ignoring the floor.
    ///
    /// Used to derive the published rates below; exposed so the fit can be re-run when new
    /// results land, rather than the constants being frozen.
    pub fn fit_rate(value_a: f64, year_a: u32, value_b: f64, year_b: u32) -> Option<f64> {
        if year_b <= year_a || value_a <= 0.0 || value_b <= 0.0 {
            return None;
        }
        let years = (year_b - year_a) as f64;
        Some((value_a / value_b).ln() / years)
    }

    /// Gate-count improvement for elliptic-curve attacks, fitted from the published series.
    ///
    /// The rate is extraordinarily fast, reflecting a field catching up after two decades
    /// in which factoring absorbed most of the optimisation effort, so the uncertainty is
    /// set wide: a catch-up rate should not be expected to persist.
    pub fn ecdlp_gates() -> Self {
        Self::from_series(ProblemFamily::Ecdlp, "ecdlp", "toffoli", 0.86, 2026)
    }

    /// Gate-count improvement for factoring, fitted from the published series.
    ///
    /// The series *rises*: 2.6e9 Toffoli in 2019 against 6.5e9 in 2025, because that line
    /// of work spent its progress reducing width instead. The fitted rate is therefore
    /// clamped at zero and the improvement shows up in [`ImprovementModel::factoring_width`].
    pub fn factoring_gates() -> Self {
        Self::from_series(ProblemFamily::Factoring, "factoring", "toffoli", 0.0, 2025)
    }

    /// Width improvement for factoring, fitted from the published series.
    pub fn factoring_width() -> Self {
        Self::from_series(ProblemFamily::Factoring, "factoring", "logical_qubits", 0.25, 2025)
    }

    /// Width improvement for elliptic-curve attacks, fitted from the published series.
    pub fn ecdlp_width() -> Self {
        Self::from_series(ProblemFamily::Ecdlp, "ecdlp", "logical_qubits", 0.11, 2026)
    }

    /// Build a model by fitting the named series, falling back if the data is unreadable.
    fn from_series(
        family: ProblemFamily,
        series_family: &str,
        metric: &str,
        fallback_rate: f64,
        fallback_year: u32,
    ) -> Self {
        match fit_from_series(series_family, metric) {
            Some((rate, year, basis)) => Self {
                family,
                rate_per_year: rate,
                rate_sd: (rate * 0.5).max(0.02),
                reference_year: year,
                basis: Box::leak(basis.into_boxed_str()),
            },
            None => Self {
                family,
                rate_per_year: fallback_rate,
                rate_sd: (fallback_rate * 0.5).max(0.02),
                reference_year: fallback_year,
                basis: "series data unavailable; using a documented fallback rate",
            },
        }
    }

    /// Symmetric primitives, where no comparable improvement series exists.
    pub fn symmetric() -> Self {
        Self {
            family: ProblemFamily::Symmetric,
            rate_per_year: 0.0,
            rate_sd: 0.02,
            reference_year: 2026,
            basis: "no published improvement series; Grover's quadratic speedup is optimal",
        }
    }

    /// Apply the model, decaying `value_now` toward `floor` by `year`.
    ///
    /// Never returns below the floor, and never improves backwards in time.
    pub fn apply(&self, value_now: f64, floor: f64, year: u32, rate: f64) -> f64 {
        let floor = floor.max(0.0).min(value_now);
        if year <= self.reference_year {
            return value_now;
        }
        let dt = (year - self.reference_year) as f64;
        floor + (value_now - floor) * (-rate * dt).exp()
    }
}

/// Leading coefficient of the elliptic-curve gate floor, in `c * n^3 / log2 n`.
///
/// # Why this is not taken from the width-record paper
///
/// The 2026 elliptic-curve results occupy different points on a Pareto frontier, and their
/// formulas are not comparable:
///
/// - Schrottenloher 2026 is **gate-optimised**: ~1,446 logical qubits and ~5.6e7 Toffoli
///   for a 256-bit curve, i.e. about `27 n^3 / log2 n`.
/// - Luo et al. 2026 is **width-optimised**: 835 logical qubits, but `919 n^3 / log2 n`,
///   roughly thirty times more gates. The EUROCRYPT 2026 width-minimised construction pays
///   about three orders of magnitude.
///
/// Deriving a *gate* floor from a *width-optimised* paper's constant produces a floor above
/// the best achieved gate count, which is incoherent — a floor cannot sit above something
/// already built. An earlier version of this module made exactly that mistake and the
/// monotonicity tests caught it.
///
/// The floor is therefore anchored on the gate-optimised operating point and set at a
/// quarter of it, reflecting that the remaining gains there are constant-factor refinements
/// of a few percent that compound slowly.
pub const ECDLP_GATE_FLOOR_COEFF: f64 = 27.0 / 4.0;

/// Physical floors below which no construction can go.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Floors {
    /// Minimum logical qubits for the data register.
    pub logical_qubits: f64,
    /// Minimum Toffoli count.
    pub toffoli: f64,
    /// Why these values.
    pub basis: &'static str,
}

impl Floors {
    /// Floors for an elliptic-curve attack on an `n`-bit curve.
    ///
    /// Width floor `2n`: a point is two coordinates over an `n`-bit field (Proos-Zalka
    /// 2003). A sub-`2n` regime would need an x-only double-scalar point addition, which is
    /// an open problem, so `2n` is used as the hard floor.
    ///
    /// Gate floor: `n` point additions, each needing several modular multiplications whose
    /// cost grows with `n`, gives a cubic envelope no rearrangement escapes. The constant
    /// is taken from the best published construction so the floor cannot sit above the
    /// current state of the art.
    pub fn ecdlp(n: u32) -> Self {
        let nf = n as f64;
        Self {
            logical_qubits: 2.0 * nf,
            toffoli: ECDLP_GATE_FLOOR_COEFF * nf.powi(3) / nf.log2(),
            basis: "Proos-Zalka 2n data register; cubic gate envelope at a quarter of the \
                    best gate-optimised published constant",
        }
    }

    /// Floors for factoring an `n`-bit modulus.
    ///
    /// The width floor is softer than the elliptic-curve case: residue arithmetic already
    /// runs below `n` at roughly `0.5n`, so the floor is placed at `0.25n` to leave room
    /// for further compression without permitting an absurd result.
    pub fn factoring(n: u32) -> Self {
        let nf = n as f64;
        Self {
            logical_qubits: 0.25 * nf,
            toffoli: 0.1 * nf.powi(3),
            basis: "residue arithmetic already at ~0.5n; cubic gate envelope",
        }
    }

    /// No meaningful floor, for families without one.
    pub fn none() -> Self {
        Self {
            logical_qubits: 1.0,
            toffoli: 1.0,
            basis: "no established floor",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecdlp_gate_rate_matches_the_published_descent() {
        let m = ImprovementModel::ecdlp_gates();
        // 1.29e11 -> 5.6e7 over nine years is about 2.4x per year.
        let annual = m.rate_per_year.exp();
        assert!(
            annual > 2.0 && annual < 3.0,
            "expected ~2.4x per year, got {annual:.2}"
        );
    }

    #[test]
    fn factoring_gate_rate_is_flat_because_the_count_rose() {
        let m = ImprovementModel::factoring_gates();
        assert_eq!(
            m.rate_per_year, 0.0,
            "the factoring gate series rises, so the fitted rate must clamp at zero"
        );
        assert!(m.basis.contains("factoring toffoli"));
    }

    #[test]
    fn factoring_progress_shows_up_in_width_not_gates() {
        let w = ImprovementModel::factoring_width();
        let g = ImprovementModel::factoring_gates();
        assert!(
            w.rate_per_year > g.rate_per_year,
            "the 2019-2025 factoring line spent its progress on width"
        );
    }

    #[test]
    fn rates_differ_by_family() {
        // The central claim: one global improvement rate would be wrong.
        let ecc = ImprovementModel::ecdlp_gates().rate_per_year;
        let rsa = ImprovementModel::factoring_gates().rate_per_year;
        assert!(ecc > rsa + 0.5, "ECC gate progress far outran factoring's");
    }

    #[test]
    fn improvement_never_falls_below_the_floor() {
        let m = ImprovementModel::ecdlp_gates();
        let floor = Floors::ecdlp(256);
        for year in [2030u32, 2050, 2100, 2200] {
            let v = m.apply(5.6e7, floor.toffoli, year, m.rate_per_year);
            assert!(
                v >= floor.toffoli - 1e-6,
                "year {year}: {v:.3e} fell below the floor {:.3e}",
                floor.toffoli
            );
        }
    }

    #[test]
    fn improvement_does_not_run_backwards() {
        let m = ImprovementModel::ecdlp_gates();
        let v = m.apply(5.6e7, 1e6, 2020, m.rate_per_year);
        assert_eq!(v, 5.6e7, "years before the reference must not improve");
    }

    #[test]
    fn improvement_is_monotone_in_year() {
        let m = ImprovementModel::ecdlp_gates();
        let floor = Floors::ecdlp(256).toffoli;
        let mut prev = f64::INFINITY;
        for year in 2026..2060 {
            let v = m.apply(5.6e7, floor, year, m.rate_per_year);
            assert!(v <= prev + 1e-9, "cost increased at {year}");
            prev = v;
        }
    }

    #[test]
    fn ecdlp_width_floor_is_two_n() {
        let f = Floors::ecdlp(256);
        assert_eq!(f.logical_qubits, 512.0);
        // The current record is 835, above the floor as it must be.
        assert!(835.0 > f.logical_qubits);
    }

    #[test]
    fn gate_floor_sits_below_the_current_state_of_the_art() {
        // A floor above something already built is incoherent. Regression guard: this
        // failed when the floor was derived from the width-optimised paper's constant.
        let f = Floors::ecdlp(256);
        assert!(
            f.toffoli < 5.6e7,
            "floor {:.3e} must not exceed the best published gate-optimised count 5.6e7",
            f.toffoli
        );
        assert!(f.toffoli > 5.6e6, "and should not be absurdly far below it either");
    }

    #[test]
    fn pareto_points_are_not_conflated() {
        // The width record (835 qubits, ~919 n^3/log n gates) and the gate record
        // (~1446 qubits, ~27 n^3/log n gates) are different operating points. The floors
        // must be anchored on the optimising direction they describe.
        let nf = 256f64;
        let width_optimised_gates = 919.0 * nf.powi(3) / nf.log2();
        let gate_optimised_gates = 27.0 * nf.powi(3) / nf.log2();
        assert!(
            width_optimised_gates > gate_optimised_gates * 10.0,
            "the width record pays heavily in gates; the two are not interchangeable"
        );
        assert!(Floors::ecdlp(256).toffoli < gate_optimised_gates);
    }

    #[test]
    fn fit_rate_rejects_nonsense_input() {
        assert!(ImprovementModel::fit_rate(1.0, 2020, 1.0, 2020).is_none());
        assert!(ImprovementModel::fit_rate(1.0, 2026, 1.0, 2020).is_none());
        assert!(ImprovementModel::fit_rate(0.0, 2020, 1.0, 2026).is_none());
    }
}
