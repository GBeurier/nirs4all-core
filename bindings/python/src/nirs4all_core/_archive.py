"""Validated native Archive V2/V3 access.

The Python facade intentionally has no ZIP parser.  The optional Rust
extension validates Archive V2 and returns the exact persisted DAG-ML package
bytes; callers must hand those bytes to DAG-ML for semantic validation and
replay.
"""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path
from typing import Any


class NativeArchiveUnavailableError(RuntimeError):
    """The installed facade has no matching native archive bridge."""


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


def write_archive_v2_from_native_payloads(
    path: str | Path,
    manifest: Mapping[str, Any],
    members: Mapping[str, bytes | bytearray | memoryview],
) -> dict[str, str]:
    """Atomically write a native Archive V2 from DAG-ML-assembled bytes.

    This facade owns no ZIP format, member hashing or DAG-ML replay semantics.
    It passes the manifest and exact opaque members to Core, which validates the
    closed Archive V2 contract and refuses existing targets before publishing.
    """

    if not isinstance(manifest, Mapping):
        raise TypeError("Archive V2 manifest must be a mapping")
    payloads: list[tuple[str, bytes]] = []
    for member_path, payload in sorted(members.items()):
        if not isinstance(member_path, str):
            raise TypeError("Archive V2 member paths must be strings")
        if not isinstance(payload, (bytes, bytearray, memoryview)):
            raise TypeError("Archive V2 member payloads must be bytes-like")
        payloads.append((member_path, bytes(payload)))
    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            "Archive V2 access requires the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error
    archive_id, archive_sha256 = _native.write_archive_v2_from_native_payloads(
        str(Path(path)), dict(manifest), payloads
    )
    return {"archive_id": str(archive_id), "archive_sha256": str(archive_sha256)}


def read_portable_refit_package_v3(path: str | Path) -> bytes:
    """Read exact Package V3 bytes from a Rust-validated Archive V3.

    The function is intentionally a transport boundary: Core validates the
    archive and its raw inventory, while DAG-ML parses the returned strict V3
    package and owns native replay.
    """

    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            "Archive V3 access requires the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error
    return bytes(_native.read_portable_refit_package_v3(str(Path(path))))


def write_archive_v3_from_native_payloads(
    path: str | Path,
    manifest: Mapping[str, Any],
    members: Mapping[str, bytes | bytearray | memoryview],
) -> dict[str, str]:
    """Atomically write a validated native Archive V3 from opaque DAG-ML bytes."""

    if not isinstance(manifest, Mapping):
        raise TypeError("Archive V3 manifest must be a mapping")
    payloads: list[tuple[str, bytes]] = []
    for member_path, payload in sorted(members.items()):
        if not isinstance(member_path, str):
            raise TypeError("Archive V3 member paths must be strings")
        if not isinstance(payload, (bytes, bytearray, memoryview)):
            raise TypeError("Archive V3 member payloads must be bytes-like")
        payloads.append((member_path, bytes(payload)))
    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            "Archive V3 access requires the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error
    archive_id, archive_sha256 = _native.write_archive_v3_from_native_payloads(
        str(Path(path)), dict(manifest), payloads
    )
    return {"archive_id": str(archive_id), "archive_sha256": str(archive_sha256)}
