"""Native execution-profile capability manifest for custom hosts.

Only the supported profiles (``native-workflow``, ``native-conformal``, the
canonical V1 ``.n4a`` profile ``native-n4a-canonical`` and the owner-neutral
storage profile ``n4a-container``) are published. The data is generated from ``compat/capabilities.toml``.
"""

from __future__ import annotations

import json
from copy import deepcopy
from typing import Any

from ._capability_profiles import EXECUTION_PROFILES_JSON

RUNTIME_SURFACES: tuple[str, ...] = (
    "python",
    "javascript_wasm",
    "rust",
    "matlab_octave",
)

# Generated from compat/capabilities.toml by scripts/generate_capability_profiles.py.
_EXECUTION_PROFILES: dict[str, Any] = json.loads(EXECUTION_PROFILES_JSON)


def runtime_surfaces() -> tuple[str, ...]:
    """Return runtime surface ids that custom hosts may target."""

    return RUNTIME_SURFACES


def execution_profiles() -> dict[str, Any]:
    """Return the versioned native execution profiles and the external R federation record."""

    return deepcopy(_EXECUTION_PROFILES)


def capability_manifest() -> dict[str, Any]:
    """Return the serializable capability manifest for custom app hosts."""

    return {
        "schema": "nirs4all-core.capabilities.v1",
        "aggregate": "nirs4all-core",
        "runtime_surfaces": list(RUNTIME_SURFACES),
        "execution_profiles": execution_profiles(),
    }
