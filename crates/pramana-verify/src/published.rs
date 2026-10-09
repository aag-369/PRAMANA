//! Published figures, in the one crate permitted to hold them.
//!
//! Law 1 confines published results to `verification/golden/` and to this crate, which
//! consumes them. The closed forms below are here rather than in the estimation crates for
//! exactly that reason: a function called `published_toffoli` living in the crate that also
//! computes Toffoli counts is indistinguishable, to a reader or a static check, from a
//! lookup table the estimation path might read.
//!
//! These are used only by the discrepancy diagnostics, which need to evaluate a source's
//! closed form across a sweep of problem sizes rather than at the single size a golden file
//! pins.

/// Gidney-Ekera 2019 logical qubit count: `3n + 0.002 n lg n`.
///
/// arXiv:1905.09749, abstract circuit model excluding distillation and routing.
pub fn ge19_logical_qubits(n: u32) -> f64 {
    let nf = n as f64;
    3.0 * nf + 0.002 * nf * nf.log2()
}

/// Gidney-Ekera 2019 Toffoli count: `0.3 n^3 + 0.0005 n^3 lg n`.
pub fn ge19_toffoli(n: u32) -> f64 {
    let nf = n as f64;
    0.3 * nf.powi(3) + 0.0005 * nf.powi(3) * nf.log2()
}

/// Roetteler et al. 2017 logical qubit count: `9n + 2 ceil(lg n) + 10`.
///
/// arXiv:1706.06752, section 5.
pub fn roetteler_logical_qubits(n: u32) -> f64 {
    let ceil_log2 = if n <= 1 { 0 } else { n.next_power_of_two().trailing_zeros() };
    9.0 * n as f64 + 2.0 * ceil_log2 as f64 + 10.0
}

/// Roetteler et al. 2017 Toffoli count: `448 n^3 lg n + 4090 n^3`.
///
/// The leading coefficient decomposes as `2 * (4*32 + 2*16 + 4*16)`: four inversions, two
/// squarings and four multiplications per point addition, iterated `2n` times.
pub fn roetteler_toffoli(n: u32) -> f64 {
    let nf = n as f64;
    448.0 * nf.powi(3) * nf.log2() + 4090.0 * nf.powi(3)
}

/// Leading coefficient of the Roetteler `n^3 lg n` term.
pub const ROETTELER_LEADING_COEFF: f64 = 448.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_forms_match_their_headline_values() {
        // RSA-2048: about 6189 logical qubits and 2.6 billion Toffoli gates.
        assert!((ge19_logical_qubits(2048) - 6189.0).abs() < 5.0);
        assert!((ge19_toffoli(2048) / 2.6242e9 - 1.0).abs() < 0.01);
        // P-256: 2330 logical qubits and 1.287e11 Toffoli gates.
        assert_eq!(roetteler_logical_qubits(256), 2330.0);
        assert!((roetteler_toffoli(256) / 1.287e11 - 1.0).abs() < 0.01);
    }
}
