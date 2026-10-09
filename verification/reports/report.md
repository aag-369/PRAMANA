# PRAMANA reproduction report

Every figure below was produced by running PRAMANA's production estimation path.
Published values live only in `verification/golden/` and are never consulted by
that path (Law 1). Divergences are diagnosed, not tuned away (spec §25).

**9 passed, 1 failed, 1 probes, 3 pending, 14 targets**

| Target | Quantity | Computed | Published | Ratio | Criterion | Result |
|---|---|---:|---:|---:|---|---|
| `schrottenloher_2026_secp256k1_gate_optimised` | logical_qubits | 1.4540e3 | 1.4620e3 | 0.995 | within 5% (actual 0.5%) | PASS |
| `schrottenloher_2026_secp256k1_gate_optimised` | toffoli_count | 5.2546e7 | 5.7620e7 | 0.912 | within 30% (actual 8.8%) | PASS |
| `schrottenloher_2026_secp256k1_space_optimised` | logical_qubits | 1.1980e3 | 1.2080e3 | 0.992 | within 5% (actual 0.8%) | PASS |
| `schrottenloher_2026_secp256k1_space_optimised` | toffoli_count | 5.8934e7 | 7.2430e7 | 0.814 | within 30% (actual 18.6%) | PASS |
| `luo_2026_ecdlp256_width_record` | - | - | - | - | - | PENDING |
| `ecdlp_data_register_floor` | - | - | - | - | - | PENDING |
| `gidney_2025_rsa2048_logical` | logical_qubits | 1.4320e3 | 1.3990e3 | 1.024 | within 10% (actual 2.4%) | PASS |
| `gidney_2025_rsa2048_logical` | toffoli_count | 5.6913e9 | 6.5000e9 | 0.876 | within 40% (actual 12.4%) | PASS |
| `gidney_2025_rsa2048` | physical_qubits | 9.2626e5 | 1.0000e6 | 0.926 | computed < published | PASS |
| `gidney_2025_rsa2048` | wall_clock_days | 4.0182e0 | 7.0000e0 | 0.574 | computed < published | PASS |
| `gidney_2025_rsa2048_conservative_curve` | physical_qubits | 1.2326e6 | 1.0000e6 | 1.233 | computed < published | FAIL |
| `chevignard_fouque_schrottenloher_2024_rsa2048` | - | - | - | - | - | PENDING |
| `gidney_ekera_2019_rsa2048_logical` | logical_qubits | 6.1830e3 | 6.1890e3 | 0.999 | within 5% (actual 0.1%) | PASS |
| `gidney_ekera_2019_rsa2048_logical` | toffoli_count | 2.6429e9 | 2.6242e9 | 1.007 | within 25% (actual 0.7%) | PASS |
| `gidney_ekera_2019_rsa2048_physical` | physical_qubits | 2.4201e7 | 2.0000e7 | 1.210 | within 40% (actual 21.0%) | PASS |
| `gidney_ekera_2019_rsa2048_physical` | wall_clock_hours | 7.3414e0 | 8.0000e0 | 0.918 | within 40% (actual 8.2%) | PASS |
| `gouzien_2023_ecdlp256_cat` | physical_qubits | 1.1780e5 | 1.2613e5 | 0.934 | within 40% (actual 6.6%) | PASS |
| `gouzien_2023_ecdlp256_cat` | wall_clock_hours | 8.3513e0 | 9.0000e0 | 0.928 | within 50% (actual 7.2%) | PASS |
| `roetteler_2017_p256_qubits` | logical_qubits | 2.3300e3 | 2.3300e3 | 1.000 | within 1% (actual 0.0%) | PASS |
| `roetteler_2017_p256_leading_coefficient` | leading_coefficient | 4.4800e2 | 4.4800e2 | 1.000 | within 1% (actual 0.0%) | PASS |
| `roetteler_2017_p256_absolute` | toffoli_count | 6.0130e10 | 1.2870e11 | 0.467 | within 25% (actual 53.3%) | FAIL |

## Sources

- `schrottenloher_2026_secp256k1_gate_optimised` - Optimized Point Addition Circuits for Elliptic Curve Discrete Logarithms (arXiv:2606.02235, 2026)
- `schrottenloher_2026_secp256k1_space_optimised` - Optimized Point Addition Circuits for Elliptic Curve Discrete Logarithms (arXiv:2606.02235, 2026)
- `luo_2026_ecdlp256_width_record` - Quantum Algorithm for Elliptic Curve Discrete Logarithms with Space-Efficient Point Addition (arXiv:2607.13816, 2026)
- `ecdlp_data_register_floor` - Shor's discrete logarithm quantum algorithm for elliptic curves (arXiv:quant-ph/0301141, 2003)
- `gidney_2025_rsa2048_logical` - How to factor 2048 bit RSA integers with less than a million noisy qubits (arXiv:2505.15917, 2025)
- `gidney_2025_rsa2048` - How to factor 2048 bit RSA integers with less than a million noisy qubits (arXiv:2505.15917, 2025)
- `gidney_2025_rsa2048_conservative_curve` - How to factor 2048 bit RSA integers with less than a million noisy qubits (arXiv:2505.15917, 2025)
- `chevignard_fouque_schrottenloher_2024_rsa2048` - Reducing the number of qubits in quantum factoring (ePrint 2024/1852, 2024)
- `gidney_ekera_2019_rsa2048_logical` - How to factor 2048 bit RSA integers in 8 hours using 20 million noisy qubits (arXiv:1905.09749, 2019)
- `gidney_ekera_2019_rsa2048_physical` - How to factor 2048 bit RSA integers in 8 hours using 20 million noisy qubits (arXiv:1905.09749, 2019)
- `gouzien_2023_ecdlp256_cat` - Performance analysis of a repetition cat code architecture: computing 256-bit elliptic curve logarithm in 9 hours with 126133 cat qubits (arXiv:2302.06639, 2023)
- `roetteler_2017_p256_qubits` - Quantum resource estimates for computing elliptic curve discrete logarithms (arXiv:1706.06752, 2017)
- `roetteler_2017_p256_leading_coefficient` - Quantum resource estimates for computing elliptic curve discrete logarithms (arXiv:1706.06752, 2017)
- `roetteler_2017_p256_absolute` - Quantum resource estimates for computing elliptic curve discrete logarithms (arXiv:1706.06752, 2017)

## Diagnoses

### `schrottenloher_2026_secp256k1_gate_optimised` (PASS)

Built from the split extended Euclidean algorithm: 401 iterations per in-place multiplication over a 670-bit compressed garbage vector, two multiplications per point addition, 28 windowed point additions. The 2017 Roetteler construction costs roughly 1144x more gates for the same curve.

### `schrottenloher_2026_secp256k1_space_optimised` (PASS)

Built from the split extended Euclidean algorithm: 401 iterations per in-place multiplication over a 670-bit compressed garbage vector, two multiplications per point addition, 28 windowed point additions. The 2017 Roetteler construction costs roughly 1020x more gates for the same curve.

### `luo_2026_ecdlp256_width_record` (PENDING)

no pipeline registered for 'shor_ecdlp_luo_2026'

### `ecdlp_data_register_floor` (PENDING)

no pipeline registered for 'shor_ecdlp_any'

### `gidney_2025_rsa2048` (PASS)

Residue-arithmetic circuit: 1432 logical qubits (89% of them the idle input register), 6.182e8 Toffoli per shot over 9.2 expected shots. Costed under the arXiv:2505.15917 Fig. 6 simulated curve giving d=25. Machine: 5.5040e5 cold + 2.0550e5 hot + 1.7035e5 compute = 9.2626e5 physical qubits, 10.48 hours per shot, FactoryLimited.

### `gidney_2025_rsa2048_conservative_curve` (PROBE)

Residue-arithmetic circuit: 1432 logical qubits (89% of them the idle input register), 6.182e8 Toffoli per shot over 9.2 expected shots. Costed under the generic analytic fit giving d=29. Machine: 7.3216e5 cold + 2.7360e5 hot + 2.2680e5 compute = 1.2326e6 physical qubits, 10.48 hours per shot, FactoryLimited.

### `chevignard_fouque_schrottenloher_2024_rsa2048` (PENDING)

no pipeline registered for 'shor_factoring_cfs'

### `gidney_ekera_2019_rsa2048_physical` (PASS)

Full chain, no published figure consulted: modulus size -> circuit (6183 logical qubits, 2.643e9 Toffoli) -> surface code at d=31 -> 2.4201e7 physical qubits in 7.34 hours. Limiting factor: ReactionLimited. Qubits split 49.1% data, 49.1% routing, 1.8% magic state factories.

### `gouzien_2023_ecdlp256_cat` (PASS)

Repetition distance d=13 solved from the source's own noise parameters (loss ratio 1e-5, n_bar 19), giving 25 physical cat qubits per logical qubit against the surface code's 337. The advantage is structural: a repetition code is linear in distance where a surface code is quadratic. Note PRAMANA's ECDLP Toffoli count omits a sub-leading term (see the roetteler_2017_p256_absolute diagnosis), so this runtime is correspondingly optimistic.

### `roetteler_2017_p256_absolute` (FAIL)

PRAMANA computes 6.0130e10 Toffoli gates against the published 1.2875e11, a ratio of 0.467.

**The leading term is reproduced exactly.** The fitted coefficient is 448.0 against the published 448, and that 448 is never written in PRAMANA's source: it emerges from composing 4 inversions, 2 squarings and 4 multiplications into a point addition (`4*32 + 2*16 + 4*16 = 224`) and iterating `2n` times.

**The entire discrepancy is the sub-leading term.** The source's expression is `448 n^3 log2 n + 4090 n^3`. At n=256 the leading term is 6.0130e10 and the sub-leading term is 6.8619e10 — the two are comparable, which is why the ratio sits near one half rather than near one.

That `+4090 n^3` is not derivable from the published subroutine table. The source obtained it by regression over its own simulated circuits ("we then again perform a regression to determine the next coefficient"), and Table 1 gives sub-leading terms for only some routines, with none for the modular inversion that dominates. Reproducing it would require implementing the group law at gate level rather than at the level of subroutine multiplicities.

It would be trivial to close this by inserting a `+511 n^2` term into the inversion cost, since `4 * 511 = 2044 ~ 2045`. That is precisely the fit-to-target this harness exists to prevent, and it is not done. Direction of error: PRAMANA is **below** the published figure, i.e. optimistic about the attacker's cost.

