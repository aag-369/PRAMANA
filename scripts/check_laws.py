#!/usr/bin/env python3
"""Mechanical enforcement of the PRAMANA laws.

The specification says CI must fail where a law is mechanically checkable. This is that
check. It is deliberately blunt: it reads the golden files, extracts every published figure,
and looks for those figures in production code.

**Law 1 — no lookup tables of results.** Published numbers may appear in exactly three
places: `verification/golden/`, the `pramana-verify` crate that consumes them, and inside
`#[cfg(test)]` blocks, which cannot be reached at runtime. A published *result* anywhere
else means the estimation path could read it instead of computing it, which is the failure
mode the whole project is built to avoid.

Published *inputs* are a different matter and are allowed: Cuccaro's cost of `2n` per
addition, Roetteler's Table 1 subroutine costs, or the grid-scanned window parameters from
Gidney's Table 5 are all parameters a construction is built from, not answers it should be
producing. The allowlist below records each one with its justification.

**Law 6 — units are typed.** Checked by `compile_fail` doctests in `pramana-units`; this
script verifies those doctests still exist rather than duplicating them.

**Law 11 — the hot path is Rust.** No costing kernel in Python outside the reference
directory.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ROOT / "crates"
GOLDEN = ROOT / "verification" / "golden"

#: Crates permitted to reference published results.
EXEMPT_CRATES = {"pramana-verify"}

#: Published *inputs* a construction is legitimately built from, with the reason each is
#: not a Law 1 violation. Anything not listed here that matches a golden figure fails.
ALLOWED_INPUTS: dict[str, str] = {
    "448": "Roetteler leading coefficient, asserted in tests as an emergent property",
    "224": "point-addition subroutine multiplicities (4 inv + 2 squ + 4 mul)",
    "16": "modular-addition incrementer count from Roetteler Table 1",
    "32": "AES S-box multiplicative complexity (Boyar-Peralta); also a distance",
    "12": "gross code logical qubits; also a window and a month count",
    "144": "gross code physical qubits, derived from the construction",
    "1.5": "Ekera-Hastad exponent multiple; also a routing factor",
}

#: Numeric literal, tolerant of underscores and scientific notation.
_NUMBER = re.compile(r"\b\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?\b")


def strip_test_modules(source: str) -> str:
    """Remove `#[cfg(test)]` blocks, which cannot execute at runtime."""
    out: list[str] = []
    i = 0
    while True:
        marker = source.find("#[cfg(test)]", i)
        if marker == -1:
            out.append(source[i:])
            break
        out.append(source[i:marker])
        brace = source.find("{", marker)
        if brace == -1:
            break
        depth = 0
        j = brace
        while j < len(source):
            if source[j] == "{":
                depth += 1
            elif source[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        i = j + 1
    return "".join(out)


def strip_comments(source: str) -> str:
    """Remove line comments and doc comments.

    A published figure quoted in a doc comment is documentation, not a lookup table. The
    distinction matters: `/// GE19 reports 2.6e9` is a citation, while
    `fn published() -> f64 { 2.6e9 }` is a result the code could return.
    """
    out = []
    for line in source.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("//"):
            continue
        idx = line.find("//")
        out.append(line[:idx] if idx != -1 else line)
    return "\n".join(out)


def golden_figures() -> dict[str, str]:
    """Every published figure in the golden files, mapped to its target id."""
    figures: dict[str, str] = {}
    for path in sorted(GOLDEN.glob("*.toml")):
        target = path.stem
        for line in path.read_text().splitlines():
            line = line.strip()
            if not line.startswith(("logical_qubits", "toffoli_count", "physical_qubits",
                                    "wall_clock_hours", "wall_clock_days",
                                    "leading_coefficient")):
                continue
            m = re.search(r"value\s*=\s*([0-9._eE+-]+)", line)
            if not m:
                continue
            raw = m.group(1).rstrip(".")
            try:
                value = float(raw.replace("_", ""))
            except ValueError:
                continue
            # Record both the literal spelling and a normalised integer form.
            figures.setdefault(raw, target)
            if value == int(value) and abs(value) < 1e9:
                figures.setdefault(str(int(value)), target)
    return figures


def check_law_1() -> list[str]:
    """Published results must not appear in production code."""
    failures: list[str] = []
    figures = golden_figures()
    interesting = {
        lit: tgt
        for lit, tgt in figures.items()
        if lit not in ALLOWED_INPUTS and len(lit.replace(".", "")) >= 3
    }

    for path in sorted(CRATES.rglob("*.rs")):
        crate = path.relative_to(CRATES).parts[0]
        if crate in EXEMPT_CRATES:
            continue
        if "/examples/" in str(path) or "\\examples\\" in str(path):
            # Examples are demonstration harnesses, not the estimation path, and their
            # whole purpose is to show computed figures beside published ones.
            continue
        source = strip_comments(strip_test_modules(path.read_text()))
        for literal, target in interesting.items():
            for m in _NUMBER.finditer(source):
                if m.group(0).replace("_", "") == literal.replace("_", ""):
                    line_no = source[: m.start()].count("\n") + 1
                    failures.append(
                        f"LAW 1: {path.relative_to(ROOT)}:{line_no} contains published "
                        f"figure {literal} (from golden target '{target}') in production "
                        f"code. Published results belong in verification/golden/ only."
                    )
                    break
    return failures


def check_law_1_naming() -> list[str]:
    """No production symbol should advertise itself as returning a published figure."""
    failures = []
    pattern = re.compile(r"pub (?:fn|const) (published_\w+|PUBLISHED_\w+)")
    for path in sorted(CRATES.rglob("*.rs")):
        crate = path.relative_to(CRATES).parts[0]
        if crate in EXEMPT_CRATES:
            continue
        source = strip_test_modules(path.read_text())
        for m in pattern.finditer(source):
            line_no = source[: m.start()].count("\n") + 1
            failures.append(
                f"LAW 1: {path.relative_to(ROOT)}:{line_no} exposes `{m.group(1)}` as "
                f"public API in an estimation crate. Move it to pramana-verify or into a "
                f"#[cfg(test)] block."
            )
    return failures


def check_law_6() -> list[str]:
    """Dimensional safety must remain compiler-enforced."""
    lib = (CRATES / "pramana-units" / "src" / "lib.rs").read_text()
    count = lib.count("```compile_fail")
    if count < 3:
        return [
            f"LAW 6: expected at least 3 compile_fail doctests proving incompatible "
            f"units do not compile, found {count}"
        ]
    return []


def check_law_11() -> list[str]:
    """No costing kernel in Python outside the reference directory."""
    failures = []
    reference = ROOT / "verification" / "reference"
    for path in sorted(ROOT.rglob("*.py")):
        if any(p in path.parts for p in ("node_modules", "build", ".venv", "tests")):
            continue
        if reference in path.parents or path.parent == Path(__file__).parent:
            continue
        text = path.read_text()
        if re.search(r"def \w*(logical_error|code_distance|solve_distance)\w*\(", text):
            failures.append(
                f"LAW 11: {path.relative_to(ROOT)} appears to implement a QEC costing "
                f"kernel in Python. The hot path belongs in Rust."
            )
    return failures


def main() -> int:
    """Run every mechanical check."""
    checks = [
        ("Law 1 (no published results in production code)", check_law_1),
        ("Law 1 (no published_* public API in estimation crates)", check_law_1_naming),
        ("Law 6 (units are compiler-enforced)", check_law_6),
        ("Law 11 (hot path is Rust)", check_law_11),
    ]
    total = 0
    for name, fn in checks:
        failures = fn()
        total += len(failures)
        status = "PASS" if not failures else f"FAIL ({len(failures)})"
        print(f"  {status:<10} {name}")
        for f in failures:
            print(f"             {f}")
    print()
    if total:
        print(f"{total} law violation(s). See docs/STATUS.md for what each law protects.")
        return 1
    print("All mechanically checkable laws hold.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
