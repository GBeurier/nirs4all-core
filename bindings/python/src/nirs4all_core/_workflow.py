"""Public dense workflows transported to the native Core CLI."""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from collections.abc import Mapping
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import Any
from uuid import uuid4

from ._dataset import dataset


def _command(cli: str | Path | None) -> str:
    value = str(cli or os.environ.get("NIRS4ALL_CORE_CLI", "nirs4all-core-archive"))
    if not Path(value).is_file() and shutil.which(value) is None:
        raise RuntimeError("Native workflows require the nirs4all-core-archive CLI; set NIRS4ALL_CORE_CLI")
    return value


def _invoke(cli: str | Path | None, operation: str, record: Any, **flags: Any) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="nirs4all-request-") as directory:
        request = Path(directory) / "input.json"
        output = Path(directory) / "output.json"
        args = [operation, "--output", str(output)]
        if record is not None:
            request.write_text(json.dumps(record, allow_nan=False), encoding="utf-8")
            args.extend(["--input", str(request)])
        for name, value in flags.items():
            if value is not None:
                args.extend(["--" + name.replace("_", "-"), str(value)])
        if cli is not None or os.environ.get("NIRS4ALL_CORE_CLI"):
            completed = subprocess.run([_command(cli), *args], capture_output=True, text=True, check=False)
            if completed.returncode:
                raise RuntimeError(completed.stderr.strip() or completed.stdout.strip() or "Native workflow failed")
        else:
            try:
                from ._native import execute_archive_command
            except ImportError:
                completed = subprocess.run([_command(None), *args], capture_output=True, text=True, check=False)
                if completed.returncode:
                    raise RuntimeError(completed.stderr.strip() or completed.stdout.strip() or "Native workflow failed")
            else:
                execute_archive_command(args)
        result = json.loads(output.read_text(encoding="utf-8"))
        if not isinstance(result, dict):
            raise TypeError("Native workflow returned a non-object result")
        return result


def _freeze(value: Any) -> Any:
    if isinstance(value, Mapping):
        return MappingProxyType({key: _freeze(child) for key, child in value.items()})
    if isinstance(value, (list, tuple)):
        return tuple(_freeze(child) for child in value)
    return value


def _thaw(value: Any) -> Any:
    if isinstance(value, Mapping):
        return {key: _thaw(child) for key, child in value.items()}
    if isinstance(value, tuple):
        return [_thaw(child) for child in value]
    return value


@dataclass(frozen=True, init=False)
class Workflow:
    """Native outcome plus the portable Archive V2 and reusable user choices."""
    archive: Path
    outcome: dict[str, Any]
    _config: Mapping[str, Any]
    _training_outcome_fingerprint: str

    def __init__(self, archive: Path, outcome: dict[str, Any], config: dict[str, Any]):
        object.__setattr__(self, "archive", Path(archive))
        object.__setattr__(self, "outcome", deepcopy(outcome))
        object.__setattr__(self, "_config", _freeze(deepcopy(config)))
        object.__setattr__(self, "_training_outcome_fingerprint", outcome["training_outcome"]["outcome_fingerprint"])

    @property
    def config(self) -> dict[str, Any]:
        """Detached user choices; mutations cannot alter the saved recipe."""
        return _thaw(self._config)

    def predict(self, x: Any, **kwargs: Any) -> dict[str, Any]:
        return predict(self, x, **kwargs)

    def retrain(self, data: Any, **kwargs: Any) -> Workflow:
        return retrain(self, data, **kwargs)

    def export(self, directory: str | Path) -> Path:
        return export(self, directory)


def run(data: Any, *, source_id: str = "spectra", components: Any = (1, 2),
        preprocessing: str = "snv_savgol", methods_library_path: str | Path | None = None,
        archive: str | Path | None = None, run_id: str | None = None,
        cli: str | Path | None = None, results_directory: str | Path | None = None) -> Workflow:
    """Compare PLS candidates by native two-fold OOF RMSE, then refit the winner."""
    counts = list(components)
    if len(counts) < 2 or len(counts) > 32 or len(set(counts)) != len(counts) or any(
        type(count) is not int or count < 1 or count > 2147483647 for count in counts
    ):
        raise ValueError("components must contain 2 to 32 distinct positive i32 integers")
    if preprocessing != "snv_savgol":
        raise ValueError("This native workflow supports preprocessing='snv_savgol'")
    library = methods_library_path or os.environ.get("N4M_LIBRARY_PATH") or os.environ.get("N4M_LIB_PATH")
    if library is None:
        raise ValueError("methods_library_path or N4M_LIBRARY_PATH is required")
    archive_path = Path(archive).resolve() if archive is not None else Path(tempfile.mkdtemp(prefix="nirs4all-run-")) / "model.n4a"
    config = {"source_id": source_id, "components": counts, "preprocessing": preprocessing}
    outcome = _invoke(cli, "workflow-run", dataset(data).to_dict(), **config,
                      methods_library=str(Path(library).resolve()), archive=archive_path,
                      run_id=run_id or "run:" + uuid4().hex,
                      results_directory=results_directory or Path(str(archive_path) + ".results"))
    return Workflow(archive_path, outcome, config)


def predict(model: Workflow | str | Path, x: Any, *, sample_ids: Any = None,
            methods_library_path: str | Path | None = None, cli: str | Path | None = None) -> dict[str, Any]:
    """Predict from an Archive V2 through fresh native replay without fitting."""
    archive = model.archive if isinstance(model, Workflow) else Path(model)
    matrix = x.tolist() if hasattr(x, "tolist") else x
    record = {"x": matrix, "sample_ids": list(sample_ids) if sample_ids is not None else [f"sample.{i}" for i in range(len(matrix))]}
    library = methods_library_path or os.environ.get("N4M_LIBRARY_PATH") or os.environ.get("N4M_LIB_PATH")
    if library is None:
        raise ValueError("methods_library_path or N4M_LIBRARY_PATH is required")
    return _invoke(cli, "workflow-predict", record, archive=Path(archive).resolve(),
                   methods_library=str(Path(library).resolve()), run_id="run:predict:" + uuid4().hex)


def retrain(model: Workflow, data: Any, **kwargs: Any) -> Workflow:
    """Start a fresh native campaign using the saved user choices."""
    if not isinstance(model, Workflow):
        raise TypeError("retrain requires a Workflow with saved configuration")
    return run(data, **(model.config | kwargs))


def export(model: Workflow, directory: str | Path) -> Path:
    """Export the native archive with portable, path-free recipe metadata."""
    target = Path(directory)
    target.mkdir(parents=True, exist_ok=False)
    reservation = target.stat()
    owned: list[tuple[Path, os.stat_result]] = []
    try:
        archive = target / "model.n4a"
        with model.archive.open("rb") as source, archive.open("xb") as output:
            owned.append((archive, os.fstat(output.fileno())))
            shutil.copyfileobj(source, output)
        metadata = target / "workflow.json"
        with metadata.open("x", encoding="utf-8") as output:
            owned.append((metadata, os.fstat(output.fileno())))
            json.dump({"schema": "nirs4all.workflow.v1", "config": model.config,
                       "training_outcome_fingerprint": model._training_outcome_fingerprint}, output, allow_nan=False)
    except BaseException:
        try:
            current = target.stat()
            if (current.st_dev, current.st_ino) == (reservation.st_dev, reservation.st_ino):
                for path, identity in owned:
                    try:
                        current_file = path.lstat()
                        if (current_file.st_dev, current_file.st_ino) == (identity.st_dev, identity.st_ino):
                            path.unlink()
                    except OSError:
                        continue
                target.rmdir()
        except OSError:
            pass
        raise
    return target


def load(directory: str | Path, *, cli: str | Path | None = None) -> Workflow:
    """Reopen a native-validated archive and bind saved configuration/outcome."""
    path = Path(directory).resolve()
    record = json.loads((path / "workflow.json").read_text(encoding="utf-8"))
    if set(record) != {"schema", "training_outcome_fingerprint", "config"} or record["schema"] != "nirs4all.workflow.v1" or not (path / "model.n4a").is_file():
        raise ValueError("Invalid workflow export")
    native = _invoke(cli, "workflow-load", None, archive=path / "model.n4a")
    if native["training_outcome"]["outcome_fingerprint"] != record["training_outcome_fingerprint"] or native["config"] != record["config"]:
        raise ValueError("Workflow metadata differs from its native archive")
    return Workflow(path / "model.n4a", native, native["config"])
