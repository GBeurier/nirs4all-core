"""Thin Python translation over Core's native n4a bridge.

Core's Rust crate owns every decision here: the strict recipe parser, the
single native fit, container parsing and validation, owner admission, the
closed native-produced model profile, identities and replay. This module only
forwards bytes, strings and file paths to the native extension and decodes the
JSON text it returns. It contains no ZIP or JSON-schema parser, no fingerprint
or hash arithmetic, no numerics and no fallback to another archive path.

Core imposes no content-size cap on a ``.n4a``. The only bound is the optional
``memory_budget_bytes`` of a call: the working memory the caller authorizes for
Core's own allocations (never the payload that stays on disk). Native model
inspection, prediction and retraining accept exact bytes or a real open
``N4aFile``. They materialize file bytes under the request budget, verify those
bytes against the opening receipt and reuse validated metadata without reopening
or reparsing. The original handle stays usable. For these calls, ``None``
inherits a file's opening budget; for bytes it means only allocator and host
constraints. An explicit budget overrides the file's budget for that call.
Elsewhere ``None`` means only allocator and host constraints, unless documented
as inheriting the handle's budget. It is not a claim about machine
RAM. A refusal for the budget, for corruption or for a changed file is a
``ValueError``. Results typed ``bytes`` are materialized by Python on request,
so their size is the caller's choice; file handles and streams avoid that.

A failure raised by the native bridge is a typed refusal and propagates as the
native ``ValueError``. A missing or mismatched extension is reported as
``NativeArchiveUnavailableError``.
"""

from __future__ import annotations

import json
import os
from collections.abc import Mapping, Sequence
from typing import Any, BinaryIO, Protocol

from ._archive import NativeArchiveUnavailableError

_VIEW_KEYS = frozenset({"manifest", "receipt", "training"})
_PREDICTION_KEYS = frozenset({"replay", "receipt", "sample_ids"})


def _native(name: str) -> Any:
    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            "n4a native actions require the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error
    function = getattr(_native, name, None)
    if not callable(function):
        raise NativeArchiveUnavailableError(
            "The installed nirs4all-core native wheel does not expose the n4a "
            f"native bridge function {name!r}; install a matching nirs4all-core "
            "distribution."
        )
    return function


def _budget(value: int | None) -> int | None:
    """Validate a caller-declared working-memory budget; Core enforces it."""
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError("memory_budget_bytes must be a non-negative int or None")
    if value < 0:
        raise ValueError("memory_budget_bytes must be a non-negative int or None")
    return value


def _decode(payload: Any, action: str, keys: frozenset[str]) -> dict[str, Any]:
    """Decode native JSON text only; the key set is a bridge-contract guard."""
    if not isinstance(payload, str):
        raise RuntimeError(f"native n4a {action} returned a non-JSON payload")
    try:
        decoded = json.loads(payload)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"native n4a {action} returned invalid JSON") from error
    if not isinstance(decoded, dict) or set(decoded) != keys:
        raise RuntimeError(f"native n4a {action} returned an unexpected mapping")
    return decoded


def produce_n4a_native(
    record: bytes,
    source_id: str,
    recipe_json: str,
    *,
    methods_library_path: str,
    methods_library_sha256: str,
    run_id: str,
    container_id: str,
    embed_training_record: bool = True,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Train once natively and return ``{"archive_bytes", "view"}``.

    ``record`` is the exact IO record bytes and ``recipe_json`` the native
    pipeline recipe text; Core parses both. The sealed bytes and the training
    view come from the same single fit, so the archive is never reopened to
    build the view. Every recipe parameter is preserved or the call is refused.
    ``archive_bytes`` is materialized once from Core's sealed buffer and admitted
    against ``memory_budget_bytes``.
    """
    archive, view = _native("produce_n4a_native_with_view")(
        record,
        source_id,
        recipe_json,
        methods_library_path=methods_library_path,
        methods_library_sha256=methods_library_sha256,
        run_id=run_id,
        container_id=container_id,
        embed_training_record=embed_training_record,
        memory_budget_bytes=_budget(memory_budget_bytes),
    )
    if not isinstance(archive, bytes):
        raise RuntimeError("native n4a production returned non-bytes archive content")
    return {"archive_bytes": archive, "view": _decode(view, "production", _VIEW_KEYS)}


def retrain_n4a_native(
    archive_bytes: bytes | N4aFile,
    *,
    record: bytes,
    methods_library_path: str,
    methods_library_sha256: str,
    run_id: str,
    container_id: str,
    embed_training_record: bool = True,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Fit a stored model on new data and return ``{"archive_bytes", "view"}``.

    Core reads the recipe and source id from the validated archive, so neither
    is an argument. ``record`` is the exact IO record bytes. The result is a new
    archive from one fit; ``archive_bytes`` stays unchanged.

    ``archive_bytes`` accepts bytes or a real open Core ``N4aFile``. File reads
    verify the materialized bytes and reuse validated metadata; the handle stays
    open. ``None`` inherits its opening budget; an explicit request overrides it
    for the whole call. Closed or changed files raise ``ValueError``; other
    archive argument types raise ``TypeError``.
    """
    archive, view = _native("retrain_n4a_native_with_view")(
        archive_bytes,
        record=record,
        methods_library_path=methods_library_path,
        methods_library_sha256=methods_library_sha256,
        run_id=run_id,
        container_id=container_id,
        embed_training_record=embed_training_record,
        memory_budget_bytes=_budget(memory_budget_bytes),
    )
    if not isinstance(archive, bytes):
        raise RuntimeError("native n4a retraining returned non-bytes archive content")
    return {"archive_bytes": archive, "view": _decode(view, "retraining", _VIEW_KEYS)}


def inspect_n4a_bytes(
    archive_bytes: bytes | N4aFile, *, memory_budget_bytes: int | None = None
) -> dict[str, Any]:
    """Return the native view of a closed native-produced model archive.

    The view has exactly three keys: the full typed manifest, the container
    receipt and the actual owner training outcome with its source id. Any other
    archive shape is refused explicitly by Core; there is no fallback.

    Accepts bytes or a real open Core ``N4aFile``. File reads verify the
    materialized bytes and reuse validated metadata without consuming the
    handle. ``None`` inherits the opening budget; an explicit request overrides
    it for the whole call. Closed or changed files raise ``ValueError``; other
    archive argument types raise ``TypeError``.
    """
    view = _native("inspect_n4a_bytes")(
        archive_bytes, memory_budget_bytes=_budget(memory_budget_bytes)
    )
    return _decode(view, "inspection", _VIEW_KEYS)


def predict_n4a_native(
    archive_bytes: bytes | N4aFile,
    *,
    record: bytes | None,
    entrypoint: str,
    source_id: str,
    partition: str,
    request_id: str,
    run_id: str,
    methods_library_path: str,
    methods_library_sha256: str,
    require_training_schema: bool = True,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Replay an n4a entrypoint and return ``{"replay", "receipt", "sample_ids"}``.

    ``sample_ids`` lists the selected partition's sample IDs in the IO record's
    physical order. The signed ``replay`` keeps the owner's own order and is
    returned unchanged, so a caller aligns predictions to record order by ID.

    ``record=None`` selects the archive's stored input; bytes select an external
    record. ``require_training_schema=False`` only allows an unavailable schema
    anchor; a present anchor that disagrees is always refused by Core.

    ``archive_bytes`` accepts bytes or a real open Core ``N4aFile``. File reads
    verify the materialized bytes and reuse validated metadata; the handle stays
    open. ``None`` inherits its opening budget; an explicit request overrides it
    for the whole call. Closed or changed files raise ``ValueError``; other
    archive argument types raise ``TypeError``.
    """
    result = _native("predict_n4a_native_json")(
        archive_bytes,
        record=record,
        entrypoint=entrypoint,
        source_id=source_id,
        partition=partition,
        request_id=request_id,
        run_id=run_id,
        methods_library_path=methods_library_path,
        methods_library_sha256=methods_library_sha256,
        require_training_schema=require_training_schema,
        memory_budget_bytes=_budget(memory_budget_bytes),
    )
    prediction = _decode(result, "prediction", _PREDICTION_KEYS)
    sample_ids = prediction["sample_ids"]
    if not isinstance(sample_ids, list) or not all(isinstance(i, str) for i in sample_ids):
        raise RuntimeError("native n4a prediction returned invalid sample_ids")
    return prediction


class N4aContainer(Protocol):
    """Immutable validated owner-neutral ``.n4a`` container held in memory.

    Opening validates the layout, manifest and every raw size and SHA-256 once;
    reads below are plain slices of that validated buffer. Host payloads are
    returned as ``bytes`` and are never deserialized by Core: whether a member
    can be executed is a separate decision of its owner, not of this handle.
    Each ``bytes`` result is a copy admitted against the budget the handle was
    opened with.
    """

    def view(self) -> dict[str, Any]:
        """Readability report; ``executability`` is always ``"not_evaluated"``."""

    def manifest_bytes(self) -> bytes:
        """The exact manifest bytes stored in the container."""

    def member(self, path: str) -> bytes:
        """The exact bytes of one inventory member (``KeyError`` if absent)."""

    def artifact_members(self, artifact_id: str) -> Mapping[str, bytes]:
        """Embedded member bytes of one artifact by archive path (``KeyError`` if unknown)."""

    def archive_bytes(self) -> bytes:
        """The exact original container bytes."""


class N4aMemberStream(Protocol):
    """Read-only binary stream over one member of an :class:`N4aFile`.

    It follows ``io.RawIOBase`` (``read``, ``readinto``, ``readable``), so
    ``io.BufferedReader(stream)`` works; it is not seekable. Bytes returned
    before the end are not yet verified. The call that would deliver the member's
    last byte first checks the whole member against the validated receipt and
    raises ``ValueError`` instead of returning those bytes if it does not match.
    A stream abandoned early has verified nothing: only ``verified`` is trusted.
    """

    @property
    def verified(self) -> bool:
        """``True`` only after the whole member was delivered and matched."""

    @property
    def byte_length(self) -> int:
        """Total member size in bytes."""

    @property
    def remaining(self) -> int:
        """Bytes not yet delivered."""

    @property
    def closed(self) -> bool:
        """Whether :meth:`close` was called."""

    def read(self, size: int | None = None) -> bytes:
        """Up to ``size`` bytes; all remaining bytes when omitted or negative."""

    def readall(self) -> bytes:
        """All remaining bytes, verified."""

    def readinto(self, buffer: bytearray | memoryview) -> int:
        """Fill ``buffer`` with at most one chunk; ``0`` means a verified end."""

    def readable(self) -> bool:
        """Always ``True``."""

    def tell(self) -> int:
        """Bytes delivered so far."""

    def flush(self) -> None:
        """Do nothing on an open read-only stream; raise ``ValueError`` if closed."""

    def close(self) -> None:
        """Release the stream."""

    def __enter__(self) -> N4aMemberStream: ...

    def __exit__(self, *exc: object) -> None: ...


class N4aFile(Protocol):
    """File-backed validated owner-neutral ``.n4a`` container.

    Opening makes one sequential verified pass and keeps only the validated
    index in memory; payload stays on disk, so an archive larger than RAM can be
    inspected, streamed and copied. The file is not locked and nothing is trusted
    from the open: every read hashes the bytes it actually returns, in that one
    pass, and compares them with the validated receipt. A file that changed or
    shrank afterwards is refused with ``ValueError``. Use it as a context manager
    or call :meth:`close`; streams already opened keep their own reference.
    """

    def view(self) -> dict[str, Any]:
        """Readability report; DAG-owned members are read and verified on demand."""

    def manifest_bytes(self) -> bytes:
        """The exact manifest bytes stored in the container."""

    def artifact_member_paths(self, artifact_id: str) -> list[str]:
        """Archive paths of an artifact, ``[]`` for a reference (``KeyError`` if unknown)."""

    def member(self, path: str, *, memory_budget_bytes: int | None = None) -> bytes:
        """One member, verified to its end, materialized as ``bytes`` (``KeyError`` if absent).

        The caller pays this materialization; ``memory_budget_bytes`` overrides the
        handle's budget for this call. Prefer :meth:`open_member` or
        :meth:`copy_member_to` for large members.
        """

    def open_member(self, path: str) -> N4aMemberStream:
        """Open one member as a verified-at-EOF binary stream (``KeyError`` if absent)."""

    def copy_member_to(self, path: str, destination: BinaryIO) -> int:
        """Stream one member into ``destination.write`` and return the byte count.

        Verification happens during this single read. On ``ValueError`` the
        destination already holds unverified bytes and the caller must discard it.
        """

    def copy_archive_to(self, destination: BinaryIO) -> int:
        """Stream the exact archive bytes, checked against the container SHA-256.

        The same discard rule as :meth:`copy_member_to` applies on ``ValueError``.
        """

    @property
    def closed(self) -> bool:
        """Whether :meth:`close` was called."""

    def close(self) -> None:
        """Release the handle; later calls raise ``ValueError``."""

    def __enter__(self) -> N4aFile: ...

    def __exit__(self, *exc: object) -> None: ...


def open_n4a_container(
    archive: bytes, *, memory_budget_bytes: int | None = None
) -> N4aContainer:
    """Validate ``archive`` once and return an immutable in-memory container handle.

    This is the owner-neutral storage route: it does not require the closed
    native-model shape that :func:`inspect_n4a_bytes` demands, and it loads no
    Methods runtime. Any container the Core reader refuses raises ``ValueError``
    with its stable refusal prefix. The handle keeps one copy of ``archive``
    (admitted against ``memory_budget_bytes``); for archives that should stay on
    disk use :func:`open_n4a_file`.
    """
    return _native("open_n4a_container")(  # type: ignore[no-any-return]
        archive, memory_budget_bytes=_budget(memory_budget_bytes)
    )


def open_n4a_file(
    path: str | os.PathLike[str], *, memory_budget_bytes: int | None = None
) -> N4aFile:
    """Open and verify a ``.n4a`` file without loading its payload into memory.

    Core's single scanner validates the layout, manifest and every raw size,
    SHA-256 and CRC in one sequential pass and keeps only the index. There is no
    size cap: only ``memory_budget_bytes`` (Core's own working memory, ``None``
    for allocator-only limits) can refuse, and a budget below the streaming
    chunk (4096 bytes) is refused. Any container the Core reader refuses raises
    ``ValueError``; a non-file or missing path raises ``ValueError`` too.
    """
    return _native("open_n4a_file")(  # type: ignore[no-any-return]
        path, memory_budget_bytes=_budget(memory_budget_bytes)
    )


def write_n4a_container(
    manifest_json: str | bytes,
    members: Sequence[tuple[str, bytes]],
    *,
    base: N4aContainer | None = None,
    memory_budget_bytes: int | None = None,
) -> bytes:
    """Write a new container and return its bytes; ``base`` is never modified.

    ``manifest_json`` is passed to Core as exact JSON text. Core derives every
    member size and SHA-256 itself, so placeholders are accepted. Without
    ``base`` the members must cover the manifest exactly; with ``base`` they are
    replacements, declared members that are not replaced keep their bytes from
    ``base`` and omitting a member from the manifest deletes it. Duplicate paths
    and members absent from the manifest are refused by Core, never collapsed.
    The result is materialized as ``bytes`` (admitted against the budget); use
    :func:`write_n4a_file` to write straight to disk.
    """
    pairs = [(path, payload) for path, payload in members]
    return _native("write_n4a_container")(  # type: ignore[no-any-return]
        manifest_json,
        pairs,
        base=base,
        memory_budget_bytes=_budget(memory_budget_bytes),
    )


def write_n4a_file(
    destination: str | os.PathLike[str],
    manifest_json: str | bytes,
    members: Sequence[tuple[str, bytes | os.PathLike[str]]],
    *,
    base: N4aContainer | N4aFile | None = None,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Write a new container straight to ``destination`` and return its receipt.

    Each member is ``(archive_path, bytes)`` or ``(archive_path, os.PathLike)``;
    a path member is read from disk by Core, twice (digest pass, write pass),
    and never held in memory. A source that changes between the passes is
    refused. A ``str`` member is rejected as ambiguous. Semantics otherwise
    match :func:`write_n4a_container`: exact manifest text, placeholders
    accepted, duplicates and undeclared members refused, and with ``base`` (an
    :class:`N4aFile` or :class:`N4aContainer`, never modified) the members are
    replacements. ``destination`` is staged and published without replacing
    anything; an existing destination is refused. The result is Core's final
    receipt, computed while writing and never re-read: ``container_sha256``,
    ``manifest_sha256``, ``byte_length`` and ``members``, a list of
    ``{"path", "byte_length", "sha256"}`` (raw SHA-256 hex) for every stored
    member, including those copied from ``base``.
    """
    pairs = [(path, payload) for path, payload in members]
    receipt = _native("write_n4a_file")(
        destination,
        manifest_json,
        pairs,
        base=base,
        memory_budget_bytes=_budget(memory_budget_bytes),
    )
    return receipt  # type: ignore[no-any-return]


__all__ = [
    "N4aContainer",
    "N4aFile",
    "N4aMemberStream",
    "inspect_n4a_bytes",
    "open_n4a_container",
    "open_n4a_file",
    "predict_n4a_native",
    "produce_n4a_native",
    "retrain_n4a_native",
    "write_n4a_container",
    "write_n4a_file",
]
