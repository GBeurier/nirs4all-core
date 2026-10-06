"""Actual SDK SQLite/Parquet and portable native session round trips."""
import json
import os
import zipfile
from pathlib import Path
from unittest.mock import patch

import numpy as np
import pytest
from nirs4all_core._browser_tuning import load_browser_tuning
from nirs4all_core._workspace import import_workspace, open_workspace, save_workspace

FIXTURE = os.environ.get("NIRS4ALL_EXPERIMENT_FIXTURE")
BROWSER = os.environ.get("NIRS4ALL_BROWSER_TUNING_FIXTURE")


@pytest.mark.skipif(not FIXTURE, reason="native model experiment qualification fixture required")
def test_sdk_store_snapshot_lifecycle_and_native_session(tmp_path):
    workspace = save_workspace([FIXTURE], tmp_path / "workspace")
    run = workspace.runs()[0]["native_run_id"]
    original = workspace.experiment(run).predictions()
    rows = workspace.query_predictions(run)
    assert len(rows) == len(original)
    assert all(row["result_metadata"]["sample_ids"] == original[row["result_metadata"]["native_row_index"]]["sample_ids"] for row in rows)
    assert (workspace.path / "store.sqlite").is_file()
    assert list((workspace.path / "arrays").glob("*.parquet"))
    assert workspace.session(run, engine="core").outcome["training_outcome"]["run_id"] == run
    session = workspace.session(run)
    input_path = Path(FIXTURE).parent / "independent.json"
    cohort = json.loads(input_path.read_text())
    predicted = session.predict(cohort["x"], sample_ids=cohort["sample_ids"])
    assert predicted.y_pred.shape[0] == len(cohort["sample_ids"])
    assert not session.closed
    archive = workspace.export(tmp_path / "workspace.n4w")
    with pytest.raises(FileExistsError):
        workspace.export(archive)
    workspace.close()
    assert session.closed
    with pytest.raises(RuntimeError, match="closed"):
        workspace.query_predictions(run)
    imported = import_workspace(archive, tmp_path / "imported")
    assert imported.experiment(run).predictions() == original
    assert len(imported.query_predictions(run)) == len(rows)
    imported.close()
    with (tmp_path / "imported" / "store.sqlite").open("ab") as stream:
        stream.write(b"tamper")
    with pytest.raises(ValueError, match="integrity"):
        open_workspace(tmp_path / "imported")
    if os.environ.get("NIRS4ALL_WORKSPACE_OUTPUT"):
        import shutil
        shutil.copytree(tmp_path / "workspace", os.environ["NIRS4ALL_WORKSPACE_OUTPUT"])


@pytest.mark.skipif(not FIXTURE, reason="native model experiment qualification fixture required")
def test_workspace_transaction_rejects_duplicate_and_path_escape(tmp_path):
    with pytest.raises(ValueError, match="Duplicate"):
        save_workspace([FIXTURE, FIXTURE], tmp_path / "workspace")
    assert not (tmp_path / "workspace").exists()
    bad = tmp_path / "bad.n4w"
    with zipfile.ZipFile(bad, "w") as archive:
        archive.writestr("../escaped", "refused")
    with pytest.raises(ValueError, match="member path"):
        import_workspace(bad, tmp_path / "invalid")
    assert not (tmp_path / "invalid").exists()
    assert not (tmp_path / "escaped").exists()


@pytest.mark.skipif(not BROWSER, reason="real browser-produced package required")
def test_browser_native_state_cold_cpu_replay_and_identity_tamper(tmp_path):
    from n4m.roles import RolePipeline
    source = json.loads(Path(BROWSER).read_text())
    with patch.object(RolePipeline, "fit", side_effect=AssertionError("Cold replay FIT forbidden")):
        model = load_browser_tuning(source["record"])
        result = model.predict(source["predictionData"])
        actual = result["node_results"][0]["predictions"][0]
        expected = source["result"]["node_results"][0]["predictions"][0]
        np.testing.assert_allclose(actual["values"], expected["values"], atol=1e-12, rtol=0)
        assert actual["sample_ids"] == expected["sample_ids"]
        saved = model.export(tmp_path / "browser.json")
        assert load_browser_tuning(saved).predict(source["predictionData"])["node_results"][0]["predictions"][0] == actual
        wrong = json.loads(json.dumps(source["predictionData"]))
        wrong["dataset"]["sources"][0]["axis_units"] = {"wavelength": "nm"}
        with pytest.raises(ValueError, match="schema"):
            model.predict(wrong)
    bad = json.loads(json.dumps(source["record"])); bad["packageJson"] += " "
    with pytest.raises(ValueError, match="hash mismatch"):
        load_browser_tuning(bad)
    bad = json.loads(json.dumps(source["record"])); bad["config"]["metric"] = "mae"
    with pytest.raises(ValueError, match="study options"):
        load_browser_tuning(bad)

@pytest.mark.skipif(not FIXTURE, reason="native model experiment qualification fixture required")
@pytest.mark.parametrize("update", ["metric='fake_metric'", "val_score=-999", "model_name='foreign-model'", "task_type='classification'", "refit_context='foreign'", "n_samples=1"])
def test_workspace_rejects_sql_projection_forgery_with_rehashed_file(tmp_path, update):
    import hashlib
    import sqlite3
    root = tmp_path / "workspace"
    save_workspace([FIXTURE], root).close()
    with sqlite3.connect(root / "store.sqlite") as writer:
        writer.execute("UPDATE predictions SET " + update)
    writer.close()
    index_path = root / "workspace.json"
    index = json.loads(index_path.read_text())
    index["files"]["store.sqlite"] = hashlib.sha256((root / "store.sqlite").read_bytes()).hexdigest()
    index_path.write_text(json.dumps(index))
    with pytest.raises(ValueError, match="metadata|provenance"):
        open_workspace(root)


@pytest.mark.skipif(not FIXTURE, reason="native model experiment qualification fixture required")
def test_workspace_rejects_unlisted_live_wal_and_changed_export_index(tmp_path):
    import sqlite3
    root = tmp_path / "workspace"
    workspace = save_workspace([FIXTURE], root)
    with sqlite3.connect(root / "store.sqlite") as writer:
        writer.execute("PRAGMA journal_mode=WAL")
        writer.execute("PRAGMA wal_autocheckpoint=0")
        writer.execute("UPDATE predictions SET metric='wal_forged_metric'")
        writer.commit()
        with pytest.raises(ValueError, match="inventory"):
            open_workspace(root)
        with pytest.raises(ValueError, match="inventory"):
            workspace.query_predictions(next(iter(workspace.index["runs"])))
    workspace.close()
    # Closed reader owns no journal on the source. The index is itself immutable.
    workspace = save_workspace([FIXTURE], tmp_path / "second")
    index = json.loads((workspace.path / "workspace.json").read_text())
    index["sdk_schema_version"] = 999
    (workspace.path / "workspace.json").write_text(json.dumps(index))
    with pytest.raises(ValueError, match="index changed"):
        workspace.export(tmp_path / "forged.n4w")
    assert not (tmp_path / "forged.n4w").exists()
    workspace.close()


@pytest.mark.skipif(not BROWSER, reason="real browser-produced package required")
@pytest.mark.parametrize("mutation", ["selected", "score", "coupled_metric", "checkpoint_score", "optimizer_bytes"])
def test_browser_loader_rejects_forged_provenance(mutation):
    source = json.loads(Path(BROWSER).read_text())["record"]
    if mutation == "selected":
        source["search"]["selected_params"] = {}
    elif mutation == "score":
        source["search"]["trials"][0]["score"] = -999
    elif mutation == "coupled_metric":
        source["config"]["metric"] = source["contracts"]["request"]["metric"] = "mae"
    elif mutation == "checkpoint_score":
        source["snapshot"]["committed"]["trials"][0]["evidence"]["score"] = -999
    else:
        source["snapshot"]["n4mopt"][0] ^= 1
    from dag_ml import DagMlRuntimeError
    from n4m._errors import N4MError
    with pytest.raises((ValueError, DagMlRuntimeError, N4MError)):
        load_browser_tuning(source)


@pytest.mark.skipif(not FIXTURE, reason="native model experiment qualification fixture required")
def test_workspace_rejects_orphan_chain_after_rehash(tmp_path):
    import hashlib
    import sqlite3
    root = tmp_path / "workspace"
    save_workspace([FIXTURE], root).close()
    with sqlite3.connect(root / "store.sqlite") as writer:
        columns = [row[1] for row in writer.execute("PRAGMA table_info(chains)")]
        projection = ["'review-orphan-chain'" if name == "chain_id" else "'review-missing-pipeline'" if name == "pipeline_id" else '"' + name + '"' for name in columns]
        writer.execute("INSERT INTO chains SELECT " + ",".join(projection) + " FROM chains LIMIT 1")
    writer.close()
    index = json.loads((root / "workspace.json").read_text())
    index["files"]["store.sqlite"] = hashlib.sha256((root / "store.sqlite").read_bytes()).hexdigest()
    (root / "workspace.json").write_text(json.dumps(index))
    with pytest.raises(ValueError, match="Invalid SDK SQLite"):
        open_workspace(root)
