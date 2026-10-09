# PRAMANA build status

Last updated: 2026-08-28. Toolchain: Rust 1.75 (see note below). Phases complete: **P0-P9, the specification's full build order**, plus the 2026 ECDLP constructions. 267 tests passing (249 Rust, 18 Python), and every mechanically checkable law now enforced in CI.

## Completed

### P0 — workspace and `pramana-units`

Typed quantities enforcing spec Law 6. `LogicalQubits`, `PhysicalQubits`, `ToffoliCount`,
`TCount`, `CliffordCount`, `MeasurementCount`, `CodeCycles`, `Seconds`, `QubitRounds`,
plus validated `ErrorRate` and `Distance`.

Design decisions worth recording:

- **Incompatible arithmetic is a compile error**, verified by three `compile_fail`
  doctests. This is the enforcement mechanism for Law 6, not documentation of an
  intention.
- **`DecompositionStrategy` stores its fraction in per-mille, not `f64`.** The strategy
  is part of the memoisation key for circuit costing (spec §13.2), and floating-point
  cache keys are a correctness hazard. Quantisation makes the type `Eq + Hash`.
- **Overflow panics rather than wrapping.** A silently wrapped resource count is worse
  than a crash. Saturating variants are available where genuinely wanted.
- **`ErrorRate::at_least_once_in` uses the exact complement `1-(1-p)^n`**, not the union
  bound. The union bound is retained separately because several published estimates use
  it, and reproductions must be like-for-like.

### P1 — `pramana-circuit`

Circuit IR with named scopes, allocation high-water tracking, and a repetition node;
reversible adders; QROM and select-swap (QROAM) table lookup with a cost optimiser;
windowed modular exponentiation with a window-size optimiser.

**On synthesis versus formula evaluation.** The spec demands that counts emerge from
assembly rather than being read off a formula. A literal reading is intractable: an
RSA-2048 modular exponentiation contains ~10^9 Toffoli gates. The IR therefore provides
`Node::Repeat`, which represents a genuinely assembled subcircuit executed a
structurally-determined number of times. Leaf subcircuits are built gate by gate; only
repetition is folded. Repeat bodies must be allocation-balanced so peak qubit usage stays
well-defined. The property that matters is preserved: changing `n` changes the assembled
body, the optimiser's window choice, and the allocation profile. Nothing is memorised.

## Two bugs found and fixed during P1

Recorded because both are the kind of error the spec exists to prevent.

1. **`coset_padding` computed bit-length instead of `ceil(log2 n)`**, differing by one at
   every power of two — precisely where RSA and ECC key sizes live.

2. **QROM uncomputation was charged as free.** The temporary-AND uncompute inside an
   *adder* genuinely costs zero T gates (measurement plus Clifford fixup). The
   `O(sqrt(L))` measurement-based uncomputation of a *table lookup* is a different thing:
   a real circuit built from real AND gates. Emitting it as free undercounted every
   windowed multiplication in the circuit. A regression test now pins the distinction.

## P2 — attack pipelines and the reproduction harness

`AttackCircuit` trait, `ShorFactoringGE19` pipeline, typed `CryptoTarget` /
`SynthesisError` / `ValidityRegime`, an explicit `RepetitionModel`, golden files with full
provenance, and the `pramana-verify` reproduction harness.

### Published anchors, retrieved from source

Fetched from the papers rather than recalled (Law 12):

| Construction | RSA-2048 Toffoli | RSA-2048 logical qubits | Source |
|---|---|---|---|
| Gidney–Ekerå 2019 | 3e9 (`0.3n^3 + 0.0005 n^3 lg n`) | ~6189 (`3n + 0.002 n lg n`) | arXiv:1905.09749 |
| CFS 2024 | 2e12 | "a bit more than 0.5n" | ePrint 2024/1852, via arXiv:2505.15917 §1 |
| Gidney 2025 | ~1e10 (>100x below CFS24) | ~0.5n | arXiv:2505.15917 |

### Reproduction achieved: logical qubit count

**PRAMANA reproduces the GE19 logical qubit count to within 0.1%** — 6183 computed
against 6189 published — and to within 0.7% across n from 1024 to 4096.

This came from fixing a double-counting error. GE19's `3n` already includes the adder's
carry workspace as one of its three registers. PRAMANA holds two registers as persistent
data and *measures* the workspace from the assembled circuit's peak ancilla; adding a
hardcoded third register on top double-counts it. The agreement is therefore not a fit:
the third register is derived, not assumed.

### Structural correction: multiplication requires two passes

Reversible multiplication cannot be done in place. A windowed modular multiplication
accumulates into a fresh register, leaving the source register occupied by the old value;
clearing it reversibly requires a second windowed multiplication by the precomputed
modular inverse. Emitting only one pass understates the whole attack by exactly 2x.

## Verification signal obtained

The window optimiser, searching from scratch with no reference to any paper, selects
**w_e = w_m = 5 for n = 2048** — the same small-window regime the published Gidney–Ekerå
construction settles on. Independent recovery of the published operating point is
evidence the cost structure is modelled correctly.

Measured output of `make synthesise`:

| n | w_e | w_m | Toffoli | peak ancilla |
|---|---|---|---|---|
| 512 | 4 | 4 | 2.71e7 | 523 |
| 1024 | 5 | 4 | 1.69e8 | 1036 |
| 2048 | 5 | 5 | 1.07e9 | 2061 |
| 3072 | 5 | 5 | 3.17e9 | 3086 |
| 4096 | 5 | 5 | 7.00e9 | 4110 |

Cost scales as ~n^3 as theory requires (test asserts a 6–11x ratio for a doubling of n).

## The 3.55x gap: closed, by reading the paper

The Toffoli divergence reported in the previous iteration is **resolved**. RSA-2048 now
computes **2.6429e9 Toffoli against the published 2.6242e9 — 0.7% error.**

Nothing was tuned. Each correction traces to an explicit sentence in arXiv:1905.09749:

| Correction | Source | Worth |
|---|---|---|
| Exponent length `ne = 1.5n` | §2.2: "The total exponent length is hence ne = 3m = 1.5n + O(1)" | confirmed, no change |
| `2n/cmul` additions per multiplication, not `n/cmul` | §2.5: "the 2n controlled additions we needed to perform within each multiplication become 2n/cmul uncontrolled additions" | 2x |
| Cuccaro adder charged at `2n` per `n`-bit addition | §2.5: "Using Cuccaro et al.'s adder, each n-bit addition has a Toffoli count and measurement depth of 2n" | ~2x |

The middle correction also **removed** a double-count: the paper's `2n` *is* the
accumulate-and-clear pair, so PRAMANA's separate "multiply/unmultiply x2" was charging it
twice. Reading the source fixed a real bug in the opposite direction from the one being
hunted.

The adder is now a first-class modelling choice (`AdderKind`), because it is worth a
factor of two and published estimates do not all make the same one. A test asserts that
swapping GE19's Cuccaro charge for the modern temporary-AND adder roughly halves the
count — exactly the kind of attribution a resource estimator should be able to make.

### The diagnostic error that preceded it

The first harness read raw ratio drift (0.78 at n=1024 down to 0.41 at n=8192) as a
scaling mismatch, and blamed PRAMANA's window optimiser for widening lookup windows as
`n` grew. That hypothesis was **falsified by controlled experiment** (`make hypothesis`):
capping window width changes nothing, because the optimiser already selects the published
5-6 bit regime at every size on its own.

The diagnostic itself was wrong. The drift was the table-lookup `n^2` term washing out —
79% of the total at n=1024, 19% at n=16384 — which drags an unconstrained power-law fit to
a spurious `n^2.70`. The harness now fits `a n^3 + b n^2` simultaneously and compares
leading coefficients. Both the rejected hypothesis and the corrected diagnostic are
retained in `crates/pramana-verify/src/diagnose.rs`.

## P2 — ECDLP (Roetteler-Naehrig-Svore-Lauter)

`arith/ec_point.rs` and `shor_ecdlp.rs`, following arXiv:1706.06752.

**One primitive is imported from the source**: a modular constant addition at
`16 n log2(n)` Toffoli, from its Table 1. Everything above that is composed, and the
published figures emerge:

| Routine | Composition | Emergent cost | Published |
|---|---|---|---|
| Montgomery multiplication | `n` modular additions | `16 n^2 log2 n` | `16 n^2 log2 n` |
| Kaliski inversion | `2n` modular additions | `32 n^2 log2 n` | `32 n^2 log2 n` |
| Point addition | 4 inv + 2 squ + 4 mul | `224 n^2 log2 n` | `224 n^2 log2 n` |
| Full ECDLP | `2n` point additions | `448 n^3 log2 n` | `448 n^3 log2 n` |

**Neither 224 nor 448 appears anywhere in PRAMANA's source.** They fall out of assembling
the circuit. Verified figures for P-256:

- logical qubits: **2330 against published 2330 — exact**, composed from the inversion's
  qubit budget plus the control qubit and two intermediate registers.
- leading coefficient: **448 against published 448 — exact**.

### Open divergence: the ECDLP sub-leading term

Absolute Toffoli count at P-256 is 6.013e10 against the published 1.287e11, ratio 0.467.
The entire gap is the sub-leading `+4090 n^3` term, which at n=256 is comparable in size
to the leading term.

That term is not derivable from the published subroutine table. The source obtained it by
regression over its own simulated circuits, and Table 1 gives no sub-leading term for the
modular inversion that dominates. Reproducing it needs the group law at gate level rather
than at the level of subroutine multiplicities.

It would take one line to close: `4 x 511 = 2044 ~ 2045`, so a `+511 n^2` term in the
inversion would do it. That is exactly the fit-to-target this harness exists to prevent,
and it is not done. PRAMANA is **below** the published figure here — optimistic about the
attacker's cost, the dangerous direction.

### A modelling detail worth recording

PRAMANA's incrementer uses `ceil(log2 n)`, the real number of carry-tree levels, while the
source's closed forms use continuous `log2(n)`. These coincide exactly at powers of two
and diverge by up to 11% at the worst case (P-521: 10 against 9.03). Real curves are
mostly not powers of two, so PRAMANA reports a rounding penalty the published closed forms
smooth over. Two tests pin both behaviours.

## P2 — Grover against symmetric primitives

`grover_symmetric.rs`. The Grover structure is fully derived: `(pi/4) 2^(k/2)` iterations,
two oracle calls each, `ceil(k/128)` cipher blocks to pin the key uniquely. The single
imported primitive is the **multiplicative complexity of the AES S-box, 32 AND gates**
(Boyar-Peralta 2011); the rest composes from FIPS 197 round structure and key schedule.

Result: AES-128 costs about **2^78 Toffoli gates**, roughly **2^47 times harder than
RSA-2048** at 2^31. That gap is the difference between "migrate now" and "not a priority",
and PRAMANA can now show its working rather than repeating received wisdom.

Depth limits are modelled too. Parallel Grover buys only `sqrt(S)`, so halving the
permitted depth quadruples the machines required; a test pins that relationship. A raw
gate count without the depth constraint overstates how close the attack is.

## P3 — the surface code

`pramana-qec`: the `QecArchitecture` trait, `QecInput` / `HardwareParams` / `QecEstimate`,
magic state factories (two-level 15-to-1 distillation and Gidney-Shutty-Jones
cultivation), Litinski's compact/intermediate/fast block layouts, and a distance solver.

### The full causal chain now runs

Modulus size to circuit to physical qubits, with no published figure consulted along the
way, and no Q-Day assumption anywhere:

| Target | logical | Toffoli | physical | d | runtime | limited by |
|---|---:|---:|---:|---:|---|---|
| RSA-1024 | 3108 | 4.04e8 | 1.08e7 | 29 | 1.1 hours | reaction |
| RSA-2048 | 6183 | 2.64e9 | 2.42e7 | 31 | 7.3 hours | reaction |
| RSA-4096 | 12330 | 1.74e10 | 5.42e7 | 33 | 2.0 days | reaction |
| P-256 | 2330 | 6.01e10 | 1.07e7 | 33 | 7.0 days | reaction |
| P-521 | 4719 | 6.34e11 | 2.37e7 | 35 | 73 days | reaction |

**Reproduction of Gidney-Ekera 2019's physical figures**, end to end:

- physical qubits: **2.42e7 against the published 20 million — ratio 1.21**
- runtime: **7.34 hours against the published 8 hours — ratio 0.92**

Both from the reaction limit and the distance solver, not from any tabulated value.

### Two findings the model produced

**Shor is reaction-limited, not factory-limited.** Every target above is bound by
classical feedback latency rather than magic state supply. This is asserted by a test
because it is the qualitative claim most often got wrong: a model that counts only magic
states will misjudge the runtime badly (spec anti-pattern 4).

**Magic state cultivation alone barely helps a GE19-style circuit.** Swapping two-level
distillation for cultivation cuts factory qubits 16-fold (4.38e5 to 2.69e4) but reduces
the total by only **1.7%**, because factories were already just 1.8% of the machine and
the run is reaction-limited. Data and routing patches dominate at roughly 49% each.

That is a useful negative result, and it explains the structure of Gidney 2025: cultivation
was never going to be sufficient on its own. The qubit count only falls once the *logical*
qubit count falls, which is what the CFS residue arithmetic delivers, with yoked codes
shrinking idle storage on top. PRAMANA arrived at that conclusion from its own numbers.

## P4 — yoked surface codes

`yoked.rs`. Two-tier storage: active logical qubits at full distance, idle ones under an
outer code at roughly half the footprint. Windowed modular exponentiation touches a narrow
working set, so the idle fraction is large and the saving is real.

**A negative result worth pinning.** Even yoking *every* idle qubit in the GE19 circuit
leaves the machine above one million physical qubits, so yoking alone cannot reach the
Gidney 2025 target. A test asserts this. It confirms from PRAMANA's own numbers what the
cultivation experiment in P3 already suggested: the 2025 result requires the **logical**
qubit count to fall, which is what the residue arithmetic delivers. Yoking and cultivation
are necessary but jointly insufficient.

## P5 — repetition cat codes

`cat.rs`, following Gouzien et al. (arXiv:2302.06639, PRL 131:040602).

The structural claim is checkable and PRAMANA checks it: cat qubits suppress bit flips
exponentially in photon number, leaving phase-only noise, which a **one-dimensional
repetition code** corrects with `2d - 1` physical qubits per logical qubit against the
surface code's `2d^2 - 1`. The saving is a full factor of `d`.

**Reproduction of the published figures**, with the repetition distance solved from the
source's own noise parameters rather than assumed:

- physical qubits: **1.178e5 against the published 126,133 — 6.6% error**
- runtime: **8.35 hours against the published 9 hours — 7.2% error**

The model refuses to run without an explicit photon number and loss ratio (Law 9: no
silent defaults), warns when the photon number is too low to suppress residual bit flips,
and a test asserts the advantage erodes monotonically as the loss ratio degrades. That
erosion curve turns a vendor headline into a falsifiable engineering requirement.

## The architecture spread — the novelty claim, measured

Identical circuits costed under every implemented architecture:

| asset | surface | yoked (0.8) | surface+cultivation | cat | spread |
|---|---:|---:|---:|---:|---:|
| RSA-2048 | 2.42e7 | 1.95e7 | 2.38e7 | 2.61e5 | 93x |
| RSA-4096 | 5.42e7 | 4.35e7 | 5.37e7 | 6.18e5 | 88x |
| P-256 | 1.07e7 | 8.65e6 | 1.02e7 | 1.18e5 | 91x |
| P-384 | 1.77e7 | 1.43e7 | 1.71e7 | 1.76e5 | 101x |

**Physical qubit requirement varies by up to 101x across architectures for an identical
asset.** This is the empirical content of the novelty claim, and it is now measured rather
than asserted. A single-architecture estimate is a point estimate wearing a lab coat.

**Normalisation disclosure** (spec 8.9). The spread is not purely a property of the codes.
A cat qubit is not a transmon is not an atom, and the columns assume different cycle times
(500 ns cat, 1 us superconducting, 200 us atoms) and different connectivity. The qLDPC
column charges logical computation overhead, not memory only; quoting its memory figure
would overstate its advantage roughly threefold. PRAMANA states which part is code
structure and which is hardware assumption rather than reporting a bare ratio.

## The keystone: Gidney 2025 reproduced

**`gidney_2025_rsa2048` passes.** PRAMANA computes **9.26e5 physical qubits (under a
million) and 4.02 days (under a week)** for RSA-2048, end to end from the modulus size.
This was the spec's designated keystone and the one target that may never be waived.

### The circuit: approximate residue arithmetic

`arith/residue.rs` and `shor_factoring_g25.rs`. Every parameter is derived from Table 2 of
the source; subroutine iteration counts and operation tallies come from Table 3, the qubit
budget from Table 4. Reproductions against Table 5:

| n | logical qubits | published | Toffoli per factoring | published |
|---|---:|---:|---:|---:|
| 2048 | **1432** | **1399** | **5.69e9** | **6.5e9** |

Logical qubits within 2.4%, Toffoli within 12.4%, and the same holds across 1024, 4096
and 8192.

The structural fact that makes it work: `m = ceil(n/2 + n/s)` input qubits, about `0.5n`
where the 2019 construction needed `3n`. The expected shot count
`(s+1)/(1-P_deviant)/0.99` is modelled explicitly, because Table 5's Toffoli column is per
*factoring* across 9.2 shots; conflating that with the per-shot cost would misreport the
result by an order of magnitude.

### The machine: three-region layout

`gidney2025.rs`. Cold storage holds the idle input register under yoked codes at roughly
triple density, hot storage holds the small active working set at `2(d+1)^2` each, and a
compute region hosts six factories plus routing. About 89% of the logical qubits sit in
the idle input register, so cold storage dominates. That is precisely why yoking matters
here and did not matter for the 2019 circuit.

### A finding: the headline rests on two units of code distance

The source selects `d = 25` from its own simulated data (Figure 6); PRAMANA's generic
analytic fit selects `d = 27` for the same per-round target. Those two steps move the
total from 9.26e5 to **1.23e6** — across the million-qubit line.

Rather than pick whichever curve gave the nicer answer, the error model is an explicit
cited choice (`LogicalErrorModel`) and **both** are reported:

- under the source's own simulated curve: **9.26e5, PASS**
- under the generic analytic fit: **1.23e6, PROBE**

Using a source's noise model when reproducing that source is the same principle as
charging Cuccaro's adder at `2n` when reproducing Gidney-Ekera 2019. Reporting only the
favourable one would not be.

The harness now distinguishes a **sensitivity probe** from a regression. A probe is
declared `expected_failure` in its golden file, reported prominently, and does not break
the build — but a probe that unexpectedly *passes* is flagged, since that means the thing
it was probing has moved.

## P5 completed: qLDPC and neutral atoms

### Bivariate bicycle codes, with the trap avoided

`qldpc.rs`. The code is **constructed and its parameters computed**, not tabulated. Working
in `F_2[x,y]/(x^l-1, y^m-1)`, PRAMANA builds `H_X = [A|B]` and `H_Z = [B^T|A^T]` from the
polynomial terms and derives `k = n - rank(H_X) - rank(H_Z)` by Gaussian elimination over
GF(2). The gross code comes out as `[[144,12,12]]`, the others as `[[72,12,6]]` and
`[[288,12,18]]`, all with weight-6 checks, and a test verifies the CSS commutation
condition `H_X H_Z^T = 0`.

**The trap this module exists to avoid** (spec anti-pattern 5). A bivariate bicycle code
stores 12 logical qubits in 144 data qubits: about 24 physical per logical against a
distance-25 surface code's 1250. Quoting that as the cost of *computing* is the commonest
error in the popular literature on qLDPC overheads, because these codes have no cheap
transversal gates and logical operations need ancilla systems or code switching.

PRAMANA reports memory-only and with-computation figures **separately and always**. The
memory-only path emits a loud warning, and a test measures the gap: memory-only accounting
**overstates the advantage roughly threefold**. The connectivity objection is also
surfaced rather than assumed away — weight-6 checks need degree-6 connectivity with
long-range couplers, so a nearest-neighbour square grid is refused outright.

### Neutral atoms, and the cost of moving them

`neutral_atom.rs`. Correlated decoding reduces syndrome rounds per logical operation from
`O(d)` to `O(1)`; PRAMANA computes both in the same run so the saving is visible rather
than asserted, and a test confirms the transversal path beats the lattice-surgery baseline.

But the modality can spend its entire advantage on mechanics. At 200 us per atom
relocation against a 200 us cycle, movement is half the per-operation budget, and RSA-2048
takes **12.2 days** where the surface code takes 7.3 hours. The architecture's `O(d)`
advantage in rounds is real and still loses to its own latency. Move time must be supplied
explicitly (Law 9), and expected atom losses are surfaced as an unmodelled optimism.

### A bug the type system caught

The `[[144,12,12]]` gross code has **even** distance. `Distance` enforced oddness, which is
a *surface-code* convention ensuring the majority-vote decoder has no ties, not a universal
one — so the gross code's distance was being silently dropped to zero in reports. `Distance`
now accepts any non-zero value and `Distance::new_odd` enforces the stricter rule where it
actually applies. A regression test pins that the family's even distances survive.

## The architecture spread, all four families

| asset | surface | yoked | cat | qLDPC | atoms | spread |
|---|---:|---:|---:|---:|---:|---:|
| RSA-2048 | 2.42e7 | 1.95e7 | 2.61e5 | 6.83e5 | 1.38e7 | 93x |
| RSA-4096 | 5.42e7 | 4.35e7 | 6.18e5 | 1.36e6 | 3.15e7 | 88x |
| P-256 | 1.07e7 | 8.65e6 | 1.18e5 | 2.57e5 | 5.96e6 | 91x |
| P-384 | 1.77e7 | 1.43e7 | 1.76e5 | 3.85e5 | 1.01e7 | 101x |

**Up to 101x variation for an identical asset.** What binds differs by family too, which is
what makes the spread interpretable rather than a bare ranking:

| architecture | RSA-2048 runtime | limited by |
|---|---|---|
| surface code | 7.34 hours | reaction |
| repetition cat | 22 minutes | factory |
| bivariate bicycle | 7.34 hours | routing |
| neutral atoms | 12.24 days | reaction |

## Harness behaviour

`make verify` runs every golden target through the production estimation path, writes
`verification/reports/report.{md,json}`, and exits non-zero on failure. Targets whose
machinery is not built report `PENDING` rather than passing or failing, because claiming
either would be false. `gidney_2025_rsa2048` is on the unwaivable list: a waiver on it
fails the build unconditionally.

Current state: **11 quantities passing, 1 failing with a diagnosis, 1 sensitivity probe, 1 pending.**

| Target | Quantity | Ratio | Result |
|---|---|---|---|
| GE19 RSA-2048 | logical qubits | 0.999 | PASS |
| GE19 RSA-2048 | Toffoli count | 1.007 | PASS |
| GE19 RSA-2048 | physical qubits | 1.210 | PASS |
| GE19 RSA-2048 | wall clock | 0.918 | PASS |
| Roetteler P-256 | logical qubits | 1.000 | PASS |
| Roetteler P-256 | leading coefficient | 1.000 | PASS |
| **Gidney 2025 RSA-2048** | **physical qubits** | **0.926** | **PASS (keystone)** |
| **Gidney 2025 RSA-2048** | **wall clock** | **0.574** | **PASS (keystone)** |
| Gidney 2025 RSA-2048 | logical qubits | 1.024 | PASS |
| Gidney 2025 RSA-2048 | Toffoli count | 0.876 | PASS |
| Gidney 2025, generic curve | physical qubits | 1.233 | PROBE (by design) |
| Gouzien P-256 cat | physical qubits | 0.934 | PASS |
| Gouzien P-256 cat | wall clock | 0.928 | PASS |
| Roetteler P-256 | absolute Toffoli | 0.467 | FAIL (diagnosed) |
| CFS 2024 | logical qubits, Toffoli | - | PENDING (no pipeline) |

The Roetteler target is deliberately split into three entries so that a structurally
correct construction is distinguished from one that merely lands near the right absolute
number. Widening a tolerance to make the third entry pass would destroy that distinction.

## P7 — the causal chain closes

`pramana-hardware` and `pramana-risk`. The chain now runs end to end:

> asset key parameters -> attack circuit -> fault-tolerant cost -> hardware trajectory ->
> break-year distribution -> Mosca resolution -> exposure score

No step consults a fixed Q-Day.

### Hardware trajectories fitted on verified logical qubits

Per the research dossier, the model fits **independently verified logical-qubit
demonstrations**, not physical qubit counts and not announced targets. Observed
physical-to-logical ratios, computed from the demonstration data:

| modality | geometric mean | range | n |
|---|---:|---|---:|
| superconducting | 105:1 | 105 | 1 |
| trapped ion | 2.7:1 | 2.0-4.7 | 3 |
| neutral atom | 15.1:1 | 4.7-49.2 | 2 |
| bosonic | 1.0:1 | 1.0 | 1 |

A physical-qubit fit would rank a 1,180-qubit machine above one delivering four times the
logical qubits. Announced targets are excluded from the capability fit by construction, and
a test asserts it; abandoned milestones are retained as right-censored data so the slip
prior cannot be biased optimistic by survivorship.

### A wrong answer the model caught on itself

The first fit used verified demonstrations alone. Those span 2024 to 2026, and a steep
slope over three years extrapolated to nonsense: **5.08x growth per year, 190 million
logical qubits by 2035**, which made every asset break by 2031.

The model reported its own error. The roadmap-comparison view flagged *every* announced
target as "history outruns the roadmap", including IBM's Blue Jay by a factor of 3,673.
When a fitted history disagrees with the people building the machine by three orders of
magnitude, the fit is wrong, not the vendor.

Two fixes, both principled rather than cosmetic:

1. **Frontier reduction.** Take the best result per year rather than pooling all
   demonstrations. Google's 1 logical qubit and Quantinuum's 12 in the same year were being
   averaged, inflating residual spread and biasing the slope. An attacker uses the best
   machine available, not the average one.
2. **Blend slip-corrected roadmap targets into the fit.** Roadmaps encode engineering
   knowledge three years of demonstrations do not, and they are systematically optimistic,
   which is exactly what the slip prior corrects. Each target enters shifted later by the
   mean slip.

Result: growth falls from 5.08x to **1.51x per year**, residual spread from 0.711 to 0.255
dex, and the roadmap comparison reads mostly "consistent". The absurd history-only fit is
retained as a regression test, because a guardrail that has never fired is not a guardrail.

### Break years and exposure

Sample output for a small estate, 3,000 Monte Carlo samples per asset:

| asset | logical | p05 | median | p95 | deadline | P(exposed) | exposure |
|---|---:|---:|---:|---:|---:|---:|---:|
| TLS RSA-2048 | 1,432 | 2038 | 2042 | 2045 | 2038 | 0.077 | 5.5 |
| TLS P-256 | 2,330 | 2039 | 2042 | 2045 | 2038 | 0.004 | 0.3 |
| Code-sign P-384 | 3,484 | 2040 | 2043 | 2046 | 2030 | 0.000 | 0.0 |
| Archive RSA-4096 | 12,330 | 2040 | 2044 | 2048 | 2061 | 0.994 | **99.4** |

Three things worth reading off that table:

- **The ordering is not by key size.** RSA-4096 breaks last and scores highest, because a
  thirty-year secrecy requirement plus a five-year migration pushes its deadline to 2061,
  well past any plausible break. This is the divergence between criticality-ranking and
  computed-exposure ranking that the novelty claim predicts.
- **Threat mode changes everything.** The code-signing asset is Critical and Restricted and
  scores zero, because a forged signature is not retroactive. The model warns that a
  long-lived trust anchor should be reclassified rather than silently discounting it.
- **Architecture choice moves the median break year by 3 years** for P-256 on this estate.
  That is the `N` in the novelty claim, now a measured number rather than a placeholder.

### External validity

Break-year medians land in 2042-2044. The Global Risk Institute's 2025 expert survey
(Mosca and Piani, 26 experts, published March 2026) puts a CRQC at 28-49% within ten years
and 51-70% within fifteen. PRAMANA's computed distribution sits at the conservative end of
that band, which is the right kind of agreement: close enough to be credible, derived
independently rather than by fitting to the survey.

## P8 — the product layer

`crates/pramana-py` and `backend/`. The estimation core is now reachable from Python and
fronted by an HTTP API that ingests real certificates.

### Bindings

PyO3 0.20, deliberately thin: every function marshals arguments, calls into the Rust
crates, and marshals results back. None of them decide anything, because business logic in
a binding layer would be invisible to both the Rust test suite and the verification
harness. The GIL is released around the expensive calls so a prefork worker pool behaves.

The headline call is `assess_asset`, which runs the whole chain for one asset and returns
break-year quantiles, per-architecture medians, the Mosca resolution, the exposure score,
and a provenance record.

### Ingestion

X.509 parsing with two rules enforced by tests:

- **No guessing.** An unrecognised curve or key type produces an `INDETERMINATE` asset that
  is surfaced, never a plausible-looking default. Ed25519 is explicitly marked
  indeterminate: it is quantum-vulnerable, but no published ECDLP circuit covers twisted
  Edwards curves, and costing it with a P-256 circuit would be exactly the substitution
  Law 9 forbids.
- **No private keys.** A PEM block containing private key material is rejected before
  parsing rather than parsed and discarded, because accepting it at all would mean it had
  already reached a request log.

Certificate chains are reconstructed on ingest, so a root's dependents are known and feed
the agility score's dependency-depth dimension.

### Crypto agility, aligned to CSWP 39

Eight weighted dimensions scored from observable evidence, feeding the migration term of
Mosca's inequality. A 90-day certificate lifetime is treated as evidence that automation
exists and works; a multi-year lifetime as evidence of a manual process. The score maps to
migration years through a published table users can override, and each asset gets a ranked
list of the cheapest improvements available to it.

### Recommendations

Standards encoded as data, current to 2026-08-29. Two rules:

- **Never recommend a pending standard.** FN-DSA and HQC are selected but not final; an
  asset migrated to a draft parameter set may have to be migrated twice. They appear as
  clearly-marked alternatives, never as the primary target, and a test asserts it.
- **Compute the size impact.** "Signatures get bigger" is not actionable. PRAMANA reports
  that SLH-DSA's 7,856-byte signature is 122.8x an ECDSA P-256 signature and exceeds a
  typical MTU.

### The novelty claim, on real certificates

Live run over a four-asset fleet of parsed X.509 certificates:

| asset | algorithm | p05 | p50 | p95 | deadline | P(exposed) | score |
|---|---|---:|---:|---:|---:|---:|---:|
| archive.acme.test | rsa-4096 | 2040 | 2044 | 2047 | 2057 | 0.989 | **98.9** |
| tls.acme.test | rsa-2048 | 2038 | 2042 | 2045 | 2036 | 0.002 | 0.1 |
| api.acme.test | ecdsa-256 | 2039 | 2042 | 2045 | 2036 | 0.001 | 0.0 |
| Acme Code Signing Root | ecdsa-384 | 2040 | 2043 | 2046 | 2028 | 0.000 | 0.0 |

**Kendall tau 0.0.** The criticality-based ranking and the computed-exposure ranking are
completely uncorrelated on this fleet. The code-signing root ranks first by criticality and
*fourth* by computed exposure, because a forged signature is not retroactive; the archive
key ranks first by computation because a thirty-year secrecy requirement pushes its
deadline past any plausible break.

That is the project's novelty claim, demonstrated end to end on parsed certificates rather
than asserted in an abstract.

## P9 — the hero figure and the frontend

### Figure 1

The exposure timeline is generated server-side as SVG (`backend/app/services/timeline_svg.py`)
and served from `/api/v1/inventories/{id}/timeline.svg`. Rendering it on the server rather
than in the browser means the artefact a user sees is byte-identical to the one that goes
into the paper, so the figure cannot drift from the published version.

It is a **ridgeline of break-year densities**, not an error-bar chart. Reducing each asset
to an interval would collapse the distribution back into the point estimate the project
exists to avoid.

Design decisions, all load-bearing:

- **Greyscale-safe.** The fill is a viridis approximation, perceptually uniform and monotone
  in lightness, so exposure ordering survives black-and-white printing.
- **The overlap is the finding.** Where an asset's migration deadline falls right of the
  density's early tail, that window is shaded explicitly rather than left to be inferred
  from two adjacent glyphs.
- **Tails are clipped, and the clip is declared.** Bounds come from each asset's own 98th
  percentile, so one distribution with a decade-long tail costs a small edge marker instead
  of half the plot width. A triangle at the axis marks a density that continues.

Four defects were found by rendering the figure and looking at it, and fixed: milestone
labels collided into illegibility, a single long tail stretched the axis to 2080 and wasted
40% of the width, the legend used unicode glyphs that did not render, and clip markers
overlapped the score column. `make figure` regenerates it deterministically.

### Frontend

React 18 with Vite and TypeScript in strict mode, building clean. Five views: the timeline,
computed exposure with an expandable provenance chain per row, prioritisation divergence,
the inventory with indeterminate assets surfaced first, and the hardware trajectory with
its physical-to-logical spread and slip caveats.

The exposure table makes Law 2 visible: every score expands to the circuit that produced
it, the trajectory it met, the seed, the sample count, and the per-architecture medians.

## The 2026 ECDLP constructions

The research dossier's top recommendation, now implemented. `arith/eea.rs` and
`shor_ecdlp_2026.rs` build Schrottenloher 2026 ([arXiv:2606.02235](https://arxiv.org/abs/2606.02235)),
the open reconstruction of the circuits Babbush et al. reported behind a zero-knowledge
proof.

### The construction

The saving comes from splitting the extended Euclidean algorithm. A textbook EEA maintains
the Bezout coefficients alongside the working pair, so a reversible implementation must hold
both. The split runs the plain Euclidean algorithm first, recording only its *branch
decisions* into a compressed bit-vector, then replays those decisions onto the coefficients.

Two consequences, both large. The two register pairs never coexist, so registers freed as
`u` and `v` shrink are reused for the garbage bits. And because the replay updates are
linear in the recorded bits, starting from `(y, 0)` rather than `(1, 0)` yields
`y·x⁻¹ mod q` — the inversion and an in-place multiplication fall out of one pass.

PRAMANA derives the iteration count from first principles rather than quoting it: each
iteration removes `log₂(8/3)` bits from the pair, so `2n` bits need `2n/log₂(8/3) ≈ 1.413n`
iterations, plus four standard deviations of slack at `0.6√n`. The 3-into-5 garbage
compression gives `2.355n + O(√n)` bits, and the Bezout replay's peak of two `n`-bit
registers plus the garbage vector gives the published `4.355n + O(√n)`.

### Reproduction

| Figure | PRAMANA | Published | Ratio |
|---|---:|---:|---:|
| Space-optimised qubits | 1,198 | 1,208 | 0.992 |
| Space-optimised Toffoli | 5.89e7 | 7.24e7 | 0.814 |
| Gate-optimised qubits | 1,454 | 1,462 | 0.995 |
| Gate-optimised Toffoli | 5.25e7 | 5.76e7 | 0.912 |

Both qubit counts within 1%. A test also pins that a generic prime costs materially more
than secp256k1's pseudo-Mersenne form, so the two curves stay distinct targets.

### The descent, now computed rather than cited

| Construction | Qubits | Toffoli | vs 2017 |
|---|---:|---:|---:|
| Roetteler et al. 2017 | 2,330 | 6.01e10 | 1x |
| Schrottenloher 2026, space-opt | 1,198 | 5.89e7 | 1,020x |
| Schrottenloher 2026, gate-opt | 1,454 | 5.25e7 | **1,144x** |

`make ecdlp-descent` prints it. This series is the empirical backing for the risk engine's
algorithmic-improvement term, and no other estimator models it.

## A finding: elliptic-curve exposure is space-bound, not time-bound

Switching the risk engine to the 2026 pipeline moved P-256's median break year by **one
year**, from 2042 to 2041 — despite an 807-fold reduction in gate count.

That is not a bug, and the arithmetic confirms the mechanism exactly. The qubit requirement
fell 1.60x, and at the fitted 1.51x annual growth in logical qubits that predicts
`log(1.60)/log(1.51) = 1.14` years. Observed: 1 year.

**The attacker is waiting for enough error-corrected qubits to host the circuit, not for the
computation to finish.** With a patient attacker budget, runtime is simply not the binding
constraint. Two tests pin this: with improvement frozen, a fourfold width saving moves the
break year several years while a thousandfold gate saving moves it barely at all.

The practical consequence for triage is direct: **for elliptic curves, the width improvement
series matters and the gate series largely does not.** A headline like "1000x fewer gates"
is close to irrelevant to when an asset actually becomes breakable.

### A caveat the same experiment surfaced

Because width improves at the historically fitted rate and decays toward a floor, two
constructions differing fourfold today converge to within a small factor inside about
fifteen years. Break years far in the future therefore depend more on the floor than on
today's width. That presumes the historical rate persists, which is a strong assumption; a
test pins the convergence so it is visible rather than discovered later.

## Holding the project to its own laws

The specification requires CI to fail where a law is mechanically checkable. That check did
not exist, so I wrote it (`scripts/check_laws.py`) and pointed it at the project.

**It found nineteen violations of Law 1**, in two genuinely different categories.

**Public API advertising published results.** `ShorFactoringGE19::published_toffoli`,
`ShorEcdlpRNSL::published_logical_qubits`, `ShorFactoringG25::published_*` and
`PUBLISHED_LEADING_COEFF` were all public functions in the *estimation* crates. The
estimation path never called them, but Law 1 says published numbers appear in exactly one
place, and a function called `published_toffoli` living in the crate that also computes
Toffoli counts is indistinguishable — to a reader or to a static check — from a lookup table
the estimation path might read. They now live in `pramana-verify::published` or inside
`#[cfg(test)]` blocks, which cannot be reached at runtime.

**The improvement series embedded as constants.** `improvement.rs` carried the historical
resource-estimate series (6189 to 1399, 2330 to 835, 1.29e11 to 5.76e7) as literals. These
are model *inputs* rather than answers, so not a violation in spirit — but they were
indistinguishable from one. They now live in `data/improvement_series.toml` with a citation
per observation, compiled in via `include_str!`. That is a better design regardless: the
series is itself a finding worth publishing, and it can be updated as new results land
without touching code.

The checker distinguishes published *results* from published *inputs*, and carries an
allowlist recording why each input is legitimate — Cuccaro's `2n` per addition, Roetteler's
Table 1 subroutine costs, Gidney's grid-scanned window parameters. All four checks now pass.

**Determinism** (Law 5) is checked at the artefact level rather than by unit test:
`scripts/check_determinism.sh` runs the verification harness and the exposure assessment
twice each and diffs the outputs. What matters is that a *report* reproduces, not merely
that a function is pure. Both are byte-identical.

**CI** (`.github/workflows/ci.yml`) runs seven jobs: laws, Rust tests with clippy at
`-D warnings`, the reproduction harness, determinism, backend tests against a freshly built
core, the frontend build, and a dependency audit.

## Assumptions

[`ASSUMPTIONS.md`](ASSUMPTIONS.md) is the canonical statement the specification asks for:
every default, its value, its justification and its source, across roughly seventy entries.

Each is classified by how much weight it can bear — **derived** (follows from the
construction), **sourced** (from a named publication), **calibrated** (chosen to reproduce a
figure, so the reproduction is no longer independent evidence for it), or **judgement** (a
defensible choice with no decisive source).

The document ends with the seven assumptions carrying the most weight and what happens if
each is wrong. The two a reviewer should attack first:

- **The qLDPC computation overhead** is the least defensible number in the system. No
  published estimate covers logical computation on bivariate bicycle codes at cryptographic
  scale. PRAMANA reports memory-only and with-computation separately and warns on the
  former, but that column should be read as order-of-magnitude.
- **Blending roadmap targets into the capability fit** is the weakest link between the model
  and reality. It imports vendor optimism, corrected only by a slip prior fitted on five
  observations dominated by one company's physical-qubit milestones. The honest summary is
  that the break-year distribution's centre is about as trustworthy as vendor roadmaps are.

It also states plainly what is **not** modelled: physical noise beyond a uniform depolarising
rate, classical control cost, money, attacker economics, interception probability, and the
quantum vulnerability of the post-quantum algorithms themselves.

## Research dossier

A detailed survey of the four open workstreams is in [`RESEARCH.md`](RESEARCH.md),
compiled 2026-08-29 against primary sources. **One of its findings changes a priority.**

**RESOLVED for the Schrottenloher construction** (see the 2026 ECDLP section above).
The finding as originally recorded: PRAMANA's ECDLP model was three generations stale. The
construction it implemented (Roetteler 2017) was superseded three times during 2026: Google/Babbush in March, an open
reconstruction by Schrottenloher in May, and a new width record by Luo et al. in July. For
a 256-bit curve the state of the art has moved from 2,330 logical qubits and 1.29e11
Toffoli to **835 qubits and ~5.6e7 Toffoli** — a factor of 2,300 on gates.

The consequence is that PRAMANA currently **understates** how exposed elliptic-curve assets
are, which for a triage tool is the dangerous direction. Chasing Roetteler's sub-leading
`+4090n^3` term would improve fidelity to a superseded constant while leaving the headline
figure wrong by three orders of magnitude, so that workstream is deprioritised in favour of
implementing the 2026 constructions. The 2026 targets and the `2n` data-register floor are
now declared in `verification/golden/ecdlp_2026_state_of_art.toml`, so the harness reports
the gap rather than leaving it in a document.

Three other findings worth flagging:

- **NIST CSWP 39 specifies PRAMANA.** Finalised December 2025 and updated June 2026, it
  calls for a "cryptographic policy-informed risk assessment engine" that continuously
  analyses the inventory and recommends mitigations according to policy and risk. That is
  this project, written by NIST. Cite it as the requirement being implemented.
- **Hardware trajectories should fit verified logical qubits, not physical.** The
  physical-to-logical ratio spans 2:1 (Quantinuum) to 105:1 (Google) across demonstrated
  systems, so a physical-qubit fit would rank a 1,180-qubit machine above one delivering
  four times the logical qubits.
- **The algorithmic-improvement term is now measurable, not assumed** — and the rate
  differs sharply by problem family, so it must be fitted per family and bounded by the
  floors.

## Next

- Close the 3.55x by testing candidates 1–3 in isolation against the source construction.
- `shor_ecdlp.rs` — Roetteler et al. Golden file is written and pending a pipeline:
  targets are 2330 logical qubits and 1.287e11 Toffoli for P-256.
- `residue.rs` (Chevignard–Fouque–Schrottenloher) and the `ShorFactoringG25` pipeline.
- `grover_symmetric.rs` with the depth-limited analysis.
- P3: `pramana-qec`, the `QecArchitecture` trait, and the surface-code model.
