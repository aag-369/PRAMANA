//! The three-region layout of Gidney 2025 (arXiv:2505.15917, section 3.2).
//!
//! The machine is split into three regions:
//!
//! - **cold storage** holds the large, idle input register under yoked surface codes,
//!   at roughly triple the density of ordinary patches;
//! - **hot storage** holds the small active working set as ordinary distance-`d` patches
//!   at `2(d+1)^2` physical qubits each;
//! - **compute** hosts magic state factories and lattice-surgery workspace.
//!
//! Because the residue-arithmetic circuit puts about 90% of its logical qubits in the
//! idle input register, cold storage dominates, and the density gain there is what brings
//! the total under a million.
//!
//! # The distance question
//!
//! This architecture is unusually sensitive to the surface-code logical error curve. The
//! source selects `d = 25` from its own simulated data (its Figure 6); a generic analytic
//! fit selects `d = 27` for the same target. Two distance steps move the total by about
//! 15%, which straddles the million-qubit line. PRAMANA therefore makes the error curve an
//! explicit, cited choice rather than a hardcoded formula, and reports which was used.

use crate::model::{
    Citation, FactoryReport, HardwareParams, LimitingFactor, QecArchitecture, QecEstimate, QecError,
    QecInput, ResourceBreakdown,
};
use pramana_units::{Distance, ErrorRate, PhysicalQubits, QubitRounds, Seconds};

/// How the per-patch per-round logical error rate is modelled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicalErrorModel {
    /// Generic analytic fit `A (p/p_th)^((d+1)/2)`.
    ///
    /// Conservative and architecture-agnostic. This is PRAMANA's default because it does
    /// not depend on any one paper's simulation campaign.
    AnalyticFit {
        /// Prefactor `A`.
        prefactor: f64,
        /// Threshold `p_th`.
        threshold: f64,
    },
    /// Calibrated to the simulated curve of arXiv:2505.15917 Figure 6, which reports that
    /// a distance-25 patch reaches `1e-15` per round at a physical error rate of `1e-3`.
    ///
    /// Using a source's own noise model when reproducing that source is the same
    /// principle as charging Cuccaro's adder at `2n` when reproducing Gidney-Ekera 2019.
    Gidney2025Simulated,
}

impl LogicalErrorModel {
    /// Logical error per patch per round.
    pub fn rate(&self, d: u32, p: ErrorRate) -> f64 {
        match self {
            LogicalErrorModel::AnalyticFit {
                prefactor,
                threshold,
            } => prefactor * (p.get() / threshold).powi(((d + 1) / 2) as i32),
            LogicalErrorModel::Gidney2025Simulated => {
                // Anchored at (d=25, p=1e-3) -> 1e-15, with the standard one-decade
                // improvement per two units of distance.
                let anchor_d = 25.0;
                let scale = (p.get() / 1e-3).log10();
                1e-15 * 10f64.powf(-(d as f64 - anchor_d) / 2.0 + scale * ((d + 1) / 2) as f64)
            }
        }
    }

    /// Name for reporting.
    pub fn name(&self) -> &'static str {
        match self {
            LogicalErrorModel::AnalyticFit { .. } => "generic analytic fit",
            LogicalErrorModel::Gidney2025Simulated => "arXiv:2505.15917 Fig. 6 simulated curve",
        }
    }
}

impl Default for LogicalErrorModel {
    fn default() -> Self {
        LogicalErrorModel::AnalyticFit {
            prefactor: 0.1,
            threshold: 0.01,
        }
    }
}

/// Density gain of yoked cold storage over ordinary hot patches.
///
/// The source: cold logical qubits are "roughly triple the density of hot logical qubits"
/// (1352 physical per hot logical against 430 per cold logical).
pub const YOKE_DENSITY_GAIN: f64 = 1352.0 / 430.0;

/// Hot patches occupied by one magic state factory.
///
/// The source describes each factory covering a 3x4 area of hot patches.
pub const PATCHES_PER_FACTORY: u64 = 12;

/// Routing multiplier on the compute region.
///
/// The source's compute region is a 7x18 block of hot patches (126) hosting six 3x4
/// factories (72) plus routing channels, i.e. 126/72.
pub const COMPUTE_ROUTING_FACTOR: f64 = 126.0 / 72.0;

/// Target logical error per patch per round, as chosen by the source.
pub const TARGET_LOGICAL_ERROR_PER_ROUND: f64 = 1e-15;

/// Magic state factories assumed available in the compute region.
pub const FACTORY_COUNT: u64 = 6;

/// Toffoli gates one factory delivers per code cycle, from cultivation plus 8T-to-CCZ.
///
/// Calibrated so that the source's stated six factories deliver its stated 12-hour shot
/// for its stated Toffoli count; recorded as a derived quantity rather than a free
/// parameter, and reported in the estimate.
pub const TOFFOLI_CYCLES_PER_FACTORY: f64 = 366.0;

/// The Gidney 2025 three-region architecture.
pub struct Gidney2025Layout {
    /// Which logical error curve to use.
    pub error_model: LogicalErrorModel,
}

impl Gidney2025Layout {
    /// Construct with an explicit error model.
    pub fn new(error_model: LogicalErrorModel) -> Self {
        Self { error_model }
    }

    /// Smallest odd distance reaching the per-round target.
    pub fn solve_distance(&self, p: ErrorRate) -> Result<Distance, QecError> {
        let mut d = 3u32;
        while d <= 201 {
            if self.error_model.rate(d, p) <= TARGET_LOGICAL_ERROR_PER_ROUND {
                return Ok(Distance::new_odd(d).expect("odd"));
            }
            d += 2;
        }
        Err(QecError::DistanceSearchExhausted { max: 201 })
    }
}

impl QecArchitecture for Gidney2025Layout {
    fn id(&self) -> &'static str {
        "gidney_2025_three_region"
    }

    fn display_name(&self) -> &'static str {
        "Gidney 2025 three-region layout (hot, yoked cold, compute)"
    }

    fn estimate(&self, input: &QecInput, hw: &HardwareParams) -> Result<QecEstimate, QecError> {
        if !(0.0..=1.0).contains(&input.idle_fraction) {
            return Err(QecError::OutOfRegime {
                architecture: self.id(),
                reason: format!("cold fraction {} outside [0, 1]", input.idle_fraction),
            });
        }
        let d = self.solve_distance(hw.physical_error_rate)?;

        let hot_density = 2 * (d.get() as u64 + 1) * (d.get() as u64 + 1);
        let cold_density = (hot_density as f64 / YOKE_DENSITY_GAIN).round() as u64;

        let total_logical = input.logical_qubits.get();
        let cold_logical = (total_logical as f64 * input.idle_fraction).round() as u64;
        let hot_logical = total_logical.saturating_sub(cold_logical);

        let compute_patches =
            ((FACTORY_COUNT * PATCHES_PER_FACTORY) as f64 * COMPUTE_ROUTING_FACTOR).round() as u64;

        let breakdown = ResourceBreakdown {
            data: cold_logical * cold_density,
            routing: hot_logical * hot_density,
            factories: compute_patches * hot_density,
        };
        let total = PhysicalQubits::new(breakdown.total());

        // Runtime: the compute region has a fixed number of factories, so the run is
        // factory limited rather than reaction limited, which is why this construction is
        // slower than the 2019 one despite needing far fewer qubits.
        let factory_time = Seconds::new(
            input.toffoli_count as f64 * TOFFOLI_CYCLES_PER_FACTORY / FACTORY_COUNT as f64
                * hw.cycle_time.get(),
        );
        let reaction_time =
            Seconds::new(input.reaction_depth as f64 * hw.reaction_time.get());
        let wall = factory_time.max(reaction_time);
        let limiting = if factory_time.get() >= reaction_time.get() {
            LimitingFactor::FactoryLimited
        } else {
            LimitingFactor::ReactionLimited
        };

        let cycles = (wall.get() / hw.cycle_time.get()).ceil();
        let achieved = self.error_model.rate(d.get(), hw.physical_error_rate);

        Ok(QecEstimate {
            physical_qubits: total,
            wall_clock: wall,
            spacetime_volume: QubitRounds::new(
                (total.get() as u128).saturating_mul(cycles as u128),
            ),
            code_distance: Some(d),
            magic_state_factory: FactoryReport {
                construction: "cultivation + 8T-to-CCZ distillation",
                count: FACTORY_COUNT,
                qubits_each: PATCHES_PER_FACTORY * hot_density,
                cycles_per_state: TOFFOLI_CYCLES_PER_FACTORY as u64,
                output_error: achieved,
            },
            breakdown,
            logical_error_achieved: achieved,
            limiting_factor: limiting,
            warnings: vec![format!(
                "distance {} selected under the {}; cold storage at {} physical per logical, \
                 hot at {}",
                d.get(),
                self.error_model.name(),
                cold_density,
                hot_density
            )],
        })
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "arXiv:2505.15917",
                title: "How to factor 2048 bit RSA integers with less than a million noisy qubits",
                year: 2025,
            },
            Citation {
                reference: "arXiv:2312.04522",
                title: "Yoked surface codes",
                year: 2023,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pramana_units::LogicalQubits;

    /// The n=2048 residue circuit as PRAMANA synthesises it.
    fn rsa2048() -> QecInput {
        QecInput {
            logical_qubits: LogicalQubits::new(1432),
            toffoli_count: 707_000_000,
            reaction_depth: 707_000_000,
            target_total_error: ErrorRate::new(0.07).unwrap(),
            // m = 1280 of 1432 logical qubits are the idle input register.
            idle_fraction: 1280.0 / 1432.0,
        }
    }

    #[test]
    fn source_calibration_selects_the_published_distance() {
        let arch = Gidney2025Layout::new(LogicalErrorModel::Gidney2025Simulated);
        let d = arch.solve_distance(ErrorRate::new(1e-3).unwrap()).unwrap();
        assert_eq!(d.get(), 25, "the source's own curve gives d=25");
    }

    #[test]
    fn generic_fit_is_more_conservative_than_the_source_curve() {
        let generic = Gidney2025Layout::new(LogicalErrorModel::default())
            .solve_distance(ErrorRate::new(1e-3).unwrap())
            .unwrap();
        let source = Gidney2025Layout::new(LogicalErrorModel::Gidney2025Simulated)
            .solve_distance(ErrorRate::new(1e-3).unwrap())
            .unwrap();
        assert!(
            generic.get() > source.get(),
            "generic fit {} should be at least as conservative as source {}",
            generic.get(),
            source.get()
        );
    }

    #[test]
    fn reaches_under_a_million_qubits_on_the_source_calibration() {
        let arch = Gidney2025Layout::new(LogicalErrorModel::Gidney2025Simulated);
        let e = arch
            .estimate(&rsa2048(), &HardwareParams::gidney_superconducting())
            .unwrap();
        let q = e.physical_qubits.get();
        assert!(q < 1_000_000, "expected under a million, got {q}");
        assert!(q > 700_000, "and not implausibly far under, got {q}");
    }

    #[test]
    fn cold_storage_dominates_the_machine() {
        let arch = Gidney2025Layout::new(LogicalErrorModel::Gidney2025Simulated);
        let e = arch
            .estimate(&rsa2048(), &HardwareParams::gidney_superconducting())
            .unwrap();
        let frac = e.breakdown.data as f64 / e.physical_qubits.get() as f64;
        assert!(
            frac > 0.5,
            "the idle input register should dominate, got {frac:.2}"
        );
    }

    #[test]
    fn shot_runtime_is_about_twelve_hours() {
        let arch = Gidney2025Layout::new(LogicalErrorModel::Gidney2025Simulated);
        let e = arch
            .estimate(&rsa2048(), &HardwareParams::gidney_superconducting())
            .unwrap();
        let h = e.wall_clock.as_hours();
        assert!(h > 8.0 && h < 18.0, "expected roughly 12 hours, got {h:.2}");
    }

    #[test]
    fn is_factory_limited_unlike_the_2019_construction() {
        // The source attributes its longer runtime to more Toffolis and fewer factories.
        let arch = Gidney2025Layout::new(LogicalErrorModel::Gidney2025Simulated);
        let e = arch
            .estimate(&rsa2048(), &HardwareParams::gidney_superconducting())
            .unwrap();
        assert_eq!(e.limiting_factor, LimitingFactor::FactoryLimited);
    }

    #[test]
    fn generic_fit_pushes_the_machine_over_a_million() {
        // The headline claim is sensitive to two units of code distance. Worth pinning,
        // because it tells a reader exactly how much of the result rests on the noise
        // model rather than on the algorithm.
        let arch = Gidney2025Layout::new(LogicalErrorModel::default());
        let e = arch
            .estimate(&rsa2048(), &HardwareParams::gidney_superconducting())
            .unwrap();
        assert!(
            e.physical_qubits.get() > 1_000_000,
            "the conservative curve should exceed a million, got {}",
            e.physical_qubits.get()
        );
    }
}
