# PRAMANA

**Cryptographic Resource Estimation & Migration Triage Engine**

*pramāṇa (प्रमाण) — "means of valid knowledge; proof"*

PRAMANA computes, per cryptographic asset, the year that asset becomes quantum-breakable —
by synthesising the attack circuit, costing its error correction across multiple
fault-tolerant architectures, and resolving Mosca's inequality against the asset's
data-retention requirement.

The full specification is in [`PRAMANA_BUILD_PROMPT.md`](PRAMANA_BUILD_PROMPT.md).
Every assumption the system makes is catalogued in [`docs/ASSUMPTIONS.md`](docs/ASSUMPTIONS.md).
Current build state and honest divergences are in [`docs/STATUS.md`](docs/STATUS.md).

## Prime directive

PRAMANA computes; it does not look up. Every number reported to a user is traceable to a
computation performed on that user's own asset, never to a table of published results.
Published figures live in exactly one place — `verification/golden/` — and are consumed
only by the verification harness.

## Quick start

```bash
make bootstrap     # provision a Rust toolchain
make test          # 226 Rust tests\nmake backend-test  # 18 Python tests over the API
make synthesise    # synthesise RSA modexp circuits and print measured resources
make verify        # reproduce published figures from 4 papers
make exposure      # full chain: asset -> circuit -> QEC -> trajectory -> exposure score
make serve         # run the API on :8000, docs at /docs
make frontend      # build the React app
make figure        # regenerate the paper figure deterministically
make determinism   # Law 5: identical manifests reproduce byte-identical results
make ci            # everything CI runs, locally
```

## What is built

| Phase | Component | State |
|---|---|---|
| P0 | Workspace, `pramana-units` (typed quantities, Law 6) | Complete, 23 tests |
| P1 | `pramana-circuit`: IR, adders, QROM/QROAM, windowed modexp | Complete, 38 tests |
| P2 | Shor factoring / ECDLP / Grover pipelines | Complete |
| P3-P5 | QEC cost models: surface, yoked, cat, qLDPC, neutral atom, G25 three-region | Complete |
| P6 | Reproduction harness; 4 published papers reproduced incl. the keystone | Complete |
| P7 | Hardware trajectories + Monte Carlo risk engine | Complete |
| P8 | Bindings, API, database, X.509 ingestion, agility, recommendations | Complete |
| P9 | Hero timeline figure, React frontend, paper figure generation | Complete |

## Law 6 is enforced by the compiler, not by convention

Mixing incommensurable quantities is a compile error, proven by `compile_fail` doctests:

```rust
let _ = LogicalQubits::new(10) + PhysicalQubits::new(10); // does not compile
let _ = ToffoliCount::new(4) + TCount::new(4);            // does not compile
let _ = CodeCycles::new(1000) + Seconds::new(1.0);        // does not compile
```

Conflating logical with physical qubits, or code cycles with seconds, is the most common
class of error in published resource estimates. Here it cannot be written.

## Licence

Apache-2.0.
