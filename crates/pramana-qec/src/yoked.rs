//! Yoked surface codes for idle storage.
//!
//! Gidney, Newman, Brooks & Jones (arXiv:2312.04522) concatenate an outer code across
//! many surface-code patches. Idle logical qubits stored under the yoke need a smaller
//! inner distance for the same logical error rate, roughly halving the space cost of
//! memory, at the price of more complex decoding and slower access.
//!
//! This matters for Shor because windowed modular exponentiation touches a narrow
//! working set: most registers are idle most of the time. Without modelling that, the
//! Gidney 2025 qubit count is unreachable.

use crate::magic::MagicStateFactory;
use crate::model::{
    Citation, HardwareParams, QecArchitecture, QecEstimate, QecError, QecInput, ResourceBreakdown,
};
use crate::surface::{Layout, SurfaceCode};
use pramana_units::PhysicalQubits;

/// Space saving on idle storage from yoking, as reported by the source.
///
/// The outer code lets an idle patch run at reduced inner distance for the same
/// effective error rate. Expressed as the fraction of the unyoked footprint retained.
pub const YOKE_IDLE_SPACE_FRACTION: f64 = 0.5;

/// Surface code with yoked idle storage.
pub struct YokedSurfaceCode<F: MagicStateFactory> {
    /// The underlying active-region model.
    pub inner: SurfaceCode<F>,
}

impl<F: MagicStateFactory> YokedSurfaceCode<F> {
    /// Wrap a surface-code model with yoked idle storage.
    pub fn new(layout: Layout, factory: F) -> Self {
        Self {
            inner: SurfaceCode::new(layout, factory),
        }
    }
}

impl<F: MagicStateFactory + 'static> QecArchitecture for YokedSurfaceCode<F> {
    fn id(&self) -> &'static str {
        "yoked_surface_code"
    }

    fn display_name(&self) -> &'static str {
        "Surface code with yoked idle storage"
    }

    fn estimate(&self, input: &QecInput, hw: &HardwareParams) -> Result<QecEstimate, QecError> {
        if !(0.0..=1.0).contains(&input.idle_fraction) {
            return Err(QecError::OutOfRegime {
                architecture: self.id(),
                reason: format!(
                    "idle fraction {} is outside [0, 1]",
                    input.idle_fraction
                ),
            });
        }
        let mut est = self.inner.estimate(input, hw)?;

        // Yoking applies to data patches holding idle logical qubits. Routing corridors
        // and factories are active by definition and are untouched.
        let idle = input.idle_fraction;
        let data = est.breakdown.data as f64;
        let yoked_data = data * (1.0 - idle) + data * idle * YOKE_IDLE_SPACE_FRACTION;

        est.breakdown = ResourceBreakdown {
            data: yoked_data.round() as u64,
            routing: est.breakdown.routing,
            factories: est.breakdown.factories,
        };
        est.physical_qubits = PhysicalQubits::new(est.breakdown.total());
        est.warnings.push(format!(
            "idle storage yoked at {:.0}% of unyoked footprint over an idle fraction of {:.2}; \
             access latency for cold qubits is not yet modelled",
            YOKE_IDLE_SPACE_FRACTION * 100.0,
            idle
        ));
        Ok(est)
    }

    fn citations(&self) -> &'static [Citation] {
        &[Citation {
            reference: "arXiv:2312.04522",
            title: "Yoked surface codes",
            year: 2023,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magic::FifteenToOneTwoLevel;
    use crate::model::HardwareParams;
    use pramana_units::{ErrorRate, LogicalQubits};

    fn input(idle: f64) -> QecInput {
        QecInput {
            logical_qubits: LogicalQubits::new(6189),
            toffoli_count: 2_642_000_000,
            reaction_depth: 2_642_000_000,
            target_total_error: ErrorRate::new(0.01).unwrap(),
            idle_fraction: idle,
        }
    }

    #[test]
    fn yoking_never_costs_more_than_not_yoking() {
        let hw = HardwareParams::gidney_superconducting();
        let plain = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)
            .estimate(&input(0.0), &hw)
            .unwrap();
        let yoked = YokedSurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)
            .estimate(&input(0.8), &hw)
            .unwrap();
        assert!(yoked.physical_qubits.get() < plain.physical_qubits.get());
    }

    #[test]
    fn zero_idle_fraction_reduces_to_the_plain_surface_code() {
        let hw = HardwareParams::gidney_superconducting();
        let plain = SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)
            .estimate(&input(0.0), &hw)
            .unwrap();
        let yoked = YokedSurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)
            .estimate(&input(0.0), &hw)
            .unwrap();
        assert_eq!(plain.physical_qubits.get(), yoked.physical_qubits.get());
    }

    #[test]
    fn saving_is_monotone_in_the_idle_fraction() {
        let hw = HardwareParams::gidney_superconducting();
        let arch = YokedSurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let mut prev = u64::MAX;
        for idle in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let q = arch.estimate(&input(idle), &hw).unwrap().physical_qubits.get();
            assert!(q <= prev, "idle={idle} increased the qubit count");
            prev = q;
        }
    }

    #[test]
    fn out_of_range_idle_fraction_is_refused() {
        let hw = HardwareParams::gidney_superconducting();
        let arch = YokedSurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        assert!(matches!(
            arch.estimate(&input(1.5), &hw),
            Err(QecError::OutOfRegime { .. })
        ));
    }

    #[test]
    fn yoking_alone_cannot_reach_the_gidney_2025_target() {
        // A negative result worth pinning: even yoking every idle qubit in the GE19
        // circuit leaves the machine far above one million physical qubits. The 2025
        // result needs the *logical* qubit count to fall, which is what the residue
        // arithmetic delivers. Yoking is necessary but nowhere near sufficient.
        let hw = HardwareParams::gidney_superconducting();
        let arch = YokedSurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel);
        let q = arch.estimate(&input(1.0), &hw).unwrap().physical_qubits.get();
        assert!(
            q > 1_000_000,
            "expected yoking alone to fall short of 1M, got {q}"
        );
    }
}
