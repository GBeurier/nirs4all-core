# Host Compatibility

## Python

Expose sklearn-compatible estimators and transformers where a pipeline component
behaves like one. Accept NumPy arrays and pandas data frames at API boundaries.
Keep optional dependencies optional.

## R

Support data frames and formula-style entry points for workflows that naturally
map to R modeling conventions. Preserve provenance in returned S3/S4 objects.

## MATLAB/Octave

Use matrices, tables, and options structs. Avoid MATLAB-only APIs in shared
functions unless a fallback or clear error is provided for Octave.

## JavaScript/WASM

Expose typed ESM APIs, browser-safe WASM loading, and serializable pipeline
descriptors. `nirs4all-web` should depend on these APIs rather than embedding
core logic.

## Rust

Expose typed wrappers and traits over the upstream crates. Use explicit errors
for missing optional components.
