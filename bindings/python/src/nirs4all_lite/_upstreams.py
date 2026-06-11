"""Upstream registry and lazy import helpers for nirs4all-lite."""

from __future__ import annotations

from dataclasses import dataclass
import importlib
import importlib.util
from types import ModuleType
from typing import Mapping


@dataclass(frozen=True)
class Upstream:
    key: str
    candidates: tuple[str, ...]
    role: str


upstreams: Mapping[str, Upstream] = {
    "dag_ml": Upstream(
        key="dag_ml",
        candidates=("dag_ml",),
        role="Leakage-safe DAG/ML execution coordinator",
    ),
    "dag_ml_data": Upstream(
        key="dag_ml_data",
        candidates=("dag_ml_data",),
        role="Sample-aligned data contracts for DAG/ML runtimes",
    ),
    "formats": Upstream(
        key="formats",
        candidates=("nirs4all_formats", "nirs4all.formats"),
        role="Spectroscopy/NIRS vendor file readers",
    ),
    "io": Upstream(
        key="io",
        candidates=("nirs4all_io", "nirs4all.io"),
        role="Dataset assembly bridge",
    ),
    "datasets": Upstream(
        key="datasets",
        candidates=("nirs4all_datasets", "nirs4all.datasets"),
        role="DOI-pinned NIRS dataset catalog",
    ),
    "methods": Upstream(
        key="methods",
        candidates=("nirs4all_methods", "n4m", "nirs4all.methods"),
        role="Portable C ABI PLS/NIRS numerical engine",
    ),
}


def _find_candidate(candidate: str) -> bool:
    try:
        return importlib.util.find_spec(candidate) is not None
    except ModuleNotFoundError:
        return False


def import_upstream(name: str) -> ModuleType | None:
    """Import an upstream module if one of its known candidates is installed."""

    item = upstreams.get(name)
    if item is None:
        raise KeyError(f"Unknown nirs4all-lite upstream: {name}")

    for candidate in item.candidates:
        if _find_candidate(candidate):
            return importlib.import_module(candidate)
    return None


def require_upstream(name: str) -> ModuleType:
    """Import an upstream module or raise a clear error."""

    module = import_upstream(name)
    if module is not None:
        return module

    item = upstreams[name]
    candidates = ", ".join(item.candidates)
    raise ImportError(
        f"nirs4all-lite upstream '{name}' is not installed. "
        f"Tried import candidates: {candidates}."
    )


def available_upstreams() -> dict[str, bool]:
    """Return availability for every registered upstream."""

    return {name: import_upstream(name) is not None for name in upstreams}


def upstream_status() -> list[dict[str, object]]:
    """Return a serializable status table for diagnostics."""

    status = []
    for name, item in upstreams.items():
        available_candidate = next(
            (candidate for candidate in item.candidates if _find_candidate(candidate)),
            None,
        )
        status.append(
            {
                "key": name,
                "available": available_candidate is not None,
                "candidate": available_candidate,
                "role": item.role,
            }
        )
    return status


class LazyUpstream:
    """Proxy that resolves an upstream module on first attribute access."""

    def __init__(self, name: str) -> None:
        if name not in upstreams:
            raise KeyError(f"Unknown nirs4all-lite upstream: {name}")
        self.name = name

    def module(self) -> ModuleType:
        return require_upstream(self.name)

    def __getattr__(self, attribute: str) -> object:
        return getattr(self.module(), attribute)

    def __repr__(self) -> str:
        return f"LazyUpstream(name={self.name!r})"
