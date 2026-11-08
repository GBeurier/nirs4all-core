# Binding Contract

## Shared requirements

Every binding must:

- publish the shared upstream-domain registry from the top-level package;
- expose runtime loaders only where a real upstream binding exists for that
  host language;
- translate host-native objects to upstream contracts for gated execution paths;
- parse the shared `nirs4all` JSON/YAML pipeline definition envelope before
  handing execution to upstream runtimes;
- preserve ownership and lifetime rules across FFI boundaries;
- report unavailable upstream components explicitly;
- report full-Python tuning, conformal and robustness artifacts as
  metadata-only until binding-specific APIs and parity fixtures exist;
- expose external operators through host-language idioms only when the upstream
  execution path can actually plan or call them;
- participate in parity checks before release.

The retained MATLAB/Octave portable Savitzky-Golay runner preserves the
methods-backed SciPy boundary-mode contract: `mirror=0`, `constant=1`,
`nearest=2`, `wrap=3`, and `interp=4`. The default remains `interp` to match
the full Python nirs4all operator; explicit `mode` and `cval` values must be
preserved in the execution plan and forwarded to the upstream binding.

Rust, JavaScript/WASM, and MATLAB/Octave publish as `nirs4all` from this core.
The R package `nirs4all` is maintained in `nirs4all-r`. That shared
name is only the package/namespace identity. The full six-domain aggregate is
metadata plus re-export/load hooks; DAG-ML local loss/metric registries are
available in R and MATLAB/Octave. The Python/JavaScript facades require the
task-bound `bind_training_loss` method, R requires `invoke_training_loss`, and
MATLAB/Octave requires `invokeTrainingLoss` before treating the registry as an
executable local-loss surface. Those hosts do not have runtime bindings for every
`formats` / `io` / `datasets` domain row.
The canonical source repository for core-owned bindings is `nirs4all-core`.
Python publishes as `nirs4all-core`; its core-owned non-Python targets publish
as `nirs4all`.

## Native conformal and workflow routes (current)

The supported contracts are the five execution profiles at version 1
(`native-workflow@1`, `native-conformal@1`, `native-methods-package@1`,
`native-n4a-canonical@1` and `n4a-container@1`); this section describes the
workflow and conformal routes (see [`EXECUTION_PROFILES.md`](EXECUTION_PROFILES.md) and
`compat/capabilities.toml`); no artifact manifest is published. A binding keeps
every algorithm upstream: the supported conformal routes call the native DAG-ML
implementation (directly, through the Core CLI, or in WASM) and never compute
quantiles, intervals or coverage locally. What each `native-conformal@1` route
actually carries today:

| Operation | Rust | Python | MATLAB/Octave | JS/WASM |
| --- | --- | --- | --- | --- |
| fit (return value) | `calibrate_archive` returns the Core wrapper JSON `nirs4all.calibrated.v1` (`calibration`, `calibration_replay`, `archive_sha256`, `model_archive`, `training_outcome`) | `CalibratedWorkflow(archive path, calibration)`, no schema id | struct with `schema` = `nirs4all.calibrated.v1`, `archive`, `calibration`, `native_json`, `cli` | `CalibratedWorkflow(archive bytes, calibration)`, no schema id |
| fit/import (internal CLI response) | n/a | wrapper JSON (`conformal-calibrate` full wrapper; `conformal-load` only `schema` + `calibration`) | same CLI responses | none: no CLI |
| import | not supported | reads the archive file, CLI validates it | reads the archive file, CLI validates it | reads archive bytes; DAG validates and `conformal_calibration` is read from the package |
| export | not supported | copies the archive file | `copyfile` of the archive | returns the archive bytes |

For `native-conformal@1`, import and export move a DAG-ML Package V2 (package
`schema_version` 2) inside the neutral Core V1 container
(`nirs4all.n4a.container.v1`), declared by the route `transport`
`native-methods-package@1`, with the immutable base carried over; the wrapper
JSON is unchanged, and the pre-V1 physical Archive V2 ZIP route is not current.
The calibration lives in the DAG-ML-owned
`PortablePredictorPackage.conformal_calibration` field. The native Methods
Package profile (`native-methods-package@1`) carries DAG-ML Package V2/V3
payloads in the same container, with Python, JavaScript/WASM and Rust entrypoints
and none for MATLAB/Octave beyond `predictMultimodalArchive` and the generic
`n4aInspect`; the container, Package schemas and Methods ABI are independent
versions. R is an external federation through the CLI, not a Core runtime. Tuning, robustness and
keyword-registry artifacts of the full Python `nirs4all` package are not
published by this aggregate.

## Python

- Distribution name: `nirs4all-core`.
- Import name: `nirs4all_core`.
- Additive import facade (LOCK-GOV, see [naming](NAMING.md)): `n4a` is the
  full brand-aligned aggregate facade (`import n4a`) over `nirs4all_core`.
  `nirs4all_core.__all__` advertises the complete aggregate contract, including
  execution helpers that delegate to upstream projects.
- `read_portable_predictor_package_v2(source)` accepts a path, an `N4aFile` or
  a `NativePackage`, validates the selected native closure of the Core V1
  container in Rust and returns exact DAG-ML Package V2 bytes. It is not a Python ZIP parser, package
  decoder, artifact executor, or prediction API.
- `inspect_methods_archive_v2_predictors(...)` content-attests libn4m, derives
  `NativePredictorDescriptorV1` from each complete N4MM member through the
  DAG-ML/Methods inspection contract, and returns typed descriptor data only.
  Historical V2 packages without an embedded descriptor stay readable; an
  embedded descriptor must exactly match the native inspection.
- `replay_methods_archive_v2(...)` and `replay_methods_archive_v3(...)` validate
  the archive in Rust, then delegate strict request/envelope semantics and a
  fresh N4MM runtime to DAG-ML. They accept numeric Methods inputs only: no
  callback, joblib sidecar, or supplemental Python controller can cross this
  portable boundary.
- Archive V2 accepts raw PLS N4MM format 1 and the exact embedded format 2
  `SNV(ddof=0) -> Savitzky-Golay(mode=interp) -> PLS` profile. Format 2
  requires the typed ABI 2.5 pipeline descriptor; Python preprocessing is not
  a fallback for a missing or contradictory descriptor.
- `predict_methods_archive_v2_matrix(...)` is the closed X-only product path.
  Core derives the replay contracts, requires one finalized output binding and
  one external data requirement, validates sample/target order, and loads only
  a private snapshot of the caller's SHA-256-attested libn4m after ABI 2.5
  preflight. It exposes no fit/refit, callback or fallback control.
- Native Methods references carry `abi_major` plus `abi_min_minor`. Core 0.3.26
  refuses an archive requiring a newer minor before N4MM import; an absent
  minor is accepted only for immutable historical V2/V3 archives and means the
  capability floor of the old payload family (ABI 2.0 for PLS N4MM, ABI 2.2
  for N4MOPT). New writers always emit the capability-derived minimum (`3` for
  imported-linear N4MM and `5` for embedded SNV -> Savitzky-Golay N4MM v2),
  never the host runtime minor.
- Historical note (Core 0.3.26): the descriptor consumer was first qualified
  against then-unpublished local candidates DAG-ML
  `dafb8b6fb98f9d380d30559a3f4b868c91e5b5c4` and Methods
  `48ad1e5a50844f68c2b99e93b02ad6a3b491c07b`. Current releases compile against
  the published registry cohort listed in the root README.
- `replay_methods_archive_v2_conformal_presentation_v2(...)` exposes the
  additive, content-bound multi-target presentation. The scalar V1 entry point
  remains unchanged; neither path recalculates quantiles, endpoints, or joins
  in Python.
- Framework idioms: sklearn-style estimators, `fit`/`predict`/`transform`,
  NumPy arrays, pandas data frames, and clear optional extras.
- Future external operator adapters should look like normal sklearn-compatible
  transformers or estimators when they participate in Python execution.
- Do not shadow the full Python `nirs4all` package until the core replacement
  migration is intentional. The `n4a` / `nirs4all_core` facades are additive and
  intentionally do **not** define a top-level `nirs4all` Python module.
- Keep `release_topology_manifest()` green against the package metadata before
  changing distribution names, facade imports, or the core execution boundary.

Owner-neutral storage (`n4a-container@1`): `open_n4a_container` returns an
immutable `N4aContainer` (validated once; `view`, `manifest_bytes`, `member`,
`artifact_members`, `archive_bytes`) and `write_n4a_container(manifest_json,
members, base=None)` writes or updates a container from opaque `bytes`. Python
never deserializes a stored host payload; a refusal is a `ValueError`.

## Rust

- Crate name: `nirs4all`.
- Use `Result`-returning APIs and typed wrappers around upstream crates.
- Future external operator adapters should use traits and typed builder APIs,
  with capabilities declared at compile time or through explicit runtime feature
  checks.
- With the public DAG-ML local-registry surface,
  `local_implementation_registry::<T>()` is a thin typed facade over the
  upstream DAG-ML `LocalImplementationRegistry<T>`. Rust callers own the
  concrete callback type `T`; DAG-ML owns the loss/metric descriptors,
  validation, exact resolution and attestation semantics.
- Keep FFI handles explicit; never hide ownership transfers.
- Historical portable KS/SNV/Savitzky-Golay/PLS references use a caller-supplied
  `libn4m` path and the retained shared full-Python `nirs4all` oracle.
- `train_dataset_package_methods_archive_v2(...)` composes one selected dense
  IO `DatasetPackage` source with DAG-ML training and Methods numerics, then
  persists the portable Package V2 as Core Archive V2. The conformal variant
  requires a distinct calibration package with matching target order and
  delegates interval derivation to DAG-ML. Multi-source fusion, N-D payloads,
  Python callbacks, and hidden preprocessing remain outside this route.
- `predict_methods_archive_v2_matrix_conformal_presentation_v2(...)` and the
  replay/load V2 functions expose validated named multi-target presentations;
  Core supplies archive and predictor identity but does not recalibrate.

Owner-neutral storage (`n4a-container@1`): `parse_n4a_manifest`,
`write_n4a_bytes`, `write_n4a_update`, `OwnedN4a::{member, artifact_members,
manifest_bytes, archive_bytes}` and `n4a_owner_portable::n4a_container_view`.
File-backed routes are `n4a_file::{open_n4a_file, write_n4a_file, write_n4a_to}`,
and `n4a_physical` exposes the sans-IO scanner and write plan for hosts that bring
their own byte source. Each takes an optional `N4aMemoryBudget`; `.n4a` V1 content
has no intrinsic cap. The archive command adds `n4a-write` and `n4a-read`; read
writes numbered `member-000000.bin` files plus the exact `manifest.json` and a
report that maps each archive path to its file, so archive paths are never OS
paths. Every `n4a-*` command accepts `--memory-budget-bytes <decimal u64>`.

## JavaScript/WASM

- npm package name: `nirs4all`.
- Expose typed ESM APIs and browser-safe WASM loaders.
- Future external operator adapters should be ESM functions/classes over
  browser-safe values and `TypedArray` data, with async initialization where
  WASM is required.
- The JS portable runner, its coefficient DTOs and execution exports are retired.
  Original source/tests remain non-executable historical references under
  `tests/parity/reference/core-js-runner/`; no compatibility replay is provided.
- `loadPipelineDefinition` retains JSON/YAML text and array / `pipeline` / `steps`
  envelopes, augmentation parsing and manifest-resolved `n4m:<method id>` roles.
  Legacy class aliases are refused. `portableClassNames` extracts supplied names,
  not supported capabilities. `N4mRolePipeline` delegates role fit/replay to Methods.
- `executionProfiles()` is the authoritative custom-host signal for the
  supported native profiles; retirement changes none of those declarations.
- `nirs4all-web` consumes this package; UI code does not live here.
- Current upstream package candidates prefer the target scoped names
  `@nirs4all/formats-wasm`, `@nirs4all/io-wasm`,
  `@nirs4all/datasets-wasm`, and `@nirs4all/methods`.
  `dag-ml-wasm` and `dag-ml-data-wasm` remain unscoped package candidates.

## R

- Package name: `nirs4all`.
- Source repository: `GBeurier/nirs4all-r`, not this core repository.
- The R product records the shared domains and delegates methods-backed
  execution to `n4m`.
- Future R controllers/adapters should expose formula/data-frame paths and S3
  methods where that is the natural R interface.
- Keep native handles opaque and expose provenance in returned objects.
- Current R package candidates include `nirs4allformats`, `nirs4allio`,
  `nirs4alldatasets`, `dagmldata`, `dagml`, and `n4m` / `pls4all` for methods.
  Its DAG-ML integration uses the upstream `dagml` contracts.

## MATLAB/Octave

- Namespace: `+nirs4all`.
- Prefer matrices/tables and explicit options structs.
- Future MATLAB/Octave controllers/adapters should use function handles or small
  handle classes with `fit`/`predict`/`transform`-style methods when execution
  support exists.
- Keep Octave compatibility in the public subset unless a function is marked
  MATLAB-only.
- The retained Core portable runner is MATLAB/Octave-only. Its
  KS/SNV/Savitzky-Golay/PLS subset executes through `nirs4all.runPortablePipeline()` by delegating to the `nirs4all-methods`
  `+n4m` MEX shims. The aggregate binding still owns only parsing,
  orchestration, and result-shape translation.
- `nirs4all.upstreams()` keeps metadata rows for `dag_ml`, `dag_ml_data`,
  `formats`, `io`, and `datasets`. The `dag_ml` row resolves to `+dagml`, and
  `nirs4all.localImplementationRegistry()` returns the upstream process-local
  handle whose native MATLAB/Octave inspection method is `count()`.
  `dag_ml_data`, `formats`, `io`, and `datasets` remain metadata-only because
  they have no MATLAB/Octave runtime candidate.
