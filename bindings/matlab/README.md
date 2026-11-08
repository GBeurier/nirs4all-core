# MATLAB/Octave Binding

Namespace: `+nirs4all`

The canonical source repository is `nirs4all-core`; the shipped MATLAB/Octave
surface keeps the bare `nirs4all` namespace and is attached to releases as
`nirs4all-matlab-octave-<version>.zip`.

The MATLAB/Octave binding exposes the top-level `nirs4all` aggregate namespace.
It parses the same portable JSON/YAML pipeline envelope as the other bindings
and delegates numerical execution to the upstream `nirs4all-methods` `+n4m`
MEX shims.

`dag_ml` and `methods` have MATLAB/Octave runtime candidates (`+dagml` and
`+n4m`). The other upstream domains are kept in `nirs4all.upstreams()` as
aggregate metadata and do not advertise npm/WASM package names or
MATLAB/Octave execution support.

## Surface

- `nirs4all.loadPipelineDefinition(source)` accepts JSON/YAML text, a
  JSON/YAML path, a direct step list, `pipeline`, or `steps`.
- `nirs4all.portableClassNames(definition)` returns the operator classes in the
  definition.
- `nirs4all.runPortablePipeline(source, dataset)` executes the portable
  Kennard-Stone/SNV/Savitzky-Golay/PLS subset through `n4m`.
- `nirs4all.executionProfiles()` and `nirs4all.capabilityManifest()` publish
  the native `native-workflow@1`, `native-conformal@1` and `native-n4a-canonical@1`
  profiles with
  per-operation supported/unsupported entries (see `docs/EXECUTION_PROFILES.md`).
- `nirs4all.upstreams()` and `nirs4all.requireUpstream(key)` expose the
  aggregate upstream registry.
- `nirs4all.localImplementationRegistry()` delegates to the upstream DAG-ML
  registry for process-local MATLAB/Octave loss and metric functions.
- `nirs4all.dataset()` normalizes an aligned raw dataset through native IO.
- `nirs4all.run()`, `predict()`, `retrain()`, `export()` and `load()` expose the
  shared dense regression CV/OOF/refit archive profile.
- `nirs4all.tune()` and `resumeTuning()` run and resume native OOF search;
  `exportTuning()` and `loadTuning()` preserve the native study identity.
- `nirs4all.calibrate()`, `predictCalibrated()`, `conformalMetrics()` and
  `robustness()` expose held-out calibration and frozen native audits.

These native workflows target `nirs4all-core-archive` 0.5.0 and a Methods
ABI 2.20 library. The retained Core 0.4.7 qualification used Methods 1.3.13
(ABI 2.20); qualification of the 0.5.0 cohort remains a release gate. Supply the
Core executable through `cli` or
`NIRS4ALL_CORE_CLI`, and the numerical library through `methodsLibrary` or
`N4M_LIBRARY_PATH`. Workflow, tuning and calibrated objects retain the supplied
CLI for subsequent operations; exports contain portable model metadata, and
the receiving host supplies its runtime when loading.

The MATLAB source facade is shared with Octave. The release gate executes
Octave; it does not establish a licensed MATLAB runtime qualification.

`dataset` is a struct with `X`, `y`, and optionally `rows`/`cols` when `X` is
flat. JSON-decoded numeric matrices and nested numeric cells are accepted so the
same oracle fixtures work in MATLAB and Octave.

The portable MATLAB/Octave Kennard-Stone path is fraction-only and refuses nonempty `n_train`; missing fields, null-empty values and empty characters remain unset, while native owner count support is unchanged.

Savitzky-Golay defaults to `mode = 'interp'` for full nirs4all parity and
preserves explicit methods-backed modes (`mirror`, `constant`, `nearest`,
`wrap`, `interp`) plus `cval`.

## Canonical `.n4a` V1 container

These functions drive the canonical V1 `.n4a` container (manifest schema
`nirs4all.n4a.container.v1`) through the `n4a-inspect`, `n4a-produce`,
`n4a-predict` and `n4a-retrain` commands of `nirs4all-core-archive`. They are
separate from the native package workflow functions above (`run`, `predict`,
`retrain`, `export`, `load`), whose archives are also physical Core N4a V1
containers (container `schema_version` 1) carrying a native-methods-package
root; the Package schema (2 predictor, 3 refit) is the separate
`package_schema_version`. Pre-V1 ZIP archives are refused by Core with no
fallback or converter here.

- `nirs4all.n4aInspect(archive, 'cli', cli)` reads the container and returns a
  reference (`archive`, `archiveSha256`, `readability`). Readability is not
  executability: no Methods runtime is loaded and `executability` is always
  `'not_evaluated'`.
- `nirs4all.n4aProduce(recordPath, recipe, archive, ...)` trains once through the
  Core owner and writes the sealed owner bytes. `recipe` is a file, JSON text or a
  struct with `steps` and `candidates`.
- `nirs4all.n4aPredict(model, 'record', path, 'partition', 'predict', ...)`
  predicts without refit. `values` follow the physical row order of the record,
  aligned once by the owner-provided sample IDs; a missing, repeated or extra ID
  is refused. `targetNames`, `targetUnits`, `sourceId` and `partition` are
  returned, and `replay` and `receipt` are the unchanged owner objects
  (`nativeJson` is the exact text). Without `'record'` the training record
  embedded by `'embedTrainingRecord', true` is predicted in-sample.
- `nirs4all.n4aRetrain(model, recordPath, archive, ...)` calls the Core owner
  `retrain_n4a_native` on the container: the owner obtains the source ID and the
  validated stored recipe from the package, so neither is an argument, and it
  performs one production. Nothing is retrained in MATLAB/Octave, the source
  container is left untouched, and owner errors are raised unchanged as
  `nirs4all:N4aRefusal`.

Scope and requirements:

- Records are passed as JSON **file paths** so the owner sees the exact bytes;
  structs and `PublicDataset` objects are refused on this route.
- The CLI must provide the `n4a-*` commands (`cli` or `NIRS4ALL_CORE_CLI`). An
  older CLI raises `nirs4all:N4aUnavailable`.
- Produce, predict and retrain need the real Methods library and its
  independently attested SHA-256: `methodsLibrary` / `N4M_LIBRARY_PATH` and
  `methodsLibrarySha256` / `N4M_EXPECTED_LIBRARY_SHA256`. There is no default
  hash and no fallback runtime.
- Which methods, targets and profiles work is decided by the Core, DAG and
  Methods owners, not by this binding. The profile exercised by the test is
  `standard_scale` + `ridge` with two ordered targets; it is not a claim of
  universal support. Refusals are raised as `nirs4all:N4aRefusal` with the native
  message.
- Linux/Octave only is exercised; this does not qualify MATLAB or Windows.

### Optional allocation budget

Every `n4a*` transport (`n4aInspect`, `n4aProduce`, `n4aPredict`, `n4aRetrain`,
`n4aWrite`, `n4aRead`) accepts `'memoryBudgetBytes', value`. It is forwarded to the
CLI as the exact decimal text of `--memory-budget-bytes <u64>`; the native Core
decides admission and refusal. Omitted or empty (`[]`, `''`) sends no flag, and
this binding adds no cap, default or memory detection.

- Accepted: a nonnegative integer-valued double up to `2^53 - 1`
  (9007199254740991), a nonnegative integer-class scalar (`uint64` keeps every
  bit, e.g. `intmax('uint64')`), or decimal digits (no sign or spaces; leading zeros
  are valid and forwarded as given) with a value up to `18446744073709551615`.
- Anything else raises `nirs4all:N4aArgument` before any CLI is resolved: a
  fractional, negative, non-finite, logical, complex, non-scalar or larger double
  is refused rather than rounded. Pass budgets above `2^53 - 1` as text or `uint64`.
- `nativeJson` is the exact report text. `jsondecode` stores numbers as doubles,
  so report integers above `2^53` are exact only in `nativeJson`.
- The budget is not a guarantee about the machine. Nothing here was run against
  the real CLI yet; the flag is source-only until the shared campaign runs.

### Owner-neutral exchange

`nirs4all.n4aWrite` and `nirs4all.n4aRead` store and return arbitrary members of
a canonical container through the `n4a-write` and `n4a-read` commands: data,
recipes, results, scores and opaque host models. They contain no model logic,
no Methods call and no numerical code, and they need neither a Methods library
nor an admission.

- `nirs4all.n4aWrite(manifestPath, membersPath, destination)` seals a container
  from an exact manifest JSON file and a member list file, a JSON array of
  `{path, file}` objects whose `file` holds the exact member bytes. Nothing is
  encoded or parsed in MATLAB/Octave, so bytes (including NUL and 0xFF) and array
  shapes reach Core unchanged. `destination` is never overwritten.
- `nirs4all.n4aWrite(..., 'base', base, 'baseSha256', sha)` updates an
  immutable base: it is verified against the independently known SHA-256, never
  modified, and the result is a new container. Members the list omits come from
  the base.
- `nirs4all.n4aRead(archive, directory, 'expectedSha256', sha, 'member', p)`
  extracts into a new directory `member-000000.bin`, `member-000001.bin`, ...
  and the exact `manifest.json` (`'artifact', id` extracts one artifact's
  members). Paths inside the container are never operating-system paths; the
  owner report maps each numbered file to its archive path.
- Relative `file` entries of the member list resolve against the directory that
  contains the list file.
- Both return the owner `report` as decoded by `jsondecode` and `nativeJson`,
  its exact text. The wrappers rely on no report key; use `nativeJson` when exact
  shapes matter.
- Readable is not executable. Writing, updating and reading verify bytes only;
  an opaque host model stays a byte member that is never loaded or run here.
  Whether any operation can execute a container is decided by its owner.
- Inputs are file paths; structs are refused. Refusals keep the native message
  as `nirs4all:N4aRefusal`; an older CLI raises `nirs4all:N4aUnavailable`.

## Requirements

Build and put the `nirs4all-methods` MATLAB/Octave binding on the path first:

```matlab
addpath('/path/to/nirs4all-methods/bindings/matlab')
addpath('/path/to/dag-ml/bindings/matlab')
```

Then add this binding:

```matlab
addpath('/path/to/nirs4all-core/bindings/matlab')
```

## Checks

Parser smoke:

```bash
octave --quiet --eval "run('bindings/matlab/tests/smoke.m')"
```

Canonical `.n4a` V1 operations (needs the real CLI, Methods library and the
fixtures written by the Rust test `n4a_cli` with `N4A_CLI_WRITE_FIXTURES=<dir>`;
without them the script prints an explicit `SKIP` line, and
`NIRS4ALL_REQUIRE_CORE_N4A=1` turns that into a failure):

```bash
NIRS4ALL_CORE_CLI=... N4M_LIBRARY_PATH=... N4M_EXPECTED_LIBRARY_SHA256=... \
NIRS4ALL_N4A_FIXTURE_DIR=<dir> NIRS4ALL_REQUIRE_CORE_N4A=1 \
octave --quiet --eval "run('bindings/matlab/tests/n4a_canonical.m')"
```

Owner-neutral exchange (needs the real CLI and the Core fixture directory
`tests/parity/fixtures/n4a_host_opaque_v1`; no Methods library; Octave only).
Qualification run, with no optional skip:

```bash
NIRS4ALL_REQUIRE_CORE_N4A=1 NIRS4ALL_CORE_CLI=/path/to/nirs4all-core-archive \
NIRS4ALL_N4A_HOST_FIXTURE_DIR=/path/to/n4a_host_opaque_v1 \
octave --quiet --eval "run('bindings/matlab/tests/n4a_exchange.m')"
```

Without `NIRS4ALL_REQUIRE_CORE_N4A=1` a missing prerequisite prints an explicit
`NOT RUN` line and nothing native is asserted. The conda Octave also needs
`OCTAVE_HOME` set to its environment prefix.

Strict execution parity against the full Python `nirs4all` oracle:

```bash
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 make test-matlab-parity
```
