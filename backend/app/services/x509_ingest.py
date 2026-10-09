"""X.509 certificate ingestion.

Extracts the fields PRAMANA needs to cost an attack: the algorithm family, the key size,
the curve where applicable, and enough context to build the chain graph.

Two rules govern this module.

**No guessing** (Law 9). An unrecognised algorithm produces an ``IndeterminateAsset``
rather than a default. An asset PRAMANA cannot cost is more useful surfaced than silently
assigned a plausible-looking number.

**No private keys.** The system accepts certificates and public keys only. A PEM block
containing a private key is rejected outright rather than parsed and discarded, because
accepting it at all would mean it had already been written to a request log.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass, field
from typing import Any

from cryptography import x509
from cryptography.hazmat.primitives.asymmetric import dsa, ec, ed448, ed25519, rsa
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

#: PEM labels that indicate private key material. Presence of any of these is fatal.
_PRIVATE_KEY_MARKERS = (
    "PRIVATE KEY",
    "RSA PRIVATE KEY",
    "EC PRIVATE KEY",
    "DSA PRIVATE KEY",
    "OPENSSH PRIVATE KEY",
)

_PEM_CERT = re.compile(
    rb"-----BEGIN CERTIFICATE-----.+?-----END CERTIFICATE-----", re.DOTALL
)

#: Curve name to the bit length PRAMANA costs the attack against.
_CURVE_BITS = {
    "secp256r1": 256,
    "prime256v1": 256,
    "secp384r1": 384,
    "secp521r1": 521,
    "secp256k1": 256,
    "sect283k1": 283,
    "brainpoolP256r1": 256,
    "brainpoolP384r1": 384,
    "brainpoolP512r1": 512,
}


class PrivateKeyRejected(ValueError):
    """Input contained private key material."""


class UnparseableCertificate(ValueError):
    """Input could not be parsed as a certificate."""


@dataclass
class ParsedAsset:
    """A certificate reduced to what PRAMANA needs."""

    fingerprint: str
    kind: str
    algorithm_family: str
    key_bits: int
    curve: str | None
    signature_algorithm: str | None
    purpose: str
    threat_mode: str
    subject: str
    issuer: str
    not_before: Any
    not_after: Any
    is_ca: bool
    indeterminate_reason: str | None = None
    raw: dict = field(default_factory=dict)

    @property
    def is_indeterminate(self) -> bool:
        """Whether PRAMANA can cost an attack on this asset."""
        return self.indeterminate_reason is not None

    def target(self) -> tuple[str, int] | None:
        """The ``(kind, size)`` pair the estimation core takes, if determinate."""
        if self.is_indeterminate:
            return None
        if self.algorithm_family == "rsa":
            return ("rsa", self.key_bits)
        if self.algorithm_family in {"ecdsa", "ecdh"}:
            return ("ecdlp", self.key_bits)
        if self.algorithm_family == "dsa":
            return ("ffdlp", self.key_bits)
        return None


def _guard_private_key(data: bytes) -> None:
    text = data[:200_000].decode("latin-1", errors="ignore").upper()
    for marker in _PRIVATE_KEY_MARKERS:
        if marker in text:
            raise PrivateKeyRejected(
                "input contains private key material. PRAMANA accepts certificates and "
                "public keys only; supply the certificate without its key."
            )


def _classify(cert: x509.Certificate) -> tuple[str, int, str | None, str | None]:
    """Return ``(family, key_bits, curve, indeterminate_reason)``."""
    key = cert.public_key()
    if isinstance(key, rsa.RSAPublicKey):
        return ("rsa", key.key_size, None, None)
    if isinstance(key, ec.EllipticCurvePublicKey):
        name = key.curve.name
        bits = _CURVE_BITS.get(name)
        if bits is None:
            return ("ecdsa", 0, name, f"unrecognised curve '{name}'")
        return ("ecdsa", bits, name, None)
    if isinstance(key, dsa.DSAPublicKey):
        return ("dsa", key.key_size, None, None)
    if isinstance(key, (ed25519.Ed25519PublicKey, ed448.Ed448PublicKey)):
        # Edwards curves are quantum-broken like any ECDLP instance, but the published
        # resource estimates cover prime-field short Weierstrass curves. Costing Ed25519
        # with a P-256 circuit would be a substitution PRAMANA does not make.
        name = "ed25519" if isinstance(key, ed25519.Ed25519PublicKey) else "ed448"
        return (
            "eddsa",
            255 if name == "ed25519" else 448,
            name,
            f"{name} is quantum-vulnerable but no published ECDLP circuit covers "
            "twisted Edwards curves; costed circuits exist only for prime-field "
            "short Weierstrass curves",
        )
    return ("unknown", 0, None, f"unsupported public key type {type(key).__name__}")


def _purpose_and_threat(cert: x509.Certificate, is_ca: bool) -> tuple[str, str]:
    """Infer what the certificate is for, and therefore how a break would harm it.

    The distinction matters more than any other single field: a break of a key-establishment
    key is retroactive via harvest-now-decrypt-later, while a break of a signing key is not.
    """
    try:
        usage = cert.extensions.get_extension_for_class(x509.KeyUsage).value
        if usage.key_encipherment or usage.key_agreement:
            return ("key_establishment", "confidentiality")
        if usage.digital_signature or usage.key_cert_sign:
            return ("signature", "authenticity" if is_ca else "both")
    except x509.ExtensionNotFound:
        pass
    # A TLS server certificate without explicit key usage is used for both.
    return ("signature", "authenticity" if is_ca else "both")


def parse_certificate(der_or_pem: bytes) -> ParsedAsset:
    """Parse one certificate."""
    _guard_private_key(der_or_pem)
    try:
        cert = (
            x509.load_pem_x509_certificate(der_or_pem)
            if b"-----BEGIN" in der_or_pem
            else x509.load_der_x509_certificate(der_or_pem)
        )
    except Exception as exc:  # noqa: BLE001 - surfaced as a typed error
        raise UnparseableCertificate(str(exc)) from exc

    try:
        bc = cert.extensions.get_extension_for_class(x509.BasicConstraints).value
        is_ca = bool(bc.ca)
    except x509.ExtensionNotFound:
        is_ca = False

    family, bits, curve, reason = _classify(cert)
    purpose, threat = _purpose_and_threat(cert, is_ca)

    spki = cert.public_key().public_bytes(Encoding.DER, PublicFormat.SubjectPublicKeyInfo)
    fingerprint = hashlib.sha256(spki).hexdigest()

    sig_alg = None
    try:
        sig_alg = cert.signature_algorithm_oid._name  # noqa: SLF001 - the public name
    except Exception:  # noqa: BLE001
        pass

    kind = "ca_root" if is_ca and cert.issuer == cert.subject else ("ca_intermediate" if is_ca else "tls_cert")

    return ParsedAsset(
        fingerprint=fingerprint,
        kind=kind,
        algorithm_family=family,
        key_bits=bits,
        curve=curve,
        signature_algorithm=sig_alg,
        purpose=purpose,
        threat_mode=threat,
        subject=cert.subject.rfc4514_string(),
        issuer=cert.issuer.rfc4514_string(),
        not_before=cert.not_valid_before_utc,
        not_after=cert.not_valid_after_utc,
        is_ca=is_ca,
        indeterminate_reason=reason,
        raw={
            "serial_number": str(cert.serial_number),
            "version": cert.version.name,
            "self_signed": cert.issuer == cert.subject,
        },
    )


def parse_bundle(data: bytes) -> list[ParsedAsset]:
    """Parse every certificate in a PEM bundle, or a single DER certificate."""
    _guard_private_key(data)
    blocks = _PEM_CERT.findall(data)
    if not blocks:
        return [parse_certificate(data)]
    out: list[ParsedAsset] = []
    for block in blocks:
        out.append(parse_certificate(block))
    return out
