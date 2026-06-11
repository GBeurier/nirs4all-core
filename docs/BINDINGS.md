# Binding Contract

## Shared requirements

Every binding must:

- expose upstream domains from the top-level package;
- translate host-native objects to upstream contracts;
- preserve ownership and lifetime rules across FFI boundaries;
- report unavailable upstream components explicitly;
- participate in parity checks before release.

## Python

- Distribution name: `nirs4all-lite`.
- Import name: `nirs4all_lite`.
- Framework idioms: sklearn-style estimators, `fit`/`predict`/`transform`,
  NumPy arrays, pandas data frames, and clear optional extras.
- Do not shadow the full Python `nirs4all` package until the core replacement
  migration is intentional.

## Rust

- Crate name: `nirs4all`.
- Use `Result`-returning APIs and typed wrappers around upstream crates.
- Keep FFI handles explicit; never hide ownership transfers.

## JavaScript/WASM

- npm package name: `nirs4all`.
- Expose typed ESM APIs and browser-safe WASM loaders.
- `nirs4all-web` consumes this package; UI code does not live here.

## R

- Package name: `nirs4all`.
- Provide formula/data-frame paths where appropriate.
- Keep native handles opaque and expose provenance in returned objects.

## MATLAB/Octave

- Namespace: `+nirs4all`.
- Prefer matrices/tables and explicit options structs.
- Keep Octave compatibility in the public subset unless a function is marked
  MATLAB-only.
