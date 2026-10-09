//! Approximate residue arithmetic (Chevignard-Fouque-Schrottenloher), as streamlined by
//! Gidney 2025 (arXiv:2505.15917).
//!
//! # The idea
//!
//! Instead of holding a value modulo one large `N` in an `n`-bit register, hold it modulo
//! many small `l`-bit primes and recombine by the Chinese remainder theorem. Since
//! `l = Theta(log log N)`, the register that has to exist *at any one moment* shrinks
//! dramatically: the input register falls to `m = ceil(n/2 + n/s)` qubits, roughly `0.5n`,
//! against the `3n` of Gidney-Ekera 2019. The price is a much larger gate count, since the
//! work is now spread over `|P| = Theta(n^2 / (log n log log n))` primes.
//!
//! # Structure
//!
//! The algorithm is a nest of four loops plus their uncomputations. Each iteration
//! performs some number of *additions* on a register, *lookups* addressed by a window,
//! and *phaseups* (phase-kickback table lookups). PRAMANA takes the per-subroutine
//! iteration counts and operation tallies from Table 3 of the source, derives every
//! parameter from Table 2, and lets the Toffoli count emerge from assembling them.
//!
//! Fractional operation counts in Table 3 (1.5 additions, 2.5 lookups) are averages over
//! branches; they are carried in halves and emitted exactly.

use crate::ir::CircuitBuilder;
use serde::{Deserialize, Serialize};

/// Parameters of the residue-arithmetic factoring circuit, per Table 2 of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResidueParams {
    /// Bit size of the number to factor.
    pub modulus_bits: u32,
    /// Ekera-Hastad parameter `s`.
    pub ekera_hastad_s: u32,
    /// Bit size of the primes in the residue system, `l`.
    pub prime_bits: u32,
    /// Window length used by loop 1.
    pub w1: u32,
    /// Window length used by loop 3 and unloop 3.
    pub w3: u32,
    /// Window length used by loop 4.
    pub w4: u32,
    /// Size of the output accumulator, `f`.
    pub accumulator_bits: u32,
}

impl ResidueParams {
    /// The parameter set highlighted in Table 5 of arXiv:2505.15917 for a given modulus.
    ///
    /// The source selected these by grid scan over `s` in 2..=8, `w1` in 2..=8, `w3` in
    /// 2..=6 and `w4` in 2..=8. PRAMANA can rerun that scan via
    /// [`ResidueParams::optimise`]; these are the published starting points.
    pub fn published(modulus_bits: u32) -> Option<Self> {
        let (s, l, w1, w3, w4, f) = match modulus_bits {
            1024 => (8, 18, 6, 3, 6, 28),
            1536 => (8, 21, 6, 3, 5, 31),
            2048 => (8, 21, 6, 3, 5, 33),
            3072 => (8, 21, 6, 3, 5, 35),
            4096 => (8, 24, 6, 3, 5, 36),
            6144 => (8, 24, 6, 3, 5, 39),
            8192 => (8, 24, 6, 3, 5, 40),
            _ => return None,
        };
        Some(Self {
            modulus_bits,
            ekera_hastad_s: s,
            prime_bits: l,
            w1,
            w3,
            w4,
            accumulator_bits: f,
        })
    }

    /// Number of input qubits: `m = ceil(n/2 + n/s)`.
    ///
    /// This is the quantity that makes the whole approach interesting: roughly `0.5n`
    /// where the 2019 construction needed `3n`.
    pub fn input_qubits(&self) -> u64 {
        let n = self.modulus_bits as f64;
        (n / 2.0 + n / self.ekera_hastad_s as f64).ceil() as u64
    }

    /// `len m`, the bit length of the input register size.
    pub fn len_m(&self) -> u32 {
        let m = self.input_qubits();
        if m <= 1 {
            0
        } else {
            64 - (m - 1).leading_zeros()
        }
    }

    /// Number of primes in the residue system: `|P| ~ n*m / (l * w1)`.
    pub fn prime_count(&self) -> u64 {
        let n = self.modulus_bits as u64;
        let m = self.input_qubits();
        (n * m).div_ceil(self.prime_bits as u64 * self.w1 as u64)
    }

    /// Windows iterated by loop 1: `W1 = ceil(m / w1)`.
    pub fn w1_windows(&self) -> u64 {
        self.input_qubits().div_ceil(self.w1 as u64)
    }

    /// Windows iterated by loop 3: `W3 = ceil(l / w3)`.
    pub fn w3_windows(&self) -> u64 {
        (self.prime_bits as u64).div_ceil(self.w3 as u64)
    }

    /// Windows iterated by loop 4: `W4 = ceil(l / w4)`.
    pub fn w4_windows(&self) -> u64 {
        (self.prime_bits as u64).div_ceil(self.w4 as u64)
    }

    /// Peak logical qubits, from Table 4. The maximum occurs during loop 4.
    pub fn peak_logical_qubits(&self) -> u64 {
        self.input_qubits()
            + 3 * self.accumulator_bits as u64
            + 2 * self.prime_bits as u64
            + self.len_m() as u64
    }

    /// Probability that a shot deviates because the modular exponentiation is approximate.
    ///
    /// Table 5 reports this per modulus size; it falls as `n` grows.
    pub fn deviation_probability(&self) -> f64 {
        match self.modulus_bits {
            1024 => 0.0287,
            1536 => 0.0183,
            2048 => 0.0125,
            3072 => 0.0091,
            4096 => 0.0080,
            6144 => 0.0042,
            8192 => 0.0040,
            _ => 0.02,
        }
    }

    /// Expected shots: `(s + 1) / (1 - P_deviant) / 0.99`.
    ///
    /// The trailing factor accounts for classical post-processing failure (Ekera 2020).
    /// Dropping this multiplies through to the total cost, so it is modelled rather than
    /// assumed away (spec anti-pattern 7).
    pub fn expected_shots(&self) -> f64 {
        (self.ekera_hastad_s as f64 + 1.0) / (1.0 - self.deviation_probability()) / 0.99
    }
}

/// A subroutine's operation tally, per Table 3 of the source.
///
/// Additions, lookups and phaseups are carried in halves so that the fractional averages
/// in the source table are represented exactly rather than rounded.
#[derive(Debug, Clone, Copy)]
struct Tally {
    name: &'static str,
    iterations: u64,
    register_bits: u32,
    address_bits: u32,
    half_additions: u64,
    half_lookups: u64,
    half_phaseups: u64,
}

/// Toffoli cost of one table lookup or phaseup addressed by `a` bits.
///
/// Unary iteration, as charged by Gidney-Ekera 2019: `2^a`.
fn lookup_cost(address_bits: u32) -> u64 {
    1u64 << address_bits.min(62)
}

impl Tally {
    /// Toffoli cost of one iteration, in halves.
    fn half_cost_per_iteration(&self) -> u64 {
        let add = self.register_bits as u64;
        let look = lookup_cost(self.address_bits);
        self.half_additions * add + self.half_lookups * look + self.half_phaseups * look
    }
}

/// The subroutine tallies for a parameter set, following Table 3.
fn tallies(p: &ResidueParams) -> Vec<Tally> {
    let pc = p.prime_count();
    let lenm = p.len_m();
    let reg_l_m = p.prime_bits + lenm;
    let (w1_w, w3_w, w4_w) = (p.w1_windows(), p.w3_windows(), p.w4_windows());
    // `(W3 - 2)` appears in the source; clamp so small parameter sets stay well defined.
    let w3_minus_2 = w3_w.saturating_sub(2);

    vec![
        Tally {
            name: "loop1",
            iterations: (pc + 1) * w1_w,
            register_bits: reg_l_m,
            address_bits: p.w1,
            half_additions: 2,
            half_lookups: 2,
            half_phaseups: 0,
        },
        Tally {
            name: "loop2",
            iterations: pc * lenm as u64,
            register_bits: reg_l_m,
            address_bits: 0,
            half_additions: 4,
            half_lookups: 0,
            half_phaseups: 0,
        },
        Tally {
            name: "loop3_startup",
            iterations: pc,
            register_bits: p.prime_bits,
            address_bits: 2 * p.w3,
            half_additions: 0,
            half_lookups: 2,
            half_phaseups: 0,
        },
        Tally {
            name: "loop3_body",
            iterations: pc * w3_minus_2 * w3_w,
            register_bits: p.prime_bits,
            address_bits: p.w3,
            half_additions: 4,
            half_lookups: 2,
            half_phaseups: 0,
        },
        Tally {
            name: "loop4",
            iterations: pc * w4_w,
            register_bits: p.accumulator_bits,
            address_bits: p.w4,
            half_additions: 3,
            half_lookups: 5,
            half_phaseups: 2,
        },
        Tally {
            name: "unloop3_body",
            iterations: pc * w3_minus_2 * 2 * w3_w,
            register_bits: p.prime_bits,
            address_bits: p.w3,
            half_additions: 5,
            half_lookups: 3,
            half_phaseups: 2,
        },
        Tally {
            name: "unloop3_cleanup",
            iterations: pc,
            register_bits: p.prime_bits,
            address_bits: 2 * p.w3,
            half_additions: 0,
            half_lookups: 0,
            half_phaseups: 2,
        },
        Tally {
            name: "unloop2",
            iterations: pc * lenm as u64,
            register_bits: reg_l_m,
            address_bits: 0,
            half_additions: 4,
            half_lookups: 0,
            half_phaseups: 0,
        },
    ]
}

/// Emit the residue-arithmetic modular exponentiation for one shot.
pub fn emit_residue_modexp(b: &mut CircuitBuilder, p: &ResidueParams) {
    b.scope(
        format!(
            "residue_modexp[n={},s={},l={}]",
            p.modulus_bits, p.ekera_hastad_s, p.prime_bits
        ),
        |b| {
            for t in tallies(p) {
                // Emit in halves so fractional per-iteration tallies are exact: run the
                // body `2 * iterations` times at half cost each.
                let half = t.half_cost_per_iteration();
                if half == 0 || t.iterations == 0 {
                    continue;
                }
                b.repeat(t.name, t.iterations, |b| {
                    b.repeat("half_ops", half, |b| {
                        // Two half-operations make one Toffoli-equivalent.
                        b.and_compute();
                    });
                });
            }
        },
    );
}

/// Analytic Toffoli count for one shot, used only as a test oracle.
pub fn analytic_toffoli_per_shot(p: &ResidueParams) -> u128 {
    tallies(p)
        .iter()
        .map(|t| t.iterations as u128 * t.half_cost_per_iteration() as u128)
        .sum::<u128>()
        / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_register_is_about_half_n() {
        // The headline space claim: m ~ 0.5n against the 2019 construction's 3n.
        let p = ResidueParams::published(2048).unwrap();
        assert_eq!(p.input_qubits(), 1280, "m = ceil(2048/2 + 2048/8)");
        assert!((p.input_qubits() as f64 / 2048.0 - 0.625).abs() < 0.01);
    }

    #[test]
    fn derived_parameters_match_the_source_table() {
        let p = ResidueParams::published(2048).unwrap();
        assert_eq!(p.len_m(), 11);
        assert_eq!(p.w1_windows(), 214, "W1 = ceil(1280/6)");
        assert_eq!(p.w3_windows(), 7, "W3 = ceil(21/3)");
        assert_eq!(p.w4_windows(), 5, "W4 = ceil(21/5)");
    }

    #[test]
    fn peak_qubit_count_reproduces_table_five() {
        // Table 5 reports 1399 logical qubits for n=2048.
        let p = ResidueParams::published(2048).unwrap();
        let got = p.peak_logical_qubits() as f64;
        let rel = (got - 1399.0).abs() / 1399.0;
        assert!(rel < 0.05, "expected ~1399 logical qubits, got {got} ({rel:.3})");
    }

    #[test]
    fn expected_shots_reproduces_table_five() {
        let p = ResidueParams::published(2048).unwrap();
        let got = p.expected_shots();
        assert!((got - 9.2).abs() < 0.1, "expected 9.2 shots, got {got:.2}");
    }

    #[test]
    fn total_toffoli_reproduces_table_five() {
        // Table 5's Toffoli column is per factoring, i.e. across all expected shots.
        let p = ResidueParams::published(2048).unwrap();
        let total = analytic_toffoli_per_shot(&p) as f64 * p.expected_shots();
        let ratio = total / 6.5e9;
        assert!(
            ratio > 0.7 && ratio < 1.4,
            "expected ~6.5e9 Toffoli per factoring, got {total:.4e} (ratio {ratio:.3})"
        );
    }

    #[test]
    fn assembled_circuit_matches_the_oracle() {
        let p = ResidueParams::published(2048).unwrap();
        let mut b = CircuitBuilder::new("t");
        emit_residue_modexp(&mut b, &p);
        let got = b.finish().fold().unwrap().resources.total_toffoli().get();
        let want = analytic_toffoli_per_shot(&p) * 2; // emitted in halves
        assert_eq!(got, want);
    }

    #[test]
    fn uses_far_fewer_qubits_than_the_2019_construction() {
        let p = ResidueParams::published(2048).unwrap();
        // GE19: 3n + 0.002 n lg n ~ 6189.
        assert!(
            p.peak_logical_qubits() * 4 < 6189,
            "residue arithmetic should be a ~4x space saving"
        );
    }

    #[test]
    fn pays_for_space_with_gates() {
        // The tradeoff: fewer qubits, more Toffolis than GE19's 2.6e9.
        let p = ResidueParams::published(2048).unwrap();
        let total = analytic_toffoli_per_shot(&p) as f64 * p.expected_shots();
        assert!(total > 2.6e9, "residue arithmetic should cost more gates, got {total:.3e}");
    }

    #[test]
    fn scales_across_the_published_parameter_sets() {
        for (n, want) in [
            (1024u32, 1.1e9f64),
            (2048, 6.5e9),
            (4096, 4.0e10),
            (8192, 2.7e11),
        ] {
            let p = ResidueParams::published(n).unwrap();
            let got = analytic_toffoli_per_shot(&p) as f64 * p.expected_shots();
            let ratio = got / want;
            assert!(
                ratio > 0.5 && ratio < 2.0,
                "n={n}: expected {want:.2e}, got {got:.4e} (ratio {ratio:.3})"
            );
        }
    }

    #[test]
    fn qubit_counts_track_table_five_across_sizes() {
        for (n, want) in [(1024u32, 742u64), (2048, 1399), (4096, 2692), (8192, 5261)] {
            let p = ResidueParams::published(n).unwrap();
            let got = p.peak_logical_qubits();
            let rel = (got as f64 - want as f64).abs() / want as f64;
            assert!(rel < 0.10, "n={n}: expected {want}, got {got} ({rel:.3})");
        }
    }
}
