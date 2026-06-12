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

## Python

- Distribution name: `nirs4all-lite`.
- Import name: `nirs4all_lite`.
- Framework idioms: sklearn-style estimators, `fit`/`predict`/`transform`,
  NumPy arrays, pandas data frames, and clear optional extras.
- External operators should look like normal sklearn-compatible transformers or
  estimators when they participate in Python execution.
- Do not shadow the full Python `nirs4all` package until the core replacement
  migration is intentional.

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
- Current upstream package candidates are `nirs4all-formats-wasm`,
  `nirs4all-io-wasm`, `@nirs4all/datasets-wasm`, `dag-ml-wasm`,
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
- Current MATLAB/Octave support is parser/registry only for the portable
  KS/SNV/Savitzky-Golay/PLS subset; it must not claim execution parity until
  those upstream method calls are exposed without reimplementing kernels.
