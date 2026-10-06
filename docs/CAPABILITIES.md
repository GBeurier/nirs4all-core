# Capability Matrix

This page reports, honestly, what each language binding of the nirs4all aggregate
(shipping as `nirs4all-core` in Python and `nirs4all` elsewhere) can actually
*do* — not what it advertises. The capability vocabulary is the ladder defined in
[`OPERATORS.md`](OPERATORS.md):

`metadata` → `plan` → `execute-local` → `execute-remote` → `parity-validated`.

The machine-readable compatibility ledger is
[`compat/capabilities.toml`](../compat/capabilities.toml). Its legacy runner and
metadata rows are enforced by `bindings/python/tests/test_capability_matrix.py`.
The newer product workflows have separate native execution gates; the legacy
ledger is not an exhaustive inventory of the public API.

## Product extensions in Core 0.4.4 and R 0.7.1

This cohort uses DAG-ML 0.3.39, IO 0.2.6 and Methods/n4m 1.3.4. The current
product paths below have separate native execution and cold-replay gates.
Numerics remain in Methods, dataset projection in IO, and fitting, selection,
scoring and replay orchestration in DAG. The older runner ledger is unchanged.

| Profile | Public consumers | Qualified boundary |
| --- | --- | --- |
| Generic native CPU pipeline | Python `run_pipeline` / `NativePipeline`; R `nirs4all_run_pipeline`; Node `runPipeline` / `NativePipeline`; MATLAB/Octave `nirs4all.runPipeline` / `nirs4all.NativePipeline` | Finite raw PLS and StandardScale → Ridge regression profiles, complete multiple targets and explicit grouped folds; persisted N4ME state replays without FIT. See [native pipeline contract](native_pipeline_contract.md). |
| Generic native browser pipeline | `runBrowserPipeline`, `BrowserNativePipeline`, `loadBrowserPipeline`, `predictBrowserPipeline` | StandardScale → Ridge with multiple regression targets and grouped folds; binary PLS-LDA label prediction. CPU/browser consumers share the exact native pipeline JSON envelope, distinct from Archive V2. See [browser pipeline contract](browser-native-pipeline.md). |
| Native CPU multimodal workflow | Python `run_multimodal` / `NativeMultimodal`; R `nirs4all_run_multimodal`, `nirs4all_native_multimodal_export/load`; Node `runMultimodal` / `NativeMultimodal`; MATLAB/Octave `nirs4all.runMultimodal` / `nirs4all.NativeMultimodal` | Dense identity plus native ragged summary, explicit missing-source policies, observed target masks and one regressor/classifier per target. This is separate from legacy `MultimodalPredictor`. See [native multimodal contract](native_multimodal_contract.md). |
| Browser tuning state consumed on CPU | Python `load_browser_tuning(...).predict(...)` | Bounded SNV/Savitzky–Golay/PLS initial-full-refit package; native request/search/checkpoint validation and frozen Methods replay without FIT. CPU optimizer continuation is outside this profile. See [browser transport](WORKSPACE_BROWSER_TRANSPORT.md). |
| CPU archive calibrated in browser | JavaScript `calibrate(cpuArchiveBytes, labelledDataset, options)` | Published dense C-native N4MM/Methods PLS Archive V2 profile with disjoint labelled calibration inputs; frozen predictor replay and DAG calibrator fitting. See [transport and calibration contract](WORKSPACE_BROWSER_TRANSPORT.md). |
| Modern SDK workspace and session bridge | Python `save_workspace`, `open_workspace`, `import_workspace`; R `nirs4all_open_workspace`, `nirs4all_workspace_*`; MATLAB/Octave `nirs4all.openWorkspace`, `nirs4all.Workspace`, `nirs4all.importWorkspace` | Actual SDK SQLite metadata and Parquet queries, validated snapshots and portable native sessions. R/MATLAB require Python Core plus the full SDK and reopen/close resources per command. See [workspace bridge](workspace-session-bridge.md). |
| Browser workspace transport | JavaScript `openWorkspace(indexBytes, members)` | Exact member hashes and native experiment/model validation; native result queries and `predictMethods`. It does not decode or independently validate SDK SQLite/Parquet relations. See [workspace transport](WORKSPACE_BROWSER_TRANSPORT.md). |

IO's opt-in masked matrix projection accepts multiple int64 classification
columns with distinct target names and same-shape boolean observation masks.
Masked placeholders become zero before float32 conversion; observed classes
must be exactly representable in float32. The complete classification projection
retains its single-vector contract. Native DAG selects each target's observed
rows for supervised fitting and scoring; ragged sources require explicit native
projection before matrix execution.

PLS-LDA qualification compares sklearn PLS scores followed by independent NumPy
class statistics and pooled covariance divided by `(n - k)`, where `k` is the
number of observed classes. It does not claim exact sklearn LDA SVD parity or a
`predict_proba` surface. Finite profiles do not qualify the whole Methods catalog,
arbitrary host-model weights or every cross-language archive path. Multimodal
SHAP, licensed MATLAB execution and Windows ARM64 remain deferred; Octave
qualification does not qualify licensed MATLAB.

## Historical product workflows: Core 0.4.3 and R 0.7.0

The following table and limits describe the earlier release only. Core 0.4.4
extensions above supersede its calibration/workspace refusals for the explicitly
qualified profiles; other historical limits remain the boundaries of that table.

The earlier product facades exposed the following bounded native paths:

| Task | Python Core | R product | JavaScript/WASM | MATLAB/Octave |
| --- | --- | --- | --- | --- |
| CV/OOF, candidate selection and refit | `run` | `nirs4all_run` / `nirs4all_native_run` | `run` | `nirs4all.run` |
| Predict, export, reload and retrain | `predict`, `export`, `load`, `retrain` | `nirs4all_workflow_*` / `nirs4all_native_*` | `predict`, `exportWorkflow`, `load`, `retrain` | `nirs4all.predict`, `export`, `load`, `retrain` |
| Dataset declaration and raw multimodal state | `Dataset`, `dataset`, `MultimodalPredictor` | `nirs4all_dataset`, `nirs4all_multimodal_*` | `dataset`, `MultimodalPredictor` | `nirs4all.dataset`, `nirs4all.MultimodalPredictor.fit/load`, predictor `.export()` |
| Persisted result views | `Experiment`, `open_experiment`, `save_experiment` | `nirs4all_open_experiment`, `nirs4all_save_experiment`, `nirs4all_result_*` | `openExperiment` | `nirs4all.saveExperiment`, `resultView`, `resultCompare`, `resultPredictions` |
| Native search and resume | `tune`, `resume_tuning`, `load_tuning` | `nirs4all_tune`, `nirs4all_resume_tuning`, `nirs4all_load_tuning` | `tune`, `tuneBrowser`, result `.resume()` | `nirs4all.tune`, `resumeTuning`, `loadTuning` |
| Calibration and frozen robustness audit | `calibrate`, `predict_calibrated`, `conformal_metrics`, `robustness` | `nirs4all_calibrate`, `nirs4all_predict_calibrated`, `nirs4all_conformal_metrics`, `nirs4all_robustness` | `calibrate`, `predictCalibrated`, `conformalMetrics`, `robustness` | `nirs4all.calibrate`, `predictCalibrated`, `conformalMetrics`, `robustness` |

The shared workflow uses a complete dense numeric source, one regression
target, independent observations and the SNV/Savitzky–Golay/PLS recipe. This is
not the general SDK pipeline parser. Raw multimodal state transport is a
separate four-source profile; it does not qualify arbitrary ragged inputs,
missing modalities, multiple targets or portable host-model weights.

Browser HPO uses its native initial-full-refit package. CPU-produced archives
and browser-produced role states have different transport contracts. Consuming
a supported CPU archive that is already calibrated is distinct from calibrating
that CPU archive in the browser, which remains refused. Result views are not a
replacement for the complete SDK workspace/session contract. Numerical methods,
orchestration, calibration and dataset assembly remain upstream-owned.

The native workflow, tuning, conformal, result and multimodal test suites qualify
these paths separately from the legacy runner matrix below. MATLAB and Octave
share sources; execution is qualified with Octave, not licensed MATLAB.

## Custom app host manifest

Custom hosts (`nirs4all-web`, Studio, `nirs4all-ui` consumers, and bespoke
browser/desktop shells) can inspect the portable controller contract without
duplicating local rules:

- Python: `nirs4all_core.capability_manifest()`,
  `nirs4all_core.controller_capabilities()`,
  `nirs4all_core.runtime_surfaces()`,
  `nirs4all_core.runtime_contracts()`, and
  `nirs4all_core.artifact_contracts()`; the same surface is exported through
  the additive `n4a` facade.
- JavaScript/WASM: `capabilityManifest()`, `controllerCapabilities`, and
  `runtimeSurfaces` / `runtimeContracts` / `artifactContracts` from the
  `nirs4all` package.
- R: the separately maintained product exposes `nirs4all_upstreams()` and its
  task APIs. Core's generic manifest getter names are not exported by the
  current R product namespace.
- Rust: `capability_manifest()`, `CONTROLLER_CAPABILITIES`, and
  `RUNTIME_SURFACES` / `RUNTIME_CONTRACTS` / `ARTIFACT_CONTRACTS` from the
  `nirs4all` crate.
- MATLAB/Octave: `nirs4all.capabilityManifest()`,
  `nirs4all.controllerCapabilities()`, `nirs4all.runtimeSurfaces()`, and
  `nirs4all.runtimeContracts()` / `nirs4all.artifactContracts()`.

The manifest schema is `nirs4all-core.capabilities.v1`. Its controller IDs are
stable for the V1 portable subset:

| Controller | Kind | Runtime path | Public parameters |
| --- | --- | --- | --- |
| `split.kennard_stone` | splitter | `portable_pipeline` | `test_size` |
| `preprocess.snv` | transform | `portable_pipeline` | none |
| `preprocess.savgol` | transform | `portable_pipeline` | `window_length`, `polyorder`, `deriv`, `mode`, `cval` |
| `model.pls_regression` | model | `portable_pipeline` | `n_components`, `_range_` |
| `pipeline.portable_methods` | pipeline | `run_portable_pipeline` | none |

Those parameter lists intentionally match the executable parsers today; they are
not future placeholders. The Python gate compares the API against the TOML
ledger, verifies full operator coverage, and requires every runtime surface to
carry an explicit capability level.

The runtime contract also separates two promises that custom app hosts must not
merge accidentally:

| Runtime | Portable pipeline execution | Serialized selected-model prediction |
| --- | --- | --- |
| Python | `parity-validated` via `run_portable_pipeline()` | not exposed |
| R | `parity-validated` via `nirs4all_run_portable_pipeline()` | not exposed |
| JavaScript/WASM | `parity-validated` via `runPortablePipeline()` | `parity-validated` via `predictPortablePipeline()` |
| Rust | `parity-validated` via `run_portable_pipeline_with_library()` | `predict_exported_portable_model_with_library()` and `PortableSession::load_with_library()` / `predict()` |
| MATLAB/Octave | `parity-validated` via `nirs4all.runPortablePipeline()` | not exposed |

JavaScript/WASM reloads its selected-model result through `predictPortablePipeline()`.
Rust reloads content-inspected Methods N4MM payloads directly or through a saved
`PortableSession`; those encodings are distinct and are not interchangeable.
Python, R and MATLAB/Octave do not expose that portable-runner selected-model
contract. The separate Archive V2/V3 product APIs, including Python prediction,
are described below; their availability is not inferred from the runner row.

## Legacy portable runner operator subset

The legacy portable runner executes exactly one operator subset — Kennard-Stone split,
SNV, Savitzky-Golay, and PLS regression — and it does so by **delegating all
numerics to the `methods` upstream** (`nirs4all-methods` / `libn4m` / `+n4m` /
`n4m`). It never re-implements a kernel. The same nine class aliases are
declared identically in all four core-owned bindings (proven by
`test_cross_language_surface.py`).

| Language | Level | Run entry point | Numerics reached via | Parity gate |
| --- | --- | --- | --- | --- |
| Python | `parity-validated` | `run_portable_pipeline()` | nirs4all-methods Python (`n4m`/`pls4all`) | `bindings/python/tests/test_execution_parity.py` |
| Rust | `parity-validated` | `run_portable_pipeline_with_library()` | caller-supplied `libn4m` (`NIRS4ALL_METHODS_LIB`) | `cargo test` `rust_binding_execution_matches_full_python_nirs4all_oracle` |
| JavaScript/WASM | `parity-validated` | `runPortablePipeline()` plus standalone `predictPortablePipeline()` | `@nirs4all/methods` | `bindings/wasm/tests/parity.test.js` |
| MATLAB/Octave | `parity-validated` | `nirs4all.runPortablePipeline()` | `+n4m` MATLAB/Octave MEX shims | `bindings/matlab/tests/parity.m` |

"`parity-validated`" here is **conditional on the `methods` upstream being
present**. Without it, every binding degrades honestly:

- the parser/inspection surface (`load_pipeline_definition`,
  `portable_class_names`, `parse_execution_plan`) still works — this is the
  `plan` level;
- the run entry point raises a clear "capability unavailable" style error
  (e.g. MATLAB's `nirs4all:MissingMethods`, the Rust
  loader error, the strict-parity skip guarded by
  `NIRS4ALL_CORE_REQUIRE_METHODS_PARITY`), never a silent local re-implementation.

The shared numeric oracle is
`tests/parity/expected/portable_python_oracle.json`, generated from the full
Python `nirs4all` library (see [`PARITY.md`](PARITY.md)).

## Generic n4m role recipes (trained envelope v8)

Besides that legacy alias subset, every binding that consults the portable
whitelist accepts the generic step token `"n4m:<catalog method id>"`, resolved
through the Methods manifest rather than a hand-maintained list, and each
executing binding fits and replays role recipes through the
`nirs4all.n4m.trained_pipeline.v8` envelope (N4ME states per fitted step):

| Language | Token resolution | Fit / replay entry point | Gate |
| --- | --- | --- | --- |
| Python | `n4m.roles.method_class` | `N4mRolePipeline.fit_recipe()` / `.from_json()` | `bindings/python/tests/test_n4m_roles.py` |
| Rust | `n4m::roles::method_info` | `N4mRolePipeline::fit_recipe()` / `::from_json()` | `cargo test -p nirs4all n4m_roles` |
| JavaScript/WASM | `@nirs4all/methods` `methodClass` | `N4mRolePipeline.fit()` / `.fromJSON()` | `bindings/wasm/tests/n4m-roles.test.js` |
| MATLAB/Octave | not yet (the `+n4m` MATLAB binding has no role manifest) | — | — |

Every binding wraps the native Methods role pipeline (ABI 2.14), which owns
recipe validation, target routing, feature identity and recipe/state
consistency; the bindings keep only the envelope JSON (including the additive
`feature_names` and per-state `contains_training_rows`). The gates replay the
Python- and R-trained fixture envelopes (`tests/parity/fixtures/n4m_roles_v8_*`)
within 1e-12, round-trip an envelope trained in the binding itself, and replay
the Methods shared fixture (`n4m_role_pipeline_methods.json`) with identical
negative cases. The envelope fields a binding reads itself are checked the same
way in the three bindings before they reach the native pipeline: `n_features`
is a positive JSON integer equal to the native width, `class_names` is a
non-empty list of unique labels (all strings or all finite numbers) labelling every fitted class
id (index = id), column names hold no NUL, and nested X/y rows match the
declared shape; the exported recipe is a snapshot of the recipe the states
attest.

## Historical Archive V2 execution and presentation (Core 0.4.3)

These operational APIs are separate from the full-Python metadata contracts
below and do not alter `compat/capabilities.toml`.

| Binding | Native Archive V2 surface | Explicit boundary |
| --- | --- | --- |
| Rust | Validate/replay N4MM format 1 raw PLS and format 2 `SNV(ddof=0) -> SG(mode=interp) -> PLS`; train one selected dense IO package source; optionally calibrate from a disjoint package; return conformal presentation V1/V2. | DAG-ML owns scheduling/calibration, Methods owns numerics, IO owns package buffers, and Core owns the archive. No Python callbacks, implicit fusion, N-D flattening, or host recalibration. |
| Python | Validate/replay the same native Methods archives and return scalar V1 or named multi-target V2 conformal presentations. | No native package-training facade and no Python preprocessing/model fallback. |
| JavaScript/WASM | Validate/replay the bounded Methods Archive V2 path, including supported CPU archives already calibrated; the product also exposes calibration for its own role-pipeline producer. | Browser calibration of a CPU archive remains refused; transport families are not interchangeable. |
| R product (`nirs4all-r`) | `nirs4all_core_archive*` exposes native ZIP replay; `nirs4all_native_*` and `nirs4all_workflow_*` expose the shared workflow cycle. | General process V3 and historical internal graph snapshots are not qualified by the new dense workflow. |
| MATLAB/Octave | `nirs4all.run`, `predict`, `export` and `load` expose the shared native workflow archive cycle. | N4MM bytes alone are not a complete archive; licensed MATLAB execution remains unqualified. |

## Upstream domains

The other upstream domains — `formats`, `io`, `datasets`, `dag_ml`,
`dag_ml_data` — are re-exported through **lazy import proxies/loaders only**. The
aggregate does not wrap or execute their operators, so its own capability over
them is `metadata`; the real execution capability is whatever the installed
upstream provides. This is recorded as `metadata` rather than dressed up as
aggregate execution.

This metadata is shared across package names, but runtime candidates are
language-specific. R declares `dagml` and MATLAB/Octave declares `+dagml` for
process-local loss/metric registries. Their remaining metadata-only rows must
not be read as npm/WASM package names or host-language runtime support.

| Domain | Aggregate level | Notes |
| --- | --- | --- |
| `formats` | `metadata` | lazy re-export; execution = upstream-provided |
| `io` | `metadata` | lazy re-export; execution = upstream-provided |
| `datasets` | `metadata` | optional/external; lazy re-export |
| `dag_ml` | `metadata` | lazy re-export plus host-local registry delegation |
| `dag_ml_data` | `metadata` | lazy re-export; execution = upstream-provided |

## Native tuning, conformal and robustness artifacts

The full Python `nirs4all` package now owns the first native conformal and
robustness artifact contracts, plus the lightweight native HPO/tuning summary
and ordered search-space contracts. `nirs4all-core` records those contracts in
[`compat/capabilities.toml`](../compat/capabilities.toml) as metadata. Those
particular SDK summary/store contracts are distinct from the executable
product workflow APIs documented above; their metadata level does not mean
that all native tuning, conformal or robustness tasks are unavailable.

| Contract | Producer today | Binding level in nirs4all-core | Allowed consumer behavior |
| --- | --- | --- | --- |
| `conformal.calibrated_result` (`nirs4all.dagml.conformal_store.v1`) | full Python `nirs4all.calibrate()` / `predict_calibrated()` | `metadata` for Python, R, JavaScript/WASM, Rust, MATLAB/Octave | Transport or display optional `conformal_guarantee_status`, `calibration_replay_source`, and `tuning_calibration_source` provenance when present; do not reimplement conformal quantiles, interval application, refit, recalibration, calibration replay, or tuning provenance interpretation. |
| `robustness.summary` (`https://nirs4all.org/schemas/robustness-summary/v1`) | full Python `RobustnessReport.summary_artifact()` / `summary.json` | `metadata` for Python, R, JavaScript/WASM, Rust, MATLAB/Octave | Render or transport a validated summary card, including optional `conformal_guarantee_status` and `spectral_replay` provenance, when a host already receives the JSON; do not recompute robustness metrics, replay spectra or infer conformal guarantees from summary rows. |
| `tuning.summary` (`https://nirs4all.org/schemas/tuning-summary/v1`) | full Python `TuningResult.summary_artifact()` / `tuning-summary.json` | `metadata` for Python, R, JavaScript/WASM, Rust, MATLAB/Octave | Render or transport a validated HPO card, including optional optimizer metadata `sampler`, `pruner`, `seed`, safe `persistence` flags and compact scalar `trials[*].diagnostics`, when a host already receives the JSON; do not drive optimizers, replay trials, infer native tuning execution, or require raw optimizer storage URIs. |
| `tuning.ordered_search_space` (`https://nirs4all.org/schemas/tuning-ordered-search-space/v1`) | full Python `inspect_tuning_space()` / `NativeTuning.inspect_space()` / `nirs4all tuning-space` | `metadata` for Python, R, JavaScript/WASM, Rust, MATLAB/Octave | Validate, transport or render an ordered pre-execution search-space preview, including `run.tuning.space` paths and `run.tuning.force_params` subset checks; do not mutate pipelines, drive optimizers, reproduce Python TCV1 fingerprints locally or infer native tuning execution from the preview. |
| `keyword.registry` (`nirs4all.keyword_registry.v1`) | full Python `nirs4all.get_keyword_registry()` / `keyword_registry_json()` / `TUNING_OPTIMIZER_PERSISTENCE_KEYS` / `ROBUSTNESS_SCENARIO_KINDS` / `ROBUSTNESS_STOCHASTIC_SCENARIO_KINDS` / `ROBUSTNESS_SCENARIO_DISTRIBUTIONS` / `ROBUSTNESS_MODES` / `ROBUSTNESS_EXECUTABLE_MODES` | `metadata` for Python, R, JavaScript/WASM, Rust, MATLAB/Octave | Discover keywords, value schemas, UI hints, invalidation effects and grouped public discovery constants; read `published_constants.ROBUSTNESS_SCENARIO_DISTRIBUTIONS = ["normal", "uniform"]` for the currently published distribution values; preserve the manifest's `required_registry_entries` such as `run.tuning.space`, `run.tuning.force_params`, `predict.coverage`, robustness scenario fields, `robustness.X`, `robustness.predictor` and `robustness.predictor_bundle`; do not infer runtime execution capability from registry presence alone. |

App hosts must evaluate the exact artifact contract and execution profile.
The SDK metadata rows above remain transport/display contracts; the new native
product paths have their own fixtures, APIs and execution gates. Neither set
of rows establishes general parity for every SDK operation or model family.

`required_registry_entries` is a metadata compatibility floor for app hosts and
bindings that mirror the full Python keyword registry. It is not a schema copy
and it is not an execution claim. In particular, `run.tuning.space` must remain
the object/mapping form consumed by full Python `run(tuning=...)` and
`NativeTuning(space=...)`, and `run.tuning.force_params` must remain a public
decoded warm-start hint rather than an optimizer implemented by `nirs4all-core`.
`robustness.X`, `robustness.predictor`, and `robustness.predictor_bundle` are
likewise metadata keys for full Python `nirs4all.robustness()` explicit-X
frozen-predictor replay. nirs4all-core hosts may preserve or display the keys
but must not claim local spectral robustness execution or serialize a Python
predictor object across bindings.

`published_constants` is intentionally narrower: it pins concrete public
constant values that metadata-only bindings need for forms and validation
without importing full Python. The first published value is
`ROBUSTNESS_SCENARIO_DISTRIBUTIONS = ["normal", "uniform"]`, aligned with the
full Python robustness registry and the shared UI scenario helpers.

The ordered search-space contract is also metadata-only. It is useful before a
run to show canonical order, public patch paths and optional decoded
`force_params`, but final TCV1 fingerprints, canonicalization and optimizer
semantics remain owned by full Python `nirs4all`.

## Why this matters for the release

The RC stop condition is explicit: *do not fake unsupported execution in a
language binding; report capability levels honestly.* This matrix + the
enforcement test are that guarantee. If a future change adds, say, a browser
`execute-remote` path or a new operator, the ledger and its test must be updated
in lockstep, and the test will fail until the claim is backed by a real symbol
and gate.
