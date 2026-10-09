//! The rotated surface code with lattice surgery.
//!
//! This is the baseline architecture and the one every published cryptographic estimate
//! is stated against. Its structure here is deliberately explicit about the three things
//! that most often go wrong in resource estimates:
//!
//! 1. **The reaction limit.** For Shor-like circuits the classical feedback latency, not
//!    magic state supply, sets the wall clock. A model that counts only magic states
//!    gets the runtime badly wrong.
//! 2. **Routing.** Logical qubits do not tile the plane at one patch each; lattice
//!    surgery needs ancilla corridors. The overhead is modelled as a selectable layout
//!    from Litinski's taxonomy, not as a fudge factor.
//! 3. **Factory sizing.** Over-provisioning factories beyond the reaction limit wastes
//!    qubits, so the allocation is solved rather than assumed.

use crate::magic::MagicStateFactory;
use crate::model::{
    Citation, Connectivity, HardwareParams, LimitingFactor, QecArchitecture, QecEstimate, QecError,
    QecInput, ResourceBreakdown,
};
use pramana_units::{Distance, ErrorRate, PhysicalQubits, QubitRounds, Seconds};

/// Prefactor `A` in `p_L = A (p/p_th)^((d+1)/2)`.
pub const LOGICAL_ERROR_PREFACTOR: f64 = 0.1;

/// Surface code threshold under circuit-level depolarising noise.
pub const SURFACE_CODE_THRESHOLD: f64 = 0.01;

/// Largest code distance the solver will consider before giving up.
pub const MAX_DISTANCE: u32 = 201;

/// Lattice-surgery layout, following Litinski's block taxonomy
/// (*A Game of Surface Codes*, arXiv:1808.02892).
///
/// The layouts trade space against the number of code cycles a logical operation costs.
/// Exposing the choice makes the space-time tradeoff visible rather than baked in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// Compact block: about 1.5 tiles per logical qubit, slowest operations.
    Compact,
    /// Intermediate block: about 2 tiles per logical qubit.
    Intermediate,
    /// Fast block: about 2 tiles plus a routing strip, fastest operations.
    Fast,
}

impl Layout {
    /// Surface-code tiles required to host `n` logical qubits under this layout.
    pub fn tiles(&self, logical: u64) -> u64 {
        match self {
            Layout::Compact => (3 * logical) / 2 + 3,
            Layout::Intermediate => 2 * logical + 4,
            Layout::Fast => 2 * logical + (8.0 * logical as f64).sqrt().ceil() as u64 + 1,
        }
    }

    /// Tiles that are data rather than routing, for the breakdown.
    pub fn data_tiles(&self, logical: u64) -> u64 {
        logical
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Layout::Compact => "compact block",
            Layout::Intermediate => "intermediate block",
            Layout::Fast => "fast block",
        }
    }
}

/// Logical error rate per patch per round.
pub fn logical_error_rate(d: u32, p: ErrorRate) -> f64 {
    let exponent = ((d + 1) / 2) as i32;
    LOGICAL_ERROR_PREFACTOR * (p.get() / SURFACE_CODE_THRESHOLD).powi(exponent)
}

/// Smallest odd distance whose accumulated logical error over `volume` patch-rounds stays
/// within `budget`.
pub fn solve_distance(p: ErrorRate, volume: f64, budget: f64) -> Result<Distance, QecError> {
    if p.get() >= SURFACE_CODE_THRESHOLD {
        return Err(QecError::AboveThreshold {
            p: p.get(),
            threshold: SURFACE_CODE_THRESHOLD,
            code: "rotated surface code",
        });
    }
    let mut d = 3u32;
    while d <= MAX_DISTANCE {
        if logical_error_rate(d, p) * volume <= budget {
            return Ok(Distance::new_odd(d).expect("odd by construction"));
        }
        d += 2;
    }
    Err(QecError::DistanceSearchExhausted { max: MAX_DISTANCE })
}

/// The surface-code architecture.
pub struct SurfaceCode<F: MagicStateFactory> {
    /// Lattice-surgery layout.
    pub layout: Layout,
    /// Magic state source.
    pub factory: F,
}

impl<F: MagicStateFactory> SurfaceCode<F> {
    /// Construct with an explicit layout and factory.
    pub fn new(layout: Layout, factory: F) -> Self {
        Self { layout, factory }
    }
}

impl<F: MagicStateFactory + 'static> QecArchitecture for SurfaceCode<F> {
    fn id(&self) -> &'static str {
        "surface_code"
    }

    fn display_name(&self) -> &'static str {
        "Rotated surface code with lattice surgery"
    }

    fn estimate(&self, input: &QecInput, hw: &HardwareParams) -> Result<QecEstimate, QecError> {
        if hw.connectivity != Connectivity::SquareGrid {
            return Err(QecError::OutOfRegime {
                architecture: self.id(),
                reason: format!(
                    "the surface code model assumes a nearest-neighbour square grid, got {:?}",
                    hw.connectivity
                ),
            });
        }

        // Step 1: the reaction limit. Every non-Clifford layer waits a full classical
        // feedback round, and no amount of hardware shortens that.
        let reaction_time = Seconds::new(input.reaction_depth as f64 * hw.reaction_time.get());

        // Step 2: size the code against the spacetime volume the run will occupy. The
        // volume depends on the runtime and the runtime may depend on the distance
        // through factory throughput, so solve once against the reaction limit and then
        // re-solve if the factories turn out to bind.
        let tiles = self.layout.tiles(input.logical_qubits.get());
        let mut wall = reaction_time;
        let mut distance;
        let mut factories;
        #[allow(unused_assignments)]
        let mut limiting = LimitingFactor::ReactionLimited;

        for _ in 0..3 {
            let cycles = (wall.get() / hw.cycle_time.get()).ceil();
            let volume = tiles as f64 * cycles;
            distance = solve_distance(
                hw.physical_error_rate,
                volume,
                input.target_total_error.get(),
            )?;

            // Step 3: how many factories are needed to keep up without over-provisioning?
            let cycles_per_state = self.factory.cycles_per_state(distance.get()) as f64;
            let states_needed = input.toffoli_count as f64;
            let states_per_factory = cycles / cycles_per_state;
            factories = (states_needed / states_per_factory.max(1.0)).ceil().max(1.0);

            // If even one factory per available slot cannot keep up within the reaction
            // time, the run is factory limited and takes longer.
            let factory_time = Seconds::new(
                states_needed * cycles_per_state / factories * hw.cycle_time.get(),
            );
            let new_wall = reaction_time.max(factory_time);
            limiting = if factory_time.get() > reaction_time.get() {
                LimitingFactor::FactoryLimited
            } else {
                LimitingFactor::ReactionLimited
            };

            if (new_wall.get() - wall.get()).abs() / wall.get().max(1e-9) < 1e-6 {
                // Converged. Assemble the answer.
                let patch = distance.rotated_patch_qubits();
                let data_tiles = self.layout.data_tiles(input.logical_qubits.get());
                let routing_tiles = tiles.saturating_sub(data_tiles);
                let factory_qubits = factories as u64 * self.factory.qubits(distance.get());
                let breakdown = ResourceBreakdown {
                    data: data_tiles * patch,
                    routing: routing_tiles * patch,
                    factories: factory_qubits,
                };
                let total = PhysicalQubits::new(breakdown.total());
                let cycles_u = cycles as u128;
                let mut warnings = Vec::new();
                let achieved = logical_error_rate(distance.get(), hw.physical_error_rate);
                if achieved * (tiles as f64 * cycles) > input.target_total_error.get() {
                    warnings.push("distance solver did not meet the error budget".into());
                }
                return Ok(QecEstimate {
                    physical_qubits: total,
                    wall_clock: wall,
                    spacetime_volume: QubitRounds::new(
                        (total.get() as u128).saturating_mul(cycles_u),
                    ),
                    code_distance: Some(distance),
                    magic_state_factory: self.factory.report(
                        factories as u64,
                        distance.get(),
                        hw.physical_error_rate,
                    ),
                    breakdown,
                    logical_error_achieved: achieved,
                    limiting_factor: limiting,
                    warnings,
                });
            }
            wall = new_wall;
        }

        Err(QecError::OutOfRegime {
            architecture: self.id(),
            reason: "distance and runtime did not converge".into(),
        })
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "arXiv:1208.0928",
                title: "Surface codes: towards practical large-scale quantum computation",
                year: 2012,
            },
            Citation {
                reference: "arXiv:1808.02892",
                title: "A Game of Surface Codes",
                year: 2019,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magic::FifteenToOneTwoLevel;
    use pramana_units::LogicalQubits;

    fn ge19_input() -> QecInput {
        QecInput {
            logical_qubits: LogicalQubits::new(6189),
            toffoli_count: 2_642_000_000,
            reaction_depth: 2_642_000_000,
            target_total_error: ErrorRate::new(0.01).unwrap(),
            idle_fraction: 0.0,
        }
    }

    #[test]
    fn logical_error_falls_with_distance() {
        let p = ErrorRate::new(1e-3).unwrap();
        let mut prev = 1.0;
        for d in [3u32, 5, 7, 11, 21, 31] {
            let e = logical_error_rate(d, p);
            assert!(e < prev, "d={d} did not improve on the previous distance");
            prev = e;
        }
    }

    #[test]
    fn error_rate_at_or_above_threshold_is_rejected() {
        let p = ErrorRate::new(0.02).unwrap();
        let e = solve_distance(p, 1e10, 0.01).unwrap_err();
        assert!(matches!(e, QecError::AboveThreshold { .. }));
    }

    #[test]
    fn distance_is_monotone_in_the_error_budget() {
        // Law 7: a tighter budget must never select a smaller distance.
        let p = ErrorRate::new(1e-3).unwrap();
        let loose = solve_distance(p, 1e14, 1e-1).unwrap().get();
        let tight = solve_distance(p, 1e14, 1e-6).unwrap().get();
        assert!(tight >= loose, "tighter budget gave smaller distance");
    }

    #[test]
    fn distance_is_monotone_in_the_physical_error_rate() {
        let better = solve_distance(ErrorRate::new(1e-4).unwrap(), 1e14, 0.01)
            .unwrap()
            .get();
        let worse = solve_distance(ErrorRate::new(1e-3).unwrap(), 1e14, 0.01)
            .unwrap()
            .get();
        assert!(better <= worse, "better hardware should not need a larger code");
    }

    #[test]
    fn shor_is_reaction_limited_not_factory_limited() {
        // The central qualitative claim about Shor under the surface code.
        let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let est = arch
            .estimate(&ge19_input(), &HardwareParams::gidney_superconducting())
            .unwrap();
        assert_eq!(est.limiting_factor, LimitingFactor::ReactionLimited);
    }

    #[test]
    fn reproduces_gidney_ekera_runtime() {
        // Published: about 8 hours.
        let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let est = arch
            .estimate(&ge19_input(), &HardwareParams::gidney_superconducting())
            .unwrap();
        let hours = est.wall_clock.as_hours();
        assert!(
            hours > 5.0 && hours < 12.0,
            "expected roughly 8 hours, got {hours:.2}"
        );
    }

    #[test]
    fn reproduces_gidney_ekera_physical_qubit_count() {
        // Published: about 20 million physical qubits.
        let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let est = arch
            .estimate(&ge19_input(), &HardwareParams::gidney_superconducting())
            .unwrap();
        let q = est.physical_qubits.get() as f64;
        let ratio = q / 20e6;
        assert!(
            ratio > 0.6 && ratio < 1.6,
            "expected ~20M physical qubits, got {q:.4e} (ratio {ratio:.3})"
        );
    }

    #[test]
    fn distance_lands_in_the_published_range() {
        let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let est = arch
            .estimate(&ge19_input(), &HardwareParams::gidney_superconducting())
            .unwrap();
        let d = est.code_distance.unwrap().get();
        assert!((25..=35).contains(&d), "expected d in the high twenties, got {d}");
    }

    #[test]
    fn layout_choice_trades_space_measurably() {
        let hw = HardwareParams::gidney_superconducting();
        let compact = SurfaceCode::new(Layout::Compact, FifteenToOneTwoLevel)
            .estimate(&ge19_input(), &hw)
            .unwrap();
        let fast = SurfaceCode::new(Layout::Fast, FifteenToOneTwoLevel)
            .estimate(&ge19_input(), &hw)
            .unwrap();
        assert!(
            compact.physical_qubits.get() < fast.physical_qubits.get(),
            "the compact block must use less space than the fast block"
        );
    }

    #[test]
    fn breakdown_accounts_for_every_qubit() {
        let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let est = arch
            .estimate(&ge19_input(), &HardwareParams::gidney_superconducting())
            .unwrap();
        assert_eq!(est.breakdown.total(), est.physical_qubits.get());
        assert!(est.breakdown.data > 0 && est.breakdown.routing > 0);
    }

    #[test]
    fn non_square_grid_connectivity_is_refused_rather_than_approximated() {
        let arch = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let mut hw = HardwareParams::gidney_superconducting();
        hw.connectivity = Connectivity::Reconfigurable;
        assert!(matches!(
            arch.estimate(&ge19_input(), &hw),
            Err(QecError::OutOfRegime { .. })
        ));
    }
}
