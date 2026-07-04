# Binding Contract

## Shared requirements

Every binding must:

- expose upstream domains from the top-level package;
- translate host-native objects to upstream contracts;
- parse the shared `nirs4all` JSON/YAML pipeline definition envelope before
  handing execution to upstream runtimes;
- preserve ownership and lifetime rules across FFI boundaries;
- report unavailable upstream components explicitly;
- expose external operators through host-language idioms only when the upstream
  execution path can actually plan or call them;
- participate in parity checks before release.

For the portable Savitzky-Golay operator, every binding normalizes the same
methods-backed SciPy boundary-mode contract: `mirror=0`, `constant=1`,
`nearest=2`, `wrap=3`, and `interp=4`. The default remains `interp` to match
the full Python nirs4all operator; explicit `mode` and `cval` values must be
preserved in the execution plan and forwarded to the upstream binding.

## Python

- Distribution name: `nirs4all-core` (renamed from `nirs4all-lite`).
- Import name: `nirs4all_lite`.
- Additive import facades (LOCK-GOV, see [naming](NAMING.md)): `n4a` is the
  full brand-aligned aggregate facade (`import n4a`), while `nirs4all_core` is
  the facade matching the `nirs4all-core` distribution name (renamed from
  `nirs4all-lite`). `nirs4all_core.__all__` advertises only inspection,
  validation, capability, release-topology, and facade APIs; legacy execution
  helpers remain reachable through passthrough but are outside the core
  contract.
- Framework idioms: sklearn-style estimators, `fit`/`predict`/`transform`,
  NumPy arrays, pandas data frames, and clear optional extras.
- External operators should look like normal sklearn-compatible transformers or
  estimators when they participate in Python execution.
- Do not shadow the full Python `nirs4all` package until the core replacement
  migration is intentional. The `n4a` / `nirs4all_core` facades are additive and
  intentionally do **not** define a top-level `nirs4all` Python module.
- Keep `release_topology_manifest()` green against the package metadata before
  changing distribution names, facade imports, or the core execution boundary.

## Rust

- Crate name: `nirs4all`.
- Use `Result`-returning APIs and typed wrappers around upstream crates.
- External operators should use traits and typed builder APIs, with capabilities
  declared at compile time or through explicit runtime feature checks.
- Keep FFI handles explicit; never hide ownership transfers.
- The portable KS/SNV/Savitzky-Golay/PLS subset executes through a caller-supplied
  `libn4m` path and is covered by the shared full-Python `nirs4all` oracle.

## JavaScript/WASM

- npm package name: `nirs4all`.
- Expose typed ESM APIs and browser-safe WASM loaders.
- External operators should be ESM functions/classes over browser-safe values
  and `TypedArray` data, with async initialization where WASM is required.
- The portable KS/SNV/Savitzky-Golay/PLS subset executes with
  `runPortablePipeline()` and predicts from its serialized selected model with
  `predictPortablePipeline()`, both delegating to `@nirs4all/methods-wasm`.
- `nirs4all-web` consumes this package; UI code does not live here.
- Current upstream package candidates are `@nirs4all/formats-wasm`,
  `@nirs4all/io-wasm`, `@nirs4all/datasets-wasm`, `dag-ml-wasm`,
  `dag-ml-data-wasm`, and `@nirs4all/methods-wasm`.

## R

- Package name: `nirs4all`.
- Provide formula/data-frame paths where appropriate.
- External operators should expose S3 generics and formula/data-frame adapters
  where that is the natural R interface.
- Keep native handles opaque and expose provenance in returned objects.
- Current R package candidates include `nirs4allformats`, `nirs4allio`,
  `nirs4alldatasets`, `dagmldata`, and `n4m` / `pls4all` for methods. `dag-ml`
  has no declared R binding yet and remains unavailable in the R aggregate.

## MATLAB/Octave

- Namespace: `+nirs4all`.
- Prefer matrices/tables and explicit options structs.
- External operators should use function handles or small handle classes with
  `fit`/`predict`/`transform`-style methods when execution support exists.
- Keep Octave compatibility in the public subset unless a function is marked
  MATLAB-only.
- The portable KS/SNV/Savitzky-Golay/PLS subset executes through
  `nirs4all.runPortablePipeline()` by delegating to the `nirs4all-methods`
  `+pls4all` MEX shims. The aggregate binding still owns only parsing,
  orchestration, and result-shape translation.
