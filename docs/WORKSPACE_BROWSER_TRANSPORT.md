# Workspace and browser transport

The portable workspace gateway creates a real modern SDK `WorkspaceStore`
(SQLite metadata and Parquet prediction arrays) from validated native experiments.
It copies prediction values, sample order, fold/variant identity and native score
provenance; it never fits an estimator or recalculates a score.

```python
from nirs4all_core import save_workspace, open_workspace, import_workspace

with save_workspace([experiment], "workspace") as workspace:
    runs = workspace.runs()
    run_id = runs[0]["native_run_id"]
    rows = workspace.query_predictions(run_id)  # actual SDK SQLite + Parquet
    session = workspace.session(run_id)        # public SDK native archive session
    prediction = session.predict(X, sample_ids=sample_ids)
    workspace.export("workspace.n4w")

with import_workspace("workspace.n4w", "restored") as restored:
    restored.experiment(run_id).compare()
```

`Workspace.session` defaults to the SDK native session; `engine="core"` returns
Core's native workflow. Closing the workspace closes its owned SDK sessions.
A missing model produces an explicit score-only run. Import requires the current
SDK schema and refuses older schemas; migrate through the SDK before exporting.
The gateway requires the SDK and the new DAG Python storage adapter. It does not
open an arbitrary historical DuckDB workspace or make host-specific model weights
portable. The gateway's native run IDs are preserved beside the SDK's own UUIDs.

JavaScript `openWorkspace(indexBytes, members)` transports the same inventory
without decoding SQLite or SDK Parquet in JavaScript. It hash-checks the exact SDK
snapshot and independently validates each native experiment/model through DAG;
`runs`, `compare`, `predictions`, `predictMethods`, `export`, and `close` operate on
that immutable native projection. Its validation level explicitly states
`hashed_sdk_snapshot_and_native_experiments`. SDK relational/Parquet consistency
is checked by the Python gateway on import and open.

Browser-produced tuning packages can now be consumed on CPU:

```python
from nirs4all_core import load_browser_tuning

model = load_browser_tuning(browser_export)
replay = model.predict(prediction_dataset)
```

The package is the existing DAG `initial-full-refit.v1` contract. DAG validates
and schedules the frozen replay and releases hydrated model handles. Methods
imports the exact N4ME states and predicts; no FIT route exists. A source-schema
change, package tampering, selected-control mismatch or target-bearing prediction
is refused. The CPU consumer preserves the browser study/checkpoint export but
currently does not resume the browser optimizer on CPU. CPU Archive V2 remains a
distinct HPO archive contract.

JavaScript `calibrate(cpuArchiveBytes, labelledDataset, options)` now accepts the
published dense CPU N4MM workflow profile. Methods executes the frozen N4MM; DAG
replays identity-bound predictions and derives calibration lineage, residuals,
quantiles and signed state. The resulting Archive V2 can be reopened on CPU with
`load_calibrated` and in WASM with `loadCalibrated`. Independent physical samples,
a complete numeric source, one finite target and valid truth masks are required;
overlap with training influence is refused. This extends the existing calibration
API without changing its native archive schema or ABI.
