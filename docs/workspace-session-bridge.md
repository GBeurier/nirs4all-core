# Modern SDK workspace sessions from R and MATLAB/Octave

The modern full SDK owns SQLite metadata, Parquet prediction arrays and native session loading. Core validates a closed byte inventory and the exact native-to-SDK run, model, metric, score, identity and array projection before querying or loading a session. Unexpected files, SQLite journals, tampered members, foreign relations and incompatible schemas are refused. Snapshot destinations must be new. This profile carries portable native experiment models; it does not transport arbitrary host estimator weights or migrate old DuckDB stores.

Install Python 3.11+, `nirs4all-core`, the full `nirs4all` SDK and their native runtime in one interpreter. Set `NIRS4ALL_WORKSPACE_PYTHON` to that interpreter's executable path. R additionally requires `processx` and `jsonlite`. Octave requires `popen2`; MATLAB uses its JVM ProcessBuilder. Calls pass argv and JSON files, without shell commands or interpreter expressions. Each command closes its SDK store and sessions before returning, including prediction errors.

```r
workspace <- nirs4all_open_workspace("SDK workspace")
run_id <- nirs4all_workspace_runs(workspace)[[1]]$native_run_id
rows <- nirs4all_workspace_predictions(workspace, run_id)
session <- nirs4all_workspace_session(workspace, run_id)
result <- nirs4all_workspace_predict(session, X, sample_ids)
nirs4all_workspace_export(workspace, "snapshot.zip")
nirs4all_workspace_close(workspace)
```

```matlab
workspace = nirs4all.openWorkspace('SDK workspace');
runs = workspace.runs();
rows = workspace.predictions(runs(1).native_run_id);
session = workspace.session(runs(1).native_run_id);
result = session.predict(X, sampleIds);
workspace.export('snapshot.zip');
workspace.close();
```

Closing the parent closes its child handles; subsequent operations fail. Import uses `nirs4all_import_workspace` in R and `nirs4all.importWorkspace` in MATLAB/Octave. Python exposes `execute_workspace_command` from `nirs4all_core.workspace_cli`, or `python -m nirs4all_core.workspace_cli --input request.json --output response.json`. Closed schema `nirs4all.workspace-command.v1` accepts open, query, predict, export, import and save. The response is `{ok: true, result: ...}` or `{ok: false, error: ..., error_type: ...}`. Predict accepts raw numeric `X` plus explicit `sample_ids`; the SDK returns its own verified sample identity.

Qualification uses a real SQLite/Parquet/native-model workspace, exact prediction comparison, Unicode paths containing spaces, export/import, close propagation and fit-forbidden prediction. Python, installed R and Octave 10.3 were executed. Licensed MATLAB execution remains unqualified. Browser workspace transport verifies the bytes and native archives but does not decode SQLite or expose the SDK query engine.
