"""Standards encoded as data, not code.

Status is current as of 2026-08-29. Two entries are deliberately marked ``pending``:
PRAMANA will not recommend an algorithm whose parameter sets are not yet fixed, because a
migration executed against a draft is a migration that may have to be done twice.
"""

from __future__ import annotations

from dataclasses import dataclass, field

LAST_VERIFIED = "2026-08-29"


@dataclass(frozen=True)
class Algorithm:
    """A post-quantum algorithm and its standardisation status."""

    name: str
    parameter_set: str
    standard: str
    status: str  # "final" | "pending"
    purpose: str  # "kem" | "signature"
    public_key_bytes: int
    output_bytes: int  # ciphertext for a KEM, signature for a signature scheme
    note: str = ""


#: Sizes are from the FIPS documents; they drive the computed size-impact figures rather
#: than a hand-waved "signatures get bigger".
ALGORITHMS: dict[str, Algorithm] = {
    "ml-kem-768": Algorithm(
        "ML-KEM", "ML-KEM-768", "FIPS 203", "final", "kem", 1184, 1088,
        "General-purpose key establishment.",
    ),
    "ml-kem-1024": Algorithm(
        "ML-KEM", "ML-KEM-1024", "FIPS 203", "final", "kem", 1568, 1568,
        "Required by CNSA 2.0 for National Security Systems.",
    ),
    "ml-dsa-65": Algorithm(
        "ML-DSA", "ML-DSA-65", "FIPS 204", "final", "signature", 1952, 3309,
        "General-purpose signatures.",
    ),
    "ml-dsa-87": Algorithm(
        "ML-DSA", "ML-DSA-87", "FIPS 204", "final", "signature", 2592, 4627,
        "Required by CNSA 2.0 for National Security Systems.",
    ),
    "slh-dsa-128s": Algorithm(
        "SLH-DSA", "SLH-DSA-SHA2-128s", "FIPS 205", "final", "signature", 32, 7856,
        "Hash-based; conservative assumptions for long-lived roots.",
    ),
    "lms": Algorithm(
        "LMS", "LMS-SHA256-M32-H10", "NIST SP 800-208", "final", "signature", 60, 2828,
        "Stateful hash-based. CNSA 2.0 specifies LMS or XMSS for software and firmware "
        "signing. Stateful schemes lose all security if state is reused, so they suit "
        "controlled signing infrastructure only.",
    ),
    "fn-dsa-512": Algorithm(
        "FN-DSA", "FN-DSA-512", "FIPS 206", "pending", "signature", 897, 666,
        "Compact signatures, which matters where ML-DSA's 3.3 KB will not fit. Submitted "
        "for NIST clearance in August 2025 and not yet final; parameter sets may change.",
    ),
    "hqc-128": Algorithm(
        "HQC", "HQC-128", "NIST (selected 2025-03-11)", "pending", "kem", 2249, 4497,
        "Code-based, providing algorithmic diversity against a lattice break. Draft "
        "expected 2026, finalisation around 2027.",
    ),
}

#: Classical baselines, for computing the size delta a migration imposes.
CLASSICAL_SIZES = {
    ("rsa", 2048): (256, 256),
    ("rsa", 3072): (384, 384),
    ("rsa", 4096): (512, 512),
    ("ecdsa", 256): (64, 64),
    ("ecdsa", 384): (96, 96),
    ("ecdsa", 521): (132, 132),
    ("dsa", 2048): (256, 64),
}


@dataclass(frozen=True)
class Deadline:
    """A dated obligation."""

    document: str
    clause: str
    year: int
    applies_to: str
    note: str = ""


DEADLINES: list[Deadline] = [
    Deadline("NIST IR 8547", "deprecation of 112-bit-security public key algorithms",
             2030, "all", "RSA-2048 and ECC P-256 deprecated after 2030."),
    Deadline("NIST IR 8547", "disallowance", 2035, "all",
             "Quantum-vulnerable algorithms removed. Hybrid modes combining an approved "
             "post-quantum algorithm with a classical one are not caught by this."),
    Deadline("CNSA 2.0", "procurement gate", 2027, "nss",
             "From 1 January 2027 all new National Security System acquisitions must be "
             "CNSA 2.0 compliant. NIAP Protection Profiles incorporating CNSA 2.0 began "
             "releasing in 2026; unvalidated products are ineligible."),
    Deadline("CNSA 2.0", "software and firmware signing", 2030, "code_signing",
             "Exclusive use of LMS or XMSS."),
    Deadline("CNSA 2.0", "networking equipment", 2030, "network",
             "Exclusive CNSA 2.0 use; equipment that cannot support it is phased out by "
             "31 December 2030."),
    Deadline("CNSA 2.0", "legacy and custom applications", 2033, "nss",
             "Updated or replaced; NSS owners report progress."),
]

#: NIST CSWP 39, finalised December 2025 and updated June 2026, describes what PRAMANA is:
#: a "cryptographic policy-informed risk assessment engine" that continuously analyses the
#: inventory and recommends mitigations according to policy and risk.
CRYPTO_AGILITY_STANDARD = {
    "document": "NIST CSWP 39upd1",
    "title": "Considerations for Achieving Crypto Agility: Strategies and Practices",
    "status": "final (December 2025, updated 29 June 2026)",
}


def deadlines_for(applies_to: str, nss: bool = False) -> list[Deadline]:
    """Deadlines relevant to an asset class."""
    out = []
    for d in DEADLINES:
        if d.applies_to == "all":
            out.append(d)
        elif d.applies_to == applies_to:
            out.append(d)
        elif d.applies_to == "nss" and nss:
            out.append(d)
    return sorted(out, key=lambda d: d.year)
