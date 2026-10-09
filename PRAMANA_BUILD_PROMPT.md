# PRAMANA — Master Build Prompt

**Cryptographic Resource Estimation & Migration Triage Engine**
*pramāṇa (प्रमाण) — "means of valid knowledge; proof"*

---

## 0. How to use this document

This is the single authoritative build specification for PRAMANA. It is written to be handed to an implementing agent (or a human engineer) as a complete brief. It assumes no prior context.

**Reading contract for the implementer:**

1. Read §1–§3 fully before writing a line of code. They define what makes this project non-trivial and what makes it fail.
2. §6 defines the build order. Do not deviate. Phases are ordered so that the hardest, most falsifiable work (the QEC models and their verification) lands *first*, before any UI exists. If you build the UI first, you will build a spreadsheet with a UI, which is the documented failure mode.
3. §7–§19 are layer specifications. Each has: purpose, inputs, outputs, algorithms, formulas, file paths, and acceptance tests.
4. §22 is the acceptance checklist. The project is not done until every box is checked.
5. Every numeric constant that appears in this document is a **target to be reproduced**, not a value to be hardcoded as an answer. See §2, Law 1.

**Prime directive:** PRAMANA computes. It does not look up. Every number the system reports to a user must be traceable to a computation performed by PRAMANA's own kernels on that user's own asset, not to a table of numbers copied from a paper.

---

## 1. Mission, thesis, and novelty claim

### 1.1 The one-sentence pitch

A platform that ingests an organisation's real cryptographic inventory and computes, per asset, the year in which that asset becomes quantum-breakable — by actually synthesising the attack circuit, costing its error correction across multiple fault-tolerant architectures, and resolving Mosca's inequality against the asset's data-retention requirement.

### 1.2 The gap

Two mature tool categories exist and do not touch each other:

| Category | Examples | What they do | What they don't do |
|---|---|---|---|
| Quantum resource estimators | Azure QRE, 1QBit TopQAD, Bench-Q, pyLIQTR | Estimate physical resources for *an algorithm* | Know nothing about your certificates |
| Crypto discovery / CBOM | Keyfactor, Venafi, IBM Quantum Safe Explorer, `cryptobom-forge` | Inventory *certificates and crypto usage* | Assume a fixed, hand-waved "Q-Day" |

Every published PQC migration roadmap prioritises by **asset criticality** plus a fixed Q-Day assumption ("2030-something"). Nobody prioritises by **computed attack cost against the specific key on the specific asset under a specific fault-tolerant architecture**.

PRAMANA is the bridge. It is the first system where the resource-estimation result *is* the migration decision, not an input to a human judgement call.

### 1.3 Novelty claim (this is the abstract of the paper)

> We present the first cryptographic risk-assessment framework that derives asset-level quantum exposure from first-principles resource estimation across heterogeneous fault-tolerant architectures, rather than from a fixed Q-Day assumption. We show that architecture choice shifts the estimated exposure window by N years for identical assets, and that migration prioritisations derived from asset criticality alone diverge substantially from those derived from computed attack cost.

The second sentence is a real, defensible, novel empirical finding that falls out of the tool for free once it works. **The tool must be capable of producing the evidence for it**: that means the architecture-sensitivity sweep (§11.5) and the prioritisation-divergence analysis (§13.6) are *first-class features*, not afterthoughts.

### 1.4 Design commitments that follow

- **Causal coupling is mandatory.** The quantum result must be structurally inseparable from the crypto decision. There must be no code path where a break-year is assigned to an asset without a circuit having been synthesised and costed for that asset's exact key parameters.
- **Multi-architecture is mandatory.** A single-architecture estimate is a point estimate and therefore a Q-Day assumption in disguise. The system's output is a *distribution over architectures and hardware trajectories*.
- **Reproduction is the proof of correctness.** If the surface-code model does not reproduce Gidney's published RSA-2048 figure within tolerance, the model is wrong and the tool is untrustworthy. The reproduction harness is not a test suite; it is the scientific contribution.

---

## 2. The twelve non-negotiable laws

These are enforcement rules. CI must fail if any is violated where mechanically checkable.

**Law 1 — No lookup tables of results.**
No file in the repository may contain a mapping from `(algorithm, key size) → (physical qubits, runtime)` that is read at request time. Published numbers appear in exactly one place: `verification/golden/*.toml`, which is consumed *only* by the verification harness, never by the estimation path. CI check: grep the runtime crates for the golden constants; any hit fails the build.

**Law 2 — Every reported number is computed.**
`AssetExposure.break_year_distribution` must carry a `provenance` chain: circuit → logical resource counts → QEC model → physical resources → hardware trajectory → year. Any missing link is a hard error, not a warning.

**Law 3 — Uncertainty is structural, not decorative.**
No API returns a bare scalar for anything derived from hardware forecasting. Break years are distributions. Exposure scores carry confidence intervals. A point estimate without an interval is a bug.

**Law 4 — Architectures are pluggable and symmetric.**
Adding a fifth QEC architecture must require implementing one trait and registering it. No architecture may be special-cased in the pipeline. CI check: a synthetic `NullArchitecture` must flow end-to-end without touching non-architecture code.

**Law 5 — Determinism and reproducibility.**
Every estimation job records: input hash, model version, parameter set hash, RNG seed, git commit. Re-running a job with the same manifest must reproduce byte-identical results. Monte Carlo uses explicit seeded PRNG (`rand_chacha::ChaCha20Rng`), never thread-local entropy.

**Law 6 — Units are typed.**
Physical qubits, logical qubits, Toffoli count, T count, code cycles, seconds, qubit-seconds, and qubit-rounds are distinct newtypes in Rust. Arithmetic between incompatible units must not compile. This prevents the single most common class of resource-estimation error.

**Law 7 — Costing is monotone-checked.**
Property tests must assert: increasing key size never decreases cost; decreasing physical error rate never increases physical qubit count at fixed target error; increasing target logical error rate never increases distance. Violations indicate model bugs.

**Law 8 — Assumptions are explicit and versioned.**
Every model carries an `AssumptionSet` struct that is serialised into every result. A user must be able to read, from any output, exactly what was assumed (error rate, cycle time, reaction time, connectivity, distillation strategy, routing overhead).

**Law 9 — No silent fallbacks.**
If a certificate cannot be parsed, an algorithm is unrecognised, or a model is out of its validated regime, the system emits a typed error and marks the asset `INDETERMINATE`. It never guesses and never substitutes a default. An `INDETERMINATE` asset is surfaced prominently in the UI.

**Law 10 — Standards citations are structured.**
Every migration recommendation carries machine-readable citations: `{document: "NIST IR 8547", section: "...", clause: "..."}`. No prose-only justifications.

**Law 11 — The hot path is Rust.**
Circuit synthesis, resource counting, QEC costing, and Monte Carlo sampling live in Rust. Python orchestrates, persists, and serves. Any Python implementation of a costing kernel is a bug. Python is permitted a *reference implementation* only inside `verification/reference/`, used solely for cross-checking the Rust.

**Law 12 — Read the paper, then verify against it.**
Constants in this document are stated to two or three significant figures for orientation. Before finalising any golden file, the implementer must retrieve the primary source and extract the exact figures and the exact assumption set from its tables. Every golden entry records its source (arXiv ID, table/figure number, page).

---

## 3. Glossary and notation

| Symbol | Meaning | Unit |
|---|---|---|
| `n` | Bit length of the modulus (RSA) or curve order (ECC) | bits |
| `p` | Physical gate/measurement error rate (uniform depolarising) | dimensionless |
| `p_th` | Code threshold | dimensionless |
| `d` | Code distance | dimensionless (odd integer) |
| `p_L` | Logical error rate per logical qubit per code cycle | dimensionless |
| `ε_tot` | Total tolerable failure probability of the whole computation | dimensionless |
| `t_cycle` | Duration of one syndrome-extraction round | seconds |
| `t_react` | Classical control feedback latency | seconds |
| `Q_log` | Peak logical qubit count (algorithmic + routing) | logical qubits |
| `Q_phys` | Total physical qubits | physical qubits |
| `N_Tof` | Toffoli gate count | count |
| `N_T` | T gate count (`N_T = 4·N_Tof` for standard decomposition, 2 with measurement-and-fixup) | count |
| `D_T` | T-depth / number of sequential magic-state consumption layers | count |
| `V` | Spacetime volume | qubit-rounds |
| `X` | Migration time for an asset | years |
| `Y` | Required secrecy lifetime of the data | years |
| `Z` | Time until a cryptographically relevant quantum computer (CRQC) exists | years |

**Mosca's inequality:** if `X + Y > Z`, the asset is already exposed. PRAMANA computes `Z` per asset and per architecture as a distribution, so the inequality resolves to a *probability of exposure*, not a boolean.

**CRQC (per-asset definition):** PRAMANA rejects the global CRQC notion. A CRQC is defined *relative to an asset*: a machine whose available physical qubit count and error rate are sufficient to run the synthesised attack circuit for that asset's key within an attacker-relevant wall-clock budget.

---

## 4. Repository layout

```
pramana/
├── README.md
├── LICENSE                                # Apache-2.0
├── CITATION.cff
├── Makefile                               # one-command dev entry points
├── docker-compose.yml
├── docker-compose.prod.yml
├── .github/workflows/{ci,verify,release}.yml
│
├── crates/                                # ── RUST: all computation ──
│   ├── pramana-units/                     # newtypes, dimensional safety (Law 6)
│   │   └── src/{lib,qubits,gates,time,volume,error_rate}.rs
│   ├── pramana-circuit/                   # circuit IR + synthesis
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ir.rs                      # CircuitIR, GateKind, resource accumulator
│   │       ├── arith/
│   │       │   ├── adder.rs               # Cuccaro ripple, carry-lookahead, Gidney temp-AND
│   │       │   ├── modmul.rs              # windowed modular multiplication
│   │       │   ├── modexp.rs              # windowed modular exponentiation
│   │       │   ├── lookup.rs              # QROM / unary-iteration table lookup
│   │       │   ├── residue.rs             # Chevignard–Fouque–Schrottenloher approx residue
│   │       │   └── ec_point.rs            # EC point addition, windowed scalar mult
│   │       ├── shor_factoring.rs          # GE19 and G25 pipelines
│   │       ├── shor_ecdlp.rs              # Roetteler-style + Gouzien-style pipelines
│   │       ├── grover_symmetric.rs        # AES/SHA Grover cost (for completeness)
│   │       └── ekera_hastad.rs            # variant with reduced circuit repetitions
│   ├── pramana-qec/                       # ── THE CORE CONTRIBUTION ──
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── model.rs                   # QecArchitecture trait (Law 4)
│   │       ├── budget.rs                  # error budget allocation
│   │       ├── magic/
│   │       │   ├── mod.rs                 # MagicStateFactory trait
│   │       │   ├── distillation.rs        # 15-to-1, 116-to-12, Litinski block strategies
│   │       │   └── cultivation.rs         # Gidney–Shutty–Jones cultivation
│   │       ├── surface.rs                 # rotated surface code + lattice surgery
│   │       ├── yoked.rs                   # yoked surface codes for idle storage
│   │       ├── qldpc.rs                   # bivariate bicycle / gross code family
│   │       ├── neutral_atom.rs            # transversal / correlated decoding, O(1) rounds
│   │       ├── cat.rs                     # repetition cat code, biased noise
│   │       └── routing.rs                 # layout, ancilla, connectivity overheads
│   ├── pramana-hardware/                  # vendor roadmap fitting + trajectories
│   │   └── src/{lib,roadmap,fit,trajectory,uncertainty}.rs
│   ├── pramana-risk/                      # Monte Carlo break-year engine
│   │   └── src/{lib,montecarlo,mosca,exposure,sensitivity}.rs
│   ├── pramana-core/                      # orchestration of the above
│   │   └── src/{lib,pipeline,manifest,provenance}.rs
│   ├── pramana-py/                        # PyO3 bindings — thin, no logic
│   │   └── src/lib.rs
│   └── pramana-cli/                       # standalone CLI (works without the web app)
│       └── src/main.rs
│
├── verification/                          # ── PROOF OF CORRECTNESS ──
│   ├── golden/
│   │   ├── gidney_ekera_2019.toml
│   │   ├── gidney_2025.toml
│   │   ├── roetteler_2017_ecdlp.toml
│   │   ├── gouzien_2023_cat.toml
│   │   ├── litinski_2019_surface.toml
│   │   ├── bravyi_2024_bb_codes.toml
│   │   └── cain_2025_transversal.toml
│   ├── harness/                           # runs models against golden, emits report
│   ├── reference/                         # slow, obviously-correct Python cross-checks
│   └── reports/                           # generated, committed on release tags
│
├── backend/                               # ── PYTHON: orchestration & serving ──
│   ├── pyproject.toml
│   ├── alembic/
│   └── app/
│       ├── main.py
│       ├── config.py
│       ├── db/{session,base,models/*.py}
│       ├── schemas/*.py                   # Pydantic v2
│       ├── api/v1/{assets,inventory,estimation,exposure,architectures,
│       │            recommendations,reports,sensitivity,admin}.py
│       ├── services/
│       │   ├── estimation.py              # calls pramana_py
│       │   ├── ingestion/
│       │   │   ├── x509.py  tls_scan.py  ssh.py  jwt.py  codesign.py
│       │   │   ├── cbom.py                # CycloneDX CBOM import/export
│       │   │   └── normalise.py           # → canonical CryptoAsset
│       │   ├── agility.py                 # crypto-agility scoring
│       │   ├── recommend.py               # PQC target selection
│       │   └── standards/                 # NIST IR 8547, CNSA 2.0, SP 800-131A rules
│       ├── workers/{celery_app,tasks}.py
│       └── tests/
│
├── frontend/                              # ── REACT ──
│   ├── package.json  vite.config.ts  tailwind.config.ts
│   └── src/
│       ├── main.tsx  App.tsx
│       ├── api/                           # generated from OpenAPI
│       ├── components/
│       │   ├── ExposureTimeline/          # ★ THE HERO VISUALISATION
│       │   ├── ArchitectureComparison/
│       │   ├── AssetTable/
│       │   ├── CircuitResourcePanel/
│       │   ├── QecBreakdown/
│       │   ├── MoscaResolver/
│       │   ├── SensitivityExplorer/
│       │   └── ProvenanceTrace/
│       └── pages/{Dashboard,Inventory,Asset,Architectures,Verification,Reports}.tsx
│
├── demo/
│   ├── hybrid_tls/                        # X25519MLKEM768 client+server via liboqs
│   └── fixtures/                          # synthetic enterprise inventory (500+ assets)
│
├── docs/
│   ├── architecture.md  models.md  assumptions.md  api.md
│   ├── verification.md                    # the reproduction writeup
│   └── paper/                             # figures, tables, LaTeX
└── scripts/{bootstrap,seed,bench,generate_figures}.sh
```

---

## 5. Technology stack

**Rust** (edition 2021, MSRV 1.78+)
`serde`, `serde_json`, `toml`, `thiserror`, `anyhow`, `rayon` (parallel Monte Carlo), `rand` + `rand_chacha` (seeded, reproducible), `nalgebra` or `ndarray` (roadmap curve fitting), `statrs` (distributions), `num-bigint`, `criterion` (benchmarks), `proptest` (property tests), `pyo3` 0.22+ + `maturin` (bindings), `clap` (CLI), `tracing`.

**Python** (3.11+)
`fastapi`, `uvicorn[standard]`, `pydantic` v2, `sqlalchemy` 2.x, `alembic`, `psycopg[binary]`, `celery[redis]`, `redis`, `cryptography` (X.509), `asn1crypto`, `pyOpenSSL`, `sslyze` (TLS scanning), `paramiko` (SSH host keys), `pyjwt`, `cyclonedx-python-lib` (CBOM), `httpx`, `structlog`, `pytest`, `pytest-asyncio`, `hypothesis`, `ruff`, `mypy --strict`.

**Frontend**
React 18 + TypeScript (strict), Vite, TanStack Query, TanStack Table, Zustand, D3 (scales/axes/shapes — hand-composed SVG for the timeline; do not use a chart wrapper for the hero view), Recharts (secondary charts only), Tailwind CSS, Radix UI primitives, `openapi-typescript` for the client.

**Infrastructure**
PostgreSQL 16 (`ltree` for the asset graph, `JSONB` for provenance, `pg_trgm` for search), Redis 7 (broker + result backend + cache), Docker Compose for dev, multi-stage Dockerfiles for prod, GitHub Actions CI.

**PQC / demo**
`liboqs` + `oqs-provider` for OpenSSL 3.x, for the hybrid TLS demonstration.

---

## 6. Phase plan

Each phase has a hard exit criterion. Do not start phase N+1 until phase N's exit criterion passes in CI.

| Phase | Deliverable | Exit criterion |
|---|---|---|
| **P0** | Repo scaffold, `pramana-units`, CI green, Docker up | `make bootstrap && make test` passes on clean clone |
| **P1** | Circuit IR + arithmetic primitives + resource accumulator | Adder/modmul Toffoli counts match closed-form formulas in property tests across `n ∈ [8, 4096]` |
| **P2** | Shor factoring pipelines (GE19, G25), ECDLP pipeline | Logical qubit and Toffoli counts reproduce published *logical* figures within tolerance |
| **P3** | `QecArchitecture` trait + surface code + magic state factories | Surface-code model reproduces Litinski and Gidney–Ekerå 2019 physical figures |
| **P4** | Yoked codes + cultivation → Gidney 2025 reproduction | **Reproduces < 1M physical qubits and < 1 week for RSA-2048.** This is the project's keystone. |
| **P5** | qLDPC, neutral-atom, cat-qubit architectures | Each reproduces its own source paper's headline figure |
| **P6** | Verification harness + report generation | `make verify` emits a signed report; all golden entries pass or are explicitly waived with documented reason |
| **P7** | Hardware trajectory model + Monte Carlo break-year engine | Break-year distributions produced with calibrated uncertainty bands; sensitivity sweep runs |
| **P8** | PyO3 bindings, FastAPI, Postgres, Celery, ingestion, Mosca, recommendations, agility | End-to-end: upload a cert bundle → get per-asset exposure distributions |
| **P9** | Frontend, hero timeline, hybrid TLS demo, paper figures | Screenshot-quality Figure 1; demo TLS handshake negotiates `X25519MLKEM768` |

### 6.1 Solo six-month schedule

No hardware access is required and no cloud quantum time is consumed, so cost is not a constraint. **Scope control is the only real risk on this project.** Indicative allocation for one person:

| Weeks | Focus | Notes |
|---|---|---|
| 1–2 | P0 + P1 | Scaffold, units, circuit IR, adders, QROM. Unglamorous and load-bearing. |
| 3–6 | P2 | Shor factoring + ECDLP synthesis. Expect to spend most of this reading papers carefully, not typing. |
| 7–11 | P3 + P4 | Surface code, factories, yoked codes. **P4 is the keystone; budget slip here and protect it.** |
| 12–15 | P5 | Three further architectures. This is the compressible phase. |
| 16–17 | P6 | Verification harness, discrepancy diagnosis, report generation. |
| 18–20 | P7 | Roadmap fitting, Monte Carlo, sensitivity sweep. |
| 21–23 | P8 | Bindings, API, DB, ingestion, Mosca, recommendations, agility. |
| 24–26 | P9 | Frontend, hero timeline, TLS demo, paper figures. |

**Guardrails.** P0–P6 are non-negotiable and constitute the research contribution — they are what makes this publishable and what makes the resume claim true. P7–P9 make it a product. If time runs short, cut breadth in P5 (ship two architectures instead of four, chosen to be maximally *different* — surface and cat, not surface and yoked-surface) before cutting depth in P4 or P6. Two architectures still supports the novelty claim; a failed reproduction does not.

**Publishable intermediate results.** Do not wait for the whole system. The P6 verification report is a standalone contribution (a reproduction study and benchmark suite for cryptographic resource estimators) and can be written up independently if the schedule slips. Treat it as a checkpoint deliverable, not a milestone on the way to one.

---

## 7. Layer A — Quantum cost core: circuit synthesis (`crates/pramana-circuit`)

### 7.1 Purpose

Construct and **measure** attack circuits. PRAMANA never simulates a quantum state and never executes a circuit. It builds a structural representation and extracts resource counts from it.

Critically: this must be **real synthesis, not formula evaluation.** The circuit is assembled from primitives; the counts fall out of the assembly. A closed-form formula may be used *only* as a property-test oracle to check the synthesised count, never as the production code path. This distinction is what separates PRAMANA from a spreadsheet.

### 7.2 Circuit IR (`ir.rs`)

```rust
pub enum GateKind {
    Clifford(CliffordKind),          // free under surface code (transversal / Pauli frame)
    Toffoli,                          // costed as magic states
    TGate,
    AndCompute { uncompute: bool },   // Gidney temporary-AND: 4 T to compute, 0 to uncompute
    Measure { basis: Basis },
    Rotation { angle_bits: u32 },     // costed via Ross–Selinger synthesis
    QromLookup { table_bits: u64, index_bits: u32 },
}

pub struct CircuitIR {
    pub name: String,
    pub logical_qubits: LogicalQubits,     // peak concurrent allocation
    pub ancilla_high_water: LogicalQubits,
    pub layers: Vec<Layer>,                 // for depth / parallelism accounting
    pub resources: ResourceCount,
    pub assumptions: AssumptionSet,
    pub provenance: Vec<SynthesisStep>,
}

pub struct ResourceCount {
    pub toffoli: u128,
    pub t_gates: u128,
    pub measurements: u128,
    pub clifford: u128,
    pub t_depth: u64,                       // sequential magic-state consumption layers
    pub reaction_depth: u64,                // layers requiring classical feedback
    pub qubit_allocation_profile: Vec<(u64, LogicalQubits)>, // time-resolved, for peak
}
```

Requirements:
- The IR must support **allocation/deallocation of ancilla with a high-water tracker**, so peak logical qubit count is measured, not assumed.
- Rotation synthesis: `Rotation` with `b` bits of precision costs `≈ 3·log2(1/ε)` T gates (Ross–Selinger); implement the standard `c₀·log2(1/ε) + c₁` fit with configurable constants.
- `AndCompute` is the workhorse: Gidney's temporary-AND construction gives a Toffoli for 4 T gates with measurement-based uncomputation costing 0 T. Model this explicitly — it materially changes counts.
- A `CircuitBuilder` must expose `.scope("name")` so the provenance chain records which subroutine contributed which gates. The UI displays this breakdown.

### 7.3 Arithmetic primitives (`arith/`)

Implement, with resource counts emerging from construction:

**`adder.rs`**
- Cuccaro ripple-carry adder: `n` Toffolis, `n+1` ancilla (in the `AndCompute` formulation: `n` AND-computes → `4n` T).
- Carry-lookahead adder (Draper et al.): lower depth, higher qubit count. Both must exist so the pipeline can trade space against time.
- Controlled and modular variants.

**`lookup.rs`**
- QROM / unary iteration (Babbush et al., Berry et al.): reading a table of `L` entries of `w` bits costs `L − 1` Toffolis with `w` output qubits; the "measurement uncomputation" trick reduces uncompute to `O(√L)`. Implement both the plain and the SELECT–SWAP (QROAM) variants with an optimiser that picks the cheaper for given `(L, w)`.
- This is the engine behind windowed arithmetic; get it right.

**`modmul.rs` / `modexp.rs`**
- Windowed modular exponentiation à la Gidney–Ekerå: with exponent window `w_e` and multiplication window `w_m`, replace controlled multiplications with table lookups plus additions.
- Expose `w_e`, `w_m` as tunable parameters and implement an **optimiser** that minimises total Toffoli count (or a configurable space–time objective) over the window sizes. Do not hardcode the paper's chosen windows; find them. Recovering the paper's optimal windows independently is itself a verification signal.
- Coset representation of modular arithmetic (Zalka) for padding-based modular reduction — include the `⌈log2 n⌉ + O(1)` padding qubits in the accounting.

**`residue.rs` — Chevignard–Fouque–Schrottenloher approximate residue arithmetic**
This is the key ingredient in the 2025 qubit-count reduction. The core idea: rather than maintaining the full `n`-bit modular register, compute in a residue/approximate representation that requires roughly `n/2 + o(n)` qubits at the cost of increased gate count. Implement:
- The approximate modular reduction with a bounded, tracked error term.
- An explicit accounting of the **success probability degradation** and the number of circuit repetitions required to compensate. The repetition count multiplies the wall-clock estimate and must not be dropped.
- Ekerå–Håstad post-processing, which reduces the required number of runs relative to naive Shor — implement in `ekera_hastad.rs` and let the pipeline select it.

**`ec_point.rs`**
- Reversible elliptic-curve point addition over `GF(p)` (Roetteler–Naehrig–Svore–Lauter construction): modular inversion via the reversible extended binary GCD dominates the cost. Implement modular inversion, multiplication, squaring, and the full controlled point addition.
- Windowed scalar multiplication for the ECDLP oracle.

### 7.4 Attack pipelines

```rust
pub trait AttackCircuit {
    fn synthesise(&self, target: &CryptoTarget, opts: &SynthesisOptions)
        -> Result<CircuitIR, SynthesisError>;
    fn repetitions(&self) -> RepetitionModel;   // expected runs incl. success probability
    fn id(&self) -> &'static str;
}
```

Implement at minimum:

1. `ShorFactoringGE19` — Gidney–Ekerå 2019 construction. Orientation targets for `n = 2048`: logical qubits on the order of `3n + 0.002·n·lg n ≈ 6.2×10³`; Toffoli count on the order of `0.3n³ + 0.0005·n³·lg n ≈ 2.7×10⁹`. Verify against the paper's tables.
2. `ShorFactoringG25` — Gidney 2025: CFS residue arithmetic + Ekerå–Håstad, roughly a 4× reduction in logical qubits (order `1.4×10³` for `n=2048`) at roughly a 2–3× increase in Toffoli count. Verify against the paper.
3. `ShorEcdlpRNSL` — Roetteler et al. 2017. Orientation targets for `n = 256`: logical qubits on the order of `9n + 2⌈lg n⌉ + 10 ≈ 2.3×10³`; Toffoli count on the order of `448·n³·lg n + 4090·n³ ≈ 1.3×10¹¹`.
4. `ShorEcdlpOptimised` — incorporating the improved adders and point-addition circuits from the 2025–2026 literature; parameterised so newer results can be slotted in.
5. `GroverSymmetric` — AES-128/192/256 and SHA-2/3 preimage costing, including the NIST-style depth-limited (`MAXDEPTH`) analysis. Needed so PRAMANA can honestly report that symmetric assets are *not* urgent, which is itself a triage output.

Each pipeline declares its **validity regime** (`n` range, assumptions). Outside it: `SynthesisError::OutOfRegime` (Law 9).

### 7.5 Acceptance tests for Layer A

- Property: synthesised Toffoli count of the modular exponentiation equals the analytic oracle within 1% for `n ∈ {512, 1024, 1536, 2048, 3072, 4096}`.
- Property: peak logical qubit count is monotone non-decreasing in `n`.
- Property: window optimiser output is a local minimum (perturbing `w_e`, `w_m` by ±1 does not reduce cost).
- Golden: logical resource counts for RSA-2048 and P-256 match `verification/golden/*.toml` within stated tolerance.

---

## 8. Layer B — QEC cost models (`crates/pramana-qec`)

**This is the core abstraction and the main technical contribution. Everything else in PRAMANA is plumbing around this.**

### 8.1 The trait (Law 4)

```rust
pub struct QecInput {
    pub logical_qubits: LogicalQubits,
    pub toffoli_count: u128,
    pub t_count: u128,
    pub t_depth: u64,
    pub reaction_depth: u64,
    pub measurement_count: u128,
    pub target_total_error: f64,        // ε_tot for the whole run
    pub qubit_time_profile: Vec<(u64, LogicalQubits)>,
}

pub struct HardwareParams {
    pub physical_error_rate: f64,       // p
    pub cycle_time: Duration,           // t_cycle
    pub reaction_time: Duration,        // t_react
    pub connectivity: Connectivity,     // SquareGrid | Degree(k) | Reconfigurable | AllToAll
    pub error_bias: Option<f64>,        // for cat qubits: η = p_phase / p_flip
    pub measurement_error: Option<f64>,
    pub atom_move_time: Option<Duration>,
}

pub struct QecEstimate {
    pub physical_qubits: PhysicalQubits,
    pub wall_clock: Duration,
    pub spacetime_volume: QubitRounds,
    pub code_distance: Option<u32>,
    pub distance_schedule: Vec<(Region, u32)>,   // memory vs. compute vs. factory
    pub magic_state_factory: FactoryReport,
    pub breakdown: ResourceBreakdown,             // data / routing / factory / idle
    pub logical_error_achieved: f64,
    pub limiting_factor: LimitingFactor,          // ReactionLimited | FactoryLimited |
                                                  // RoutingLimited | MemoryLimited
    pub assumptions: AssumptionSet,
    pub warnings: Vec<ModelWarning>,
}

pub trait QecArchitecture: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn estimate(&self, input: &QecInput, hw: &HardwareParams)
        -> Result<QecEstimate, QecError>;
    fn validity(&self) -> ValidityRegime;
    fn citations(&self) -> &[Citation];
}
```

`LimitingFactor` is not cosmetic — it drives the UI's explanation of *why* an asset has the exposure it has, and it is the mechanism by which the architecture-sensitivity finding becomes interpretable.

### 8.2 Error budget allocation (`budget.rs`)

Given `ε_tot`, split across:
- Idle/memory storage error: `ε_mem`
- Logical operation (lattice surgery / transversal) error: `ε_op`
- Magic state error: `ε_magic`

Default: equal thirds, configurable. Then solve for the distance in each region independently — this is what produces `distance_schedule` and is a meaningful modelling improvement over uniform-distance estimates.

### 8.3 Surface code (`surface.rs`)

**Logical error rate per patch per round:**
```
p_L(d, p) = A · (p / p_th)^⌊(d+1)/2⌋
```
with `A ≈ 0.1` and `p_th ≈ 0.01` for circuit-level depolarising noise on a square grid. Both constants configurable via `AssumptionSet`; the defaults must be calibrated so the model reproduces the golden numbers, and the calibration procedure documented in `docs/models.md`.

**Distance selection:** smallest odd `d` such that
```
p_L(d, p) · V_logical  ≤  ε_region
```
where `V_logical` is the logical qubit-rounds in that region.

**Space:** a rotated surface code patch of distance `d` needs `2d² − 1` physical qubits (`d²` data + `d²−1` measure). Add routing/ancilla overhead: for lattice-surgery-based computation, a standard fast-block layout costs roughly a factor of 1.5–2× the data patch area. Model this explicitly in `routing.rs` with a selectable layout (`CompactBlock`, `IntermediateBlock`, `FastBlock` — the Litinski taxonomy), not as a fudge factor.

**Time:** two regimes, and the model must report which binds.
```
t_reaction_limited = reaction_depth · t_react
t_factory_limited  = (N_T / factory_throughput) · t_cycle
t_wall             = max(t_reaction_limited, t_factory_limited, t_data_movement)
```

**Spacetime volume:** `V = Q_phys · (t_wall / t_cycle)` in qubit-rounds. This is the architecture-neutral currency for cross-comparison (§8.8).

### 8.4 Magic state factories (`magic/`)

```rust
pub trait MagicStateFactory {
    fn cost_per_state(&self, target_error: f64, hw: &HardwareParams)
        -> FactoryCost;   // { physical_qubits, cycles_per_state, output_error, footprint }
    fn throughput(&self, allocated_qubits: PhysicalQubits, hw: &HardwareParams) -> f64;
}
```

**`distillation.rs`** — Bravyi–Kitaev 15-to-1 and its multi-level compositions; Litinski's `(15-to-1)^n` and 116-to-12 block constructions with their published footprints and error scaling `ε_out ≈ 35·ε_in³` per 15-to-1 level. Implement the level-selection optimiser: choose the cheapest composition meeting `ε_magic / N_T`.

**`cultivation.rs`** — Gidney–Shutty–Jones magic state cultivation: grows T states inside a surface code patch at roughly the cost of a lattice-surgery CNOT of equivalent reliability, reaching logical error `~2×10⁻⁹` at `p = 10⁻³` and `~4×10⁻¹¹` at `p = 5×10⁻⁴`, using roughly an order of magnitude fewer qubit-rounds than prior distillation. Implement the cost model with an escape/retry accounting (cultivation is post-selected; the retry rate matters).

**Factory sizing:** the pipeline must solve the allocation problem — how many parallel factories to instantiate so that `factory_throughput ≥ N_T / t_reaction_limited`, since over-provisioning factories beyond the reaction limit wastes qubits. Report the chosen allocation in `FactoryReport`.

### 8.5 Yoked surface codes (`yoked.rs`)

Gidney–Newman–Brooks–Jones: concatenate an outer classical/quantum code across many surface-code patches to protect *idle* logical qubits at reduced overhead, roughly halving the space cost of memory at the price of more complex decoding and slower access. Model:
- Two-tier storage: "hot" working qubits at full surface-code distance, "cold" idle qubits under the yoke at reduced distance.
- The access latency penalty for pulling a cold qubit into the working set.
- The classification of which of the algorithm's logical qubits are idle for what fraction of the run — this comes from `qubit_time_profile` in `QecInput`, which is why Layer A must produce it.

This is a major contributor to the Gidney 2025 result. Without it, P4 will not reproduce.

### 8.6 qLDPC / bivariate bicycle (`qldpc.rs`)

Bravyi–Cross–Gambetta–Maslov–Rall–Yoder bivariate bicycle codes. Implement the code family generically:
- A BB code is specified by two polynomials over a bivariate group algebra `F₂[x,y]/(xˡ−1, yᵐ−1)`. Construct `A = A₁+A₂+A₃`, `B = B₁+B₂+B₃` from monomial terms; the CSS check matrices are `H_X = [A | B]`, `H_Z = [Bᵀ | Aᵀ]`.
- **Compute `n`, `k`, and the code rate directly from the construction** (rank over `GF(2)`); do not hardcode. Distance `d` may be taken from published values or bounded numerically — document which, and mark estimated distances as such.
- Include the `[[144,12,12]]` "gross" code, `[[72,12,6]]`, `[[288,12,18]]`, and the larger family members. The gross code uses 144 data + 144 check = 288 physical qubits for 12 logical qubits.
- Weight-6 checks and the required degree-6 connectivity graph, including the long-range couplers — model the connectivity requirement explicitly as a hardware constraint, since it is the main practical objection to qLDPC.
- **Logical operations are the hard part and must not be hand-waved.** qLDPC codes lack cheap transversal gates; computation requires lattice surgery via ancilla systems or code switching to a surface code for magic state injection. Model the overhead of the chosen scheme (e.g. the "tour de gross" modular architecture approach) and state the assumption loudly. A qLDPC model that counts only memory overhead and ignores computation overhead is the single easiest way to produce a wrong and over-optimistic answer. Do not do it.
- Report the memory-only figure *and* the with-computation figure separately, so the user can see the gap. Aggressive published claims in this family (e.g. the "Pinnacle"-style accounting asserting RSA-2048 with fewer than 100,000 qubits) rest almost entirely on how the logical-operation overhead is charged. PRAMANA's job is to make that charge explicit and let the user see what the claim costs when it is included. Add such claims to the golden set and diagnose the divergence rather than dismissing or adopting them.

### 8.7 Neutral atom transversal (`neutral_atom.rs`)

Based on the Cain et al. / Zhou et al. algorithmic fault tolerance line of work:
- Transversal logical gates between surface-code (or other CSS) patches, enabled by physically moving atoms.
- **Correlated decoding reduces syndrome-extraction rounds per logical operation from `O(d)` to `O(1)`** — this is the central claim and the source of the space–time advantage. Model the round count as `c · 1` with configurable `c`, and compare against the `O(d)` baseline in the same run so the saving is visible.
- Atom movement time as an explicit cost term (`atom_move_time`), typically the dominant time cost, since it is far slower than a superconducting cycle.
- Atom loss and reloading overhead as a configurable leakage/erasure channel.
- Reconfigurable connectivity means routing overhead is much lower than square-grid; model this as a distinct `Connectivity::Reconfigurable` branch in `routing.rs`, not as a scaling constant.
- Report the space–time tradeoff curve, not a point: the architecture's characteristic result is that you can buy time with qubits over a wide range.

### 8.8 Cat qubits / bosonic (`cat.rs`)

Alice & Bob repetition-cat architecture (Gouzien–Ruiz–Le Régent–Guillaud–Sangouard):
- Cat qubits exponentially suppress bit-flips with mean photon number `n̄`; only phase-flips remain, so a **1D repetition code** suffices rather than a 2D surface code. This is the source of the large claimed overhead reduction.
- Model: `p_flip ∝ exp(−2n̄)`, `p_phase ∝ n̄ · κ₁/κ₂` where `κ₁/κ₂` is the single-to-two-photon loss ratio. Repetition code of distance `d` gives `p_L ≈ (p_phase/p_th)^((d+1)/2)`.
- The published reference point: 256-bit ECDLP in ~9 hours with 126,133 cat qubits, at `κ₁/κ₂ = 10⁻⁵`, cycle time 500 ns, `n̄ ≈ 19`. This is the golden target for this architecture.
- Model the bias parameter `η` explicitly and expose it — the vendor's headline overhead advantage (commonly quoted as roughly a 30:1 reduction in physical qubits versus an unbiased surface-code baseline) is entirely a function of achievable bias. Reproduce that ratio from the model rather than asserting it, then sweep `η` downward and show how fast the advantage erodes. That erosion curve is one of the most useful figures the project can produce, because it converts a marketing number into a falsifiable engineering requirement.
- Include the physical-resource caveat: cat qubits require substantial microwave hardware per qubit. Provide an optional "control hardware" cost dimension so comparisons are not purely qubit-count.

### 8.9 Cross-architecture normalisation

Architectures are not directly comparable on qubit count alone (a cat qubit ≠ a transmon ≠ an atom). Therefore:
- Always report **spacetime volume in qubit-rounds** and **wall-clock seconds** alongside qubit count.
- Provide an explicit, documented, user-overridable `PhysicalQubitEquivalence` mapping so that cross-architecture comparisons state their normalisation assumption rather than hiding it.
- The UI must never present a bare cross-architecture qubit-count bar chart without the normalisation disclosure.

---

## 9. Layer C — Verification harness (`verification/`)

**This is the deliverable that makes the tool trustworthy, and it is a publishable result in its own right.**

### 9.1 Golden file format

```toml
[[target]]
id = "gidney_2025_rsa2048"
source = { arxiv = "2505.15917", title = "How to factor 2048 bit RSA integers with less than a million noisy qubits", table = "1", year = 2025 }
circuit = "ShorFactoringG25"
architecture = "surface_yoked_cultivation"

[target.problem]
kind = "rsa_factoring"
modulus_bits = 2048

[target.hardware]
physical_error_rate = 0.001
cycle_time_us = 1.0
reaction_time_us = 10.0
connectivity = "square_grid_nearest_neighbour"

[target.expected]
physical_qubits       = { value = 1_000_000, comparator = "less_than" }
wall_clock_days       = { value = 7,         comparator = "less_than" }
logical_qubits        = { value = 1400,      tolerance_pct = 25 }
toffoli_count         = { value = 6.5e9,     tolerance_pct = 30 }

[target.notes]
text = "Extract exact figures from the paper's resource table before finalising."
```

### 9.2 Required golden entries

| ID | Source | Headline figure to reproduce |
|---|---|---|
| `gidney_ekera_2019_rsa2048` | arXiv:1905.09749 | ~20M physical qubits, ~8 hours, `p=10⁻³` |
| `gidney_2025_rsa2048` | arXiv:2505.15917 | **< 1M physical qubits, < 1 week** |
| `roetteler_2017_p256` | arXiv:1706.06752 | ~2330 logical qubits, ~1.3×10¹¹ Toffoli |
| `gouzien_2023_ecdlp256_cat` | arXiv:2302.06639 / PRL 131:040602 | 126,133 cat qubits, ~9 hours |
| `bravyi_2024_gross` | Nature 627:778 (arXiv:2308.07915) | `[[144,12,12]]`, 288 physical qubits, threshold ~0.7% |
| `gidney_shutty_jones_cultivation` | arXiv:2409.17595 | `ε ≈ 2×10⁻⁹` at `p=10⁻³`; `ε ≈ 4×10⁻¹¹` at `p=5×10⁻⁴` |
| `litinski_2019_blocks` | arXiv:1808.02892 | Compact/intermediate/fast block footprints and cycle counts |
| `cain_2025_transversal` | arXiv:2406.17653 / Nature 2025 | `O(1)` syndrome rounds per logical operation |

Add every additional published estimate you can find with a complete assumption set. Breadth of the golden set is directly proportional to the credibility of the paper.

### 9.3 Harness behaviour

`make verify` must:
1. Run every golden target through the real production estimation path (Law 1: no special-casing).
2. Emit `verification/reports/report-<git-sha>.md` and `.json` with: expected, computed, ratio, pass/fail, and the full assumption set used.
3. Produce a **discrepancy analysis** for every failure: which term dominates the divergence (distance selection, factory cost, routing overhead, reaction limit). A failure with a diagnosis is a research finding; a failure without one is a bug report.
4. Fail CI on regression against the last committed report.
5. Support explicit, documented waivers (`waived = true, reason = "..."`) — but a waiver on `gidney_2025_rsa2048` fails the build unconditionally. That one must pass.

### 9.4 Cross-implementation check

`verification/reference/` holds a slow, deliberately naive Python implementation of the surface-code cost model and the distance solver. A test asserts Rust and Python agree to within floating-point tolerance across a randomised parameter sweep. This catches optimisation bugs in the Rust hot path.

---

## 10. Layer D — Hardware trajectory model (`crates/pramana-hardware`)

### 10.1 Purpose

Convert "this attack needs `Q_phys` physical qubits at error rate `p`" into "this becomes feasible in year `Y`, with uncertainty."

### 10.2 Roadmap ingestion

`data/roadmaps/*.toml` — one file per vendor, each entry a public, dated, cited claim:

```toml
vendor = "IBM"
modality = "superconducting"
[[milestone]]
name = "Condor";    year = 2023; physical_qubits = 1121; two_qubit_error = 1e-2
[[milestone]]
name = "Kookaburra"; year = 2026; note = "first qLDPC memory module with attached LPU"
[[milestone]]
name = "Starling";  year = 2029; logical_qubits = 200; logical_ops = 1e8; physical_qubits_est = 1e4+
[[milestone]]
name = "Blue Jay";  year = 2033; logical_qubits = 2000; logical_ops = 1e9
```

Cover at minimum: IBM (Kookaburra 2026 → Cockatoo 2027 → Starling 2029, ~200 logical qubits / 10⁸ gates → Blue Jay 2033, ~2000 logical qubits / 10⁹ gates), Quantinuum (Helios 2025 → Sol → Apollo ~2029/2030, universal fully fault-tolerant; include Lumos and any other named intermediate systems as they are announced), Google, PsiQuantum, Pasqal, QuEra, Alice & Bob, IonQ, Atom Computing, Infleqtion. Include *historical* delivered milestones as well as promises — the gap between them is the basis for the slip model.

Roadmaps change. Treat `data/roadmaps/` as a living dataset with a `last_verified` date per file, a CI warning when any file is older than 6 months, and a documented refresh procedure. A stale roadmap silently poisons every break-year distribution in the system, so make staleness visible in the UI rather than invisible in a data file.

### 10.3 Fitting

- Fit `log₁₀(physical_qubits) ~ year` per modality with weighted least squares (or a hierarchical Bayesian model if you want the uncertainty to be principled — preferred).
- Fit the error-rate trajectory `log₁₀(p) ~ year` similarly.
- **Fit a schedule-slip distribution from historical vendor performance**: for each delivered milestone, compute `(actual_year − originally_announced_year)`. Fit a distribution (log-normal or gamma shifted) to these slips. Apply it as a prior to future milestones. This is the honest way to handle roadmap optimism and it is defensible in review.
- Output: `Trajectory { modality, qubits_at(year) -> Distribution, error_rate_at(year) -> Distribution }`.

### 10.4 Guardrails

- The model must not extrapolate more than a configurable horizon (default 25 years) without emitting `ModelWarning::ExtrapolationBeyondHorizon`.
- Never present a trajectory as prediction. Language throughout the UI and API: "under the fitted trajectory with slip prior", not "will happen in".

---

## 11. Layer E — Break-year distribution engine (`crates/pramana-risk`)

### 11.1 The Monte Carlo

For each asset, for `N` samples (default 20,000, seeded):

1. Sample an architecture from a prior over `{surface, yoked+cultivation, qLDPC, neutral-atom, cat}`. The prior is user-configurable; default is uniform with a documented rationale, and the UI must let the user pin a single architecture.
2. Sample hardware trajectory parameters (qubit growth rate, error-rate improvement rate, slip) from the fitted posteriors in §10.
3. Sample an **algorithmic improvement factor**: historical resource estimates for RSA-2048 have fallen by roughly a factor of 20 over six years (2019→2025) purely from better algorithms and codes. Fit an exponential improvement rate to the historical series of published estimates and sample from it. Include this or the model is systematically pessimistic about `Z`, which is the dangerous direction of error.
4. Sample an **attacker budget**: acceptable wall-clock time (hours/days/months) and acceptable machine cost. A nation-state attacker tolerating a 6-month run has a very different `Z` than one requiring 8 hours.
5. Synthesise + cost the circuit for the asset's exact parameters under the sampled configuration.
6. Solve for the earliest year at which the sampled trajectory delivers sufficient physical qubits at sufficient error rate to complete the attack within the sampled budget.

Output: an empirical distribution over break year, summarised as `{p05, p10, p25, p50, p75, p90, p95, mean}` plus the full histogram, plus a per-architecture decomposition.

Performance: memoise circuit synthesis per `(algorithm, key_size, pipeline, options)` — the circuit does not change across hardware samples, only the QEC costing does. Parallelise with `rayon`. Target: < 5 seconds per asset for 20k samples on a laptop; < 60 seconds for a 500-asset inventory using shared memoisation.

### 11.2 Mosca resolution (`mosca.rs`)

For each asset compute
```
P(exposed) = P( X + Y > Z )
```
by integrating the sampled `Z` distribution against the asset's `X` and `Y`.

- `Y` (secrecy lifetime) comes from asset metadata; if absent, the asset is `INDETERMINATE` for Mosca purposes and the UI demands the input (Law 9 — do not default it silently).
- `X` (migration time) is estimated from the crypto-agility score (§15) via a documented mapping, and is user-overridable.
- Report the **harvest-now-decrypt-later date**: the calendar date after which data captured today is at risk, i.e. `today + Z_p05`. For confidentiality assets this is the number that actually matters.

### 11.3 Exposure score

A `0–100` score, defined explicitly and documented:
```
exposure = 100 · P(X + Y > Z) · w_criticality · w_data_class
```
with a published confidence interval derived from the Monte Carlo standard error. The formula must be visible in the UI on hover. An unexplained score is a spreadsheet output.

### 11.4 Distinguish threat modes

Signature assets and encryption assets have fundamentally different risk profiles and PRAMANA must model both:
- **Confidentiality (KEM/encryption):** vulnerable to harvest-now-decrypt-later. `Y` is the full data retention period. Urgency is high today.
- **Authenticity (signatures):** generally not retroactively breakable — a forged signature is only useful after the CRQC exists. `Y` collapses toward zero, but the *validity period of long-lived roots* (code-signing roots, firmware trust anchors, CA roots with 20-year lifetimes) reintroduces long `Y`.

This distinction changes prioritisation dramatically and is a strong differentiator versus tools that treat all assets identically.

### 11.5 Architecture sensitivity sweep (`sensitivity.rs`) — **paper-critical**

Run the full pipeline for a fixed asset set across every architecture, holding everything else constant, and report:
- The spread in median break year, in years — **this is the `N` in the novelty claim.**
- Which asset classes are most architecture-sensitive.
- Tornado sensitivity over all model parameters (`p`, `t_cycle`, `t_react`, threshold, routing overhead, algorithmic improvement rate, slip prior), ranked by influence on the median break year.

This must be a first-class API endpoint and UI view, not a script.

---

## 12. Layer F — Crypto inventory ingestion (`backend/app/services/ingestion/`)

### 12.1 Canonical asset model

Everything normalises to:

```python
class CryptoAsset(BaseModel):
    id: UUID
    fingerprint: str                       # stable dedup key
    source: SourceRef                      # scanner, file, host, connector
    kind: AssetKind                        # TLS_CERT | CA_ROOT | CA_INTERMEDIATE |
                                           # SSH_HOST_KEY | CODE_SIGNING_CERT |
                                           # JWT_SIGNING_KEY | VPN_TUNNEL | S_MIME |
                                           # DB_TDE_KEY | DOCUMENT_SIGNATURE | OTHER
    algorithm: AlgorithmSpec               # family, key size, curve, padding, hash
    purpose: CryptoPurpose                 # KEY_ESTABLISHMENT | SIGNATURE | ENCRYPTION
    threat_mode: ThreatMode                # CONFIDENTIALITY | AUTHENTICITY | BOTH
    not_before: date | None
    not_after: date | None
    subject: str | None
    issuer: str | None
    san: list[str]
    chain: list[UUID]                      # parent certificates (asset graph)
    deployment: DeploymentContext          # env, exposure, host, port, owner, business unit
    data_classification: DataClass | None
    secrecy_lifetime_years: float | None    # Y
    criticality: int | None                 # 1-5, human-assigned
    agility: AgilityAssessment | None       # §15
    raw: dict                               # full parsed structure, JSONB
```

### 12.2 Parsers

- **X.509** (`cryptography` + `asn1crypto`): PEM/DER, PKCS#7, PKCS#12, Java keystores. Extract algorithm OIDs, key sizes, curve names, signature algorithms, key usage, EKU, basic constraints, validity, SANs, CT/SCT extensions. **Build the chain graph** — a root's exposure propagates to everything under it, and this is a genuine differentiator.
- **TLS scanning** (`sslyze` as the primary library integration; support importing `testssl.sh` JSON output as an alternative path for teams already running it): live host scan → negotiated cipher suites, supported groups, certificate chains, TLS versions, whether hybrid PQC groups (`X25519MLKEM768`, `SecP256r1MLKEM768`) are offered. Rate-limited, explicit-target-list only, with a hard allowlist and a clearly logged consent record. Never scan anything not explicitly authorised.
- **SSH host keys** (`paramiko` or direct banner/kex parsing): key types, sizes, supported KEX algorithms including `sntrup761x25519-sha512` / `mlkem768x25519-sha256`.
- **JWT / JWKS**: fetch and parse JWKS endpoints, static configs, `alg` values (`RS256`, `ES256`, `EdDSA`, …), key rotation evidence.
- **Code-signing**: Authenticode, JAR signatures, GPG keys, Sigstore/Fulcio certs, container image signatures (`cosign`).
- **CBOM**: import and export CycloneDX 1.6 cryptographic BOMs. Import gives PRAMANA compatibility with existing discovery tools (Keyfactor, IBM Quantum Safe, `cryptobom-forge`); export makes PRAMANA a good citizen. **This is the integration story that makes the tool adoptable** — PRAMANA does not need to win at discovery, it needs to win at what happens after discovery.
- **Manual / CSV import** with a documented schema, plus a source-code scanner (regex + AST) for hardcoded algorithm usage in application code.

### 12.3 Deduplication & graph

Assets dedup on `(public key fingerprint, deployment context)`. Build the certificate chain graph in Postgres using `ltree` or an adjacency table with recursive CTEs. Support "show me everything that depends on this root."

---

## 13. Layer G — Exposure resolution & prioritisation (backend + `pramana-risk`)

### 13.1 Endpoint contract

`POST /api/v1/estimation/jobs` → enqueue Celery job → `GET /api/v1/estimation/jobs/{id}` → status/progress → `GET /api/v1/exposure/assets?...` → results.

Jobs are long-running. Report granular progress (assets completed / total, current phase). Support cancellation. Persist partial results.

### 13.2 Caching

Circuit synthesis and QEC costing are pure functions of `(problem, pipeline, architecture, hardware, options, model_version)`. Cache keyed on the hash of that tuple in Redis with the model version in the key so a model change invalidates cleanly. This is what makes 500-asset inventories fast: most enterprises have three distinct key configurations across thousands of certificates.

### 13.3 Prioritisation outputs

Produce two ranked lists and their divergence:
- `rank_criticality` — the industry-standard ordering (criticality × data class), which is what every existing roadmap does.
- `rank_computed` — PRAMANA's ordering by computed exposure.
- `divergence` — Kendall's tau and Spearman's rho between them, plus the assets that move most. **This is §1.3's second sentence, operationalised.** Ship it as an endpoint and a UI view.

### 13.4 Reports

Generate per-inventory reports (PDF via a proper typesetting path, plus XLSX and JSON) containing: executive summary, exposure timeline figure, top-N triaged assets with rationale, per-asset provenance appendix, full assumption set, and the standards citation table.

---

## 14. Layer H — Migration recommendation engine (`backend/app/services/recommend.py`)

Encode the standards as **data, not code**: `services/standards/*.yaml` holding NIST IR 8547 timelines, SP 800-131A transitions, CNSA 2.0 requirements and dates, FIPS 203/204/205 parameter sets, and (where relevant) BSI/ANSSI/ETSI guidance. A rule engine evaluates assets against them.

### 14.1 Target selection

| Situation | Recommendation | Rationale source |
|---|---|---|
| TLS key establishment, general | Hybrid `X25519MLKEM768` now → ML-KEM-768 standalone later | FIPS 203; IETF hybrid drafts; NIST IR 8547 |
| NSS / CNSA 2.0 scope | ML-KEM-1024 + ML-DSA-87 | CNSA 2.0 |
| General signatures | ML-DSA-65 (FIPS 204) | NIST IR 8547 |
| Long-lived roots, firmware & code signing | SLH-DSA (FIPS 205) or stateful LMS/XMSS (SP 800-208) | CNSA 2.0 explicitly specifies LMS/XMSS for software/firmware signing; conservative hash-based assumptions |
| Lattice-diversification hedge | HQC as a code-based KEM alternative | NIST 5th-round selection |
| Signature-size-constrained (embedded, DNSSEC-like) | FN-DSA / Falcon when standardised, with the floating-point implementation caveat stated | NIST |
| Symmetric | AES-256, SHA-384/512 — flag as low urgency with the Grover analysis attached | CNSA 2.0; NIST depth-limited Grover analysis |

Every recommendation must carry: the target algorithm and parameter set, the applicable deadline(s) with citation, the hybrid-vs-standalone decision with reasoning, the expected performance/size impact (key sizes, signature sizes, handshake byte deltas — computed from the actual parameter sets, not hand-waved), and known interoperability constraints.

### 14.2 Sequencing

Emit a migration *plan*, not a list: order by computed exposure, respect chain dependencies (you cannot migrate a leaf before its issuing CA can sign with a PQC algorithm), respect the deadline calendar, and level the load against a user-supplied capacity constraint. Output a Gantt-able schedule.

---

## 15. Layer I — Crypto-agility scoring (`backend/app/services/agility.py`)

NIST CSWP 39 asks organisations to *measure* crypto-agility. Essentially nobody has tooled this. Doing it well is a genuine contribution and it feeds `X` in the Mosca inequality.

Score each asset `0–100` across weighted dimensions, each with concrete evidence-based indicators:

1. **Algorithm negotiability** — is the algorithm negotiated at runtime (TLS cipher suite) or compiled in?
2. **Configuration surface** — can it change by config file / policy, or does it require a code change and redeploy?
3. **Key/certificate lifecycle automation** — ACME/EST/SCEP present? Automated rotation observed? Certificate lifetime short (evidence of working automation) or 397 days (evidence of manual process)?
4. **Dependency depth** — how many downstream systems pin this key/algorithm? Derived from the asset graph.
5. **Protocol/library constraint** — does the deployed library version support PQC at all? Map library versions to PQC capability.
6. **Hardware binding** — HSM/TPM/secure element with fixed algorithm support is the worst case; flag it explicitly, since HSM firmware refresh cycles are the real long pole in many enterprises.
7. **Vendor dependency** — is the asset in a third-party product where the customer cannot change the algorithm at all?
8. **Size/performance headroom** — will larger keys and signatures break MTU, packet size, storage, or latency budgets? (ML-DSA-65 signatures are ~3.3 KB versus ~64 bytes for ECDSA P-256 — for some protocols this is fatal.)

Map score to migration time `X` via a documented, calibratable table (e.g. 90–100 → 0.25 yr; 20–39 → 3 yr; 0–19 → 5+ yr or "requires re-architecture"). Surface the mapping and let users override it. Emit a per-dimension breakdown so the score is actionable, and a "cheapest agility improvements" list.

---

## 16. Layer J — Backend

### 16.1 Database schema (PostgreSQL 16)

Tables: `organisations`, `inventories`, `assets`, `asset_edges` (chain graph), `deployments`, `algorithm_specs`, `agility_assessments`, `estimation_jobs`, `circuit_results`, `qec_results`, `break_year_distributions` (histogram in JSONB), `exposures`, `recommendations`, `architectures`, `hardware_roadmaps`, `trajectory_fits`, `assumption_sets`, `model_versions`, `verification_runs`, `audit_log`, `users`, `api_keys`.

Requirements: every result row carries `model_version_id`, `assumption_set_id`, `input_hash`, `rng_seed`, `git_commit` (Law 5). Index `assets(inventory_id, kind, algorithm_family)`, `exposures(inventory_id, exposure_score DESC)`, GIN on JSONB provenance. Full Alembic migration history, no schema drift.

### 16.2 API surface (`/api/v1`)

```
POST   /inventories                        create
POST   /inventories/{id}/ingest            upload certs / CBOM / CSV
POST   /inventories/{id}/scan              live TLS/SSH scan (authorised targets only)
GET    /inventories/{id}/assets            filter, sort, paginate
GET    /assets/{id}                        full detail incl. chain
PATCH  /assets/{id}                        set Y, criticality, data class, X override
POST   /estimation/jobs                    run estimation over an inventory
GET    /estimation/jobs/{id}               status + progress
GET    /exposure/assets                    exposure results, sortable
GET    /exposure/timeline                  data for the hero visualisation
GET    /exposure/prioritisation-divergence criticality vs computed ranking
POST   /sensitivity/sweep                  architecture / parameter sweep
GET    /architectures                      registered models + validity regimes + citations
POST   /architectures/{id}/estimate        direct estimator access (no asset needed)
GET    /circuits/{id}/provenance           full synthesis → cost trace
GET    /recommendations                    per-asset PQC targets + plan
GET    /verification/report                latest reproduction report
POST   /reports/generate                   PDF / XLSX export
GET    /healthz  /readyz  /metrics         ops
```

All endpoints typed with Pydantic v2; OpenAPI schema generated and used to generate the TypeScript client. Auth: API keys + JWT sessions, org-scoped row-level access. Rate limiting on scan endpoints.

### 16.3 Workers

Celery with separate queues: `estimation` (CPU-heavy, Rust-bound), `ingestion` (I/O-bound), `scanning` (network, rate-limited), `reporting`. Idempotent tasks keyed on input hash. Progress via Redis pub/sub → SSE or WebSocket to the frontend.

### 16.4 PyO3 boundary (`crates/pramana-py`)

Thin. Exposes `synthesise_circuit`, `estimate_qec`, `fit_trajectories`, `break_year_distribution`, `sensitivity_sweep`, `run_verification`, `list_architectures`. Accepts and returns serde-serialisable structs mapped to Pydantic models. **No business logic in the binding layer.** Release the GIL (`py.allow_threads`) around long computations so Celery prefork workers behave.

---

## 17. Layer K — Frontend

### 17.1 The hero visualisation: `ExposureTimeline`

**This is Figure 1 of the paper and the hero image of the portfolio. Build it with the most care of anything in the UI.**

Specification:
- **X axis:** calendar years, from today to `today + 30`.
- **Y axis:** assets, sorted by exposure score descending (or grouped by business unit / asset class, user-selectable).
- **Per asset:** a horizontal violin/gradient band representing the break-year probability density. Dark saturated core at the median, fading to the tails. Not an error bar — a density. The visual point is that this is a distribution, not a date.
- **Overlaid per asset:** a vertical marker at `not_after` (certificate expiry), and a marker at `today + X + Y` (the Mosca deadline). Where the Mosca marker falls to the right of the band's left tail, the asset is exposed — render that overlap region in a distinct alarm treatment.
- **Global vertical rules:** NIST IR 8547 deprecation (2030) and disallowance (2035), CNSA 2.0 category deadlines. Labelled, dashed, with tooltips citing the source.
- **Colour encoding:** exposure score on a perceptually uniform sequential scale (viridis or a custom ramp). Must survive greyscale printing and pass WCAG AA contrast — it is going in a paper.
- **Architecture toggle:** switching architectures animates the bands, making the sensitivity finding *visible in a single interaction*. This interaction is the most persuasive thing in the entire product. Make it smooth and make it fast (precompute all architectures server-side).
- **Interaction:** hover → asset detail card with break-year quantiles, limiting factor, and recommendation. Click → asset page. Brush on the X axis to zoom. Filter by kind, algorithm, business unit, threat mode.
- **Performance:** must render 1,000 assets at 60 fps. Use SVG with virtualisation, or canvas with an SVG overlay for interactive elements. Not a naive per-asset React component tree.
- **Export:** SVG and PNG at publication DPI, with a legend and caption. One click.

### 17.2 Other required views

- **`ArchitectureComparison`** — small multiples of the same asset costed under every architecture; qubit count, wall clock, spacetime volume, limiting factor, with the normalisation disclosure visible.
- **`CircuitResourcePanel`** — the synthesis breakdown by scope: which subroutine contributed which Toffolis, the chosen window sizes, the qubit allocation profile over time. This is what proves to a reviewer that real synthesis happened.
- **`QecBreakdown`** — a spacetime-volume treemap: data patches / routing / magic state factories / idle storage, with the chosen distance schedule.
- **`MoscaResolver`** — interactive `X`, `Y`, `Z` visualisation per asset with live re-resolution as the user adjusts `X` and `Y`.
- **`SensitivityExplorer`** — tornado chart of parameter influence, plus the architecture spread number displayed prominently.
- **`ProvenanceTrace`** — the full chain from asset → circuit → QEC → trajectory → year, expandable, with every assumption. This is Law 2 made visible and it is what makes the tool defensible in front of an auditor.
- **`VerificationDashboard`** — the golden reproduction table, live. Green ticks against published papers, in the product, is an extraordinarily strong trust signal. Put it in the main navigation, not buried in docs.

### 17.3 UX principles

Every number is hoverable and explains itself. Every distribution shows its uncertainty. `INDETERMINATE` assets are prominent, never hidden. No number appears without a path to its provenance. Empty states teach the workflow.

---

## 18. Layer L — Hybrid PQC TLS demo (`demo/hybrid_tls/`)

Purpose: the tool should not only advise migration, it should demonstrate it. This closes the loop and is disproportionately compelling in a demo.

- Build `liboqs` + `oqs-provider` against OpenSSL 3.x in a container.
- Run a TLS 1.3 server and client negotiating `X25519MLKEM768`; capture the handshake, show the key share sizes, and compare handshake bytes and latency against classical X25519.
- Issue an ML-DSA-65 certificate from a test CA and complete a full PQC-authenticated handshake.
- Feed the resulting endpoint back into PRAMANA's scanner so the tool detects the migrated asset and moves its exposure to zero. **That round trip — scan, triage, migrate, rescan, verify — is the demo.**
- Include a measurement harness reporting handshake size and time deltas, since "will PQC break my performance budget" is the first question every practitioner asks.

---

## 19. Layer M — Reproducibility and paper artifacts

- `make figures` regenerates every paper figure from committed data with fixed seeds.
- `make verify` regenerates the reproduction report.
- Every release tag commits: verification report, figure set, model version manifest, and a `results/` archive with input hashes.
- `CITATION.cff` and a `docs/paper/` directory with the LaTeX source.
- A `docs/assumptions.md` that is the single canonical statement of every default, its value, its justification, and its source. Reviewers will go straight here.
- Package the golden set + harness as a standalone artifact so others can run their estimators against it. **A community benchmark for cryptographic resource estimators is arguably a bigger contribution than the tool itself** — position it that way.

---

## 20. Testing strategy

| Level | Scope | Tooling | Bar |
|---|---|---|---|
| Unit | Every arithmetic primitive, distance solver, factory model | `cargo test`, `pytest` | > 85% line coverage on `crates/` |
| Property | Monotonicity (Law 7), unit safety, dimensional consistency, window optimiser local-minimality | `proptest`, `hypothesis` | All Law 7 invariants encoded |
| Golden | Published reproductions | verification harness | `gidney_2025_rsa2048` must pass |
| Cross-impl | Rust vs Python reference | `pytest` | Agreement to 1e-9 relative |
| Integration | Ingest → estimate → exposure → recommend | `pytest` + testcontainers | Full pipeline on the 500-asset fixture |
| Contract | OpenAPI ↔ TS client | `schemathesis` | No drift |
| E2E | Upload → timeline render → export | Playwright | Hero timeline screenshot diff |
| Performance | 500-asset inventory, 20k MC samples | `criterion`, k6 | < 60 s wall clock; timeline 60 fps |
| Determinism | Same manifest → same bytes | CI job | Byte-identical |
| Security | Dependency audit, SAST, secret scan | `cargo-audit`, `pip-audit`, `bandit`, `gitleaks` | Zero high severity |

---

## 21. Security, performance, operations

- **Scanning is dangerous.** Live scanning requires an explicit authorised-target allowlist, per-target consent records in the audit log, strict rate limits, and a global kill switch. Default deny. Document the legal caveat in the README.
- **Uploaded material is sensitive.** Certificates are public but inventories are not — an inventory is a map of an organisation's cryptographic weak points. Encrypt at rest, org-scoped access control, full audit log, configurable retention and purge. Never log private key material; the system should never *accept* private keys — public keys and certificates only. Reject any upload containing a private key with a clear error.
- **Performance:** memoise aggressively (§13.2), parallelise Monte Carlo with `rayon`, stream large result sets, paginate everything, put the timeline data behind a precomputed materialised view.
- **Observability:** structured logging (`structlog` / `tracing`), Prometheus metrics on job duration, cache hit rate, model invocation counts. Every estimation job traceable end to end.
- **Deployment:** multi-stage Docker (Rust build → maturin wheel → slim Python runtime), health/readiness probes, graceful worker shutdown, DB migrations gated in CI.

---

## 22. Acceptance criteria

The project is complete when **all** of the following are true:

**Research core**
- [ ] Circuit synthesis produces resource counts by construction, not formula lookup, for RSA factoring (two pipelines), ECDLP (two pipelines), and Grover symmetric.
- [ ] Four QEC architectures implemented behind one trait, each with a declared validity regime and citations.
- [ ] Magic state distillation *and* cultivation both implemented, with the optimiser selecting between them.
- [ ] Yoked surface codes implemented and contributing to the Gidney 2025 reproduction.
- [ ] qLDPC model accounts for logical *computation* overhead, not memory only, and reports both.
- [ ] `make verify` passes with `gidney_2025_rsa2048` reproducing **< 1M physical qubits and < 1 week**, unwaived.
- [ ] Every other golden target passes or carries a documented waiver with a diagnosed discrepancy.
- [ ] Rust and Python reference implementations agree across a randomised sweep.

**Risk engine**
- [ ] Hardware trajectories fitted from cited public roadmaps with a slip prior derived from historical vendor performance.
- [ ] Break-year output is a distribution with quantiles, per-architecture decomposition, and an algorithmic-improvement term.
- [ ] Mosca resolves to `P(exposed)` with a confidence interval; confidentiality and authenticity threat modes handled distinctly.
- [ ] Architecture sensitivity sweep produces the headline `N`-year spread number.
- [ ] Prioritisation divergence (criticality vs computed) computed and exposed via API and UI.

**Product**
- [ ] Ingestion works for X.509, TLS scan, SSH, JWT/JWKS, code-signing, and CycloneDX CBOM import/export.
- [ ] Certificate chain graph built; root exposure propagates to dependents.
- [ ] Crypto-agility scored across all eight dimensions with per-dimension evidence, feeding `X`.
- [ ] Recommendations carry structured standards citations and computed size/performance impact.
- [ ] Migration plan respects chain dependencies and deadline calendar.
- [ ] Hero timeline renders 1,000 assets at 60 fps, exports at publication DPI, and animates the architecture toggle.
- [ ] Provenance trace visible in the UI for every reported number.
- [ ] Verification dashboard in the main navigation.
- [ ] Hybrid TLS demo completes the scan → triage → migrate → rescan → verify round trip.

**Engineering**
- [ ] All twelve laws enforced, with CI checks where mechanically checkable.
- [ ] Determinism test passes.
- [ ] `docs/assumptions.md` is complete and canonical.
- [ ] One-command bootstrap on a clean machine.
- [ ] `make figures` regenerates every paper figure from committed data.

---

## 23. Anti-patterns — do not do these

1. **Building the UI first.** The UI is the last 10%. Build it last. A beautiful UI over a lookup table is the documented failure mode of this project.
2. **Hardcoding published results as the answer.** The entire value proposition dies here. Published numbers live only in `verification/golden/`.
3. **Uniform code distance everywhere.** Real estimates use different distances for memory, computation, and factories. Uniform distance is a tell that the model is superficial.
4. **Ignoring the reaction limit.** For Shor, the classical feedback latency frequently dominates wall clock. A model that only counts magic states will get the runtime badly wrong.
5. **qLDPC memory-only accounting.** Counting only storage overhead for qLDPC while ignoring the cost of logical operations produces spectacularly over-optimistic numbers. This is the most common error in the popular literature. Do not replicate it.
6. **Point estimates.** A single break year is a Q-Day assumption wearing a lab coat. Everything is a distribution.
7. **Dropping the repetition count.** Shor variants with reduced qubit counts often pay in success probability and require multiple runs. Ignoring this understates `Z`.
8. **Omitting algorithmic improvement.** Estimates have improved ~20× in six years. A model that assumes today's algorithms are final is systematically wrong in the unsafe direction.
9. **Treating signatures and encryption identically.** Harvest-now-decrypt-later applies to one and not the other. Getting this wrong invalidates the prioritisation.
10. **Python in the hot path.** Law 11.
11. **Scanning without authorisation.** Legal and ethical hazard. Default deny, always.
12. **Scope creep into discovery.** PRAMANA is not competing with Keyfactor on certificate discovery. It ingests from them via CBOM. Stay in the lane where the contribution is.

---

## 24. Primary references

**Resource estimation & attack circuits**
- Gidney, *How to factor 2048 bit RSA integers with less than a million noisy qubits*, arXiv:2505.15917 (2025).
- Gidney & Ekerå, *How to factor 2048 bit RSA integers in 8 hours using 20 million noisy qubits*, Quantum 5:433 (2021), arXiv:1905.09749.
- Chevignard, Fouque & Schrottenloher, *Reducing the number of qubits in quantum factoring*, EUROCRYPT (2025), ePrint 2024/1852.
- Ekerå & Håstad, *Quantum algorithms for computing short discrete logarithms and factoring RSA integers*, PQCrypto (2017).
- Roetteler, Naehrig, Svore & Lauter, *Quantum resource estimates for computing elliptic curve discrete logarithms*, ASIACRYPT (2017), arXiv:1706.06752.
- Gidney, *Windowed quantum arithmetic*, arXiv:1905.07682.
- Babbush et al., *Encoding electronic spectra in quantum circuits with linear T complexity* (unary iteration / QROM), PRX 8:041015 (2018).
- Ross & Selinger, *Optimal ancilla-free Clifford+T approximation of z-rotations*, arXiv:1403.2975.

**Quantum error correction**
- Fowler, Mariantoni, Martinis & Cleland, *Surface codes: towards practical large-scale quantum computation*, PRA 86:032324 (2012).
- Litinski, *A Game of Surface Codes*, Quantum 3:128 (2019), arXiv:1808.02892.
- Gidney, Shutty & Jones, *Magic state cultivation: growing T states as cheap as CNOT gates*, arXiv:2409.17595 (2024).
- Gidney, Newman, Brooks & Jones, *Yoked surface codes*, arXiv:2312.04522 (2023).
- Bravyi, Cross, Gambetta, Maslov, Rall & Yoder, *High-threshold and low-overhead fault-tolerant quantum memory*, Nature 627:778 (2024), arXiv:2308.07915.
- *Tour de gross: a modular quantum computer based on bivariate bicycle codes*, arXiv:2506.03094 (2025).
- Cain et al., *Correlated decoding of logical algorithms with transversal gates*, PRL 133:240602 (2024); *Fast correlated decoding of transversal logical algorithms*, arXiv:2505.13587 (2025).
- Zhou et al., *Low-overhead transversal fault tolerance for universal quantum computation*, Nature (2025).
- Gouzien, Ruiz, Le Régent, Guillaud & Sangouard, *Performance analysis of a repetition cat code architecture: computing 256-bit elliptic curve logarithm in 9 hours with 126133 cat qubits*, PRL 131:040602 (2023), arXiv:2302.06639.

**Standards & policy**
- NIST IR 8547 (ipd), *Transition to Post-Quantum Cryptography Standards* (Nov 2024) — RSA-2048/ECC-256 deprecated after 2030, disallowed after 2035.
- NSA CNSA 2.0 algorithm suite and FAQ — ML-KEM-1024, ML-DSA-87, LMS/XMSS for software/firmware signing; 2027 procurement gate; 2030/2033 category deadlines; NSM-10 alignment by 2035.
- FIPS 203 (ML-KEM), FIPS 204 (ML-DSA), FIPS 205 (SLH-DSA); NIST SP 800-208 (stateful hash-based signatures); NIST SP 800-131A Rev. 3.
- NIST CSWP 39, *Considerations for Achieving Crypto Agility*.
- Mosca, *Cybersecurity in an era with quantum computers: will we be ready?*, IEEE S&P 16:38 (2018).
- CycloneDX 1.6 cryptographic BOM specification.

---

## 25. Final instruction to the implementing agent

Build in the order given in §6. After each phase, run the full test suite and the verification harness, and commit. Do not begin Layer K (frontend) until `make verify` passes with the Gidney 2025 target green.

When a published number cannot be reproduced, **do not tune constants until it matches.** Diagnose the divergence, document it in the verification report, and either fix the structural omission or record it as a known limitation with a hypothesis. A model that reproduces four out of six targets with six honest diagnoses is a far stronger scientific contribution — and a far better product — than one that reproduces six out of six by fitting.

The tool's authority rests entirely on that discipline.
