"""Validated native Archive V2 access.

The Python facade intentionally has no ZIP parser.  The optional Rust
extension validates Archive V2 and returns the exact persisted DAG-ML package
bytes; callers must hand those bytes to DAG-ML for semantic validation and
replay.
"""

from __future__ import annotations

from pathlib import Path


class NativeArchiveUnavailableError(RuntimeError):
    """The installed Python facade has no matching native Archive V2 bridge."""


def read_portable_predictor_package_v2(path: str | Path) -> bytes:
    """Read exact Package V2 bytes from a Rust-validated Archive V2.

    This function neither opens ZIP members in Python nor decodes the package.
    Archive integrity and version dispatch remain owned by ``nirs4all-core``;
    DAG-ML owns the returned package's semantic validation and execution.
    """

    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            "Archive V2 access requires the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error

    return bytes(_native.read_portable_predictor_package_v2(str(Path(path))))
