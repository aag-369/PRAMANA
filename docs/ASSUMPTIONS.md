# PRAMANA assumptions

Every default the system uses, its value, its justification, and its source. This is the
canonical statement: where a number appears here and in code, this document is what the
code is answerable to.

Assumptions are classified by how much weight they can bear:

| Class | Meaning |
|---|---|
| **Derived** | Follows from the construction. Changing it would be a bug, not a policy choice. |
| **Sourced** | Taken from a named publication, cited. Legitimate to import; the source is answerable for it. |
| **Calibrated** | Chosen so that a model reproduces a published figure. Honest, but the reproduction is then not independent evidence for that number. |
| **Judgement** | A defensible choice with no decisive source. These are where a reviewer should push hardest. |

A reader in a hurry should read the **Judgement** rows and §9.

---

## 1. Circuit synthesis

### 1.1 Gate decomposition (`pramana-units/src/gates.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Textbook Toffoli | 7 T | Sourced | Standard decomposition |
| Catalysed Toffoli | 4 T | Sourced | Standard |
| Temporary-AND | 4 T to compute, 0 to uncompute | Sourced | Gidney, *Halving the cost of quantum addition*, Quantum 2:74 |
| Rotation synthesis | `3 · log₂(1/ε)` T gates | Sourced | Ross & Selinger, arXiv:1403.2975 |

### 1.2 Arithmetic (`arith/adder.rs`, `arith/lookup.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Gidney adder | `n − 1` AND per `n`-bit addition | Derived | Emerges from the assembled carry chain |
| Cuccaro adder as charged by GE19 | `2n` Toffoli per `n`-bit addition | Sourced | arXiv:1905.09749 §2.5, quoted verbatim |
| CDKM controlled addition | `3n` Toffoli | Sourced | arXiv:2606.02235 §4 |
| Gidney controlled addition | `2n` Toffoli, needs `~n` ancilla | Sourced | Same |
| Gidney constant adder, dirty ancilla | `3n` Toffoli | Sourced | arXiv:2507.23079 via arXiv:2606.02235 |
| QROM unary iteration | `L − 1` AND for `L` entries | Sourced | Babbush et al., PRX 8:041015 |
| QROM measurement uncomputation | `2√L` AND | Sourced | Berry et al. |
| Coset padding | `⌈log₂ n⌉ + slack`, slack = 2 | Judgement | Zalka's coset representation; the slack is a choice |

**Note on lookup uncomputation.** The `O(√L)` uncomputation of a *table lookup* is charged
as real Toffoli gates. This differs from the temporary-AND uncomputation inside an *adder*,
which genuinely costs zero T gates. Conflating the two undercounts every windowed
multiplication, and an early version of this code did exactly that.

### 1.3 Shor factoring, GE19 (`shor_factoring.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Exponent length | `1.5n` | Sourced | arXiv:1905.09749 §2.2: "total exponent length is hence ne = 3m = 1.5n + O(1)" |
| Additions per multiplication | `2n / cmul` | Sourced | §2.5, quoted verbatim |
| Expected runs | 3 | Judgement | Ekera-Hastad needs several runs; the exact number is not pinned by the source |
| Window search bound | `w ≤ 12` | Judgement | Generous; the optimiser lands at 5-6 unaided |

### 1.4 Shor factoring, G25 (`arith/residue.rs`)

All parameters are **sourced** from Table 5 of arXiv:2505.15917 (`s`, `ℓ`, `w1`, `w3`, `w4`,
`f`, `m`) and its Table 3 operation tallies. These are *inputs* the source obtained by grid
scan, not results.

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Input register | `m = ⌈n/2 + n/s⌉` | Sourced | Table 2 |
| Prime count | `\|P\| ≈ nm / (ℓ·w1)` | Sourced | Table 2 |
| Expected shots | `(s+1) / (1 − P_deviant) / 0.99` | Sourced | Table 5 caption; the 0.99 is post-processing failure |
| Lookup / phaseup cost | `2^(address bits)` | Sourced | Unary iteration |

### 1.5 ECDLP 2017, Roetteler (`arith/ec_point.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Modular constant addition | `16 n log₂ n` Toffoli | Sourced | Table 1 of arXiv:1706.06752. **The single imported primitive**; everything above it composes |
| Point addition composition | 4 inversions + 2 squarings + 4 multiplications | Sourced | Fig. 11 discussion |
| Point additions | `2n` | Sourced | Same |
| Incrementer levels | `⌈log₂ n⌉` | Judgement | Real carry-tree depth. Differs from the source's continuous `log₂ n` by up to 11% at P-521 |

### 1.6 ECDLP 2026, Schrottenloher (`arith/eea.rs`, `shor_ecdlp_2026.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Bits removed per EEA iteration | `log₂(8/3) ≈ 1.415` | Derived | `uv` shrinks by `1/2·1/2 + 1/2·1/4 = 3/8` per step |
| Iterations | `2n / log₂(8/3) ≈ 1.413n` | Derived | From the above |
| Iteration slack | 4 standard deviations at `0.6√n` | Sourced | arXiv:2606.02235 §3.1, chosen for 10,000-input success |
| Register padding | `2.3√n` | Sourced | Same, experimentally determined |
| Garbage compression | 3 iterations into 5 bits, 5 Toffoli | Derived | `3³ = 27 < 2⁵` |
| Approximate comparison | 45 most significant bits | Sourced | Source uses 40-50 |
| Pseudo-Mersenne reduction | 40 low bits | Judgement | Within the source's stated regime; the exact width sets the success probability |
| Window | `w = 16` | Sourced | §2, following Babbush et al. |
| Removable additions | 4 | Sourced | §2, first becomes a lookup, last three go to post-processing |
| In-place multiplication share | 90% of point addition | Sourced | Table 3 |

### 1.7 Grover (`grover_symmetric.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| AES S-box | 32 AND gates | Sourced | Boyar & Peralta multiplicative complexity |
| Grover iterations | `(π/4)·2^(k/2)` | Derived | Standard |
| Oracle calls per iteration | 2 | Derived | Compute and uncompute |
| Blocks to pin the key | `⌈k/128⌉` | Derived | One block leaves `2^(k−128)` spurious keys |
| Parallel speedup | `√S` | Sourced | Standard; the reason depth limits cannot be bought off |

---

## 2. Error correction

### 2.1 Surface code (`pramana-qec/src/surface.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Logical error rate | `A (p/p_th)^⌊(d+1)/2⌋` | Sourced | Fowler et al., PRA 86:032324 |
| Prefactor `A` | 0.1 | **Judgement** | Published fits span roughly 0.01–0.1. See §9.1 |
| Threshold `p_th` | 0.01 | Judgement | Circuit-level depolarising on a square grid |
| Rotated patch | `2d² − 1` physical qubits | Derived | `d²` data + `d²−1` measure |
| Layout: compact | `1.5n + 3` tiles | Sourced | Litinski, arXiv:1808.02892 |
| Layout: intermediate | `2n + 4` tiles | Sourced | Same. **PRAMANA's default** |
| Layout: fast | `2n + √(8n) + 1` tiles | Sourced | Same |
| Maximum distance searched | 201 | Judgement | Guardrail; a solver reaching it signals a problem |

### 2.2 Magic states (`magic.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| 15-to-1 error scaling | `35 ε³` per level | Sourced | Bravyi-Kitaev |
| Two-level factory footprint | 12 patches | Sourced | Litinski block accounting |
| Two-level factory latency | `6d` cycles | Sourced | Same |
| Cultivation error | interpolated from `2e-9 @ p=1e-3` and `4e-11 @ p=5e-4` | Sourced | Gidney, Shutty & Jones, arXiv:2409.17595 |
| Cultivation footprint | 2 patches | Judgement | Fits within a patch plus an escape region |
| Cultivation retry factor | 2 | Judgement | Cultivation is post-selected; the rate is not pinned |

### 2.3 Yoked storage and the Gidney 2025 layout

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Yoke idle space fraction | 0.5 | Sourced | arXiv:2312.04522, roughly halves memory |
| Yoke density gain | `1352/430 ≈ 3.14` | Sourced | arXiv:2505.15917 §3.2, "roughly triple the density" |
| Hot patch | `2(d+1)²` | Sourced | Same |
| Target logical error | `1e-15` per patch-round | Sourced | Same |
| Factories | 6 | Sourced | Same |
| Patches per factory | 12 (3×4) | Sourced | Same |
| Compute routing factor | `126/72` | Sourced | Same: a 7×18 region hosting six 3×4 factories |
| Toffoli cycles per factory | 366 | **Calibrated** | Set so six factories give the source's 12-hour shot. See §9.2 |

### 2.4 Cat qubits (`cat.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Physical per logical | `2d − 1` | Derived | 1-D repetition code: `d` data + `d−1` ancilla |
| Phase error | `n̄ · κ₁/κ₂` | Judgement | Linear model; the coefficient is set to 1 |
| Bit-flip suppression | `exp(−2n̄)` | Sourced | Standard cat-qubit scaling |
| Repetition threshold | 0.1 | **Judgement** | Not pinned by the source. See §9.3 |
| Reference parameters | `κ₁/κ₂ = 1e-5`, 500 ns cycle, `n̄ = 19` | Sourced | Gouzien et al., arXiv:2302.06639 |

### 2.5 qLDPC (`qldpc.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| `n`, `k`, check weight | computed from the construction | Derived | GF(2) rank of `H_X`, `H_Z` |
| Distance | from the source | Sourced | Computing qLDPC distance is NP-hard in general |
| Memory qubits per logical | `2n/k` | Derived | Data plus checks |
| Ancilla ratio for computation | 3.0 | **Judgement** | Tour-de-gross style; the least defensible number here. See §9.4 |
| Cycle penalty | 10× | **Judgement** | Same |

### 2.6 Neutral atoms (`neutral_atom.rs`)

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Rounds per logical operation | 1 (`O(1)`) | Sourced | Cain et al., correlated decoding |
| Tiles per logical qubit | 1.25 | Judgement | Movable qubits need less corridor than a square grid |
| Atom loss per move | `1e-3` | Judgement | Surfaced as an unmodelled optimism rather than corrected for |
| Cycle and move time | 200 µs each | Judgement | Order-of-magnitude for tweezer arrays |

---

## 3. Hardware trajectories

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Fitted quantity | **verified logical qubits** | Judgement | Physical-to-logical spans 2:1 to 105:1; a physical fit misranks machines |
| Admissible data | delivered + independently verified only | Judgement | Announced targets and press-release counts excluded |
| Frontier reduction | best result per year | Judgement | An attacker uses the best machine, not the average |
| Roadmap blending | targets shifted by the mean slip | **Judgement** | See §9.5 |
| Minimum fit points | 3 | Judgement | Below this the fit is refused |
| Censored slip penalty | 3 years per abandoned milestone | **Judgement** | Abandoned milestones are right-censored; ignoring them biases the prior optimistic |
| Extrapolation horizon | 25 years | Judgement | Beyond it a warning is emitted |

---

## 4. Risk engine

| Assumption | Value | Class | Basis |
|---|---|---|---|
| Improvement series | `data/improvement_series.toml` | Sourced | Every point cited |
| Rate fitting | first to last observation, per family and metric | Judgement | Two-point fit; more points would support a regression |
| Rate uncertainty | 50% of the rate | Judgement | Wide, reflecting that catch-up rates should not be expected to persist |
| ECDLP width floor | `2n` | Sourced | Proos-Zalka: a point is two field coordinates |
| ECDLP gate floor | `(27/4)·n³/log₂ n` | **Judgement** | A quarter of the best gate-optimised constant. See §9.6 |
| Factoring width floor | `0.25n` | Judgement | Residue arithmetic is already at `0.5n` |
| Attacker budget, nation state | 90 days median, σ = 1.2 | Judgement | Log-normal |
| Attacker budget, opportunistic | 1 day median, σ = 0.8 | Judgement | Same |
| Architecture prior | uniform | Judgement | The user can pin one; uniform is a stated default, not a belief |
| Total error budget | 0.05 | Judgement | Per attack run |
| Monte Carlo samples | 4,000 interactive | Judgement | Median standard error well under a year |
| Exposed threshold | `P > 0.5` | Judgement | Where "already exposed" is asserted |

---

## 5. Crypto agility (`backend/app/services/agility.py`)

Dimension weights sum to 1.0 and are all **judgement**, aligned to the vocabulary of
NIST CSWP 39upd1:

| Dimension | Weight |
|---|---|
| Algorithm negotiability | 0.16 |
| Configuration surface | 0.14 |
| Lifecycle automation | 0.16 |
| Dependency depth | 0.12 |
| Library support | 0.14 |
| Hardware binding | 0.12 |
| Vendor dependency | 0.08 |
| Size headroom | 0.08 |

Score-to-migration-time mapping (judgement, user-overridable): 90+ → 0.25 yr; 75+ → 0.5;
60+ → 1.0; 40+ → 2.0; 20+ → 3.0; below → 5.0.

A certificate lifetime of 100 days or less is read as evidence of working automation. That
is a heuristic, and a long-lived certificate rotated by hand-run scripts would be scored
too harshly.

---

## 6. Standards (`backend/app/services/standards.py`)

All **sourced**, current to 2026-08-29. Status is tracked because PRAMANA will not
recommend a pending standard: FIPS 206 (FN-DSA) is in NIST clearance and HQC was selected
in March 2025 with a draft expected 2026, so both appear only as marked alternatives.

Key dates: NIST IR 8547 deprecation 2030 and disallowance 2035; CNSA 2.0 procurement gate
1 January 2027, networking exclusive 2030, legacy 2033.

**NIST IR 8547 remains a draft.** Sources conflict on whether a final has issued. It is
cited as a draft and should be re-verified before any publication relies on it.

---

## 7. What is *not* modelled

Stating these plainly, because an omission a reader discovers themselves reads as a defect
while a declared one reads as scope.

- **Physical-layer noise beyond a uniform depolarising rate.** No leakage, no crosstalk, no
  correlated noise, no drift.
- **Classical control cost.** Decoder throughput, wiring, cryogenic budget. The cat-qubit
  model notes that a mode carries substantial microwave hardware, but does not cost it.
- **Cost in money.** Everything is qubits and seconds.
- **Attacker economics beyond a time budget.** No modelling of whether an attack is worth
  mounting.
- **Interception probability.** PRAMANA computes whether traffic *can* be decrypted, not
  whether it was captured. Harvest-now-decrypt-later feasibility is a separate question.
- **Atom loss and reloading**, flagged in the estimate rather than charged.
- **Sub-leading terms** in the Roetteler ECDLP construction, deliberately (see STATUS.md).
- **Post-quantum algorithms' own quantum vulnerability.** ML-KEM and ML-DSA are treated as
  targets, not as things to be attacked.

---

## 8. Where each assumption lives

| Area | File |
|---|---|
| Units and decomposition | `crates/pramana-units/src/gates.rs` |
| Adders, lookups, windows | `crates/pramana-circuit/src/arith/` |
| Split EEA | `crates/pramana-circuit/src/arith/eea.rs` |
| Surface code | `crates/pramana-qec/src/surface.rs` |
| Magic states | `crates/pramana-qec/src/magic.rs` |
| Gidney 2025 layout | `crates/pramana-qec/src/gidney2025.rs` |
| Cat, qLDPC, neutral atom | `crates/pramana-qec/{cat,qldpc,neutral_atom}.rs` |
| Trajectories and slip | `crates/pramana-hardware/src/{fit,trajectory}.rs` |
| Improvement and floors | `crates/pramana-risk/src/improvement.rs`, `data/improvement_series.toml` |
| Roadmap data | `data/roadmaps/*.toml` |
| Agility | `backend/app/services/agility.py` |
| Standards | `backend/app/services/standards.py` |

---

## 9. The assumptions that carry the most weight

Where a reviewer should push hardest, and what happens if each is wrong.

### 9.1 Surface-code prefactor `A = 0.1`

Published fits span roughly 0.01 to 0.1, a full decade. The choice moves the selected code
distance by about two, which moves physical qubit counts by roughly 15%. This is exactly why
`LogicalErrorModel` is a pluggable, cited choice in the Gidney 2025 layout rather than a
constant: under the generic fit that target needs 1.23e6 physical qubits, and under the
source's own simulated curve 9.26e5 — across the million-qubit line. **The headline claim of
that paper is sensitive to two units of code distance, and PRAMANA reports both.**

### 9.2 Toffoli cycles per factory = 366

Calibrated so six factories reproduce the source's stated 12-hour shot. It is the only
number in the Gidney 2025 model fitted to its target rather than derived, and it therefore
weakens the runtime reproduction into a consistency check. The qubit count, which is the
headline, does not depend on it.

### 9.3 Repetition-code threshold = 0.1

Not pinned by Gouzien et al. It sets the distance and therefore the cat-qubit count
directly. The reproduction lands within 6.6% of the published figure, which is evidence the
value is reasonable — but the reproduction is not independent of it.

### 9.4 qLDPC computation overhead

The ancilla ratio of 3.0 and cycle penalty of 10× are the least defensible numbers in the
system. No published estimate covers logical computation on bivariate bicycle codes at
cryptographic scale. PRAMANA's response is to report memory-only and with-computation
figures separately and always, and to warn on the former — quantifying that memory-only
accounting overstates the advantage roughly threefold. **A reader should treat the qLDPC
column as an order-of-magnitude statement.**

### 9.5 Blending roadmap targets into the capability fit

Verified demonstrations span only 2024–2026, and a history-only fit extrapolates to
absurdity: 5.08× growth per year and 190 million logical qubits by 2035. Blending
slip-corrected vendor targets tames it to 1.51×. But it imports vendor optimism, corrected
only by a slip prior fitted on five observations dominated by one company's physical-qubit
milestones. **This is the weakest link between the model and reality**, and the honest
summary is that the break-year distribution's *centre* is roughly as trustworthy as vendor
roadmaps are.

### 9.6 The ECDLP gate floor

Set at a quarter of the best gate-optimised constant. An earlier version derived it from the
*width*-optimised paper's constant and produced a floor above an already-achieved count —
incoherent, and caught by a monotonicity test. The lesson generalises: the 2026 results
occupy different points on a Pareto frontier and their formulas are not interchangeable.

### 9.7 The improvement rate persisting

The bounded-decay model presumes the historically fitted rate continues. Because width
improves quickly and decays toward a floor, two constructions differing fourfold today
converge within about fifteen years, so far-future break years depend more on the floor than
on today's width. If improvement stalls, PRAMANA's break years are too early; if a new
technique appears, too late. The rate uncertainty is set wide for this reason, but the
functional form is still an assumption.
