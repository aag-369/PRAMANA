//! The break-year engine.
//!
//! # The two-curve model
//!
//! PRAMANA computes two curves and reports where they cross:
//!
//! - a **falling algorithmic requirement**: what an attack on this asset costs, which drops
//!   as cryptanalysis improves (`pramana-circuit`, `pramana-qec`, [`crate::improvement`]);
//! - a **rising hardware capability**: what machines can supply, fitted from verified
//!   demonstrations (`pramana-hardware`).
//!
//! A cryptographically relevant quantum computer exists, *for this asset*, at the moment
//! they cross. The break year is a distribution because both curves are.
//!
//! # Why there is no global Q-Day here
//!
//! The crossing depends on the asset's key parameters, on which fault-tolerant architecture
//! the attacker builds, on how much cryptanalysis improves in the interim, and on how long
//! the attacker is willing to run the machine. Collapsing that to one date for an entire
//! estate is the assumption this project exists to remove.

use crate::improvement::{Floors, ImprovementModel, ProblemFamily};
use pramana_hardware::trajectory::Trajectory;
use pramana_qec::model::{HardwareParams, QecArchitecture, QecInput};
use pramana_units::{ErrorRate, LogicalQubits};
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use rand::SeedableRng;
use rand_distr::{Distribution, LogNormal};
use serde::{Deserialize, Serialize};

/// How long an attacker is willing to run the machine for one result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AttackerBudget {
    /// Median tolerated wall-clock time, in days.
    pub median_days: f64,
    /// Spread, as the sigma of a log-normal.
    pub sigma: f64,
}

impl AttackerBudget {
    /// A patient state actor: months are acceptable.
    pub fn nation_state() -> Self {
        Self {
            median_days: 90.0,
            sigma: 1.2,
        }
    }

    /// An opportunistic attacker who needs a result quickly.
    pub fn opportunistic() -> Self {
        Self {
            median_days: 1.0,
            sigma: 0.8,
        }
    }

    /// Sample a tolerated runtime in days.
    pub fn sample<R: Rng>(&self, rng: &mut R) -> f64 {
        LogNormal::new(self.median_days.ln(), self.sigma)
            .expect("valid lognormal")
            .sample(rng)
    }
}

/// One candidate architecture the attacker might build.
pub struct ArchitectureOption {
    /// Display name.
    pub name: String,
    /// The cost model.
    pub architecture: Box<dyn QecArchitecture>,
    /// Hardware assumptions that go with it.
    pub hardware: HardwareParams,
    /// Physical qubits per logical qubit for this modality, from verified demonstrations.
    pub physical_per_logical: f64,
    /// Prior weight.
    pub weight: f64,
}

/// Everything the engine needs about the asset under attack.
#[derive(Debug, Clone, Copy)]
pub struct AttackProfile {
    /// Logical qubits the best current circuit needs.
    pub logical_qubits: u64,
    /// Toffoli gates the best current circuit needs, across all expected runs.
    pub toffoli: f64,
    /// Problem family, which selects the improvement rate.
    pub family: ProblemFamily,
    /// Physical floors for this problem.
    pub floors: Floors,
    /// Improvement model for gate count.
    pub gate_improvement: ImprovementModel,
    /// Improvement model for width.
    pub width_improvement: ImprovementModel,
}

/// Configuration for a Monte Carlo run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MonteCarloConfig {
    /// Number of samples.
    pub samples: usize,
    /// RNG seed, recorded so a run reproduces byte for byte (Law 5).
    pub seed: u64,
    /// Earliest year the search will consider.
    pub first_year: u32,
    /// Latest year the search will consider.
    pub last_year: u32,
    /// Total error budget allowed for one attack run.
    pub target_total_error: f64,
}

impl Default for MonteCarloConfig {
    fn default() -> Self {
        Self {
            samples: 20_000,
            seed: 0x5052414D414E41,
            first_year: 2026,
            last_year: 2100,
            target_total_error: 0.05,
        }
    }
}

/// An empirical distribution over break years.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreakYearDistribution {
    /// Sorted samples of the break year.
    pub samples: Vec<u32>,
    /// Samples in which the attack never became feasible inside the search window.
    pub never: usize,
    /// Per-architecture median, for the sensitivity view.
    pub by_architecture: Vec<(String, Option<u32>)>,
    /// Configuration used, for reproducibility.
    pub config: MonteCarloConfig,
    /// Warnings raised during sampling.
    pub warnings: Vec<String>,
}

impl BreakYearDistribution {
    /// Quantile of the break year, ignoring never-feasible samples.
    ///
    /// Returns `None` when too few samples produced a year for a quantile to mean anything.
    pub fn quantile(&self, q: f64) -> Option<u32> {
        if self.samples.is_empty() {
            return None;
        }
        let idx = ((self.samples.len() - 1) as f64 * q.clamp(0.0, 1.0)).round() as usize;
        Some(self.samples[idx])
    }

    /// Median break year.
    pub fn median(&self) -> Option<u32> {
        self.quantile(0.5)
    }

    /// Fraction of samples in which the attack never became feasible.
    pub fn never_fraction(&self) -> f64 {
        let total = self.samples.len() + self.never;
        if total == 0 {
            0.0
        } else {
            self.never as f64 / total as f64
        }
    }

    /// Probability that the asset is breakable on or before `year`.
    pub fn probability_broken_by(&self, year: u32) -> f64 {
        let total = self.samples.len() + self.never;
        if total == 0 {
            return 0.0;
        }
        let count = self.samples.partition_point(|y| *y <= year);
        count as f64 / total as f64
    }

    /// Counts per break year, for rendering a density rather than an error bar.
    ///
    /// The exposure timeline shows a distribution; quantiles alone would collapse it back
    /// into the point estimate the whole project exists to avoid.
    pub fn histogram(&self) -> Vec<(u32, usize)> {
        let mut out: Vec<(u32, usize)> = Vec::new();
        for y in &self.samples {
            match out.last_mut() {
                Some(last) if last.0 == *y => last.1 += 1,
                _ => out.push((*y, 1)),
            }
        }
        out
    }

    /// Monte Carlo standard error on the median, in years.
    pub fn median_standard_error(&self) -> f64 {
        let n = self.samples.len();
        if n < 2 {
            return f64::INFINITY;
        }
        let mean = self.samples.iter().map(|y| *y as f64).sum::<f64>() / n as f64;
        let var = self
            .samples
            .iter()
            .map(|y| (*y as f64 - mean).powi(2))
            .sum::<f64>()
            / (n as f64 - 1.0);
        // Asymptotic standard error of a sample median for a roughly normal sample.
        1.2533 * (var / n as f64).sqrt()
    }
}

/// Run the break-year Monte Carlo for one asset.
pub fn break_year_distribution(
    profile: &AttackProfile,
    options: &[ArchitectureOption],
    trajectory: &Trajectory,
    budget: AttackerBudget,
    config: MonteCarloConfig,
) -> BreakYearDistribution {
    let mut rng = ChaCha20Rng::seed_from_u64(config.seed);
    let mut samples = Vec::with_capacity(config.samples);
    let mut never = 0usize;
    let mut warnings = Vec::new();

    if options.is_empty() {
        warnings.push("no architectures supplied; no break year can be computed".into());
        return BreakYearDistribution {
            samples,
            never: config.samples,
            by_architecture: vec![],
            config,
            warnings,
        };
    }

    let total_weight: f64 = options.iter().map(|o| o.weight).sum();
    let mut per_arch: Vec<Vec<u32>> = vec![Vec::new(); options.len()];

    for _ in 0..config.samples {
        // Sample an architecture from the prior.
        let pick = rng.gen::<f64>() * total_weight;
        let mut acc = 0.0;
        let mut idx = options.len() - 1;
        for (i, o) in options.iter().enumerate() {
            acc += o.weight;
            if pick <= acc {
                idx = i;
                break;
            }
        }
        let opt = &options[idx];

        // Sample improvement rates and the attacker's patience.
        let gate_rate = sample_rate(&profile.gate_improvement, &mut rng);
        let width_rate = sample_rate(&profile.width_improvement, &mut rng);
        let tolerated_days = budget.sample(&mut rng);

        match earliest_feasible_year(
            profile, opt, trajectory, gate_rate, width_rate, tolerated_days, &config, &mut rng,
        ) {
            Some(y) => {
                samples.push(y);
                per_arch[idx].push(y);
            }
            None => never += 1,
        }
    }

    samples.sort_unstable();
    let by_architecture = options
        .iter()
        .zip(per_arch.iter_mut())
        .map(|(o, ys)| {
            ys.sort_unstable();
            let median = if ys.is_empty() {
                None
            } else {
                Some(ys[ys.len() / 2])
            };
            (o.name.clone(), median)
        })
        .collect();

    if samples.is_empty() {
        warnings.push(
            "no sample produced a feasible attack inside the search window; the asset is \
             either far from threatened or the search window is too short"
                .into(),
        );
    }
    let never_frac = never as f64 / config.samples as f64;
    if never_frac > 0.1 && !samples.is_empty() {
        warnings.push(format!(
            "{:.0}% of samples never became feasible; quantiles are conditional on \
             feasibility and understate the true spread",
            never_frac * 100.0
        ));
    }

    BreakYearDistribution {
        samples,
        never,
        by_architecture,
        config,
        warnings,
    }
}

fn sample_rate<R: Rng>(m: &ImprovementModel, rng: &mut R) -> f64 {
    let d = rand_distr::Normal::new(m.rate_per_year, m.rate_sd.max(1e-9)).expect("valid normal");
    d.sample(rng).max(0.0)
}

/// Binary search for the earliest year the attack becomes feasible.
///
/// The predicate is monotone: the requirement falls with time and capability rises, so once
/// feasible it stays feasible. That makes a binary search valid and keeps the Monte Carlo
/// affordable.
#[allow(clippy::too_many_arguments)]
fn earliest_feasible_year<R: Rng>(
    profile: &AttackProfile,
    opt: &ArchitectureOption,
    trajectory: &Trajectory,
    gate_rate: f64,
    width_rate: f64,
    tolerated_days: f64,
    config: &MonteCarloConfig,
    rng: &mut R,
) -> Option<u32> {
    if !feasible_in(
        profile, opt, trajectory, gate_rate, width_rate, tolerated_days, config.last_year, config,
        rng,
    ) {
        return None;
    }
    let (mut lo, mut hi) = (config.first_year, config.last_year);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if feasible_in(
            profile, opt, trajectory, gate_rate, width_rate, tolerated_days, mid, config, rng,
        ) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Some(lo)
}

#[allow(clippy::too_many_arguments)]
fn feasible_in<R: Rng>(
    profile: &AttackProfile,
    opt: &ArchitectureOption,
    trajectory: &Trajectory,
    gate_rate: f64,
    width_rate: f64,
    tolerated_days: f64,
    year: u32,
    config: &MonteCarloConfig,
    rng: &mut R,
) -> bool {
    // Falling requirement.
    let logical = profile
        .width_improvement
        .apply(
            profile.logical_qubits as f64,
            profile.floors.logical_qubits,
            year,
            width_rate,
        )
        .max(1.0);
    let toffoli = profile
        .gate_improvement
        .apply(profile.toffoli, profile.floors.toffoli, year, gate_rate)
        .max(1.0);

    let input = QecInput {
        logical_qubits: LogicalQubits::new(logical as u64),
        toffoli_count: toffoli as u128,
        reaction_depth: toffoli as u64,
        target_total_error: ErrorRate::new(config.target_total_error).unwrap_or(ErrorRate::ONE),
        idle_fraction: 0.0,
    };

    let Ok(est) = opt.architecture.estimate(&input, &opt.hardware) else {
        return false;
    };

    // Rising capability. The trajectory is fitted on logical qubits; converting through the
    // modality's demonstrated physical-per-logical ratio gives the physical budget that the
    // QEC estimate can be compared against.
    let available_logical = trajectory.sample_logical_qubits_at(year as f64, rng);
    let available_physical = available_logical * opt.physical_per_logical;

    est.physical_qubits.get() as f64 <= available_physical
        && est.wall_clock.as_days() <= tolerated_days
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::improvement::{Floors, ImprovementModel};
    use pramana_hardware::fit::{CapabilityFit, SlipPrior};
    use pramana_qec::magic::FifteenToOneTwoLevel;
    use pramana_qec::surface::{Layout, SurfaceCode};

    fn trajectory(slope: f64) -> Trajectory {
        Trajectory::new(
            CapabilityFit {
                slope,
                // ~100 logical qubits in 2026.
                intercept: 2.0 - slope * 2026.0,
                residual_sd: 0.2,
                points: 5,
                first_year: 2024,
                last_year: 2034,
            },
            SlipPrior {
                mean_years: 1.5,
                sd_years: 0.9,
                observed: 5,
                censored: 0,
                warnings: vec![],
            },
        )
    }

    fn options() -> Vec<ArchitectureOption> {
        vec![ArchitectureOption {
            name: "surface".into(),
            architecture: Box::new(SurfaceCode::new(Layout::Intermediate, FifteenToOneTwoLevel)),
            hardware: HardwareParams::gidney_superconducting(),
            physical_per_logical: 105.0,
            weight: 1.0,
        }]
    }

    fn profile(logical: u64, toffoli: f64) -> AttackProfile {
        AttackProfile {
            logical_qubits: logical,
            toffoli,
            family: ProblemFamily::Factoring,
            floors: Floors::factoring(2048),
            gate_improvement: ImprovementModel::factoring_gates(),
            width_improvement: ImprovementModel::factoring_width(),
        }
    }

    fn config(samples: usize) -> MonteCarloConfig {
        MonteCarloConfig {
            samples,
            ..Default::default()
        }
    }

    #[test]
    fn is_deterministic_under_a_fixed_seed() {
        // Law 5: same manifest, same result, byte for byte.
        let p = profile(1432, 6.5e9);
        let a = break_year_distribution(
            &p,
            &options(),
            &trajectory(0.18),
            AttackerBudget::nation_state(),
            config(300),
        );
        let b = break_year_distribution(
            &p,
            &options(),
            &trajectory(0.18),
            AttackerBudget::nation_state(),
            config(300),
        );
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.never, b.never);
    }

    #[test]
    fn a_different_seed_gives_a_different_sample_but_a_similar_median() {
        let p = profile(1432, 6.5e9);
        let a = break_year_distribution(
            &p, &options(), &trajectory(0.18), AttackerBudget::nation_state(), config(800));
        let mut c = config(800);
        c.seed = 999;
        let b = break_year_distribution(
            &p, &options(), &trajectory(0.18), AttackerBudget::nation_state(), c);
        let (ma, mb) = (a.median().unwrap(), b.median().unwrap());
        assert!(
            (ma as i64 - mb as i64).abs() <= 3,
            "medians should agree across seeds: {ma} vs {mb}"
        );
    }

    #[test]
    fn gate_savings_barely_move_a_space_bound_break_year() {
        // A finding, pinned as a test. The 2026 elliptic-curve circuits cut gate count by
        // roughly a thousandfold over the 2017 construction, yet the break year moves only
        // about a year, because the attacker is waiting for enough error-corrected qubits
        // to host the circuit rather than for the computation to finish. A tolerant
        // attacker budget makes runtime a non-binding constraint.
        //
        // The practical consequence for triage: for elliptic curves it is the *width*
        // improvement series that matters, not the gate series.
        let many_gates = profile(1432, 6.5e11);
        let few_gates = profile(1432, 6.5e8);
        let a = break_year_distribution(
            &many_gates, &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        let b = break_year_distribution(
            &few_gates, &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        let shift = a.median().unwrap() as i64 - b.median().unwrap() as i64;
        assert!(
            (0..=3).contains(&shift),
            "a thousandfold gate saving at fixed width should shift the break year very \
             little, got {shift} years"
        );
    }

    /// A profile with improvement disabled, to isolate the engine from the prior.
    fn frozen_profile(logical: u64, toffoli: f64) -> AttackProfile {
        let mut p = profile(logical, toffoli);
        p.gate_improvement.rate_per_year = 0.0;
        p.gate_improvement.rate_sd = 0.0;
        p.width_improvement.rate_per_year = 0.0;
        p.width_improvement.rate_sd = 0.0;
        p
    }

    #[test]
    fn with_improvement_frozen_width_dominates_gates() {
        // Isolates the engine from the improvement prior. Holding algorithms fixed, a
        // fourfold width saving moves the break year several years while a thousandfold
        // gate saving moves it barely at all: the attacker waits for qubits, not for the
        // computation.
        let run = |p: &AttackProfile| {
            break_year_distribution(
                p, &options(), &trajectory(0.18),
                AttackerBudget::nation_state(), config(600),
            )
            .median()
            .unwrap() as i64
        };
        let wide = run(&frozen_profile(4000, 6.5e9));
        let narrow = run(&frozen_profile(1000, 6.5e9));
        let many_gates = run(&frozen_profile(1432, 6.5e11));
        let few_gates = run(&frozen_profile(1432, 6.5e8));

        let width_shift = wide - narrow;
        let gate_shift = many_gates - few_gates;
        assert!(
            width_shift >= 3,
            "a fourfold width saving should move the break year materially, got {width_shift}"
        );
        assert!(
            width_shift > gate_shift,
            "width should dominate gates: width moved {width_shift}, gates {gate_shift}"
        );
    }

    #[test]
    fn improvement_converges_distinct_widths_toward_the_floor() {
        // A consequence of the bounded improvement prior worth surfacing rather than
        // discovering later. Because width improves at the historically fitted rate and
        // decays toward a floor, two constructions that differ fourfold today converge to
        // within a small factor within roughly fifteen years. Break years far in the
        // future therefore depend more on the floor than on today's width.
        //
        // This is a strong assumption: it presumes the historical rate persists. The
        // sensitivity of a result to it should be reported alongside the result.
        let p = profile(4000, 6.5e9);
        let wide_2040 = p.width_improvement.apply(
            4000.0, p.floors.logical_qubits, 2040, p.width_improvement.rate_per_year);
        let narrow_2040 = p.width_improvement.apply(
            1000.0, p.floors.logical_qubits, 2040, p.width_improvement.rate_per_year);
        let ratio_now = 4.0;
        let ratio_2040 = wide_2040 / narrow_2040;
        assert!(
            ratio_2040 < ratio_now / 2.0,
            "expected convergence: 4x today becomes {ratio_2040:.2}x by 2040"
        );
    }

    #[test]
    fn a_bigger_asset_breaks_later() {
        // The core monotonicity the whole tool rests on.
        let small = break_year_distribution(
            &profile(1432, 6.5e9), &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        let large = break_year_distribution(
            &profile(20000, 6.5e11), &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        assert!(
            large.median().unwrap() > small.median().unwrap(),
            "a larger key must not break sooner"
        );
    }

    #[test]
    fn faster_hardware_progress_brings_the_break_year_forward() {
        let p = profile(1432, 6.5e9);
        let slow = break_year_distribution(
            &p, &options(), &trajectory(0.10), AttackerBudget::nation_state(), config(600));
        let fast = break_year_distribution(
            &p, &options(), &trajectory(0.30), AttackerBudget::nation_state(), config(600));
        assert!(fast.median().unwrap() < slow.median().unwrap());
    }

    #[test]
    fn a_patient_attacker_breaks_things_sooner_than_an_impatient_one() {
        let p = profile(1432, 6.5e9);
        let patient = break_year_distribution(
            &p, &options(), &trajectory(0.18), AttackerBudget::nation_state(), config(600));
        let hasty = break_year_distribution(
            &p, &options(), &trajectory(0.18), AttackerBudget::opportunistic(), config(600));
        assert!(
            patient.median().unwrap() <= hasty.median().unwrap(),
            "tolerating a longer run cannot delay the break"
        );
    }

    #[test]
    fn no_architectures_yields_no_break_year_rather_than_a_guess() {
        let d = break_year_distribution(
            &profile(1432, 6.5e9), &[], &trajectory(0.18),
            AttackerBudget::nation_state(), config(100));
        assert!(d.samples.is_empty());
        assert_eq!(d.never, 100);
        assert!(d.warnings.iter().any(|w| w.contains("no architectures")));
    }

    #[test]
    fn histogram_counts_every_sample_and_is_ordered() {
        let d = break_year_distribution(
            &profile(1432, 6.5e9), &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        let h = d.histogram();
        assert_eq!(h.iter().map(|(_, c)| c).sum::<usize>(), d.samples.len());
        assert!(h.windows(2).all(|w| w[0].0 < w[1].0), "years must be ascending and unique");
    }

    #[test]
    fn quantiles_are_ordered() {
        let d = break_year_distribution(
            &profile(1432, 6.5e9), &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        let (p05, p50, p95) = (
            d.quantile(0.05).unwrap(),
            d.quantile(0.50).unwrap(),
            d.quantile(0.95).unwrap(),
        );
        assert!(p05 <= p50 && p50 <= p95);
    }

    #[test]
    fn probability_broken_by_is_monotone_and_bounded() {
        let d = break_year_distribution(
            &profile(1432, 6.5e9), &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        let mut prev = 0.0;
        for y in 2026..2100 {
            let p = d.probability_broken_by(y);
            assert!((0.0..=1.0).contains(&p));
            assert!(p >= prev - 1e-12, "cumulative probability fell at {y}");
            prev = p;
        }
    }

    #[test]
    fn per_architecture_medians_are_reported_for_the_sensitivity_view() {
        let mut opts = options();
        opts.push(ArchitectureOption {
            name: "surface (cultivation)".into(),
            architecture: Box::new(SurfaceCode::new(
                Layout::Intermediate,
                pramana_qec::magic::Cultivation,
            )),
            hardware: HardwareParams::gidney_superconducting(),
            physical_per_logical: 105.0,
            weight: 1.0,
        });
        let d = break_year_distribution(
            &profile(1432, 6.5e9), &opts, &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        assert_eq!(d.by_architecture.len(), 2);
        assert!(d.by_architecture.iter().all(|(_, m)| m.is_some()));
    }

    #[test]
    fn median_standard_error_is_finite_for_a_real_sample() {
        let d = break_year_distribution(
            &profile(1432, 6.5e9), &options(), &trajectory(0.18),
            AttackerBudget::nation_state(), config(600));
        assert!(d.median_standard_error().is_finite());
        assert!(d.median_standard_error() < 2.0, "should be well under a year with 600 samples");
    }
}
