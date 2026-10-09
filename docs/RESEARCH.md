# PRAMANA research dossier

Compiled 2026-08-29. Covers the four open workstreams: the ECDLP sub-leading term, the
P6 cross-implementation check, the P7 hardware trajectory model and risk engine, and the
PQC layer.

**Headline finding: one of these questions has changed underneath us.** The ECDLP
sub-leading term is no longer worth chasing, because the construction it belongs to was
superseded three times over during 2026. §1 explains what happened and what to do instead.

---

## 1. ECDLP: the 2026 collapse

### 1.1 What PRAMANA currently models, and why it is stale

PRAMANA's `shor_ecdlp.rs` implements Roetteler-Naehrig-Svore-Lauter 2017
([arXiv:1706.06752](https://arxiv.org/abs/1706.06752)): `9n + 2⌈log₂n⌉ + 10` logical qubits
and `448n³log₂n + 4090n³` Toffoli gates, giving **2,330 qubits and 1.29e11 Toffoli** for a
256-bit curve. PRAMANA reproduces the leading coefficient exactly and reports a diagnosed
gap on the empirical `+4090n³` sub-leading term.

That open divergence is real, but it is now the wrong thing to spend effort on. Here is
the actual state of the art:

| Date | Source | Logical qubits (256-bit) | Toffoli | Note |
|---|---|---:|---:|---|
| 2003 | [Proos-Zalka](https://arxiv.org/abs/quant-ph/0301141) | — | — | Establishes the `2n` data-register floor |
| 2017 | [Roetteler et al.](https://arxiv.org/abs/1706.06752) | 2,330 | 1.29e11 | The academic baseline PRAMANA implements |
| 2020 | [Häner, Jaques, Naehrig, Roetteler, Soeken](https://arxiv.org/abs/2001.09580) | — | — | Improved windowed arithmetic |
| 2023 | [Litinski](https://arxiv.org/abs/2306.08585) | — | ~2.0e8 | Active-volume architecture; windowed point multiples |
| 2026-03 | Google / Babbush et al. | 1,175 space-opt / 1,425 gate-opt | withheld | Published a **zero-knowledge proof** that circuits meeting the counts exist, withholding the circuits |
| 2026-05 | [Schrottenloher](https://arxiv.org/abs/2606.02235) | 1,192 / 1,446 (`4.355n + O(√n)`) | ~5.6e7 | Open reconstruction; matches Google on qubits, beats it on gates by 6.5–10% |
| 2026 | [Chevignard-Fouque-Schrottenloher, EUROCRYPT 2026](https://eprint.iacr.org/2026/280) | ~1,098–1,193 | +3 orders of magnitude | Width-minimised; pays heavily in gates |
| 2026-07 | [Luo, Yang, Luo, Wang, Su, Sun, Li, Li](https://arxiv.org/abs/2607.13816) | **835** (`3n + 6⌊log₂n⌋ + O(1)`) | `919n³/log₂n + O(n²)` | Current width record |

**From 1.29e11 to ~5.6e7 Toffoli is a factor of ~2,300. From 2,330 to 835 qubits is a
factor of 2.8.** Both happened while PRAMANA was being built.

### 1.2 What drove it

Three distinct forces, per [Ivezic's June 2026 survey](https://postquantum.com/post-quantum/quantum-attack-ecc-circuit-floor/):

1. **Better field arithmetic.** Windowing the modular multiplications, reorganising curve
   arithmetic to spread or avoid per-addition inversions. Incremental, well-understood.
2. **Approximate and measurement-based circuitry — the big lever.** The modular inversion
   inside point addition is normally Extended Euclidean. The 2026 circuits split it into a
   forward pass that records branch decisions into a compressed bit-vector and a Bézout
   reconstruction that replays them, letting the modular multiplication run in place with
   no separate inversion register. The technique descends from Google's October 2025 work
   on [decoded quantum interferometry](https://arxiv.org/abs/2510.10967). This is most of
   why the leading qubit constant fell from `9n` to `4.355n`. Luo et al. push further with
   register-sharing refinements over Proos-Zalka, adding length registers and
   location-controlled arithmetic, reaching a modular inversion in `2n + 6⌊log₂n⌋ + O(1)`
   qubits and `195n² + O(n log₂n)` Toffoli.
3. **Crowd-sourced constant-factor search.** [ecdsa.fail](https://www.ecdsa.fail/), an
   Eigen Labs leaderboard, scores submissions on the *product* of logical qubits and
   Toffoli count for secp256k1 point addition. Contributors include AI autoresearch agents.

A curve-specific note: secp256k1's prime `2²⁵⁶ − 4294968273` is pseudo-Mersenne, so its
modular reductions collapse into small constant additions. **Estimates tuned to secp256k1
do not transfer unchanged to NIST P-256.** PRAMANA must keep them as distinct targets.

### 1.3 The floors

Both bounds matter for the risk engine, because they say where the algorithmic improvement
curve stops.

- **Data-register floor ≈ 2n = 512 qubits.** An elliptic-curve point is two coordinates
  over an n-bit field. Proos-Zalka noted in 2003 that ECC needs roughly twice the data
  width of an equivalent factoring problem. Today's ~4.3n is mostly ancilla, so the gap
  from 1,175 to ~512 is engineering, not physics.
- **A possible sub-`2n` regime, unproven.** A point's y-coordinate is determined by x up to
  sign, and classical Montgomery-ladder arithmetic carries only x. An x-only quantum point
  addition would halve the register toward `n` ≈ 256. The obstacle is that ECDLP needs a
  *double-scalar* operation `[a]P + [b]Q` over two independent base points, and x-only
  differential addition is built for single-base ladders. Whether a clean x-only
  double-scalar addition exists is open.
- **Gate floor: a cubic envelope.** Shor on an n-bit curve needs ~n point additions, each
  needing several modular multiplications, each with irreducible cost growing with n. The
  `n³` cannot be escaped by rearrangement. Recent gains are single-digit percentages on the
  constant, which asymptote.

### 1.4 Recommendation for PRAMANA

**Retire the sub-leading-term workstream. Replace it with a modern ECDLP pipeline.**

Chasing Roetteler's empirical `+4090n³` would make PRAMANA reproduce a 2017 constant more
precisely while its headline ECDLP number stays ~2,300x too pessimistic. That is precision
in the wrong direction, and for a triage tool the direction matters: PRAMANA currently
*understates* how exposed elliptic-curve assets are.

Concretely:

1. Keep `ShorEcdlpRNSL` as a **historical baseline**, relabelled as such. Its value is now
   as a fixed point in the algorithmic-improvement series (§4.3), not as a live estimate.
2. Add `ShorEcdlpSchrottenloher2026` from [arXiv:2606.02235](https://arxiv.org/abs/2606.02235).
   Reference implementation is public at
   `gitlab.inria.fr/capsule/qarton-projects/ec-point-addition`, which makes it the most
   verifiable of the 2026 constructions — the others are withheld (Google) or very recent.
   Target: 1,192 qubits space-optimised / 1,446 gate-optimised, ~5.6e7 Toffoli, with the
   qubit count scaling as `4.355n + O(√n)`.
3. Add `ShorEcdlpLuo2026` from [arXiv:2607.13816](https://arxiv.org/abs/2607.13816) as the
   width record: `3n + 6⌊log₂n⌋ + O(1)` qubits, `919n³/log₂n + O(n²)` Toffoli, 835 qubits
   at n=256. Note it supersedes the authors' own [arXiv:2604.02311](https://arxiv.org/abs/2604.02311).
4. Golden entries for each, with the 2017 baseline retained so the harness displays the
   whole descent. **The descent is itself a finding PRAMANA should publish.**
5. Model the pseudo-Mersenne advantage explicitly so secp256k1 and P-256 differ.

### 1.5 A verification methodology worth copying

The [ecdsa.fail challenge repository](https://github.com/ecdsafail/ecdsafail-challenge)
enforces conditions PRAMANA's own harness should adopt for any circuit it claims to have
synthesised:

- every submission checked over **9,024 test cases**;
- ancilla must be uncomputed to `|0⟩` before being freed;
- global phase must come out clean, with no leftover kickback from sloppy uncomputation;
- running the circuit then its inverse must restore the original state exactly.

A Toffoli saving obtained by skipping uncomputation or leaking phase **fails** rather than
scoring lower. PRAMANA already enforces allocation balance in `Node::Repeat` and checks
`net_alloc == 0`; the phase-cleanliness and inverse-composition conditions are not yet
modelled and would strengthen the claim that its circuits are real.

---

## 2. P6: the cross-implementation check

Spec §9.4 calls for a slow, obviously-correct reference implementation to cross-check the
Rust. Since that was written, the ecosystem has matured and there are better options than
writing a naive Python surface-code model from scratch.

### 2.1 Available reference estimators

| Tool | Owner | Scope | Notes |
|---|---|---|---|
| [Azure Quantum Resource Estimator](https://arxiv.org/abs/2311.05801) | Microsoft | Logical → physical, surface code, several qubit models | Successor to QCTraceSimulator; well-documented parameter sets; the closest match to PRAMANA's layering |
| [Qualtran](https://qualtran.readthedocs.io/) | Google | Bloq-based algorithm representation with cost analysis | Open source Python; has a library of quantum algorithms including arithmetic primitives; best match for cross-checking PRAMANA's *circuit* layer |
| BenchQ | Zapata AI (DARPA Quantum Benchmarking) | End-to-end resource estimation | Zapata wound down; check maintenance status before depending on it |
| pyLIQTR | MIT Lincoln Laboratory | Hamiltonian simulation oriented | Less relevant to arithmetic circuits |
| [AutoQuREO](https://arxiv.org/abs/2608.12936) | — | Automated resource estimation and optimisation | 2026, newer framework worth evaluating |

There is prior art in comparing them: studies exist comparing Qualtran, BenchQ and Azure
QRE on physical qubit counts and runtimes for the same computations. **Those comparisons
found material disagreement between tools**, which is exactly the observation that makes
PRAMANA's verification harness a contribution rather than a formality.

### 2.2 Recommended plan

Replace the "naive Python reference" with a **three-way agreement check**:

1. **Circuit layer against Qualtran.** Qualtran has adders, QROM/QROAM and modular
   arithmetic bloqs. Cross-check PRAMANA's `arith::adder`, `arith::lookup` and
   `arith::modexp` Toffoli counts against Qualtran's for matched parameters. This is the
   highest-value check because it tests the layer where PRAMANA does original work.
2. **QEC layer against Azure QRE.** Feed identical logical inputs (logical qubits, T count,
   error budget, hardware parameters) into Azure QRE's surface-code model and PRAMANA's
   and compare physical qubits and runtime. Expect disagreement; **the interesting output
   is a decomposition of where the models diverge**, not agreement.
3. **A minimal internal Python reference** retained only for the distance solver and the
   logical-error curve, which are small enough to reimplement obviously-correctly and are
   where a subtle Rust optimisation bug would be most damaging.

Deliverable: `verification/cross/` with a report comparing all three, published alongside
the reproduction report. A documented disagreement between four independent estimators on
the same input is a genuinely useful artefact for the field.

---

## 3. P7: the hardware trajectory model

### 3.1 The metric problem, and its solution

The spec's roadmap model fits physical qubit count against year. **That is the wrong
metric**, and 2026 data shows why: the physical-to-logical ratio varies by more than an
order of magnitude across modalities.

Verified logical-qubit demonstrations as of April 2026:

| Organisation | Logical | Physical | Ratio | Code / note |
|---|---:|---:|---:|---|
| QuEra | **96** | 448 | **4.7:1** | `[[16,6,4]]` high-rate codes, below-threshold suppression, Nature Jan 2026 |
| Quantinuum | 48 | 98 | 2.0:1 | Trapped ion, high-fidelity gates |
| Atom Computing | 24 | 1,180 | 49:1 | Neutral atom |
| Google | 1 | 105 | 105:1 | Surface code, Willow |
| Nord Quantique | 1 | 1 | 1:1 | GKP bosonic encoding, single-mode |

A model fitted on physical qubits alone would rank Atom Computing's 1,180 physical qubits
above QuEra's 448 while QuEra delivers four times the logical qubits. **PRAMANA should fit
verified logical qubits, and carry the physical-to-logical ratio as a separate,
modality-specific distribution.**

This connects directly to work PRAMANA has already done: the architecture spread analysis
found up to 101x variation in physical qubits for identical assets. The hardware side shows
the same heterogeneity from the other direction. The two must be modelled jointly — pairing
a cat-qubit cost model with a superconducting trajectory is a category error.

Note also QuEra's 4.7:1 ratio implies ~1,000 logical qubits at ~4,700 atoms. Against the
2026 ECDLP circuits needing ~835–1,450 logical qubits, that arithmetic is worth stating
plainly in any exposure report.

### 3.2 Roadmap targets to ingest

| Year | Organisation | Target |
|---|---|---|
| late 2026 | Atom Computing / Microsoft / QuNorth "Magne" | 50 logical qubits |
| 2026 | IBM Kookaburra | first qLDPC memory module with attached LPU |
| 2027 | IBM Cockatoo | inter-module entanglement via l-couplers |
| 2027 | Pasqal, Quantinuum | 100+ logical qubits |
| 2027 | QuEra | 200+ logical qubits |
| 2029 | IBM Starling | 200 logical qubits, 10⁸ gates |
| 2029 | Quantinuum Apollo | universal fully fault-tolerant |
| 2029 | IonQ | 200,000 physical qubits (after the January 2026 SkyWater acquisition) |
| 2029 | PsiQuantum | ~10⁶ physical photonic qubits |
| 2033 | IBM Blue Jay | 2,000 logical qubits, 10⁹ gates |

Commentary consistently notes 100 logical qubits should be verified by roughly Q2 2027,
with 1,000 verified logical qubits unlikely before 2029.

### 3.3 The slip prior

Analysts have converged on the view that quantum roadmaps are systematically optimistic —
milestones "always seem to arrive five years from now" — and independent trackers now
maintain explicit *track record* sections to sanity-check vendor claims against past
delivery. Some argue it is irresponsible to place error correction on business roadmaps
before the underlying research is complete.

PRAMANA's slip prior should therefore be fitted from
`(actual_year − originally_announced_year)` over delivered milestones, exactly as the spec
requires. Two practical cautions:

- **Survivorship bias.** Cancelled or silently dropped milestones never produce an "actual
  year" and so never enter the fit, biasing the slip distribution optimistic. Track
  abandoned milestones explicitly as right-censored observations.
- **Definitional drift.** "Logical qubit" has meant different things across announcements.
  Prefer the verified-demonstration table in §3.1 over press-release counts, and record the
  verification standard alongside each data point.

### 3.4 Two curves, one crossing

The framing worth adopting, and which PRAMANA is unusually well placed to compute: an
**algorithmic-requirement curve** that falls as cryptanalysis improves, and a
**hardware-capability curve** that rises as engineering matures. A CRQC exists the moment
they cross.

PRAMANA already computes the first curve from first principles. §3.1–3.3 supply the second.
The break-year distribution is the distribution of their crossing. That is a cleaner
statement of the product than the spec's original framing, and it should be adopted
verbatim in the paper.

A caution that falls out of §1.3: the algorithmic curve is flattening against its floors,
so the *remaining* uncertainty in ECC exposure sits almost entirely on the hardware curve.
For RSA the same is not yet true. PRAMANA should report which curve dominates the variance
per asset class — that decomposition is a novel and directly actionable output.

---

## 4. The risk engine

### 4.1 Calibration data: the GRI expert survey

The [Global Risk Institute Quantum Threat Timeline Report 2025](https://globalriskinstitute.org/publication/quantum-threat-timeline-report-2025b/),
published 9 March 2026 by Michele Mosca and Marco Piani, surveys 26 international experts.
It has run annually since 2019, making it one of very few longitudinal datasets on this
question.

Current headline: a CRQC within 10 years is estimated at **28–49%** depending on how
responses are aggregated — the highest 10-year figure in the report's seven-year history —
and **51–70%** within 15 years.

Two distinct uses for PRAMANA:

1. **Calibration, not input.** PRAMANA's whole thesis is that break years should be
   *computed*, not surveyed. But a computed distribution that diverges wildly from 26
   experts' aggregate deserves scrutiny. Use the survey as an external validity check on
   the Monte Carlo output and report the comparison. If PRAMANA's `Z` distribution sits far
   from the expert band, that is either a finding or a bug, and either way it should be
   visible.
2. **The seven-year series as a prior on drift.** Successive reports moving earlier is
   itself data about how estimates update on new information.

### 4.2 Mosca's inequality: a notation warning

Sources in the field are inconsistent about the variable names. The spec and several
sources use **X = migration time, Y = secrecy lifetime**; other reputable sources swap
them, writing X for data shelf life and Y for migration time. The inequality `X + Y > Z` is
symmetric in the two, so the arithmetic is unaffected — but any user-facing label,
API field name or report column must be unambiguous. Recommend PRAMANA drop the letters in
its UI entirely and use `migration_years`, `secrecy_lifetime_years`, `years_to_crqc`.

### 4.3 The algorithmic improvement factor is now empirically measurable

The spec requires sampling an algorithmic improvement rate, on the grounds that a model
assuming today's algorithms are final is systematically wrong in the unsafe direction. That
was a design judgement. It is now a measurement, and the data is striking:

**RSA-2048 logical Toffoli count:**

| Year | Source | Toffoli | Logical qubits |
|---|---|---:|---:|
| 2019 | Gidney-Ekerå | 2.6e9 | 6,189 |
| 2024 | Chevignard-Fouque-Schrottenloher | 2.0e12 | ~1,100 |
| 2025 | Gidney | 6.5e9 | 1,399 |

**ECDLP-256 Toffoli count** (from §1.1): 1.29e11 (2017) → 2.0e8 (2023) → 5.6e7 (2026).

Two observations the risk engine must encode:

- **The rate is not uniform across problems.** ECC improved ~2,300x in nine years while RSA
  improved ~2.5x in six on gate count, having instead spent its progress on width. Fitting
  one global improvement rate across all asset classes would be wrong. Fit **per problem
  family**, and note the causal reason: factoring absorbed two decades of optimisation
  before the curves got comparable attention.
- **The rate must decelerate toward the floors.** §1.3 gives hard bounds — `2n` on ECC
  width, a cubic envelope on gates. An unbounded exponential improvement prior will
  eventually predict sub-floor costs, which is unphysical. Use a bounded form that
  asymptotes to the floor rather than a pure exponential.

### 4.4 Harvest-now-decrypt-later

Multiple government agencies have formally stated HNDL is already occurring, and
[arXiv:2603.01091](https://arxiv.org/html/2603.01091v1) examines its practical feasibility —
worth reading before PRAMANA asserts anything about interception probability, since the
feasibility of *capture* is a separate question from the feasibility of *decryption* and
PRAMANA currently models only the latter.

There is also a published competing model: an `I × L × A` exposure score over interception
likelihood, data longevity and algorithm vulnerability. PRAMANA's computed exposure should
be compared against it in the paper — it is the closest existing thing to a competitor, and
its weakness is precisely that `A` is a categorical judgement where PRAMANA computes a cost.

---

## 5. The PQC layer

### 5.1 Standards status, August 2026

| Standard | Status | Note |
|---|---|---|
| FIPS 203 (ML-KEM) | **Final**, 13 Aug 2024 | Key establishment |
| FIPS 204 (ML-DSA) | **Final**, 13 Aug 2024 | Signatures |
| FIPS 205 (SLH-DSA) | **Final**, 13 Aug 2024 | Hash-based signatures |
| FIPS 206 (FN-DSA, Falcon) | **In clearance** | Submitted for NIST/Commerce clearance 28 Aug 2025; draft and finalisation expected 2026–2027 |
| HQC | **Selected 11 Mar 2025** | Code-based KEM, algorithmic diversity against a lattice break; draft expected 2026, final ~2027 |
| NIST IR 8547 | **Draft** (ipd Nov 2024) | Comment period closed 10 Jan 2025; sources conflict on whether a final has been issued — **verify before citing as final** |
| NIST SP 800-208 | Final | Stateful hash-based signatures (LMS, XMSS) |
| **NIST CSWP 39** | **Final Dec 2025; upd1 published 29 Jun 2026** | Crypto agility — see §5.3 |

PRAMANA's recommendation engine should treat FN-DSA and HQC as *pending* rather than
available, and must not recommend an algorithm whose parameter sets are not yet fixed.

### 5.2 CNSA 2.0 timeline

- **1 January 2027** — all new National Security System acquisitions must be CNSA 2.0
  compliant. This is the binding near-term gate and it is 16 months away.
- **2026** — CNSSP-15 updated to deprecate CNSA 1.0; NIAP began releasing updated Protection
  Profiles incorporating CNSA 2.0 algorithms. Products not validated against the updated
  profiles are ineligible for new NSS deployments.
- **31 December 2030** — exclusive CNSA 2.0 use for networking equipment; equipment that
  cannot support it must be phased out.
- **2033** — custom applications and legacy equipment updated or replaced; NSS owners report
  progress.

Algorithms: ML-KEM-1024, ML-DSA-87, AES-256, SHA-384/512, and **LMS or XMSS (LMS 256-192
recommended) with SHA3-384/512 for software and firmware signing** — hash-based, not
lattice-based, because code that must remain verifiable for decades rests better on
conservative assumptions.

### 5.3 CSWP 39 describes PRAMANA

This is the most strategically useful finding in the dossier.

NIST CSWP 39, in describing what crypto agility requires, proposes a **"cryptographic
policy-informed risk assessment engine"** that continuously analyses the inventory, monitors
cryptography in use, and recommends or automates mitigations according to policy and risk.

That is PRAMANA's specification, written by NIST, published in final form in December 2025
and updated June 2026. The project should cite CSWP 39 as the requirement it implements.
The differentiator remains unchanged: existing tools assess risk against a *policy* and a
fixed Q-Day; PRAMANA assesses it against a *computed attack cost*.

CSWP 39 also anchors the crypto-agility scoring in §15 of the build spec, which until now
rested on PRAMANA's own dimension list. Realign those eight dimensions to CSWP 39's
vocabulary before publication.

### 5.4 The discovery ecosystem, and where PRAMANA sits

The market consolidated sharply:

- **Keyfactor** acquired **InfoSec Global** (2025) for CBOM-grade inventory, and has since
  launched a joint enterprise quantum-safe solution with **IBM Consulting** combining
  Keyfactor discovery and CLM with IBM's governance frameworks. Their agent-based approach
  puts sensors on endpoints to scan file systems, registries and memory for keys and certs,
  identify crypto libraries and versions, and inspect configurations and API calls.
- **SandboxAQ** (Alphabet spin-off, acquired Cryptosense) offers **AQtive Guard**, a
  multi-method inventory platform with AI-driven discovery at scale.
- **IBM Quantum Safe Explorer** targets enterprise codebases and mainframes.

CBOMs went from academic concept to compliance requirement in under two years, driven by
the finalised NIST standards and the **EU Cyber Resilience Act's** cryptographic
transparency mandate.

**This validates the spec's positioning decision.** PRAMANA should not compete on
discovery — that market has well-funded incumbents with endpoint agents and mainframe
parsers. It should ingest their output via CycloneDX CBOM and win on what happens *after*
discovery. The vendor material is explicit that "what comes after discovery" is the open
question, and CSWP 39 names the answer as a risk assessment engine. That is the gap.

Practical consequence: **CBOM import should be promoted from one ingestion path among many
to the primary one**, and the CycloneDX Authoritative Guide to CBOM should be the schema
PRAMANA normalises to.

---

## 6. Consolidated recommendations

Ordered by value, with the reasoning above.

1. **Retire the ECDLP sub-leading-term workstream.** Replace with Schrottenloher 2026 and
   Luo 2026 pipelines; keep Roetteler as a labelled historical baseline. PRAMANA currently
   understates ECC exposure by ~2,300x on gates, which for a triage tool is the dangerous
   direction. *(§1.4)*
2. **Publish the descent itself.** The 2017→2026 ECDLP series and the 2019→2025 RSA series
   are the empirical backing for the algorithmic-improvement term, and no other tool models
   this. *(§4.3)*
3. **Fit hardware trajectories on verified logical qubits, not physical**, carrying the
   physical-to-logical ratio as a modality-specific distribution. The ratio spans 2:1 to
   105:1 across demonstrated systems. *(§3.1)*
4. **Adopt the two-curve framing** — falling algorithmic requirement against rising hardware
   capability, CRQC at the crossing — and report which curve dominates variance per asset
   class. *(§3.4)*
5. **Cite CSWP 39 as the requirement PRAMANA implements**, and realign the crypto-agility
   dimensions to its vocabulary. *(§5.3)*
6. **Promote CBOM import to the primary ingestion path.** *(§5.4)*
7. **Replace the naive-Python reference with a three-way check** against Qualtran (circuit
   layer) and Azure QRE (QEC layer). *(§2.2)*
8. **Adopt ecdsa.fail's verification conditions** — phase cleanliness and inverse
   composition — to strengthen the claim that PRAMANA's circuits are real. *(§1.5)*
9. **Bound the algorithmic-improvement prior by the floors** so it cannot predict sub-`2n`
   width or sub-cubic gate counts. *(§4.3)*
10. **Drop the X/Y letters from all user-facing surfaces.** The literature is inconsistent
    about which is which. *(§4.2)*
11. **Model the pseudo-Mersenne advantage** so secp256k1 and P-256 are distinct targets.
    *(§1.2)*
12. **Treat FN-DSA and HQC as pending**, not available, in the recommendation engine.
    *(§5.1)*
13. **Verify NIST IR 8547's final status** before citing it as final anywhere. *(§5.1)*
14. **Track abandoned roadmap milestones as right-censored** in the slip prior, or it will
    be biased optimistic. *(§3.3)*

---

## 7. Sources

**ECDLP circuits**
- Proos & Zalka, *Shor's discrete logarithm quantum algorithm for elliptic curves*, [quant-ph/0301141](https://arxiv.org/abs/quant-ph/0301141)
- Roetteler, Naehrig, Svore & Lauter, ASIACRYPT 2017, [arXiv:1706.06752](https://arxiv.org/abs/1706.06752)
- Häner, Jaques, Naehrig, Roetteler & Soeken, [arXiv:2001.09580](https://arxiv.org/abs/2001.09580)
- Litinski, [arXiv:2306.08585](https://arxiv.org/abs/2306.08585)
- Schrottenloher, *Optimized Point Addition Circuits for ECDLP*, [arXiv:2606.02235](https://arxiv.org/abs/2606.02235); code at `gitlab.inria.fr/capsule/qarton-projects/ec-point-addition`
- Chevignard, Fouque & Schrottenloher, EUROCRYPT 2026, [ePrint 2026/280](https://eprint.iacr.org/2026/280)
- Luo et al., [arXiv:2607.13816](https://arxiv.org/abs/2607.13816), superseding [arXiv:2604.02311](https://arxiv.org/abs/2604.02311)
- *New Quantum Circuits for ECDLP*, [ePrint 2026/106](https://eprint.iacr.org/2026/106.pdf)
- *Brace for impact: ECDLP challenges for quantum cryptanalysis*, [arXiv:2508.14011](https://arxiv.org/pdf/2508.14011)
- Decoded quantum interferometry, [arXiv:2510.10967](https://arxiv.org/abs/2510.10967)
- [ecdsa.fail](https://www.ecdsa.fail/) and its [challenge repository](https://github.com/ecdsafail/ecdsafail-challenge)
- Ivezic, *Why the Quantum Attack on ECC Keeps Getting Cheaper*, [PostQuantum.com, 23 Jun 2026](https://postquantum.com/post-quantum/quantum-attack-ecc-circuit-floor/)
- APS Physics, *Quantum Hacking Coming Sooner than Expected*, [physics.aps.org/articles/v19/117](https://physics.aps.org/articles/v19/117)

**Factoring**
- Gidney & Ekerå, Quantum 5:433, [arXiv:1905.09749](https://arxiv.org/abs/1905.09749)
- Gidney, [arXiv:2505.15917](https://arxiv.org/abs/2505.15917)
- Chevignard, Fouque & Schrottenloher, [ePrint 2024/1852](https://eprint.iacr.org/2024/1852)

**Resource estimation tooling**
- Azure Quantum Resource Estimator, [arXiv:2311.05801](https://arxiv.org/abs/2311.05801)
- [Qualtran](https://qualtran.readthedocs.io/)
- AutoQuREO, [arXiv:2608.12936](https://arxiv.org/html/2608.12936)

**Hardware**
- [Quantum Logical Qubit Leaderboard 2026](https://quantumzeitgeist.com/quantum-logical-qubit-leaderboard/)
- [Logical Qubit Progress Tracker 2026](https://entangledfuture.com/stack/logical-qubits/)
- [Quantum Computing Roadmap Tracker 2026](https://entangledfuture.com/roadmaps/)
- [What 60+ Quantum Hardware Roadmaps Actually Tell Us](https://postquantum.com/quantum-computing-companies/quantum-computing-companies-roadmaps/)
- [IBM large-scale FTQC roadmap](https://www.ibm.com/quantum/blog/large-scale-ftqc)
- [Quantinuum accelerated roadmap](https://www.quantinuum.com/press-releases/quantinuum-unveils-accelerated-roadmap-to-achieve-universal-fault-tolerant-quantum-computing-by-2030)

**Threat timeline and risk**
- Mosca & Piani, *Quantum Threat Timeline Report 2025*, [Global Risk Institute, 9 Mar 2026](https://globalriskinstitute.org/publication/quantum-threat-timeline-report-2025b/)
- *On the Practical Feasibility of Harvest-Now, Decrypt-Later Attacks*, [arXiv:2603.01091](https://arxiv.org/html/2603.01091v1)
- Mosca, *Cybersecurity in an era with quantum computers*, IEEE S&P 16:38 (2018)

**Standards**
- [NIST PQC standardization](https://csrc.nist.gov/projects/post-quantum-cryptography/post-quantum-cryptography-standardization)
- NIST IR 8547 ipd, [nvlpubs](https://nvlpubs.nist.gov/nistpubs/ir/2024/NIST.IR.8547.ipd.pdf)
- [NIST CSWP 39upd1, Considerations for Achieving Crypto Agility](https://csrc.nist.gov/pubs/cswp/39/upd1/considerations-for-achieving-crypto-agility/final)
- [NSA CNSA 2.0 algorithms](https://media.defense.gov/2025/May/30/2003728741/-1/-1/0/CSA_CNSA_2.0_ALGORITHMS.PDF)
- Stern, *CNSA 2.0 and New Security Requirements*, [RSAC / NCF, 17 Mar 2026](https://cryptologicfoundation.org/wp-content/uploads/2026/03/Dr.-Morgan-Stern-QRC_NCF_Conf_20260317.published.pdf)

**Ecosystem**
- [Cryptographic Inventory Vendors and Methodologies](https://postquantum.com/post-quantum/cryptographic-inventory-vendors/)
- [Keyfactor + IBM Consulting joint solution](https://www.keyfactor.com/press-releases/keyfactor-and-ibm-consulting-launch-joint-solution-to-accelerate-enterprise-quantum-safe-transformation/)
- [NIST NCCoE Migration to PQC](https://pages.nist.gov/nccoe-migration-post-quantum-cryptography/)
