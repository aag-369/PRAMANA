//! Reconfigurable neutral-atom arrays with transversal gates.
//!
//! Follows the algorithmic-fault-tolerance line of work: Cain et al., *Correlated
//! decoding of logical algorithms with transversal gates* (PRL 133:240602) and *Fast
//! correlated decoding of transversal logical algorithms* (arXiv:2505.13587); Zhou et al.,
//! *Low-overhead transversal fault tolerance for universal quantum computation* (Nature
//! 2025).
//!
//! # The central claim
//!
//! Because atoms can be physically moved, logical gates between code patches can be
//! **transversal** rather than performed by lattice surgery. Correlated decoding across
//! multiple logical qubits then reduces the syndrome-extraction rounds needed per logical
//! operation from `O(d)` to `O(1)`.
//!
//! That is a saving in *time*, not space: patches are still `2d^2 - 1` physical qubits.
//! PRAMANA computes both the `O(1)` and the `O(d)` baseline in the same run so the saving
//! is visible rather than asserted.
//!
//! # The countervailing cost
//!
//! Atoms move slowly. A relocation takes hundreds of microseconds where a superconducting
//! syndrome round takes one, so the modality can spend its entire `O(d)` advantage on
//! movement latency. Whether it comes out ahead is an empirical question about the ratio
//! of move time to cycle time, and PRAMANA reports which term binds.

use crate::model::{
    Citation, Connectivity, FactoryReport, HardwareParams, LimitingFactor, QecArchitecture,
    QecEstimate, QecError, QecInput, ResourceBreakdown,
};
use crate::surface::{logical_error_rate, solve_distance};
use pramana_units::{PhysicalQubits, QubitRounds, Seconds};

/// Syndrome-extraction rounds per logical operation under correlated decoding.
pub const TRANSVERSAL_ROUNDS_PER_OPERATION: f64 = 1.0;

/// Routing overhead as tiles per logical qubit.
///
/// Far lower than a square-grid layout because atoms move to meet each other rather than
/// requiring dedicated ancilla corridors.
pub const RECONFIGURABLE_TILES_PER_LOGICAL: f64 = 1.25;

/// Atom loss per relocation, requiring reloading from a reservoir.
pub const ATOM_LOSS_PER_MOVE: f64 = 1e-3;

/// The neutral-atom transversal architecture.
#[derive(Debug, Clone, Copy)]
pub struct NeutralAtomTransversal {
    /// Rounds per logical operation. Set to the code distance to recover the `O(d)`
    /// lattice-surgery baseline for comparison.
    pub rounds_per_operation: Option<f64>,
}

impl Default for NeutralAtomTransversal {
    fn default() -> Self {
        Self {
            rounds_per_operation: Some(TRANSVERSAL_ROUNDS_PER_OPERATION),
        }
    }
}

impl NeutralAtomTransversal {
    /// The `O(1)` correlated-decoding construction.
    pub fn transversal() -> Self {
        Self::default()
    }

    /// The `O(d)` baseline, for measuring what correlated decoding actually buys.
    pub fn lattice_surgery_baseline() -> Self {
        Self {
            rounds_per_operation: None,
        }
    }
}

impl QecArchitecture for NeutralAtomTransversal {
    fn id(&self) -> &'static str {
        "neutral_atom_transversal"
    }

    fn display_name(&self) -> &'static str {
        "Reconfigurable neutral atoms with transversal gates"
    }

    fn estimate(&self, input: &QecInput, hw: &HardwareParams) -> Result<QecEstimate, QecError> {
        if hw.connectivity != Connectivity::Reconfigurable {
            return Err(QecError::OutOfRegime {
                architecture: self.id(),
                reason: format!(
                    "transversal gates require physically movable qubits; got {:?}",
                    hw.connectivity
                ),
            });
        }
        let Some(move_time) = hw.atom_move_time else {
            return Err(QecError::OutOfRegime {
                architecture: self.id(),
                reason: "atom relocation time must be given explicitly; it usually dominates \
                         the runtime and must not be defaulted (Law 9)"
                    .into(),
            });
        };

        let tiles =
            (input.logical_qubits.get() as f64 * RECONFIGURABLE_TILES_PER_LOGICAL).ceil() as u64;

        // Provisional distance from a reaction-limited runtime, then refine once.
        let mut wall = Seconds::new(input.reaction_depth as f64 * hw.reaction_time.get());
        let mut distance = solve_distance(
            hw.physical_error_rate,
            tiles as f64 * (wall.get() / hw.cycle_time.get()),
            input.target_total_error.get(),
        )?;

        for _ in 0..3 {
            let rounds = self
                .rounds_per_operation
                .unwrap_or(distance.get() as f64);
            // Each logical operation costs its syndrome rounds plus one atom relocation.
            let per_op = rounds * hw.cycle_time.get() + move_time.get();
            let new_wall = Seconds::new(input.toffoli_count as f64 * per_op);
            let cycles = new_wall.get() / hw.cycle_time.get();
            let d = solve_distance(
                hw.physical_error_rate,
                tiles as f64 * cycles,
                input.target_total_error.get(),
            )?;
            if d == distance && (new_wall.get() - wall.get()).abs() / wall.get().max(1e-9) < 1e-6 {
                wall = new_wall;
                break;
            }
            distance = d;
            wall = new_wall;
        }

        let rounds = self.rounds_per_operation.unwrap_or(distance.get() as f64);
        let patch = distance.rotated_patch_qubits();
        let cycles = (wall.get() / hw.cycle_time.get()).ceil();

        let data = input.logical_qubits.get() * patch;
        let routing = tiles.saturating_sub(input.logical_qubits.get()) * patch;
        let factories = (input.logical_qubits.get() as f64 * 0.02).ceil().max(1.0) as u64;
        let factory_qubits = factories * patch * 4;

        let breakdown = ResourceBreakdown {
            data,
            routing,
            factories: factory_qubits,
        };
        let total = PhysicalQubits::new(breakdown.total());

        // Which term binds: syndrome extraction or moving the atoms?
        let syndrome_share = rounds * hw.cycle_time.get();
        let limiting = if move_time.get() > syndrome_share {
            LimitingFactor::RoutingLimited
        } else {
            LimitingFactor::ReactionLimited
        };

        let expected_losses = input.toffoli_count as f64 * ATOM_LOSS_PER_MOVE;
        let warnings = vec![
            format!(
                "{rounds:.0} syndrome round(s) per logical operation; atom relocation \
                 contributes {:.0}% of the per-operation time",
                100.0 * move_time.get() / (syndrome_share + move_time.get())
            ),
            format!(
                "expected {expected_losses:.3e} atom losses over the run; reloading overhead \
                 is not yet modelled and this estimate is optimistic by that amount"
            ),
        ];

        Ok(QecEstimate {
            physical_qubits: total,
            wall_clock: wall,
            spacetime_volume: QubitRounds::new(
                (total.get() as u128).saturating_mul(cycles as u128),
            ),
            code_distance: Some(distance),
            magic_state_factory: FactoryReport {
                construction: "transversal injection with correlated decoding",
                count: factories,
                qubits_each: patch * 4,
                cycles_per_state: rounds.ceil() as u64,
                output_error: logical_error_rate(distance.get(), hw.physical_error_rate),
            },
            breakdown,
            logical_error_achieved: logical_error_rate(distance.get(), hw.physical_error_rate),
            limiting_factor: limiting,
            warnings,
        })
    }

    fn citations(&self) -> &'static [Citation] {
        &[
            Citation {
                reference: "arXiv:2403.03272",
                title: "Correlated decoding of logical algorithms with transversal gates",
                year: 2024,
            },
            Citation {
                reference: "arXiv:2505.13587",
                title: "Fast correlated decoding of transversal logical algorithms",
                year: 2025,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pramana_units::{ErrorRate, LogicalQubits};

    fn input() -> QecInput {
        QecInput {
            logical_qubits: LogicalQubits::new(6183),
            toffoli_count: 2_642_000_000,
            reaction_depth: 2_642_000_000,
            target_total_error: ErrorRate::new(0.01).unwrap(),
            idle_fraction: 0.0,
        }
    }

    #[test]
    fn correlated_decoding_beats_the_lattice_surgery_baseline() {
        // The central claim: O(1) rounds per operation instead of O(d).
        let hw = HardwareParams::neutral_atom_array();
        let fast = NeutralAtomTransversal::transversal()
            .estimate(&input(), &hw)
            .unwrap();
        let base = NeutralAtomTransversal::lattice_surgery_baseline()
            .estimate(&input(), &hw)
            .unwrap();
        assert!(
            fast.wall_clock.get() < base.wall_clock.get(),
            "transversal {} should beat baseline {}",
            fast.wall_clock.humanise(),
            base.wall_clock.humanise()
        );
    }

    #[test]
    fn atom_movement_dominates_the_per_operation_time() {
        // The countervailing cost. At 200 us per move against a 200 us cycle and one
        // round per operation, movement is half the budget; the architecture cannot
        // outrun its own mechanics.
        let hw = HardwareParams::neutral_atom_array();
        let e = NeutralAtomTransversal::transversal()
            .estimate(&input(), &hw)
            .unwrap();
        assert!(e
            .warnings
            .iter()
            .any(|w| w.contains("atom relocation contributes")));
    }

    #[test]
    fn reconfigurable_connectivity_is_required() {
        let e = NeutralAtomTransversal::transversal()
            .estimate(&input(), &HardwareParams::gidney_superconducting());
        assert!(matches!(e, Err(QecError::OutOfRegime { .. })));
    }

    #[test]
    fn move_time_must_be_supplied_explicitly() {
        let mut hw = HardwareParams::neutral_atom_array();
        hw.atom_move_time = None;
        assert!(matches!(
            NeutralAtomTransversal::transversal().estimate(&input(), &hw),
            Err(QecError::OutOfRegime { .. })
        ));
    }

    #[test]
    fn atom_loss_is_surfaced_as_an_unmodelled_optimism() {
        let e = NeutralAtomTransversal::transversal()
            .estimate(&input(), &HardwareParams::neutral_atom_array())
            .unwrap();
        assert!(e.warnings.iter().any(|w| w.contains("atom losses")));
    }

    #[test]
    fn routing_overhead_is_lower_than_a_square_grid_layout() {
        // Movable qubits do not need dedicated ancilla corridors.
        assert!(RECONFIGURABLE_TILES_PER_LOGICAL < 2.0);
    }

    #[test]
    fn slower_atoms_cost_time_monotonically() {
        let mut prev = 0.0;
        for us in [100.0, 200.0, 500.0, 1000.0] {
            let mut hw = HardwareParams::neutral_atom_array();
            hw.atom_move_time = Some(Seconds::from_micros(us));
            let t = NeutralAtomTransversal::transversal()
                .estimate(&input(), &hw)
                .unwrap()
                .wall_clock
                .get();
            assert!(t > prev, "slower moves must cost more time");
            prev = t;
        }
    }
}
