"""Portable runtime/controller capability manifest for custom hosts."""

from __future__ import annotations

from copy import deepcopy
from typing import Any

from ._pipeline import PORTABLE_OPERATOR_CLASSES

RUNTIME_SURFACES: tuple[str, ...] = (
    "python",
    "r",
    "javascript_wasm",
    "rust",
    "matlab_octave",
)

_PARITY_RUNTIME: dict[str, str] = {
    surface: "parity-validated" for surface in RUNTIME_SURFACES
}

_RUNTIME_CONTRACTS: tuple[dict[str, Any], ...] = (
    {
        "surface": "python",
        "pipeline_execution": "parity-validated",
        "pipeline_entrypoint": "run_portable_pipeline",
        "serialized_model_predict": False,
        "predict_entrypoint": None,
    },
    {
        "surface": "r",
        "pipeline_execution": "parity-validated",
        "pipeline_entrypoint": "nirs4all_run_portable_pipeline",
        "serialized_model_predict": False,
        "predict_entrypoint": None,
    },
    {
        "surface": "javascript_wasm",
        "pipeline_execution": "parity-validated",
        "pipeline_entrypoint": "runPortablePipeline",
        "serialized_model_predict": True,
        "predict_entrypoint": "predictPortablePipeline",
    },
    {
        "surface": "rust",
        "pipeline_execution": "parity-validated",
        "pipeline_entrypoint": "run_portable_pipeline_with_library",
        "serialized_model_predict": False,
        "predict_entrypoint": None,
    },
    {
        "surface": "matlab_octave",
        "pipeline_execution": "parity-validated",
        "pipeline_entrypoint": "runPortablePipeline",
        "serialized_model_predict": False,
        "predict_entrypoint": None,
    },
)

_PORTABLE_CONTROLLERS: tuple[dict[str, Any], ...] = (
    {
        "id": "split.kennard_stone",
        "kind": "splitter",
        "domain": "methods",
        "label": "Kennard-Stone split",
        "operator_classes": (
            "nirs4all.operators.splitters.KennardStoneSplitter",
            "nirs4all.operators.splitters.splitters.KennardStoneSplitter",
        ),
        "ports": {
            "inputs": ("X",),
            "outputs": ("train_indices", "test_indices"),
        },
        "parameters": ("test_size",),
        "runtime": _PARITY_RUNTIME,
        "execution_path": "portable_pipeline",
    },
    {
        "id": "preprocess.snv",
        "kind": "transform",
        "domain": "methods",
        "label": "Standard normal variate",
        "operator_classes": (
            "nirs4all.operators.transforms.SNV",
            "nirs4all.operators.transforms.StandardNormalVariate",
            "nirs4all.operators.transforms.scalers.StandardNormalVariate",
        ),
        "ports": {
            "inputs": ("X",),
            "outputs": ("X_transformed",),
        },
        "parameters": (),
        "runtime": _PARITY_RUNTIME,
        "execution_path": "portable_pipeline",
    },
    {
        "id": "preprocess.savgol",
        "kind": "transform",
        "domain": "methods",
        "label": "Savitzky-Golay",
        "operator_classes": (
            "nirs4all.operators.transforms.SavitzkyGolay",
            "nirs4all.operators.transforms.nirs.SavitzkyGolay",
        ),
        "ports": {
            "inputs": ("X",),
            "outputs": ("X_transformed",),
        },
        "parameters": ("window_length", "polyorder", "deriv", "mode", "cval"),
        "runtime": _PARITY_RUNTIME,
        "execution_path": "portable_pipeline",
    },
    {
        "id": "model.pls_regression",
        "kind": "model",
        "domain": "methods",
        "label": "PLS regression",
        "operator_classes": (
            "sklearn.cross_decomposition.PLSRegression",
            "sklearn.cross_decomposition._pls.PLSRegression",
        ),
        "ports": {
            "inputs": ("X", "y"),
            "outputs": ("predictions", "model"),
        },
        "parameters": ("n_components", "_range_"),
        "runtime": _PARITY_RUNTIME,
        "execution_path": "portable_pipeline",
    },
    {
        "id": "pipeline.portable_methods",
        "kind": "pipeline",
        "domain": "methods",
        "label": "Portable methods pipeline",
        "operator_classes": (),
        "ports": {
            "inputs": ("pipeline", "dataset"),
            "outputs": ("execution_result", "predictions", "model"),
        },
        "parameters": (),
        "runtime": _PARITY_RUNTIME,
        "execution_path": "run_portable_pipeline",
        "composes": (
            "split.kennard_stone",
            "preprocess.snv",
            "preprocess.savgol",
            "model.pls_regression",
        ),
    },
)


def runtime_surfaces() -> tuple[str, ...]:
    """Return runtime surface ids that custom hosts may target."""

    return RUNTIME_SURFACES


def runtime_contracts() -> tuple[dict[str, Any], ...]:
    """Return per-runtime execution and serialized-model prediction contracts."""

    return tuple(deepcopy(item) for item in _RUNTIME_CONTRACTS)


def controller_capabilities() -> tuple[dict[str, Any], ...]:
    """Return portable controller capability descriptors for host UIs."""

    return tuple(deepcopy(item) for item in _PORTABLE_CONTROLLERS)


def capability_manifest() -> dict[str, Any]:
    """Return the serializable V1 capability manifest for custom app hosts."""

    manifest = {
        "schema": "nirs4all-core.capabilities.v1",
        "aggregate": "nirs4all-core",
        "runtime_surfaces": RUNTIME_SURFACES,
        "runtime_contracts": _RUNTIME_CONTRACTS,
        "portable_operator_classes": tuple(sorted(PORTABLE_OPERATOR_CLASSES)),
        "controllers": _PORTABLE_CONTROLLERS,
    }
    return deepcopy(manifest)
