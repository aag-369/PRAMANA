//! Magic state supply.
//!
//! Clifford gates are free under a surface code (tracked in the Pauli frame); every
//! non-Clifford operation must consume a distilled or cultivated magic state. For Shor's
//! algorithm this is where a large fraction of the physical qubits go, so the choice of
//! construction is material rather than a detail.

use crate::model::FactoryReport;
use pramana_units::ErrorRate;

/// A source of magic states.
pub trait MagicStateFactory: Send + Sync {
    /// Name of the construction.
    fn name(&self) -> &'static str;

    /// Physical qubits occupied by one factory at the given code distance.
    fn qubits(&self, distance: u32) -> u64;

    /// Code cycles for one factory to emit one state.
    fn cycles_per_state(&self, distance: u32) -> u64;

    /// Output error per state, given the physical error rate.
    fn output_error(&self, p: ErrorRate) -> f64;

    /// Build a report for a chosen parallel allocation.
    fn report(&self, count: u64, distance: u32, p: ErrorRate) -> FactoryReport {
        FactoryReport {
            construction: self.name(),
            count,
            qubits_each: self.qubits(distance),
            cycles_per_state: self.cycles_per_state(distance),
            output_error: self.output_error(p),
        }
    }
}

/// Bravyi-Kitaev 15-to-1 distillation, two levels, as costed by Litinski.
///
/// One level takes 15 noisy states to one with error `~35 p^3`; two levels are needed to
/// reach the error rates a cryptographic computation requires. Litinski's *A Game of
/// Surface Codes* (arXiv:1808.02892) gives block footprints in units of `d x d` patches;
/// the figures here follow that accounting.
#[derive(Debug, Clone, Copy, Default)]
pub struct FifteenToOneTwoLevel;

/// Error amplification of one 15-to-1 distillation level: `35 p^3`.
pub const DISTILLATION_CUBIC_COEFF: f64 = 35.0;

impl MagicStateFactory for FifteenToOneTwoLevel {
    fn name(&self) -> &'static str {
        "15-to-1 distillation (two levels)"
    }

    fn qubits(&self, distance: u32) -> u64 {
        // A two-level 15-to-1 factory occupies roughly 12 surface-code patches worth of
        // area in Litinski's block accounting.
        let patch = 2 * distance as u64 * distance as u64;
        12 * patch
    }

    fn cycles_per_state(&self, distance: u32) -> u64 {
        // Roughly 6d cycles per output state for a two-level factory.
        6 * distance as u64
    }

    fn output_error(&self, p: ErrorRate) -> f64 {
        let level1 = DISTILLATION_CUBIC_COEFF * p.get().powi(3);
        DISTILLATION_CUBIC_COEFF * level1.powi(3)
    }
}

/// Magic state cultivation (Gidney, Shutty & Jones, arXiv:2409.17595).
///
/// Grows a T state inside a single surface-code patch at roughly the cost of a
/// lattice-surgery CNOT of equivalent reliability, using about an order of magnitude
/// fewer qubit-rounds than distillation. The construction is post-selected, so the retry
/// rate is part of the cost and is modelled explicitly.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cultivation;

/// Expected attempts per accepted cultivated state, reflecting post-selection.
pub const CULTIVATION_RETRY_FACTOR: f64 = 2.0;

impl MagicStateFactory for Cultivation {
    fn name(&self) -> &'static str {
        "magic state cultivation"
    }

    fn qubits(&self, distance: u32) -> u64 {
        // Cultivation fits inside a patch, plus a small escape region.
        let patch = 2 * distance as u64 * distance as u64;
        2 * patch
    }

    fn cycles_per_state(&self, distance: u32) -> u64 {
        ((distance as f64) * CULTIVATION_RETRY_FACTOR).ceil() as u64
    }

    fn output_error(&self, p: ErrorRate) -> f64 {
        // The source reports logical error near 2e-9 at p = 1e-3 and near 4e-11 at
        // p = 5e-4. Interpolating in log-log space between those two anchors gives the
        // steep power-law dependence the construction exhibits.
        let (p1, e1) = (1e-3f64, 2e-9f64);
        let (p2, e2) = (5e-4f64, 4e-11f64);
        let slope = (e2.ln() - e1.ln()) / (p2.ln() - p1.ln());
        (e1.ln() + slope * (p.get().ln() - p1.ln())).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distillation_error_falls_cubically_per_level() {
        let f = FifteenToOneTwoLevel;
        let e = f.output_error(ErrorRate::new(1e-3).unwrap());
        // Level 1: 35e-9. Level 2: 35 * (35e-9)^3 ~ 1.5e-24.
        assert!(e < 1e-20, "two-level output error should be tiny, got {e:.3e}");
    }

    #[test]
    fn cultivation_reproduces_its_published_anchors() {
        let f = Cultivation;
        let a = f.output_error(ErrorRate::new(1e-3).unwrap());
        let b = f.output_error(ErrorRate::new(5e-4).unwrap());
        assert!((a / 2e-9 - 1.0).abs() < 0.01, "expected 2e-9 at p=1e-3, got {a:.3e}");
        assert!((b / 4e-11 - 1.0).abs() < 0.01, "expected 4e-11 at p=5e-4, got {b:.3e}");
    }

    #[test]
    fn cultivation_is_cheaper_in_spacetime_than_distillation() {
        let (c, d) = (Cultivation, FifteenToOneTwoLevel);
        let dist = 27;
        let c_vol = c.qubits(dist) * c.cycles_per_state(dist);
        let d_vol = d.qubits(dist) * d.cycles_per_state(dist);
        assert!(
            c_vol * 5 < d_vol,
            "cultivation should be much cheaper per state: {c_vol} vs {d_vol}"
        );
    }

    #[test]
    fn cultivation_retry_factor_is_charged() {
        assert!(Cultivation.cycles_per_state(27) > 27);
    }
}
