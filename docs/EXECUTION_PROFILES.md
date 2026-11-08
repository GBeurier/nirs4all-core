# Execution profiles

Core publishes one machine-readable, versioned registry of what its bindings
support: `compat/capabilities.toml`. It is a **metadata contract**. It names
operations, entrypoints and content types; it is not numerical certification, a
release gate or a statement about any deployed package.

## Supported set

Exactly five profiles, all at version 1:

| Profile | Content type | Owner of the content type |
| --- | --- | --- |
| `native-workflow@1` | `nirs4all.native-pipeline.v1` (native pipeline JSON envelope) | `nirs4all-core` |
| `native-conformal@1` | `nirs4all.calibrated.v1` (Core wrapper JSON, on the routes listed below only) | `nirs4all-core` |
| `native-methods-package@1` | `dagml.portable_predictor_package` (Package V2 root artifact `model`) and `dagml.portable_refit_package` (Package V3 root artifact `full_refit`), both carried in a Core V1 container (`nirs4all.n4a.container.v1`) that requires the `native-methods-package@1` feature | `dag-ml` (carried by `nirs4all-core`) |
| `native-n4a-canonical@1` | `nirs4all.n4a.container.v1` (raw canonical `.n4a` archive bytes) and the input IO record `nirs4all.dataset.v1` | `nirs4all-core`, `nirs4all-io` |
| `n4a-container@1` | `nirs4all.n4a.container.v1` (raw canonical `.n4a` archive bytes holding opaque owner or host members) | `nirs4all-core` |

### Native Methods Package profile

`native-methods-package@1` transports the fixed native scientific closure of a
DAG-ML Package V2 (predictor) or Package V3 (standalone refit child). The
physical transport is the neutral Core V1 container
(`nirs4all.n4a.container.v1`, required feature `native-methods-package@1`);
Package V2/V3 are payload schemas inside it, and the Methods ABI and wire
formats are separate dimensions. It declares `import` (read), `export` (write)
and `predict`; every other operation, including `inspect`, `read`, `write` and
`update` of arbitrary members, is an explicit unsupported cell owned by
`native-n4a-canonical` or `n4a-container`.

| Action | Python | JavaScript/WASM | Rust |
| --- | --- | --- | --- |
| `import` | `open_native_package`, `read_archive_v2_payloads` | `readPortableArchiveV2` | `read_native_package` |
| `export` | `write_native_predictor_package`, `write_native_refit_package` | `writeNativePredictorPackage`, `writeNativeRefitPackage` | `write_native_package_file` |
| `predict` | `predict_methods_archive_v2_matrix` | `replayMethodsArchiveV2` | `predict_methods_archive_v2_matrix` |

MATLAB/Octave declares no entrypoint in this profile: its only Package-specific
route is `predictMultimodalArchive` (a specific Package V2 route) plus the
generic `n4aInspect`, with no general Package V2/V3 read or replay admission.
Package V3 execution is not claimed on JavaScript/WASM (readable and storable,
execution refused). `native-n4a-canonical` stays closed and refuses this feature
explicitly. Pre-V1 physical Archive V2/V3 ZIP files are refused with a
re-export message. The Python host profile with trusted joblib stays
non-portable and is not part of this registry. This is source support only; no
published version, pin or executed qualification is claimed.

### Owner-neutral storage profile

`n4a-container@1` declares storage only: `inspect`, `read`, `write` and `update`.
`compile`, `fit`, `predict`, `import`, `export`, `produce`, `calibrate` and
`retrain` are explicit unsupported cells. It has no numerical method list and
needs no Methods library; Core validates once, returns exact bytes and derives
sizes and hashes. Readable is not executable: a stored host model (including a
pickled or joblib model) is returned unchanged and never deserialized by Core,
and unknown owners stay opaque in the inspect view.

| Action | Python | JavaScript/WASM | Rust | MATLAB/Octave |
| --- | --- | --- | --- | --- |
| `inspect` | `open_n4a_container` | `openN4aContainer` | `n4a_container_view` | `n4aInspect` |
| `read` | `N4aContainer.member` | `openN4aContainer` | `open_owned_n4a_bytes` | `n4aRead` |
| `write` | `write_n4a_container` | `writeN4aContainer` | `write_n4a_bytes` | `n4aWrite` |
| `update` | `write_n4a_container` (`base=`) | `writeN4aContainer` | `write_n4a_update` | `n4aWrite` (`--archive` base) |

MATLAB/Octave and R go through the `n4a-write`, `n4a-read` and `n4a-inspect` CLI
commands. The storage actions use their own symbols, so they never alias a
canonical native action. The profile is source-declared and its common
write/read/update/refusal gate over `tests/parity/fixtures/n4a_host_opaque_v1`
is pending a first executed run; the JavaScript/WASM and MATLAB/Octave entries
name the agreed symbols that those host files must define.

### Canonical V1 `.n4a` profile

`native-n4a-canonical@1` declares the real canonical actions `produce`,
`inspect`, `predict`, `calibrate` and `retrain`, each with the entrypoint that
exists in the host source:

| Action | Python | JavaScript/WASM | Rust | MATLAB/Octave |
| --- | --- | --- | --- | --- |
| `produce` | `produce_n4a_native` | `produceN4aNative` | `produce_n4a_native` | `n4aProduce` |
| `inspect` | `inspect_n4a_bytes` | `inspectN4aBytes` | `inspect_n4a_view_json` | `n4aInspect` |
| `predict` | `predict_n4a_native` | `predictN4aNative` | `predict_n4a_native` | `n4aPredict` |
| `calibrate` | unsupported | unsupported | `calibrate_n4a_native` | unsupported |
| `retrain` | `retrain_n4a_native` | `retrainN4aNative` | `retrain_n4a_native` | `n4aRetrain` |

`compile`, `fit`, `import` and `export` are explicitly unsupported in this
profile: `inspect` is not `import` (it reads and reports and grants no
executable admission), `calibrate` is not `fit`, and the sealed bytes are
returned by `produce` and `retrain`. The generator refuses one symbol under two
canonical actions. Conversely the workflow and conformal profiles declare the canonical
actions unsupported, because the workflow profile transfers its own JSON envelope and the
conformal profile transfers a DAG-ML Package V2 inside the neutral Core V1
container (`transport` = `native-methods-package@1`), and `native-methods-package@1` declares them unsupported because the
canonical actions belong to this profile. R is an external federation over the same `n4a-*` CLI commands
(`nirs4all_n4a_produce`, `_inspect`, `_predict`, `_retrain`) and declares no
calibration.

**Source support is not qualification.** `supported` means the entrypoint exists
in the host source. Executed evidence is so far bounded to the IO JSON record to
`.n4a` route through the Python native wheel and the Rust owner on Linux. The
portable, retrain, WASM, CLI, MATLAB/Octave and R runtime gates are pending;
JavaScript/WASM has no calibration (and that bound does not narrow the native
Rust behavior); browser, whole-Core, general-ingestion and publication claims
remain unqualified. The container schema, profile version, Methods ABI and wire
format, recipe language and package release are independent dimensions.

Content is declared **per operation, surface and route** (`content_routes`), never
as one global assumption. For `native-conformal@1` the routes are:

- `public_return`: what the host function returns. Rust `calibrate_archive`
  returns the Core wrapper JSON `nirs4all.calibrated.v1` (`calibration`,
  `calibration_replay`, `archive_sha256`, `model_archive`, `training_outcome`).
  The Python and JS/WASM `CalibratedWorkflow` host objects carry no schema id.
  The MATLAB struct carries `schema` = `nirs4all.calibrated.v1` with `archive`,
  `calibration`, `native_json` and `cli`.
- `cli_response`: the internal CLI JSON behind the Python and MATLAB routes.
  `conformal-calibrate` returns the full wrapper; `conformal-load` returns only
  `schema` and `calibration` (no `model_archive` or `archive_sha256`).
- `archive_transfer`: import and export move a DAG-ML Package V2 (package
  `schema_version` 2) carried by the neutral Core V1 container
  (`nirs4all.n4a.container.v1`; route `transport` = `native-methods-package@1`,
  immutable base carried over). The pre-V1 physical Archive V2 ZIP route is not
  current. The calibration is the DAG-ML-owned
  `PortablePredictorPackage.conformal_calibration` field; no wrapper schema id
  is attached. Rust has no public import or export entrypoint in this profile.

Every schema id and field in the registry is checked against the producing
source. DAG-ML has no validator under `nirs4all.calibrated.v1`. The conformal
and workflow profiles transfer their own envelopes or Archive V2 bytes; the
canonical V1 container is declared only by `native-n4a-canonical@1`. Content schema
versions, archive container versions, Methods wire formats, recipe-language
versions and package releases are independent compatibility dimensions. DAG-ML
owns the numerical calibration, scoring types and context; Methods owns
numerical kernels; Core owns the wrappers above.

Nothing else is supported: no parity-labelled, legacy or extension profile
exists. The temporary portable runner and compiler that active callers still use
are tracked as removal work in the ecosystem removal ledger (kept by the ecosystem root, starting from `_backlogs/BACKLOG_CONSOLIDE.md`), never here.

## API

Every host returns a fresh deep copy of the same payload
(`nirs4all-core.execution-profiles.v1`):

| Host | Profiles | Manifest |
| --- | --- | --- |
| Python (`nirs4all_core`, `n4a`) | `execution_profiles()` | `capability_manifest()` |
| JavaScript/WASM | `executionProfiles()` | `capabilityManifest()` |
| Rust | `execution_profiles()` | `capability_manifest()` |
| MATLAB/Octave | `nirs4all.executionProfiles()` | `nirs4all.capabilityManifest()` |
| R (`nirs4all-r`, external) | `nirs4all_execution_profiles()` | (federation record only) |

The manifest holds `schema`, `aggregate`, the runtime surfaces and the profiles.
Payload fields:

- `runtime_surfaces`: `python`, `javascript_wasm`, `rust`, `matlab_octave`. R is
  not one of them.
- `profiles[]`: `id`, `version`, `title`, `surfaces`, `dependencies` (upstream
  keys), `runtime_requirements` (per surface), `content_types[]`
  (`schema`, `owner`, `carried_in`), `content_routes[]` (`operation`, `surface`, `route`, `entrypoint`, `carries`, optional `schema`/`fields`/`cli_command`, and for archive transfers `mechanism`, `archive_schema_version`, `calibration_in_archive`), `operations` and `limits`.
- `operations`: for each of `compile`, `fit`, `predict`, `import`, `export`,
  `produce`, `inspect`, `calibrate`, `retrain` and each runtime surface, either `{status: "supported", entrypoints: [{symbol,
  source}]}` or `{status: "unsupported", reason}`. `unsupported` means *not
  claimed by the profile*; it is a scope statement with a reason, not proof that
  no code exists.
- `runtime_requirements` is per surface because the routes differ. JavaScript/WASM
  runs in the browser and needs matching Core, DAG-ML, Methods and IO WASM
  builds, with no native library path and no CLI. For the workflow and conformal
  profiles Python and MATLAB/Octave shell out to the `nirs4all-core-archive` CLI
  and need a Methods native library; the canonical profile needs the Python
  native extension (no CLI) and, on MATLAB/Octave, the CLI with the `n4a-*`
  commands. The Rust library API takes the libn4m path directly and needs no CLI.
- `external_federation[]`: the `r` record (`core_runtime_surface: false`,
  `status: "externally-federated"`) pointing to `nirs4all-r` contracts, evidence
  and entrypoints, restricted to the profiles above. R levels, fixtures and
  numerics are owned and evidenced by `nirs4all-r`.

## Ownership and generation

`compat/capabilities.toml` is the only hand-written source. Run

```bash
python3.11 scripts/generate_capability_profiles.py          # regenerate
python3.11 scripts/generate_capability_profiles.py --check  # drift gate
```

to refresh the embedded copies (Python, JavaScript, Rust, MATLAB, the canonical
fixture `tests/parity/expected/capability_profiles.json`) and the optional
external targets (the `nirs4all-r` data file and the `nirs4all-web` metadata
snapshot) together. `--check` fails on any drift and reports an absent optional
checkout as `unavailable`. The Web snapshot is a metadata definition only: it
does not certify what the pinned numerical shim staged in Web exports.

`bindings/python/tests/test_capability_matrix.py` parses the TOML owner, rebuilds
the payload, and compares it structurally with every embedded copy, with the
returned manifests, and with the source files that define each claimed
entrypoint. The JavaScript, Rust, MATLAB and R suites compare their live return
values with the same fixture.

## Qualification receipt

`scripts/qualify_execution_profiles.py` writes a receipt with schema
`nirs4all.execution-profile-qualification.v1`, separate from
`compat/local-qualification.json`. Each step ends in one status:

- `executed`: the command ran, passed, and a non-zero number of test bodies
  ran (counts are recorded; skipped, TODO and expected-failure bodies are never
  counted as passed).
- `unavailable`: a required executable or dependency is missing, or every body
  was skipped. An environment-gated script that exits 0 without running its body
  is `unavailable`, never `executed`.
- `unsupported`: the registry declares the operation out of scope and the host
  API actually refuses it.
- `failed`: the command or an executed assertion failed (the runner exits
  non-zero).

Each step records its `evidence` kind: `test_bodies` (counted passed/skipped/TODO bodies), `validation_check` (the generator drift check and `tsc`, counted as checks, not test bodies), `in_process_assertions` or `script_sentinel`. An executed step always has exit code 0 and a non-zero passed count of its kind. Only an exactly recognized missing dependency (for example the Web vitest DAG asset mismatch) is `unavailable`; any other startup, config, source or report error is `failed`. The Quali step runs against the Methods stage resolved by Quali's own helper (recording version, ABI, hashes and delivery) with Core replaced by a boundary stub, so it is not live Core integration.

Metadata *source* assertions (this registry against the sources) are reported
separately from native *scientific replay* (which needs `libn4m`, the CLI and
installed packages). MATLAB and Octave are separate steps. The receipt records
source revisions, dirty-content digests and the hashes of the API implementation
files, the generated data and the tests. `--require-all` additionally fails on
any unavailable step or skipped body. A receipt never certifies a release.
