"""Native trial budgets, checkpoint resumption and variant generation."""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import tempfile
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import uuid4

from ._dataset import dataset
from ._workflow import _invoke


def generate(choices: dict[str, list[Any]], *, strategy: str = "cartesian", constraints: dict[str, Any] | None = None,
             count: int | None = None, seed: int = 0, max_variants: int = 10000, cli: str | Path | None = None) -> list[dict[str, Any]]:
    """Enumerate or sample native variants, preserving array-valued choices."""
    record = {"choices": choices, "strategy": strategy, "constraints": constraints or {}, "count": count,
              "seed": seed, "max_variants": max_variants}
    return _invoke(cli, "generate", record)["variants"]


def tune(data: Any, *, trials: int = 8, seed: int = 91, sampler: str = "random", metric: str = "rmse",
         source_id: str = "spectra", checkpoint: TuningResult | str | Path | None = None,
         methods_library_path: str | Path | None = None, archive: str | Path | None = None,
         run_id: str | None = None, cli: str | Path | None = None) -> TuningResult:
    """Run native OOF HPO on PLS components 1..3 and scaling, then refit the winner.

    `trials` is the total study history budget. Resuming from an Archive V2
    retains completed trial evidence and the native N4MOPT optimizer state.
    """
    if type(trials) is not int or trials < 1 or trials > 256:
        raise ValueError("trials must be a total budget between 1 and 256")
    if type(seed) is not int or seed < 0 or seed > 2**64 - 1:
        raise ValueError("seed must be an unsigned 64-bit integer")
    library = methods_library_path or os.environ.get("N4M_LIBRARY_PATH") or os.environ.get("N4M_LIB_PATH")
    if library is None:
        raise ValueError("methods_library_path or N4M_LIBRARY_PATH is required")
    target = Path(archive).resolve() if archive is not None else Path(tempfile.mkdtemp(prefix="nirs4all-tuning-")) / "model.n4a"
    previous = checkpoint.archive if isinstance(checkpoint, TuningResult) else checkpoint
    config = {"source_id": source_id, "trials": trials, "seed": seed, "sampler": sampler, "metric": metric}
    outcome = _invoke(cli, "tuning-run", dataset(data).to_dict(), **config,
                      methods_library=Path(library).resolve(), archive=target,
                      checkpoint_archive=Path(previous).resolve() if previous is not None else None,
                      run_id=run_id or "run:" + uuid4().hex)
    return TuningResult(target, outcome, config)


def resume_tuning(checkpoint: TuningResult | str | Path, data: Any, **kwargs: Any) -> TuningResult:
    """Continue a native study from its verified Archive V2 checkpoint."""
    return tune(data, checkpoint=checkpoint, **kwargs)

@dataclass(frozen=True)
class TuningResult:
    """A native selected model with its durable optimizer/trial evidence."""
    archive: Path
    outcome: dict[str, Any]
    config: dict[str, Any]

    def predict(self, x: Any, **kwargs: Any) -> dict[str, Any]:
        from ._workflow import predict
        return predict(self.archive, x, **kwargs)

    def resume(self, data: Any, *, trials: int, **kwargs: Any) -> TuningResult:
        return tune(data, **(self.config | kwargs | {"trials": trials, "checkpoint": self.archive}))

    def retrain(self, data: Any, **kwargs: Any) -> TuningResult:
        return tune(data, **(self.config | kwargs))

    def trials(self) -> list[dict[str, Any]]:
        return deepcopy(self.outcome["training_outcome"]["methods_hpo_resume_state"]["terminal_trials"])

    def summary(self) -> dict[str, Any]:
        native = self.outcome["training_outcome"]
        state = native["methods_hpo_resume_state"]
        return {"run_id": native["run_id"], "winner_variant_id": native["selected_variant_id"],
                "trial_history_len": state["trial_history_len"], "total_budget": self.config["trials"],
                "metric": self.config["metric"], "sampler": self.config["sampler"],
                "trial_ids": [terminal["trial"]["id"] for terminal in state["terminal_trials"]]}

    def export(self, directory: str | Path) -> Path:
        target = Path(directory)
        # Stage all members before reserving the caller's destination. The
        # native publisher refuses existing directories without replacing them.
        target.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".nirs4all-tuning-", dir=target.parent) as staging:
            stage = Path(staging) / "export"
            stage.mkdir()
            shutil.copyfile(self.archive, stage / "model.n4a")
            record = {"schema": "nirs4all.tuning.v1", "config": self.config,
                      "training_outcome_fingerprint": self.outcome["training_outcome"]["outcome_fingerprint"],
                      "archive_sha256": hashlib.sha256((stage / "model.n4a").read_bytes()).hexdigest()}
            (stage / "tuning.json").write_text(json.dumps(record, sort_keys=True, allow_nan=False), encoding="utf-8")
            try:
                from ._native import publish_directory_noreplace
            except ImportError:
                _invoke(None, "publish-directory", None, input=stage, destination=target)
            else:
                publish_directory_noreplace(str(stage), str(target))
        return target


def load_tuning(directory: str | Path, *, cli: str | Path | None = None) -> TuningResult:
    """Reload a native-validated model, checkpoint and its signed user options."""
    path = Path(directory).resolve()
    record = json.loads((path / "tuning.json").read_text(encoding="utf-8"))
    if set(record) != {"schema", "config", "training_outcome_fingerprint", "archive_sha256"} or record["schema"] != "nirs4all.tuning.v1":
        raise ValueError("Invalid tuning export")
    archive = path / "model.n4a"
    if hashlib.sha256(archive.read_bytes()).hexdigest() != record["archive_sha256"]:
        raise ValueError("Tuning archive hash mismatch")
    native = _invoke(cli, "tuning-load", None, archive=archive)
    if native["training_outcome"]["outcome_fingerprint"] != record["training_outcome_fingerprint"] or native["config"] != record["config"]:
        raise ValueError("Tuning metadata disagrees with native archive")
    return TuningResult(archive, native, deepcopy(native["config"]))
