//! Repetition cat codes (bosonic, biased noise).
//!
//! Follows Gouzien, Ruiz, Le Regent, Guillaud & Sangouard, *Performance analysis of a
//! repetition cat code architecture* (PRL 131:040602, arXiv:2302.06639), which reports
//! computing a 256-bit elliptic curve discrete logarithm in 9 hours with 126,133 cat
//! qubits, assuming a single-to-two-photon loss ratio of 1e-5, a 500 ns cycle time, and
//! about 19 photons per cat state.
//!
//! # Why this architecture is interesting
//!
//! A cat qubit suppresses bit flips exponentially in the mean photon number, leaving
//! essentially only phase flips. Phase-only noise can be corrected by a **one-dimensional
//! repetition code**, which needs `2d - 1` physical qubits per logical qubit, against the
//! surface code's `2d^2 - 1`. That is the entire source of the claimed overhead
//! reduction, and it is a structural property PRAMANA can verify directly rather than
//! taking on faith.
//!
//! The advantage is therefore a function of achievable noise bias. PRAMANA exposes the
//! bias parameters explicitly so that the erosion of the advantage, as bias falls short
//! of the assumed value, can be measured rather than assumed away.

use crate::magic::MagicStateFactory;
use crate::model::{
    Citation, FactoryReport, HardwareParams, LimitingFactor, QecArchitecture, QecEstimate, QecError,
    QecInput, ResourceBreakdown,
};
use pramana_units::{ErrorRate, PhysicalQubits, QubitRounds, Seconds};

/// Coefficient relating phase-flip rate to `n_bar * (kappa_1 / kappa_2)`.
pub const PHASE_ERROR_COEFFICIENT: f64 = 1.0;

/// Threshold of the repetition code against phase errors under circuit-level noise.
pub const REPETITION_THRESHOLD: f64 = 0.1;

/// Largest repetition distance the solver will consider.
pub const MAX_DISTANCE: u32 = 201;

/// Physical cat qubits per logical qubit at repetition distance `d`.
///
/// `d` data modes plus `d - 1` ancilla modes for syndrome extraction. Contrast the
/// surface code's `2d^2 - 1`: the saving is quadratic in `d`, which is the whole point.
pub fn cat_qubits_per_logical(d: u32) -> u64 {
    2 * d as u64 - 1
}

/// Phase-flip probability per cycle for a cat qubit.
pub fn phase_error_rate(mean_photons: f64, loss_ratio: f64) -> f64 {
    PHASE_ERROR_COEFFICIENT * mean_photons * loss_ratio
}

/// Bit-flip probability per cycle, exponentially suppressed in the photon number.
pub fn bit_flip_rate(mean_photons: f64) -> f64 {
    (-2.0 * mean_photons).exp()
}

/// Achieved noise bias: phase errors per bit flip.
pub fn noise_bias(mean_photons: f64, loss_ratio: f64) -> f64 {
    phase_error_rate(mean_photons, loss_ratio) / bit_flip_rate(mean_photons)
}

/// The repetition-cat architecture.
pub struct RepetitionCat<F: MagicStateFactory> {
    /// Routing overhead expressed as tiles per logical qubit, matching the surface-code
    /// layout convention so that cross-architecture comparison is like for like.
    pub tiles_per_logical: f64,
    /// Magic state source.
    pub factory: F,
}

impl<F: MagicStateFactory> RepetitionCat<F> {
    /// Construct with an explicit routing overhead.
    pub fn new(tiles_per_logical: f64, factory: F) -> Self {
        Self {
            tiles_per_logical,
            factory,
        }
    }
}

impl<F: MagicStateFactory + 'static> QecArchitecture for RepetitionCat<F> {
    fn id(&self) -> &'static str {
        "repetition_cat"
    }

    fn display_name(&self) -> &'static str {
        "Repetition cat code (biased-noise bosonic)"
    }

    fn estimate(&self, input: &QecInput, hw: &HardwareParams) -> Result<QecEstimate, QecError> {
        let (Some(n_bar), Some(loss)) = (hw.mean_photon_number, hw.loss_ratio) else {
            return Err(QecError::OutOfRegime {
                architecture: self.id(),
                reason: "cat qubits need a mean photon number and a single-to-two-photon \
                         loss ratio; neither may be defaulted (Law 9)"
                    .into(),
            });
        };

        let p_phase = phase_error_rate(n_bar, loss);
        if p_phase >= REPETITION_THRESHOLD {
            return Err(QecError::AboveThreshold {
                p: p_phase,
                threshold: REPETITION_THRESHOLD,
                code: "repetition code",
            });
        }

        // The source pipelines its arithmetic, so the machine advances roughly one
        // non-Clifford operation per cycle rather than stalling for classical feedback.
        // Both limits are computed and the binding one is reported.
        let cycle_limited = Seconds::new(input.toffoli_count as f64 * hw.cycle_time.get());
        let reaction_limited =
            Seconds::new(input.reaction_depth as f64 * hw.reaction_time.get());
        let wall = cycle_limited.max(reaction_limited);
        let limiting = if reaction_limited.get() > cycle_limited.get() {
            LimitingFactor::ReactionLimited
        } else {
            LimitingFactor::FactoryLimited
        };

        let cycles = (wall.get() / hw.cycle_time.get()).ceil();
        let tiles = (input.logical_qubits.get() as f64 * self.tiles_per_logical).ceil() as u64;
        let volume = tiles as f64 * cycles;

        // Solve the repetition distance against the phase-error budget.
        let mut d = 3u32;
        #[allow(unused_assignments)]
        let mut achieved = 1.0;
        loop {
            if d > MAX_DISTANCE {
                return Err(QecError::DistanceSearchExhausted { max: MAX_DISTANCE });
            }
            achieved = (p_phase / REPETITION_THRESHOLD).powi(((d + 1) / 2) as i32);
            if achieved * volume <= input.target_total_error.get() {
                break;
            }
            d += 2;
        }

        // Residual bit flips are not corrected by a repetition code; if the photon
        // number is too low to suppress them below the budget, say so rather than
        // quietly reporting a number that assumes perfect bias.
        let mut warnings = Vec::new();
        let p_bit = bit_flip_rate(n_bar);
        if p_bit * volume > input.target_total_error.get() {
            warnings.push(format!(
                "residual bit-flip rate {p_bit:.3e} is not suppressed enough at n_bar={n_bar:.1}; \
                 the repetition code cannot correct it and the estimate is optimistic"
            ));
        }
        warnings.push(format!(
            "achieved noise bias {:.3e}; the overhead advantage is entirely a function of this",
            noise_bias(n_bar, loss)
        ));

        let per_logical = cat_qubits_per_logical(d);
        let data = input.logical_qubits.get() * per_logical;
        let routing = tiles.saturating_sub(input.logical_qubits.get()) * per_logical;

        let p_eff = ErrorRate::new(p_phase.min(1.0)).unwrap_or(ErrorRate::ZERO);
        let cycles_per_state = self.factory.cycles_per_state(d).max(1) as f64;
        let factories = ((input.toffoli_count as f64 * cycles_per_state) / cycles)
            .ceil()
            .max(1.0);
        let factory_qubits = factories as u64 * (2 * per_logical);

        let breakdown = ResourceBreakdown {
            data,
            routing,
            factories: factory_qubits,
        };
        let total = PhysicalQubits::new(breakdown.total());

        Ok(QecEstimate {
            physical_qubits: total,
            wall_clock: wall,
            spacetime_volume: QubitRounds::new(
                (total.get() as u128).saturating_mul(cycles as u128),
            ),
            code_distance: pramana_units::Distance::new_odd(d).ok(),
            magic_state_factory: FactoryReport {
                construction: "repetition-cat Toffoli factory",
                count: factories as u64,
                qubits_each: 2 * per_logical,
                cycles_per_state: cycles_per_state as u64,
                output_error: self.factory.output_error(p_eff),
            },
            breakdown,
            logical_error_achieved: achieved,
            limiting_factor: limiting,
            warnings,
        })
    }

    fn citations(&self) -> &'static [Citation] {
        &[Citation {
            reference: "arXiv:2302.06639",
            title: "Performance analysis of a repetition cat code architecture",
            year: 2023,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magic::Cultivation;
    use crate::surface::{Layout, SurfaceCode};
    use pramana_units::LogicalQubits;

    /// The ECDLP-256 circuit as PRAMANA synthesises it.
    fn ecdlp256() -> QecInput {
        QecInput {
            logical_qubits: LogicalQubits::new(2330),
            toffoli_count: 60_130_000_000,
            reaction_depth: 60_130_000_000,
            target_total_error: ErrorRate::new(0.01).unwrap(),
            idle_fraction: 0.0,
        }
    }

    #[test]
    fn repetition_code_is_linear_in_distance_not_quadratic() {
        // The structural claim behind the whole architecture.
        for d in [11u32, 21, 31] {
            let cat = cat_qubits_per_logical(d);
            let surface = pramana_units::Distance::new(d).unwrap().rotated_patch_qubits();
            assert!(
                cat * (d as u64) < surface,
                "d={d}: cat {cat} should be about a factor of d below surface {surface}"
            );
        }
    }

    #[test]
    fn bit_flips_are_exponentially_suppressed_by_photon_number() {
        assert!(bit_flip_rate(19.0) < 1e-16);
        assert!(bit_flip_rate(4.0) > bit_flip_rate(19.0) * 1e10);
    }

    #[test]
    fn missing_bias_parameters_are_refused_rather_than_defaulted() {
        let arch = RepetitionCat::new(2.0, Cultivation);
        let hw = HardwareParams::gidney_superconducting();
        assert!(matches!(
            arch.estimate(&ecdlp256(), &hw),
            Err(QecError::OutOfRegime { .. })
        ));
    }

    #[test]
    fn reproduces_gouzien_qubit_count_within_a_factor() {
        let arch = RepetitionCat::new(2.0, Cultivation);
        let est = arch
            .estimate(&ecdlp256(), &HardwareParams::gouzien_cat())
            .unwrap();
        let q = est.physical_qubits.get() as f64;
        let ratio = q / 126_133.0;
        assert!(
            ratio > 0.5 && ratio < 2.0,
            "expected ~126k cat qubits, got {q:.4e} (ratio {ratio:.3})"
        );
    }

    #[test]
    fn reproduces_gouzien_runtime_within_a_factor() {
        let arch = RepetitionCat::new(2.0, Cultivation);
        let est = arch
            .estimate(&ecdlp256(), &HardwareParams::gouzien_cat())
            .unwrap();
        let hours = est.wall_clock.as_hours();
        assert!(
            hours > 4.0 && hours < 20.0,
            "expected roughly 9 hours, got {hours:.2}"
        );
    }

    #[test]
    fn cat_beats_the_surface_code_on_the_same_circuit() {
        let cat = RepetitionCat::new(2.0, Cultivation)
            .estimate(&ecdlp256(), &HardwareParams::gouzien_cat())
            .unwrap();
        let surf = SurfaceCode::new(Layout::Intermediate, Cultivation)
            .estimate(&ecdlp256(), &HardwareParams::gidney_superconducting())
            .unwrap();
        assert!(
            cat.physical_qubits.get() * 10 < surf.physical_qubits.get(),
            "cat {} should be far below surface {}",
            cat.physical_qubits.get(),
            surf.physical_qubits.get()
        );
    }

    #[test]
    fn the_advantage_erodes_as_bias_degrades() {
        // The vendor claim is a function of achievable bias. Degrading the loss ratio
        // must visibly cost qubits; this curve is the falsifiable engineering
        // requirement hiding behind the headline number.
        let arch = RepetitionCat::new(2.0, Cultivation);
        let mut prev = 0u64;
        for loss in [1e-5, 1e-4, 1e-3] {
            let mut hw = HardwareParams::gouzien_cat();
            hw.loss_ratio = Some(loss);
            let q = arch.estimate(&ecdlp256(), &hw).unwrap().physical_qubits.get();
            assert!(q > prev, "worse bias must cost more qubits: {q} vs {prev}");
            prev = q;
        }
    }

    #[test]
    fn low_photon_number_raises_an_explicit_warning() {
        let arch = RepetitionCat::new(2.0, Cultivation);
        let mut hw = HardwareParams::gouzien_cat();
        hw.mean_photon_number = Some(3.0);
        let est = arch.estimate(&ecdlp256(), &hw).unwrap();
        assert!(
            est.warnings.iter().any(|w| w.contains("bit-flip")),
            "insufficient bias must be surfaced, not hidden"
        );
    }
}
