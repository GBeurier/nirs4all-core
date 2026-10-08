# Changelog

## 0.4.5

- Compile the Rust, Python-native and WASM-native aggregate with DAG-ML 0.3.41,
  including its production fix for redundant immutable metric validation.
- Require DAG-ML 0.3.41 in the optional Python and JavaScript host integrations.
- Run complete parity and end-to-end qualification locally on Linux/WSL and
  Windows; require source-bound evidence before release packaging and publication.
  GitHub remains responsible for portable builds, package smokes and provenance.

## 0.4.4

- Add generic native estimator pipelines and native multimodal campaigns with
  explicit source policies, ragged summaries and per-target observation masks.
- Train and replay the supported estimator pipelines in WASM; share exact native
  state with CPU facades, and retain native JSON fragments without integer loss.
- Replay browser tuning state on CPU and calibrate native CPU archives in the
  browser. Reject unsupported profiles before training or state import.
- Open, query, predict, export and import modern SDK workspace snapshots through
  validated Python, R and MATLAB/Octave commands with bounded process lifetime.
- Require DAG-ML 0.3.39, IO 0.2.6 and Methods 1.3.4; preserve Data 0.2.13,
  Formats 0.2.11 and the Methods ABI 2.17.

See [native pipeline](docs/native_pipeline_contract.md),
[native multimodal](docs/native_multimodal_contract.md),
[browser pipeline](docs/browser-native-pipeline.md) and
[workspace transport](docs/WORKSPACE_BROWSER_TRANSPORT.md) for the finite
supported profiles and explicit refusals.

## 0.4.3

- Preserve the MATLAB/Octave native CLI through workflow prediction, retraining,
  tuning resume, exports and calibrated replay. Honor the configured CLI when
  constructing a dataset and inherit the dataset's runtime for new campaigns.
- Gate MATLAB/Octave releases on native workflow, tuning, conformal, robustness,
  result-view and runtime-path tests as well as strict Python-oracle parity.
- Include executable source tests and their synthetic dense fixture in the
  reproducible MATLAB/Octave ZIP distribution.
- Rebuild MATLAB/Octave ZIPs from a fresh staging archive so obsolete members
  cannot survive a rebuild into an existing output path.

## 0.4.2

- Align the aggregate with DAG-ML 0.3.37, DAG-ML Data 0.2.13, IO 0.2.5 and Formats 0.2.11 across Rust, Python and npm.
- Ship the shared archive/workflow dispatcher in the Python native extension so installed wheels can execute public workflows without an external Core CLI.
- Route native tuning winners through their RolePipeline controller for calibration, calibrated prediction and frozen robustness. Calibrated tuning predictions use the existing scalar native presentation contract, with the validated interval block.
- Stage tuning exports and clean failed calibrated copies without overwriting existing destinations; use Core's exclusive directory publisher for MATLAB exports.
- Generate Rust results/HPO qualification fixtures in the tests and validate the extended WASM public export surface.

## 0.4.1

- Store and replay the closed native RolePipeline/N4ME Archive V3 profiles alongside existing V2 routes, retaining genuine parent/refit identities.
- Require DAG 0.3.34, IO 0.2.4 and n4m 0.4.0 in Rust; align documented Python/npm extras with Methods 1.3.2 and ABI 2.17.
- Keep orchestration and numerics in their owning upstreams; Python Torch sidecars remain trusted host content rather than portable weights.


All notable changes to **nirs4all-core** are documented here. The project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The Rust
crate `[package]` version in `bindings/rust/nirs4all/Cargo.toml` is the
single source of truth; `scripts/bump_version.sh` propagates it to every other
binding manifest.

## [Unreleased]

## [0.4.0] - 2026-09-27

Requires Methods 1.2.1 (ABI 2.14, crate `n4m` 0.3.0) and DAG-ML 0.3.30. Minor
bump: the Rust `RolePredictions::Labels` now holds `ClassLabel` values
(breaking for Rust callers matching on it).

### Fixed

Core side of the 2026-09-27 n4m re-audit (R04, R05, R14, R16), following the
shared label/input contract.

- JS/WASM: nested `X` rows (`flattenMatrix`, used by `N4mRolePipeline` and the
  portable execution) and nested `y` rows of `N4mRolePipeline.fit` are
  checked before flattening: the row count must match the declared `rows` (and
  X), and every row must have the declared width (`y`: the width of row 0,
  at least one target). A ragged `y` that kept the `n*q` total, or X rows with
  extra columns, used to be accepted and silently realigned (R04). The JS
  estimator controller refuses nested X whose width contradicts a declared
  `cols`.
- `class_names` of a v8 envelope is checked on import in Python, JS/WASM and
  Rust: a non-empty list of unique labels, all strings or all finite numbers
  (no null, NaN, ±Inf, boolean or mix of both; an integral number beyond
  ±2^53 is refused rather than rounded into another label) that labels every native class id of the fitted
  classifier (`0 <= id < len`; a longer table keeps labels a filter removed),
  and only on a final classifier. `[]`, `["only"]` and duplicated names used
  to import, then fail or merge at predict (R05). Python also refuses a
  missing or non-finite label at fit. Column names holding NUL are refused on
  import, fit and predict (Python, JS/WASM) before any C string is built.
- Rust: `class_names` may hold finite numbers, as the Python writer produces;
  classifier predictions are `RolePredictions::Labels(Vec<ClassLabel>)` with
  the new `ClassLabel::{Name, Number}`.
- `n_features` must be a positive JSON integer (not a boolean, string or
  fractional number) and equal the native width exactly, in Python, JS/WASM and
  Rust; Python used to truncate `6.9` to `6` (R16). Python also requires a
  boolean `contains_training_rows`.
- Python and JS/WASM `N4mRolePipeline` snapshot a deep copy of the recipe at
  fit and import, export and retrain from it, and return a copy from `recipe`:
  editing the caller's dict (or `recipe`) after fit no longer produces an
  envelope whose recipe contradicts its states (R14).

## [0.3.37] - 2026-09-27

Requires Methods 1.2.0 (ABI 2.14, crate `n4m` 0.3.0) and DAG-ML 0.3.29, so the
Rust build links a single `n4m`.

### Changed

- `N4mRolePipeline` (Python, Rust, JS/WASM) is a thin wrapper over the native
  Methods role pipeline (`n4m.roles.RolePipeline`, `n4m::roles::RolePipeline`,
  `RolePipeline`, Methods ABI 2.14): recipe validation, fit routing, feature
  identity and recipe/state consistency are native, the three bindings keep
  only the v8 envelope JSON. This fixes, on the Core side, the 2026-09-27
  integration audit findings F03 (column identity: names are stored and a
  renamed or reordered column is refused), F05 (an envelope whose recipe
  contradicts its states, or an empty pipeline, is refused on import), F06
  (WASM passes every target column to supervised intermediate steps) and F10
  (exporting a state that embeds training rows needs an explicit opt-in:
  Python `to_json(allow_training_rows=True)`, JS
  `toJSON({ allowTrainingRows: true })`, Rust `to_json(true)`).
- The `nirs4all.n4m.trained_pipeline.v8` envelope gains two additive fields:
  top-level `feature_names` (written when the fit had column names) and
  per-state `contains_training_rows`. Envelopes without them still load.
- Rust API: `N4mRolePipeline::fit_recipe` and `predict` take optional feature
  names, `to_json` takes the training-row opt-in; `feature_names()` and
  `pipeline()` are new. JS: datasets take `featureNames`; `featureNames` and
  `pipeline` are new. Python: `feature_names` and `pipeline` are new;
  `estimators` is gone.
- Requires Methods 1.2.0 (`nirs4all-methods>=1.2.0,<2`, npm
  `@nirs4all/methods ^1.2.0`, crate `n4m =0.3.0`).

### Added

- Parity fixtures `n4m_role_pipeline_methods.json` (the Methods shared
  role-pipeline fixture) and `n4m_roles_v8_python_named.json` (a
  Python-trained envelope with `feature_names` and a kernel PLS state
  exported with the training-row opt-in), replayed with the same negative
  cases by the Python, Rust and JS/WASM suites.

## [0.3.36] - 2026-09-27

### Added

- Generic n4m role recipes. The Python, Rust and JS/WASM bindings accept the
  language-neutral step token `"n4m:<catalog method id>"` wherever the
  portable whitelist is consulted, resolving it through the Methods manifest
  (`n4m.roles.method_class`, `n4m::roles::method_info`, `methodClass`) rather
  than a hand-maintained list; `n4m_role_capabilities()` /
  `n4mRoleCapabilities()` list the usable steps from that manifest.
- `N4mRolePipeline` in the three bindings fits role recipes (sample filters,
  transformers/selectors, one regressor or classifier) through the Methods
  estimator roles and reads/writes the cross-language trained envelope
  `nirs4all.n4m.trained_pipeline.v8` (per-step N4ME states). The npm package
  replays Python- and R-trained envelopes in the browser (level L2); the gates
  replay the shared fixtures within 1e-12 and round-trip their own fits.
- JS/WASM portable recipes execute one train-only native X augmentation step
  (`train_augmentation` with class `n4m.NativeXAugmentation`, the closed
  22-kind R contract with positional values and a seed) through the Methods
  `augmentNative` kernel. It transforms only the split training rows, is
  recorded in the run result and is never replayed at prediction; it must
  precede every preprocessing step and the model.

### Changed

- Require the Methods release carrying ABI 2.13 estimator roles (Methods
  1.1.0): Rust `n4m =0.2.0`, npm peer `@nirs4all/methods ^1.1.0`, and the
  Python `methods`/`all` extras `nirs4all-methods>=1.1.0,<2`.
- Move the Rust coordinator pins to DAG-ML 0.3.28 (`dag-ml`/`dag-ml-core`
  `=0.3.28`, also in the WASM-native bridge), the release that links the same
  `n4m` 0.2.0 binding, so the Rust graph carries exactly one `n4m`/libn4m
  loader; the npm test harness resolves `dag-ml-wasm` 0.3.28. The Python
  `dag-ml` extra keeps its `>=0.3.27,<0.4` floor.
- Archive V2 replay takes its libn4m ABI verdict from DAG-ML's
  `MethodsRuntime::configure` preflight instead of a second n4m instance. A
  Rust process therefore fixes one libn4m file for role recipes and Archive V2
  replay; select it with `preflight_methods_archive_v2_library` when a process
  does both.
- `compat/upstreams.toml` pins DAG-ML v0.3.28, Methods v1.1.0 and
  nirs4all-formats v0.2.9 (the version the manifests require).

## [0.3.31] - 2026-09-24

### Changed

- Align the portable aggregate with DAG-ML 0.3.27 and DAG-ML Data 0.2.12.
  Rust pins and Python dependency floors now select the parity-qualified
  coordinator and the same unchanged data contracts across bindings.

## [0.3.30] - 2026-09-23

### Changed

- Bumped the aggregate to 0.3.30 and repaired atomic Archive V1/V2 publication
  on Windows. Core now opens the parent directory with write access and
  `FILE_FLAG_BACKUP_SEMANTICS` before flushing its metadata, instead of using
  the read-only directory handle that made `FlushFileBuffers` fail with
  `ERROR_ACCESS_DENIED` after the archive had already become visible. Cleanup
  and durability errors remain fail-closed, and CI now exercises both archive
  writers on a real Windows runner.
- Bumped the aggregate to 0.3.29 and pinned the corrected V1 release train:
  DAG-ML 0.3.25, dag-ml-data 0.2.11, nirs4all-io 0.1.18, and
  nirs4all-methods/pls4all 1.0.18. The immutable upstream revisions and every
  Python/Rust/WASM dependency floor now describe the same qualified stack.
- Bumped the aggregate to 0.3.28 and made the libn4m canonical-path check
  accept the ordinary absolute spelling returned by Windows applications when
  it differs from `std::fs::canonicalize` only by the verbatim `\\?\` or
  `\\?\UNC\` prefix. Other lexical aliases and symlink-resolved paths remain
  refused.
- Repinned the native aggregate to `nirs4all-formats` 0.2.9 and the coherent
  `nirs4all-io` / `nirs4all-io-dagml` 0.1.14 family. The Python Formats and IO
  extras now carry the same security-release floors. Upgraded the native
  Python bridge to PyO3/pythonize 0.29 to remove the two RustSec advisories in
  the prior PyO3 0.22 dependency.
- Canonicalized the private libn4m snapshot path before storing, comparing,
  and returning it. This keeps macOS `/var` and `/private/var` aliases from
  being mistaken for two different process-wide Methods library identities.
- Bumped the aggregate to 0.3.26 and repinned both `nirs4all-io` and
  `nirs4all-io-dagml` to 0.1.13 so downstream products resolve the restored
  bounded `RoleTaggedReadLimits` API from one coherent IO family. The Python
  IO extra now carries the same 0.1.13 minimum.
- Repaired the 0.3.25 post-tag qualification path: the release topology now
  includes the native predictor inspection export, CI installs wasm-pack
  0.15.0 explicitly, and Rust integration tests build libn4m from the exact
  published n4m 0.1.4 source tag. The wheel validator accepts Windows CRLF
  source files, and a PyPI repair rebuilds the immutable 0.3.25 source while
  loading its corrected validator from the descendant repair commit. Manual
  repair runs are restricted to a 0.3.25 descendant so the release tag is
  never moved.
- Advanced the native release candidate from Core 0.3.25 with exact Rust pins
  on DAG-ML 0.3.23, dag-ml-data 0.2.10, nirs4all-formats 0.2.8,
  nirs4all-io 0.1.12, and n4m 0.1.4. The release lock now resolves those
  published crates directly from crates.io, with no checkout-local patches.
  Product replay preflights the Methods ABI 2.5 contract; published Python
  extras retain their existing compatibility floors.
- Added dual-read support for capability-derived `abi_min_minor` on native
  Methods archive references. Historical references without the field retain
  their payload-family floors (ABI 2.0 for PLS N4MM and 2.2 for N4MOPT);
  current Rust and WASM replay refuse a payload whose declared minimum is newer
  than the selected runtime before native import.

### Added

- Added Rust, Python, and JavaScript/WASM access to DAG-ML's typed
  `NativePredictorDescriptorV1`. Descriptor fields come only from complete
  Methods N4MM inspection, are checked at load/predict boundaries, and remain
  optional for immutable historical Archive V2 packages.
- Added native N4MM format 2 replay for the exact embedded
  `SNV(ddof=0) -> Savitzky-Golay(mode=interp) -> PLS` pipeline. Raw PLS remains
  N4MM format 1; the format 2 descriptor and ABI 2.5 semantics fail closed
  rather than falling back to Python preprocessing.
- Added the Rust `DatasetPackage` training composition: IO owns the selected
  dense source and target buffers, DAG-ML owns folds/training/refit, Methods
  owns numerics, and Core persists the resulting portable Archive V2. A second
  Rust entry point calibrates against an explicitly disjoint package and
  persists the calibrated archive.
- Added the closed Rust `Archive V2` matrix-prediction entry point for product
  hosts. It derives the signed PREDICT replay from one X-only external data
  requirement, preserves sample/target order, and loads libn4m from a private
  content-attested snapshot whose canonical source path and SHA-256 identity
  cannot change during the process. Product hosts can run the same closed
  attestation and ABI 2.5 verification as a preflight without receiving an
  injectable runtime path or native handle.
- Added bounded Archive V2 replay to the JavaScript/WASM binding. The existing
  Core Rust reader now exposes its same byte-oriented validation path to WASM,
  closing the stored-ZIP inventory and raw digests before DAG-ML validates the
  package and Methods artifact binding. The JavaScript ownership layer invokes
  one multi-target prediction through the public Methods C ABI; there is no
  JavaScript estimator or fallback fit.
- Added the `Archive V2 → DAG-ML → Methods` replay facade for portable native
  prediction. Core validates and opens the archive, then delegates Package V2
  parsing and N4MM execution to the published DAG-ML 0.3.15 runtime; it does
  not duplicate package parsing or numerical execution.
- Added the `Archive V3 → DAG-ML → Methods` target-bound full-refit replay
  facade. Core exposes only validated archive bytes and attested current-cohort
  inputs; DAG-ML remains the owner of Package V3 validation, scheduling and
  invocation-local N4MM hydration through the published 0.3.15 runtime.
- Added closed conformal presentation replay for calibrated Archive V2
  packages. Rust and Python bindings now return DAG-ML's self-validating,
  identity-bound `ConformalPresentationV1` without recalculating intervals or
  accepting a Python model callback.
- Added the additive `ConformalPresentationV2` Rust and Python replay surface
  for named multi-target outputs. V1 remains available for scalar consumers;
  V2 transports DAG-ML-validated evidence without host-side recalibration.

- Exposed the DAG-ML process-local loss and metric registry through the Python,
  R, Rust, MATLAB/Octave, and WASM aggregate bindings, pinned to the verified
  DAG-ML `0.3.23` release contract.

## [0.3.10] - 2026-07-10

### Fixed

- Removed unpublished legacy WASM upstream aliases from the `nirs4all` npm
  package peer dependencies and runtime loader. The aggregate now exposes only
  the canonical `@nirs4all/*-wasm` upstream package names plus the published
  `dag-ml-wasm` and `dag-ml-data-wasm` packages.

## [0.3.9] - 2026-07-10

### Changed

- Bumped the portable aggregate release train to the published upstream stack:
  `dag-ml-data 0.2.9`, `nirs4all-formats 0.2.7`,
  `nirs4all-io 0.1.11`, and `nirs4all-methods 1.0.9`.
- Updated Python dependency floors, Rust `dag-ml-data` dependency metadata,
  and the machine-readable upstream checkout lock used by release parity jobs.

## [0.3.1] - 2026-07-08

### Fixed

- Removed remaining documentation wording that implied a maintained
  `nirs4all-lite` public alias. The V1 target has no legacy alias release.

## [0.3.0] - 2026-07-07

### Changed

- Made `nirs4all_core` the canonical Python implementation package for
  `nirs4all-core`.
- Kept `n4a` as the additive brand facade over `nirs4all_core`.
- Removed public legacy alias rows from the release topology and wheel package
  list.
- Standardized strict parity gates on `NIRS4ALL_CORE_*` environment variables.

## [0.2.12] - 2026-07-07

RC16 runtime-contract honesty release.

### Added

- Added `runtime_contracts` / `runtimeContracts` across Python, R, Rust,
  MATLAB/Octave, and JavaScript/WASM capability manifests.
- Declared portable pipeline execution and standalone serialized-model
  prediction as separate custom-host contracts.
- Guarded the manifest so only JavaScript/WASM currently claims
  `predictPortablePipeline()` parity for serialized selected-model replay.

## [0.2.11] - 2026-07-07

RC15 changelog catch-up release.

### Fixed

- Added the missing changelog entries for the `0.2.9` and `0.2.10` release
  candidates so source releases document the custom-host and lockfile fixes.

## [0.2.10] - 2026-07-07

RC14 lockfile consistency release.

### Fixed

- Bumped the tracked root `Cargo.lock` package entry alongside the Rust crate
  version after the `0.2.9` source tree still reported `nirs4all = 0.2.8` in
  the lockfile.
- Extended `scripts/bump_version.sh --check` so future release bumps also
  validate the root Cargo lockfile's local `nirs4all` package version.

## [0.2.9] - 2026-07-07

RC13 custom-host capability manifest release.

### Added

- Added the cross-language V1 capability manifest for custom app hosts across
  Python, WASM/JavaScript, R, Rust, and MATLAB/Octave bindings.
- Exposed stable controller capability IDs for Kennard-Stone splitting, SNV,
  Savitzky-Golay, PLS regression, and the portable methods pipeline.
- Documented runtime surfaces and the custom-host manifest contract.

### Changed

- Synchronized `nirs4all-web` with the vendored core WASM custom-host surface.

## [0.2.8] - 2026-07-07

RC12 methods package-name alignment.

### Changed

- Updated JavaScript/WASM upstream metadata from `@nirs4all/methods-wasm` to
  the V1 package `@nirs4all/methods`.
- Advertised MATLAB/Octave methods as `+n4m` with `+pls4all` compatibility.

### Fixed

- Republished the core aggregate surface after `0.2.7` was tagged against the
  older methods WASM package name.

## [0.2.7] - 2026-07-06

RC11 core aggregate release head.

### Fixed

- Normalized the Python package license expression to the canonical SPDX
  casing `CECILL-2.1 OR AGPL-3.0-or-later`.

### Added

- Custom-host composition documentation for using the `nirs4all` WASM package
  with host-provided UI/runtime layers.

### Fixed

- Stabilized the R multimodal roundtrip E2E environment checks without changing
  the runtime contract.

## [0.2.6] - 2026-07-06

RC V1 package-name and upstream compatibility hardening.

### Changed

- Locked core Rust dependencies and merged the core main gates into the RC
  release train.
- Accepted the published/scoped WASM upstream package-name variants used by the
  release train.

## [0.2.5] - 2026-07-06

RC V1 topology: this historical train combined the first `LOCK-GOV` facade
slice (additive) with the Python distribution rename from the retired
`nirs4all-lite` name to `nirs4all-core` (Phase R1 of `docs/CORE_RENAME.md`,
executed by RC-A on the RC V1 control-board decision).

### Changed (RC V1 rename)

- Python distribution renamed from the retired `nirs4all-lite` name to
  **`nirs4all-core`** (`bindings/python/pyproject.toml`). This 0.2.x train
  briefly kept transitional Python import compatibility; that compatibility
  surface is superseded by the 0.3.0 canonical `nirs4all_core` + `n4a` package.
  Rust/npm/R/MATLAB names are unaffected (already the bare `nirs4all`).
- `release_topology_manifest()` schema bumped to
  `nirs4all-core.release-topology.v2`: `aggregate.id = "nirs4all-core"`,
  `python.distribution = "nirs4all-core"`, install rows flipped, and the
  source/SBOM artifact renamed `nirs4all-core-source-sbom`.
- Release workflows build/validate/publish under the new name
  (`nirs4all_core-*` wheel, `nirs4all-core-<version>-src.*` source prefix,
  PyPI project `nirs4all-core`). The V1 RC target does not publish or maintain
  a public `nirs4all-lite` compatibility release.
- User-facing diagnostics across the five bindings now say
  "nirs4all-core portable subset".

First safe `LOCK-GOV` slice — **additive only**, no legacy import removed.

### Added

- Python `n4a` import facade — a brand-aligned root (`import n4a`) that
  re-exports the aggregate public surface and adds no behavior.
- Python `nirs4all_core` import root for the `nirs4all-core` aggregate.
  (Introduced additively; it became canonical in the 0.3.0 naming cleanup.)
- `docs/NAMING.md` documenting the per-language aggregate names, the lite→core
  direction, the facades, and the `n4a` token disambiguation (`n4a` import vs
  `.n4a` bundle extension vs `n4a-datasets` CLI) for `GOV-004`.
- `bindings/python/tests/test_facade.py` proving surface parity, object
  identity, `__getattr__` passthrough, and full-`nirs4all` coexistence.

### Fixed

- Removed the stale `License :: OSI Approved :: MIT License` trove classifier
  from the Python `pyproject.toml`; the SPDX `License-Expression`
  (`CECILL-2.1 OR AGPL-3.0-or-later`) is authoritative (PEP 639). The wheel
  metadata is no longer self-contradictory.

## [0.2.0] - 2026-06-14

**Breaking** (pre-1.0 minor bump, 0.1.0 → 0.2.0) — coordinated with the breaking
**nirs4all-methods 1.0.0** (C ABI 2.0 + the `n4m.<role>` namespace). The
then-current aggregate re-exported the methods surface, so consumers had to move
to the methods 1.0.0 / ABI-2 surface.

### Changed (breaking)

- Re-exports the ABI-2 `nirs4all-methods` surface. The Python aggregate now
  imports methods through the new `n4m.<role>` namespace
  (e.g. `n4m.transform.scatter`, `n4m.transform.smoothing`,
  `n4m.model_selection.splitters`) instead of the old flat `n4m.sklearn.*`
  layout.
- The Rust/WASM bindings load the ABI-2 C symbols: `n4m_pp_*` preprocessing
  entry points are now `n4m_transform_*`, and `n4m_split_*` selection entry
  points are now `n4m_model_selection_*`.
- Pinned `nirs4all-methods >= 1.0.0` (was `>= 0.99.0`) in the Python
  `methods`, `all`, and bundled-aggregate extras.

### Versioning

- Bumped the then-named lite project version `0.1.0` → `0.2.0` across every packaging
  manifest: the Rust crate (source of truth), the WASM `package.json` /
  `package-lock.json`, the Python `pyproject.toml`, and the R `DESCRIPTION`.
  The MATLAB/Octave archive version derives from the Rust crate version at
  build time.
