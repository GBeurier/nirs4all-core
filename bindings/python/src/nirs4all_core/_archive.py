"""Validated native Package V2/V3 access and callback-free Methods replay.

The Python facade intentionally has no ZIP parser, root selection or entrypoint
admission. The optional Rust extension reads the owner-neutral Core V1 container,
validates the selected native scientific closure (owner, content type, profile,
operation, links and fitted states) and delegates signed package/replay semantics
to DAG-ML. Package schema versions 2 and 3 are the Package roots; the container,
Methods ABI and wire versions are independent.

Every reader and action accepts a path (``os.PathLike``), a retained open
:class:`~nirs4all_core.N4aFile` or a ``NativePackage`` returned by
:func:`open_native_package`, plus keyword-only ``entrypoint`` and
``memory_budget_bytes``. A ``NativePackage`` reuses its selected proof; an omitted
budget inherits the retained handle's budget. The portable execution surface
accepts numeric Methods inputs only; it has no Python callback or serialized
host-model path.
"""

from __future__ import annotations

import json
import os
from collections.abc import Mapping
from pathlib import Path
from typing import Any


class NativeArchiveUnavailableError(RuntimeError):
    """The installed Python facade has no matching native archive bridge."""


def _input(source: Any) -> Any:
    """Forward a path as text; a retained ``N4aFile``/``NativePackage`` unchanged (Core types it)."""
    if isinstance(source, (str, os.PathLike)):
        return str(Path(source))
    return source


def _budget(value: int | None) -> int | None:
    from ._n4a import _budget as validate

    return validate(value)


def _selection(entrypoint: str | None, memory_budget_bytes: int | None) -> dict[str, Any]:
    """The keyword-only Core selection shared by every native reader and action.

    Only supplied values are forwarded: an omitted ``entrypoint`` lets Core select the unique
    matching one and an omitted budget inherits the retained handle's budget.
    """
    if entrypoint is not None and not isinstance(entrypoint, str):
        raise TypeError("entrypoint must be a string or None")
    selection: dict[str, Any] = {}
    if entrypoint is not None:
        selection["entrypoint"] = entrypoint
    budget = _budget(memory_budget_bytes)
    if budget is not None:
        selection["memory_budget_bytes"] = budget
    return selection


def open_native_package(
    path: Any,
    *,
    entrypoint: str | None = None,
    operation: str = "predict-external",
    memory_budget_bytes: int | None = None,
) -> Any:
    """Open one ephemeral ``NativePackage``: Core's selected, validated native closure.

    Core selects the unique entrypoint declaring ``operation`` (or checks the explicit
    ``entrypoint``), validates the selected closure once and retains the source and proof.
    Pass the result to any native reader or action to reuse that proof. Calibration opens
    with ``operation="calibrate-external"``; a predict proof is not a calibration admission.
    The facade never looks up entrypoint ids or infers a root from artifact order.
    """
    if not isinstance(operation, str) or not operation:
        raise TypeError("operation must be a non-empty string")
    opener = _native_replay("open_native_package", "native package")
    return opener(_input(path), operation=operation, **_selection(entrypoint, memory_budget_bytes))


def read_native_package_view(
    path: Any,
    *,
    entrypoint: str | None = None,
    operation: str = "predict-external",
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Return the retained handle's native view (``package_schema_version`` 2 or 3)."""
    package = open_native_package(
        path, entrypoint=entrypoint, operation=operation, memory_budget_bytes=memory_budget_bytes
    )
    view = package.view()
    if not isinstance(view, dict):  # pragma: no cover - native bridge contract guard
        raise RuntimeError("native package view did not return a mapping")
    return view


def read_archive_v2_payloads(
    path: Any, *, entrypoint: str | None = None, memory_budget_bytes: int | None = None
) -> dict[str, Any]:
    """Read the selected, Core-validated closure of a native Package V2 container.

    Returns ``{manifest, members, package, training_outcome, view}``: the neutral Core V1
    manifest, the selected physical members only (no aliases or nested archive), the exact
    package root bytes, the exact training outcome bytes and the native view. Core has
    already validated the closure; the result must not be passed to a physical-manifest
    validator.
    """
    read = _native_replay("read_archive_v2_payloads", "V2 payload inventory")
    result = read(_input(path), **_selection(entrypoint, memory_budget_bytes))
    keys = {"manifest", "members", "package", "training_outcome", "view"}
    if not isinstance(result, dict) or set(result) != keys:
        raise RuntimeError("native Package V2 inventory returned an invalid mapping")
    if not (
        isinstance(result["manifest"], dict)
        and isinstance(result["members"], dict)
        and isinstance(result["view"], dict)
        and isinstance(result["package"], bytes)
        and isinstance(result["training_outcome"], bytes)
    ):
        # A native bridge contract violation is a runtime failure, not bad user input.
        raise RuntimeError("native Package V2 inventory returned invalid payloads")  # noqa: TRY004
    if not all(isinstance(path, str) and isinstance(data, bytes)
               for path, data in result["members"].items()):
        raise RuntimeError("native Package V2 inventory returned invalid member bytes")
    return result


def write_native_predictor_package(
    path: str | os.PathLike[str],
    outcome: bytes,
    package: bytes,
    *,
    container_id: str,
    memory_budget_bytes: int | None = None,
) -> tuple[str, str]:
    """Write a NEW native Package V2 container; return ``(container_id, file_sha256)``.

    ``outcome`` and ``package`` are the exact TrainingOutcome and PortablePredictorPackage V2
    JSON bytes. Core builds the native scientific closure through DAG-ML and writes the
    owner-neutral V1 container; it never replaces an existing destination (a caller that
    overwrites stages a new path and replaces it itself).
    """
    return _write_native("write_native_predictor_package", path, (outcome, package), container_id, memory_budget_bytes)


def write_native_refit_package(
    path: str | os.PathLike[str],
    package: bytes,
    *,
    container_id: str,
    memory_budget_bytes: int | None = None,
) -> tuple[str, str]:
    """Write a NEW native Package V3 container; return ``(container_id, file_sha256)``.

    ``package`` is the exact PortableRefitPackage V3 JSON bytes. The child is stored
    standalone: no parent outcome, ``RefitOf`` link or score is invented. Core never replaces
    an existing destination.
    """
    return _write_native("write_native_refit_package", path, (package,), container_id, memory_budget_bytes)


def _write_native(
    name: str,
    path: str | os.PathLike[str],
    payloads: tuple[bytes, ...],
    container_id: str,
    memory_budget_bytes: int | None,
) -> tuple[str, str]:
    if not isinstance(path, (str, os.PathLike)):
        raise TypeError("a native package destination must be a path")
    if not isinstance(container_id, str) or not container_id:
        raise TypeError("container_id must be a non-empty string")
    if not all(isinstance(payload, bytes) for payload in payloads):
        raise TypeError("native package writers take exact JSON bytes")
    write = _native_replay(name, "native package writing")
    container, digest = write(_input(path), *payloads, container_id=container_id, **_selection(None, memory_budget_bytes))
    return str(container), str(digest)


def _contract_json(value: Any, label: str) -> str:
    if isinstance(value, bytes):
        try:
            return value.decode("utf-8")
        except UnicodeDecodeError as error:
            raise ValueError(f"{label} bytes must be UTF-8 JSON") from error
    if isinstance(value, str):
        return value
    try:
        return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)
    except (TypeError, ValueError) as error:
        raise TypeError(f"{label} must be JSON-compatible") from error


def _native_replay(name: str, version: str) -> Any:
    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            f"Archive {version} replay requires the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error
    replay = getattr(_native, name, None)
    if not callable(replay):
        raise NativeArchiveUnavailableError(
            f"The installed nirs4all-core native wheel does not expose Archive {version} replay; "
            "install a matching nirs4all-core distribution."
        )
    return replay


def _decode_replay_outcome(payload: Any, version: str) -> dict[str, Any]:
    if not isinstance(payload, str):
        raise RuntimeError(f"native Archive {version} replay returned a non-JSON payload")
    try:
        outcome = json.loads(payload)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"native Archive {version} replay returned invalid JSON") from error
    if not isinstance(outcome, dict):
        raise RuntimeError(f"native Archive {version} replay returned a non-object outcome")
    return outcome


def read_archive_v3_view(
    path: Any, *, entrypoint: str | None = None, memory_budget_bytes: int | None = None
) -> dict[str, Any]:
    """Project a Rust-validated native Package V3 container into its native view.

    The returned mapping is the native view (``package_schema_version`` 3, root and
    package member, receipt). This facade never opens ZIP members in Python, decodes a
    DAG-ML/N4MM payload, or executes a replay.
    """

    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            "Archive V3 access requires the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error
    result = _native.read_archive_v3_view(_input(path), **_selection(entrypoint, memory_budget_bytes))
    if not isinstance(result, dict):  # pragma: no cover - native bridge contract guard
        raise RuntimeError("native Archive V3 view did not return a mapping")
    return result


def read_portable_predictor_package_v2(
    path: Any, *, entrypoint: str | None = None, memory_budget_bytes: int | None = None
) -> bytes:
    """Read exact Package V2 root bytes from a Rust-validated native Package V2 container.

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

    return bytes(_native.read_portable_predictor_package_v2(_input(path), **_selection(entrypoint, memory_budget_bytes)))


def read_portable_refit_package_v3(
    path: Any, *, entrypoint: str | None = None, memory_budget_bytes: int | None = None
) -> bytes:
    """Read exact Package V3 refit root bytes from a Rust-validated native Package V3 container.

    This function neither opens ZIP members in Python nor decodes the package.
    Archive integrity and version dispatch remain owned by ``nirs4all-core``;
    DAG-ML owns the returned package's semantic validation and execution.
    """

    try:
        from . import _native
    except ImportError as error:  # pragma: no cover - depends on wheel build
        raise NativeArchiveUnavailableError(
            "Archive V3 access requires the nirs4all-core native wheel; "
            "install a matching nirs4all-core distribution."
        ) from error

    return bytes(_native.read_portable_refit_package_v3(_input(path), **_selection(entrypoint, memory_budget_bytes)))


def inspect_methods_archive_v2_predictors(
    path: Any,
    *,
    methods_library_path: str | Path,
    methods_library_sha256: str,
    entrypoint: str | None = None,
    memory_budget_bytes: int | None = None,
) -> list[dict[str, Any]]:
    """Return native predictor descriptors derived from Archive V2 bytes.

    Rust validates the archive and attests the selected libn4m before DAG-ML
    derives each descriptor through Methods' complete N4MM inspection API.
    Historical Archive V2 packages without an embedded descriptor remain
    supported; Python never reconstructs or trusts descriptor metadata.
    """

    inspect = _native_replay("inspect_methods_archive_v2_predictors_json", "V2 inspection")
    payload = inspect(
        _input(path),
        str(Path(methods_library_path)),
        methods_library_sha256,
        **_selection(entrypoint, memory_budget_bytes),
    )
    if not isinstance(payload, str):
        raise RuntimeError("native Archive V2 inspection returned a non-JSON payload")
    try:
        descriptors = json.loads(payload)
    except json.JSONDecodeError as error:
        raise RuntimeError("native Archive V2 inspection returned invalid JSON") from error
    if not isinstance(descriptors, list) or not all(
        isinstance(descriptor, dict) for descriptor in descriptors
    ):
        raise RuntimeError("native Archive V2 inspection returned an invalid descriptor list")
    return descriptors


def replay_methods_archive_v2(
    path: Any,
    request: Any,
    data_envelopes: Any,
    methods_inputs: Any,
    *,
    methods_library_path: str | Path,
    outcome_id: str,
    run_id: str,
    warnings: Any = (),
    diagnostics: Any = None,
    entrypoint: str | None = None,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Replay a portable Methods Archive V2 without Python model callbacks.

    Rust Core validates and opens the archive before DAG-ML parses the signed
    replay contracts or configures a fresh N4MM runtime. ``methods_inputs`` is
    an exact requirement-keyed map of numeric matrices; callbacks, estimator
    handles, pickle, and joblib sidecars are not accepted by this API.
    """

    replay = _native_replay("replay_methods_archive_v2_json", "V2")
    payload = replay(
        _input(path),
        _contract_json(request, "replay request"),
        _contract_json(data_envelopes, "data envelopes"),
        _contract_json(methods_inputs, "Methods inputs"),
        str(Path(methods_library_path)),
        outcome_id,
        run_id,
        _contract_json(warnings, "warnings"),
        _contract_json({} if diagnostics is None else diagnostics, "diagnostics"),
        **_selection(entrypoint, memory_budget_bytes),
    )
    return _decode_replay_outcome(payload, "V2")


def predict_methods_archive_v2_matrix(
    path: Any,
    sample_ids: list[str],
    x: list[list[float]],
    expected_target_names: list[str],
    *,
    methods_library_path: str | Path,
    methods_library_sha256: str,
    request_id: str,
    outcome_id: str,
    run_id: str,
    warnings: Any = (),
    diagnostics: Any = None,
    entrypoint: str | None = None,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Execute one closed, attested X-only Archive V2 prediction.

    Core derives every DAG-ML replay contract, requires one finalized output
    binding and one external data requirement, checks sample/target order, and
    loads a private snapshot of the exact SHA-256-addressed libn4m after ABI
    2.5 preflight. Python only transports the host matrix and strict JSON
    result; it cannot select a fallback, fit/refit phase or callback.
    """

    predict = _native_replay("predict_methods_archive_v2_matrix_json", "V2 matrix prediction")
    payload = predict(
        _input(path),
        _contract_json(
            {
                "sample_ids": sample_ids,
                "x": x,
                "expected_target_names": expected_target_names,
                "methods_library_path": str(Path(methods_library_path)),
                "methods_library_sha256": methods_library_sha256,
                "request_id": request_id,
                "outcome_id": outcome_id,
                "run_id": run_id,
                "warnings": warnings,
                "diagnostics": {} if diagnostics is None else diagnostics,
            },
            "Archive V2 matrix prediction request",
        ),
        **_selection(entrypoint, memory_budget_bytes),
    )
    return _decode_replay_outcome(payload, "V2 matrix prediction")


def replay_methods_archive_v2_conformal_presentation_v1(
    path: Any,
    request: Any,
    data_envelopes: Any,
    methods_inputs: Any,
    *,
    methods_library_path: str | Path,
    outcome_id: str,
    run_id: str,
    warnings: Any = (),
    diagnostics: Any = None,
    entrypoint: str | None = None,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Replay a calibrated Archive V2 into a closed UI presentation.

    The native DAG-ML coordinator computes and validates the complete
    presentation from the package calibration and replay outcome. Python only
    serializes input contracts and decodes the returned JSON object; it does
    not calculate conformal quantiles, bounds, fingerprints, or sample joins.
    """

    replay = _native_replay(
        "replay_methods_archive_v2_conformal_presentation_v1_json", "V2 conformal"
    )
    payload = replay(
        _input(path),
        _contract_json(request, "replay request"),
        _contract_json(data_envelopes, "data envelopes"),
        _contract_json(methods_inputs, "Methods inputs"),
        str(Path(methods_library_path)),
        outcome_id,
        run_id,
        _contract_json(warnings, "warnings"),
        _contract_json({} if diagnostics is None else diagnostics, "diagnostics"),
        **_selection(entrypoint, memory_budget_bytes),
    )
    return _decode_replay_outcome(payload, "V2 conformal presentation")


def replay_methods_archive_v2_conformal_presentation_v2(
    path: Any,
    request: Any,
    data_envelopes: Any,
    methods_inputs: Any,
    *,
    methods_library_path: str | Path,
    outcome_id: str,
    run_id: str,
    warnings: Any = (),
    diagnostics: Any = None,
    entrypoint: str | None = None,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Replay a calibrated Archive V2 into its complete native presentation.

    DAG-ML returns the content-bound multi-target V2 contract. Python only
    serializes replay inputs and decodes that validated JSON; predictor,
    archive, calibration and presentation fingerprints stay native-owned.
    """

    replay = _native_replay(
        "replay_methods_archive_v2_conformal_presentation_v2_json",
        "V2 conformal presentation V2",
    )
    payload = replay(
        _input(path),
        _contract_json(request, "replay request"),
        _contract_json(data_envelopes, "data envelopes"),
        _contract_json(methods_inputs, "Methods inputs"),
        str(Path(methods_library_path)),
        outcome_id,
        run_id,
        _contract_json(warnings, "warnings"),
        _contract_json({} if diagnostics is None else diagnostics, "diagnostics"),
        **_selection(entrypoint, memory_budget_bytes),
    )
    return _decode_replay_outcome(payload, "V2 conformal presentation V2")


def replay_methods_archive_v3(
    path: Any,
    request: Any,
    data_envelopes: Any,
    methods_inputs: Any,
    *,
    methods_library_path: str | Path,
    outcome_id: str,
    run_id: str,
    warnings: Any = (),
    diagnostics: Any = None,
    entrypoint: str | None = None,
    memory_budget_bytes: int | None = None,
) -> dict[str, Any]:
    """Replay a portable Methods Archive V3 without host controllers.

    The Python surface always supplies an empty supplemental-controller
    registry. A V3 package requiring a Python/plugin controller therefore
    fails closed instead of hydrating a callback or serialized host model.
    """

    replay = _native_replay("replay_methods_archive_v3_json", "V3")
    payload = replay(
        _input(path),
        _contract_json(request, "replay request"),
        _contract_json(data_envelopes, "data envelopes"),
        _contract_json(methods_inputs, "Methods inputs"),
        str(Path(methods_library_path)),
        outcome_id,
        run_id,
        _contract_json(warnings, "warnings"),
        _contract_json({} if diagnostics is None else diagnostics, "diagnostics"),
        **_selection(entrypoint, memory_budget_bytes),
    )
    return _decode_replay_outcome(payload, "V3")
