"""Loader for the compiled Rust estimation core.

The core is a native extension built from ``crates/pramana-py``. It is imported lazily and
through one place, so a missing build produces one clear error rather than an import
traceback from somewhere deep in a request handler.
"""

from __future__ import annotations

import functools
import sys
from typing import Any

from .config import SETTINGS


class CoreUnavailable(RuntimeError):
    """The compiled estimation core could not be imported."""


@functools.lru_cache(maxsize=1)
def core() -> Any:
    """Import and return the ``pramana`` extension module."""
    if SETTINGS.core_path not in sys.path:
        sys.path.insert(0, SETTINGS.core_path)
    try:
        import pramana  # type: ignore
    except ImportError as exc:  # pragma: no cover - exercised only without a build
        raise CoreUnavailable(
            "the compiled PRAMANA core is not importable. Build it with\n"
            "    cargo build --release -p pramana-py\n"
            "then copy target/release/libpramana.so to build/pramana.so, or set "
            "PRAMANA_CORE_PATH to its directory."
        ) from exc
    return pramana


def core_version() -> str:
    """Version of the loaded core, for provenance records."""
    return str(core().__version__)
