"""SDK SQLite/Parquet workspace bridge with native experiment closures."""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import sqlite3
import tempfile
import zipfile
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


class Workspace:
    """Closeable SDK-backed workspace and exact native experiment collection."""
    def __init__(self, path: Path, index: dict[str, Any], store: Any, experiments: dict[str, Experiment]):
        self.path, self.index, self._store, self._experiments = path, index, store, experiments
        self.closed = False
        self._sessions = []

    def _open(self) -> None:
        if self.closed:
            raise RuntimeError("workspace is closed")
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
    total = 0
    for member, digest in index["files"].items():
        _member(member)
        source = root / member
        if source.is_symlink() or not source.is_file() or not source.resolve().is_relative_to(root.resolve()) or _digest(source) != digest:
            raise ValueError("Workspace member integrity mismatch")
        total += source.stat().st_size
    if total > _MAX_BYTES or "store.sqlite" not in index["files"]:
        raise ValueError("Workspace inventory exceeds limits or lacks SDK store")
    connection = sqlite3.connect(f"{(root / 'store.sqlite').resolve().as_uri()}?mode=ro", uri=True)
    try:
        if connection.execute("PRAGMA user_version").fetchone()[0] != SCHEMA_VERSION or connection.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise ValueError("Invalid SDK SQLite workspace")
    finally:
        connection.close()
    experiments = {}
    store = WorkspaceStore(root, read_only=True)
    try:
        for run_id, record in index["runs"].items():
            _member(record["path"])
            experiment = open_experiment(root / record["path"])
            if experiment.index["run_id"] != run_id:
                raise ValueError("Workspace run identity differs from native experiment")
            sdk = store.get_run(record["sdk_run_id"])
            if sdk is None or sdk["status"] != "completed" or sdk["config"]["native_run_id"] != run_id:
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
                native = rows[position]
                if position in seen or metadata["native_run_id"] != run_id or metadata["sample_ids"] != native["sample_ids"]:
                    raise ValueError("SDK prediction identities differ from native experiment")
                for key in ("y_true", "y_pred", "y_proba", "sample_indices", "weights"):
                    actual = arrays.get(key)
                    flattened = [] if actual is None else actual.reshape(-1).tolist()
                    if flattened != native.get(key, []):
                        raise ValueError("SDK Parquet values differ from native experiment")
                if row["fold_id"] != native["fold_id"] or row["partition"] != native["partition"] or metadata["native_score_set"] != experiment.view["score_set"]:
                    raise ValueError("SDK fold, partition or metric provenance differs")
                seen.add(position)
            if seen != set(range(len(rows))):
                raise ValueError("SDK workspace prediction inventory differs from native experiment")
            experiments[run_id] = experiment
        return Workspace(root, index, store, experiments)
    except BaseException:
        store.close()
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
