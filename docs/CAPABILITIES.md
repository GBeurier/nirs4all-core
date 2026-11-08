# Capability Matrix

This page reports, honestly, what each language binding of the nirs4all aggregate
(shipping as `nirs4all-core` in Python and `nirs4all` elsewhere) can actually
*do* — not what it advertises. The capability vocabulary is the ladder defined in
[`OPERATORS.md`](OPERATORS.md):

`metadata` → `plan` → `execute-local` → `execute-remote` → `parity-validated`.

The machine-readable registry is
[`compat/capabilities.toml`](../compat/capabilities.toml). It declares exactly
five supported execution profiles at version 1: `native-workflow@1`,
`native-conformal@1`, `native-methods-package@1`, `native-n4a-canonical@1` and
`n4a-container@1`; see [`EXECUTION_PROFILES.md`](EXECUTION_PROFILES.md). The product paths below
have separate native execution and cold-replay gates and are not exhaustive of
the public API. The registry is a metadata contract, not numerical
certification.

## Product extensions in Core 0.4.4 and R 0.7.1

This cohort uses DAG-ML 0.3.39, IO 0.2.6 and Methods/n4m 1.3.4. The current
product paths below have separate native execution and cold-replay gates.
Numerics remain in Methods, dataset projection in IO, and fitting, selection,
scoring and replay orchestration in DAG.

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
these paths separately from the metadata registry below. MATLAB and Octave
share sources; execution is qualified with Octave, not licensed MATLAB.

## Execution profiles

Custom app hosts read the supported profiles from `execution_profiles()` /
`executionProfiles()` / `nirs4all.executionProfiles()` (Rust
`execution_profiles()`), also embedded in `capability_manifest()` /
`capabilityManifest()` next to the runtime surface list. Host copies are
generated from `compat/capabilities.toml` by
`scripts/generate_capability_profiles.py` (`--check` is the drift gate).

| Profile | Scope |
| --- | --- |
| `native-workflow@1` | generic native pipeline compile / fit / predict / import / export per surface; content type `nirs4all.native-pipeline.v1` |
| `native-conformal@1` | native calibration; per-route content (Core wrapper `nirs4all.calibrated.v1` only where listed; import/export move DAG-ML Package V2 inside the neutral Core V1 container via `native-methods-package@1`); numerics are DAG-ML's |
| `native-methods-package@1` | Methods Package V2/V3 scientific closure carried by the neutral Core V1 container (`nirs4all.n4a.container.v1`, feature `native-methods-package@1`): import/export (read/write) and predict on Python, JavaScript/WASM and Rust; MATLAB/Octave declares no entrypoint (its general Package V2/V3 read/replay is not admitted). Package V3 execution is not claimed on JavaScript/WASM. DAG-ML Package V2/V3 payload schemas and the Methods ABI/wire formats are independent of the container |
| `native-n4a-canonical@1` | canonical V1 `.n4a` actions produce, inspect, predict, calibrate (Rust only) and retrain; stays closed and separate from the Methods Package feature |
| `n4a-container@1` | owner-neutral storage: inspect, read, write and update of opaque members; no execution |

Each operation is `supported` with entrypoints or `unsupported` with a reason
(`unsupported` means not claimed). The R product is recorded as an externally
federated surface, not a Core runtime surface. No coarse parity ledger, runtime
contract or controller manifest is published; the temporary portable runner and
compiler that active callers still use are tracked as removal work, not as a
profile.

## Generic n4m role recipes (trained envelope v8)

Besides the temporary portable-runner aliases, every binding that consults the
portable whitelist accepts the generic step token `"n4m:<catalog method id>"`, resolved
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

This section is historical. It describes the physical Archive V2 ZIP surface of
Core 0.4.3, not the current writer: the current native Methods Package route
writes Package V2/V3 inside the neutral Core V1 container
(`native-methods-package@1`, see [`EXECUTION_PROFILES.md`](EXECUTION_PROFILES.md)),
and pre-V1 physical Archive V2/V3 ZIP files are refused with a re-export message.

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

## What this registry does not claim

Metadata does not certify numerics, a release or a deployed package. Every
claim must be backed by an entrypoint that exists in the host source (checked by
`bindings/python/tests/test_capability_matrix.py`) and by the native execution
gates. A change that adds or removes a supported operation edits
`compat/capabilities.toml`, regenerates the host copies and updates the exact
expectations in the same batch.
