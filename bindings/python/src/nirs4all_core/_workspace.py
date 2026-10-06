"""SDK SQLite/Parquet workspace bridge with native experiment closures."""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import sqlite3
import tempfile
import zipfile
from copy import deepcopy
from pathlib import Path
from typing import Any, Self

from ._results import Experiment, open_experiment

_SCHEMA = "nirs4all.workspace.v1"
_MAX_BYTES = 1024 * 1024 * 1024


def _digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def _member(name: str) -> None:
    if not isinstance(name, str) or "\\" in name or not name or name.startswith("/") or ":" in name or any(part in ("", ".", "..") for part in name.split("/")):
        raise ValueError("Invalid workspace member path")


def _inventory(root: Path, index: dict[str, Any]) -> None:
    expected = {"workspace.json", *index["files"]}
    actual = set()
    for source in root.rglob("*"):
        if source.is_symlink():
            raise ValueError("Workspace inventory contains a symlink")
        if source.is_file():
            name = source.relative_to(root).as_posix()
            if source.name.endswith(("-wal", "-shm")):
                raise ValueError("Workspace inventory contains an active SQLite journal")
            actual.add(name)
    if actual != expected:
        raise ValueError("Workspace inventory contains missing or unlisted members")


class Workspace:
    """Closeable SDK-backed workspace and exact native experiment collection."""
    def __init__(self, path: Path, index: dict[str, Any], store: Any, experiments: dict[str, Experiment]):
        self.path, self._index, self._store, self._experiments = path, deepcopy(index), store, experiments
        self.closed = False
        self._sessions = []
        self._snapshot = None
        self._index_digest = _digest(path / "workspace.json")

    @property
    def index(self) -> dict[str, Any]:
        return deepcopy(self._index)

    def _open(self) -> None:
        if self.closed:
            raise RuntimeError("workspace is closed")
        if _digest(self.path / "workspace.json") != self._index_digest:
            raise ValueError("Workspace index changed after opening")
        _inventory(self.path, self.index)
        for member, digest in self.index["files"].items():
            source = self.path / member
            if source.is_symlink() or not source.is_file() or _digest(source) != digest:
                raise ValueError("Workspace changed after opening")

    def __enter__(self) -> Self:
        self._open()
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def close(self) -> None:
        if not self.closed:
            for session in self._sessions:
                session.close()
            self._store.close()
            if self._snapshot is not None:
                shutil.rmtree(self._snapshot, ignore_errors=True)
            self.closed = True

    def runs(self) -> list[dict[str, Any]]:
        self._open()
        return [{"native_run_id": run, "sdk_run_id": record["sdk_run_id"], **self._experiments[run].summary()} for run, record in self.index["runs"].items()]

    def experiment(self, run_id: str) -> Experiment:
        self._open()
        return self._experiments[run_id]

    def query_predictions(self, run_id: str) -> list[dict[str, Any]]:
        """Query the actual SDK relational store and load its Parquet sidecars."""
        self._open()
        sdk_run = self.index["runs"][run_id]["sdk_run_id"]
        frame = self._store.query_predictions(run_id=sdk_run)
        output = []
        for row in frame.to_dicts():
            arrays = self._store.array_store.load_single(row["prediction_id"], row["dataset_name"])
            output.append({**row, **(arrays or {})})
        return output

    def session(self, run_id: str, *, engine: str = "sdk", **options: Any) -> Any:
        """Load the native archive as a closeable SDK session or Core workflow."""
        archive = self.experiment(run_id).model_archive
        if archive is None:
            raise ValueError("This workspace run has no portable predictor")
        if engine == "sdk":
            from nirs4all import load_session
            session = load_session(archive, engine="native", **options)
            self._sessions.append(session)
            return session
        if engine == "core":
            from ._workflow import Workflow, _invoke
            native = _invoke(options.pop("cli", None), "workflow-load", None, archive=archive)
            if options:
                raise ValueError("Unsupported Core session options")
            return Workflow(archive, native, native["config"])
        raise ValueError("session engine must be sdk or core")

    def export(self, destination: str | Path) -> Path:
        """Publish an immutable archive without replacing an existing file."""
        self._open()
        target = Path(destination)
        target.parent.mkdir(parents=True, exist_ok=True)
        descriptor, name = tempfile.mkstemp(prefix=".workspace-", dir=target.parent)
        os.close(descriptor)
        scratch = Path(name)
        try:
            with zipfile.ZipFile(scratch, "w", compression=zipfile.ZIP_STORED) as archive:
                archive.write(self.path / "workspace.json", "workspace.json")
                for member, digest in self.index["files"].items():
                    source = self.path / member
                    if source.is_symlink() or _digest(source) != digest:
                        raise ValueError("Workspace changed during export")
                    archive.write(source, member)
            os.link(scratch, target)
            return target
        finally:
            scratch.unlink(missing_ok=True)


def save_workspace(experiments: list[Experiment | str | Path], destination: str | Path) -> Workspace:
    """Create an SDK-native workspace from semantically validated DAG experiments."""
    from dag_ml.sdk_workspace import publish_experiment_to_sdk
    from nirs4all.pipeline.storage.store_schema import SCHEMA_VERSION
    from nirs4all.pipeline.storage.workspace_store import WorkspaceStore

    target = Path(destination)
    if target.exists() or target.is_symlink():
        raise FileExistsError(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix=".workspace-", dir=target.parent))
    runs = {}
    try:
        with WorkspaceStore(scratch) as store:
            for value in experiments:
                experiment = open_experiment(value.path if isinstance(value, Experiment) else value)
                run_id = experiment.index["run_id"]
                if run_id in runs:
                    raise ValueError("Duplicate native workspace run")
                folder = "experiments/" + hashlib.sha256(run_id.encode()).hexdigest()
                shutil.copytree(experiment.path, scratch / folder)
                sdk_run = publish_experiment_to_sdk(store, experiment.view, native_run_id=run_id,
                                                   experiment_path=folder, winner_variant_id=experiment.winner_variant_id)
                runs[run_id] = {"path": folder, "sdk_run_id": sdk_run}
        for lock in scratch.rglob("*.lock"):
            lock.unlink()
        files = {path.relative_to(scratch).as_posix(): _digest(path) for path in scratch.rglob("*") if path.is_file() and not path.name.endswith((".lock", "-wal", "-shm"))}
        index = {"schema": _SCHEMA, "sdk_schema_version": SCHEMA_VERSION, "runs": runs, "files": files}
        (scratch / "workspace.json").write_text(json.dumps(index, sort_keys=True, separators=(",", ":")))
        opened = open_workspace(scratch)
        opened.close()
        # Existing directories cannot be silently replaced even under a race.
        target.mkdir()
        try:
            for path in sorted(scratch.iterdir(), key=lambda path: path.name == "workspace.json"):
                path.rename(target / path.name)
        except BaseException:
            shutil.rmtree(target)
            raise
        return open_workspace(target)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)


def open_workspace(path: str | Path) -> Workspace:
    """Verify native closures and SQLite/Parquet consistency before a read-only open."""
    from nirs4all.pipeline.storage.store_schema import SCHEMA_VERSION
    from nirs4all.pipeline.storage.workspace_store import WorkspaceStore

    root = Path(path)
    if (root / "workspace.json").is_symlink():
        raise ValueError("Workspace index must be regular")
    index = json.loads((root / "workspace.json").read_text())
    if set(index) != {"schema", "sdk_schema_version", "runs", "files"} or index["schema"] != _SCHEMA:
        raise ValueError("Unsupported workspace schema")
    if index["sdk_schema_version"] != SCHEMA_VERSION:
        raise ValueError("SDK workspace schema migration required before portable export")
    _inventory(root, index)
    total = 0
    for member, digest in index["files"].items():
        _member(member)
        source = root / member
        if source.is_symlink() or not source.is_file() or not source.resolve().is_relative_to(root.resolve()) or _digest(source) != digest:
            raise ValueError("Workspace member integrity mismatch")
        total += source.stat().st_size
    if total > _MAX_BYTES or "store.sqlite" not in index["files"]:
        raise ValueError("Workspace inventory exceeds limits or lacks SDK store")
    connection = sqlite3.connect(f"{(root / 'store.sqlite').resolve().as_uri()}?mode=ro&immutable=1", uri=True)
    try:
        if connection.execute("PRAGMA user_version").fetchone()[0] != SCHEMA_VERSION or connection.execute("PRAGMA integrity_check").fetchone()[0] != "ok" or connection.execute("PRAGMA foreign_key_check").fetchall():
            raise ValueError("Invalid SDK SQLite workspace")
    finally:
        connection.close()
    # SDK opens a private closed snapshot: a concurrent source writer cannot
    # introduce a WAL or change SQLite/Parquet after our integrity checks.
    snapshot = Path(tempfile.mkdtemp(prefix="nirs4all-workspace-read-"))
    try:
        for member in ("workspace.json", *index["files"]):
            target = snapshot / member
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / member, target)
        _inventory(root, index)
        for member, digest in index["files"].items():
            if _digest(snapshot / member) != digest:
                raise ValueError("Workspace member integrity changed during snapshot")
    except BaseException:
        shutil.rmtree(snapshot, ignore_errors=True)
        raise
    experiments = {}
    all_predictions, all_pipelines, all_chains = set(), set(), set()
    store = WorkspaceStore(snapshot, read_only=True)
    try:
        for run_id, record in index["runs"].items():
            _member(record["path"])
            experiment = open_experiment(snapshot / record["path"])
            if experiment.index["run_id"] != run_id:
                raise ValueError("Workspace run identity differs from native experiment")
            sdk = store.get_run(record["sdk_run_id"])
            expected_config = {"engine": "dag-ml", "native_run_id": run_id, "native_experiment": record["path"]}
            expected_summary = {"native_run_id": run_id, "native_experiment": record["path"],
                                "winner_variant_id": experiment.winner_variant_id, "native_score_set": experiment.view["score_set"]}
            if (sdk is None or sdk["status"] != "completed" or sdk["name"] != run_id
                    or sdk["config"] != expected_config or sdk["summary"] != expected_summary
                    or sdk["datasets"] != [] or sdk["error"] is not None):
                raise ValueError("SDK run metadata differs from native experiment")
            frame = store.query_predictions(run_id=record["sdk_run_id"])
            rows = experiment.predictions()
            seen = set()
            for row in frame.to_dicts():
                arrays = store.array_store.load_single(row["prediction_id"], row["dataset_name"])
                if arrays is None:
                    raise ValueError("SDK prediction lacks Parquet arrays")
                metadata = arrays["result_metadata"]
                position = metadata["native_row_index"]
                if not isinstance(position, int) or isinstance(position, bool) or not 0 <= position < len(rows):
                    raise ValueError("SDK native row index is invalid")
                native = rows[position]
                expected_fields = {
                    "dataset_name": native["dataset"], "model_name": native["model_name"],
                    "model_class": native["model_name"], "metric": native["metric"],
                    "task_type": native["task_type"], "refit_context": native["refit_context"],
                    "val_score": native.get("val_score"), "test_score": native.get("test_score"),
                    "train_score": native.get("train_score"), "n_samples": len(native["sample_indices"]),
                    "n_features": 0, "best_params": "{}", "preprocessings": "", "branch_id": None,
                    "branch_name": None, "exclusion_count": 0, "exclusion_rate": 0.0,
                }
                for key in ("prediction_scope", "prediction_level", "evaluation_scope", "reduction_role", "reduction_id",
                            "physical_sample_id", "origin_sample_id", "derived_unit_id", "unit_level", "unit_id", "row_id", "sample_influence_weight"):
                    expected_fields[key] = None
                if any(row[key] != expected for key, expected in expected_fields.items()) or json.loads(row["scores"]) != native.get("scores", {}):
                    raise ValueError("SDK prediction metadata differs from native experiment")
                pipeline = store.get_pipeline(row["pipeline_id"])
                chain = store.get_chain(row["chain_id"])
                if (pipeline is None or pipeline["run_id"] != record["sdk_run_id"]
                        or pipeline["name"] != native["variant_id"] or pipeline["status"] != "completed"
                        or pipeline["expanded_config"] != {"native_variant_id": native["variant_id"]}
                        or pipeline["dataset_name"] != native["dataset"]
                        or chain is None or chain["pipeline_id"] != row["pipeline_id"]
                        or chain["model_class"] != native["model_name"]):
                    raise ValueError("SDK pipeline or model provenance differs from native experiment")
                expected_pipeline = {"original_template": None, "generator_choices": [], "dataset_hash": "",
                                     "best_val": None, "best_test": None,
                                     "metric": experiment.view["score_set"].get("selection_metric", ""), "duration_ms": 0, "error": None}
                expected_chain = {"steps": [], "model_step_idx": -1, "preprocessings": "", "fold_strategy": "native_results",
                                  "fold_artifacts": {}, "shared_artifacts": {}, "dataset_name": native["dataset"], "cv_fold_count": 0}
                for key in ("branch_path", "source_index", "model_name", "metric", "task_type", "best_params", "cv_val_score", "cv_test_score",
                            "cv_train_score", "cv_scores", "final_test_score", "final_train_score", "final_scores", "final_agg_test_score",
                            "final_agg_train_score", "final_agg_scores", "relation_replay_manifest", "relation_replay_version", "relation_replay_fingerprint"):
                    expected_chain[key] = None
                if (any(pipeline[key] != expected for key, expected in expected_pipeline.items())
                        or any(chain[key] != expected for key, expected in expected_chain.items())):
                    raise ValueError("SDK pipeline or chain metadata differs from native projection")
                all_predictions.add(row["prediction_id"])
                all_pipelines.add(row["pipeline_id"])
                all_chains.add(row["chain_id"])
                expected_metadata = {"schema": "dagml.sdk-workspace-row.v1", "native_run_id": run_id,
                                     "native_row_index": position, "variant_id": native["variant_id"],
                                     "sample_ids": native["sample_ids"], "target_names": native.get("target_names", []),
                                     "native_score_set": experiment.view["score_set"],
                                     "shapes": {key: native[key + "_shape"] for key in ("y_true", "y_pred", "y_proba")}}
                if metadata != expected_metadata:
                    raise ValueError("SDK Parquet shape or result provenance differs from native experiment")
                if position in seen or metadata["native_run_id"] != run_id or metadata["sample_ids"] != native["sample_ids"]:
                    raise ValueError("SDK prediction identities differ from native experiment")
                for key in ("y_true", "y_pred", "y_proba", "sample_indices", "weights"):
                    actual = arrays.get(key)
                    flattened = [] if actual is None else actual.reshape(-1).tolist()
                    if key in ("y_true", "y_pred", "y_proba") and actual is not None and list(actual.shape) != (native[key + "_shape"][:1] if len(native[key + "_shape"]) == 2 and native[key + "_shape"][1] == 1 else native[key + "_shape"]):
                        raise ValueError("SDK Parquet array shape differs from native experiment")
                    if flattened != native.get(key, []):
                        raise ValueError("SDK Parquet values differ from native experiment")
                if row["fold_id"] != native["fold_id"] or row["partition"] != native["partition"] or metadata["native_score_set"] != experiment.view["score_set"]:
                    raise ValueError("SDK fold, partition or metric provenance differs")
                seen.add(position)
            if seen != set(range(len(rows))):
                raise ValueError("SDK workspace prediction inventory differs from native experiment")
            experiments[run_id] = experiment
        if {run["run_id"] for run in store.list_runs().to_dicts()} != {entry["sdk_run_id"] for entry in index["runs"].values()}:
            raise ValueError("SDK run inventory differs from native experiment inventory")
        if ({row["prediction_id"] for row in store.query_predictions().to_dicts()} != all_predictions
                or {row["pipeline_id"] for row in store.list_pipelines().to_dicts()} != all_pipelines
                or {row["chain_id"] for pipeline in all_pipelines for row in store.get_chains_for_pipeline(pipeline).to_dicts()} != all_chains):
            raise ValueError("SDK relational inventory differs from native projection")
        if json.loads((snapshot / "workspace.json").read_text()) != index:
            raise ValueError("Workspace index changed during snapshot")
        workspace = Workspace(root, index, store, experiments)
        workspace._snapshot = snapshot
        return workspace
    except BaseException:
        store.close()
        shutil.rmtree(snapshot, ignore_errors=True)
        raise


def import_workspace(archive_path: str | Path, destination: str | Path) -> Workspace:
    """Import a bounded immutable snapshot; invalid imports publish no directory."""
    target = Path(destination)
    if target.exists() or target.is_symlink():
        raise FileExistsError(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix=".workspace-import-", dir=target.parent))
    try:
        with zipfile.ZipFile(archive_path) as archive:
            entries = archive.infolist()
            names = [entry.filename for entry in entries]
            if len(names) > 4096 or len(names) != len(set(names)) or sum(entry.file_size for entry in entries) > _MAX_BYTES:
                raise ValueError("Workspace archive inventory exceeds limits or repeats members")
            for entry in entries:
                _member(entry.filename)
                if entry.is_dir() or (entry.external_attr >> 16) & 0o170000 == 0o120000:
                    raise ValueError("Workspace archive only admits regular members")
                member = scratch / entry.filename
                member.parent.mkdir(parents=True, exist_ok=True)
                with member.open("xb") as stream, archive.open(entry) as source:
                    shutil.copyfileobj(source, stream)
        workspace = open_workspace(scratch)
        if set(names) != {"workspace.json", *workspace.index["files"]}:
            workspace.close()
            raise ValueError("Workspace archive includes unlisted members")
        workspace.close()
        target.mkdir()
        try:
            for path in sorted(scratch.iterdir(), key=lambda path: path.name == "workspace.json"):
                path.rename(target / path.name)
        except BaseException:
            shutil.rmtree(target)
            raise
        return open_workspace(target)
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
