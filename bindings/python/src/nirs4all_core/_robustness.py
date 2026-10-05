"""Frozen native Gaussian perturbation audits with keyed regression truth."""
from __future__ import annotations

from pathlib import Path
from typing import Any
from uuid import uuid4

from ._conformal import CalibratedWorkflow, _library, _uncertainty_input
from ._workflow import Workflow, _invoke


def robustness(model: Workflow | CalibratedWorkflow | str | Path, x: Any, y: Any, *,
               sample_ids: Any = None, source_id: str | None = None, scenarios: Any = None,
               methods_library_path: str | Path | None = None,
               cli: str | Path | None = None) -> dict[str, Any]:
    """Audit a schema-checked Dataset or positional X with explicit identities.

    Native diagnostics qualify positional schema/physical-identity limits.
    """
    archive = model.archive if isinstance(model, (Workflow, CalibratedWorkflow)) else Path(model)
    values = y.tolist() if hasattr(y, "tolist") else list(y)
    if values and not isinstance(values[0], (list, tuple)):
        values = [[value] for value in values]
    return _invoke(cli, "robustness", {
        **_uncertainty_input(x, sample_ids, source_id),
        "truth": values, "scenarios": list(scenarios) if scenarios is not None else [
            {"id": "observed", "kind": "observed", "severity": 0.0, "seed": 0},
            {"id": "gaussian", "kind": "spectral_noise", "severity": 0.01, "seed": 1},
        ]}, archive=Path(archive).resolve(), methods_library=_library(methods_library_path),
        run_id="run:robustness:" + uuid4().hex)
