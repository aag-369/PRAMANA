"""Migration recommendation engine.

Selects a post-quantum target for an asset, with the deadline that binds it, the reasoning,
and the computed size impact. Two rules:

**Never recommend a pending standard.** FN-DSA and HQC are selected but not final. An asset
migrated to a draft parameter set may have to be migrated twice.

**Compute the size impact rather than asserting it.** "Signatures get bigger" is not
actionable; "3,309 bytes against 64, a 52x increase" is.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .standards import ALGORITHMS, CLASSICAL_SIZES, Algorithm, deadlines_for


@dataclass
class Recommendation:
    """A migration target with its justification."""

    target_algorithm: str
    parameter_set: str
    hybrid: bool
    status: str
    rationale: str
    citations: list[dict[str, str]] = field(default_factory=list)
    deadlines: list[dict[str, Any]] = field(default_factory=list)
    size_impact: dict[str, Any] = field(default_factory=dict)
    alternatives: list[dict[str, str]] = field(default_factory=list)

    def as_dict(self) -> dict[str, Any]:
        """Serialisable form."""
        return {
            "target_algorithm": self.target_algorithm,
            "parameter_set": self.parameter_set,
            "hybrid": self.hybrid,
            "status": self.status,
            "rationale": self.rationale,
            "citations": self.citations,
            "deadlines": self.deadlines,
            "size_impact": self.size_impact,
            "alternatives": self.alternatives,
        }


def _size_impact(
    algorithm_family: str, key_bits: int, target: Algorithm
) -> dict[str, Any]:
    """Compute the byte deltas a migration imposes."""
    classical = CLASSICAL_SIZES.get((algorithm_family, key_bits))
    out: dict[str, Any] = {
        "target_public_key_bytes": target.public_key_bytes,
        "target_output_bytes": target.output_bytes,
    }
    if classical is None:
        out["note"] = "no classical baseline recorded for this algorithm and key size"
        return out
    pk, sig = classical
    out.update(
        {
            "classical_public_key_bytes": pk,
            "classical_output_bytes": sig,
            "public_key_growth": round(target.public_key_bytes / pk, 1),
            "output_growth": round(target.output_bytes / sig, 1),
        }
    )
    if target.output_bytes > 1500:
        out["warning"] = (
            f"{target.output_bytes} bytes exceeds a typical 1500-byte MTU; check packet "
            "size, storage and latency budgets before committing"
        )
    return out


def recommend(
    *,
    kind: str,
    algorithm_family: str,
    key_bits: int,
    purpose: str,
    is_ca: bool,
    nss: bool = False,
    size_constrained: bool = False,
) -> Recommendation:
    """Choose a migration target."""
    citations = [
        {"document": "NIST IR 8547", "clause": "transition timeline"},
    ]

    if algorithm_family in {"aes", "symmetric"}:
        return Recommendation(
            target_algorithm="AES",
            parameter_set="AES-256",
            hybrid=False,
            status="available",
            rationale=(
                "Symmetric primitives are not urgent. Grover's search gives only a "
                "quadratic speedup, and PRAMANA computes the attack on AES-128 at roughly "
                "2^78 Toffoli gates against 2^31 for RSA-2048: about 2^47 times harder. "
                "Move to AES-256 on the normal refresh cycle."
            ),
            citations=citations + [{"document": "CNSA 2.0", "clause": "AES-256"}],
            deadlines=[d.__dict__ for d in deadlines_for("all", nss)],
        )

    # National Security Systems have their own suite and their own dates.
    if nss:
        key = "ml-kem-1024" if purpose == "key_establishment" else "ml-dsa-87"
        alg = ALGORITHMS[key]
        if kind == "code_signing_cert":
            alg = ALGORITHMS["lms"]
        return Recommendation(
            target_algorithm=alg.name,
            parameter_set=alg.parameter_set,
            hybrid=False,
            status=alg.status,
            rationale=(
                f"CNSA 2.0 requires {alg.parameter_set} for this asset class. {alg.note}"
            ),
            citations=citations + [{"document": "CNSA 2.0", "clause": alg.parameter_set}],
            deadlines=[d.__dict__ for d in deadlines_for(
                "code_signing" if kind == "code_signing_cert" else "nss", nss=True)],
            size_impact=_size_impact(algorithm_family, key_bits, alg),
        )

    # Long-lived trust anchors get hash-based signatures: the most conservative assumptions
    # available, for code that must stay verifiable for decades.
    if is_ca or kind == "code_signing_cert":
        alg = ALGORITHMS["slh-dsa-128s"]
        return Recommendation(
            target_algorithm=alg.name,
            parameter_set=alg.parameter_set,
            hybrid=False,
            status=alg.status,
            rationale=(
                "Long-lived trust anchors should rest on hash-based signatures, whose "
                "security assumptions are the most conservative available. CNSA 2.0 "
                "specifies stateful LMS or XMSS for software and firmware signing; "
                "SLH-DSA is the stateless alternative and avoids the catastrophic "
                "consequences of state reuse."
            ),
            citations=citations + [
                {"document": "FIPS 205", "clause": "SLH-DSA"},
                {"document": "NIST SP 800-208", "clause": "stateful hash-based signatures"},
                {"document": "CNSA 2.0", "clause": "software and firmware signing"},
            ],
            deadlines=[d.__dict__ for d in deadlines_for("code_signing", nss)],
            size_impact=_size_impact(algorithm_family, key_bits, alg),
            alternatives=[
                {
                    "parameter_set": "LMS-SHA256-M32-H10",
                    "why": "stateful, smaller signatures, but state reuse is fatal; "
                           "suitable only for controlled signing infrastructure",
                }
            ],
        )

    if purpose == "key_establishment":
        alg = ALGORITHMS["ml-kem-768"]
        alternatives = []
        hqc = ALGORITHMS["hqc-128"]
        alternatives.append(
            {
                "parameter_set": hqc.parameter_set,
                "why": f"code-based hedge against a lattice break. NOT YET AVAILABLE: "
                       f"{hqc.note}",
            }
        )
        return Recommendation(
            target_algorithm=alg.name,
            parameter_set=f"X25519{alg.parameter_set.replace('-', '')}",
            hybrid=True,
            status=alg.status,
            rationale=(
                "Deploy the hybrid X25519MLKEM768 now rather than ML-KEM-768 alone. The "
                "hybrid retains classical security if the lattice assumption is broken, "
                "and hybrid modes are not caught by the 2035 disallowance, so it is a "
                "safe intermediate state rather than a migration that must be redone."
            ),
            citations=citations + [{"document": "FIPS 203", "clause": "ML-KEM"}],
            deadlines=[d.__dict__ for d in deadlines_for("all", nss)],
            size_impact=_size_impact(algorithm_family, key_bits, alg),
            alternatives=alternatives,
        )

    # General signatures.
    alg = ALGORITHMS["ml-dsa-65"]
    alternatives = []
    if size_constrained:
        fn = ALGORITHMS["fn-dsa-512"]
        alternatives.append(
            {
                "parameter_set": fn.parameter_set,
                "why": f"{fn.output_bytes}-byte signatures against ML-DSA-65's "
                       f"{alg.output_bytes}. NOT YET AVAILABLE: {fn.note}",
            }
        )
    return Recommendation(
        target_algorithm=alg.name,
        parameter_set=alg.parameter_set,
        hybrid=False,
        status=alg.status,
        rationale=(
            "ML-DSA-65 is the general-purpose signature target. "
            + (
                "This asset is size-constrained; FN-DSA would suit it better but is not "
                "yet final, so ML-DSA is the only standardised option today."
                if size_constrained
                else "No size constraint identified."
            )
        ),
        citations=citations + [{"document": "FIPS 204", "clause": "ML-DSA"}],
        deadlines=[d.__dict__ for d in deadlines_for("all", nss)],
        size_impact=_size_impact(algorithm_family, key_bits, alg),
        alternatives=alternatives,
    )
