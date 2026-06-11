# Repository Guidelines

## Scope

`nirs4all-lite` is the canonical portable aggregate of the low-level nirs4all
stack. It wraps and re-exports `dag-ml`, `dag-ml-data`, `nirs4all-formats`,
`nirs4all-io`, `nirs4all-datasets`, and `nirs4all-methods`.

Do not implement new parsers, numerical methods, dataset loaders, or ML
coordinators here. Add that work to the owning upstream project, then expose it
through this aggregate.

## Structure

- `bindings/python`: Python distribution named `nirs4all-lite`.
- `bindings/rust`: Rust crate named `nirs4all`.
- `bindings/wasm`: npm/WASM package named `nirs4all`.
- `bindings/r`: R package skeleton named `nirs4all`.
- `bindings/matlab`: MATLAB/Octave `+nirs4all` namespace.
- `docs`: binding, parity, compatibility, and release contracts.
- `compat`: machine-readable upstream registry.

## Checks

Run the checks matching the touched area:

- `cargo test --workspace`
- `PYTHONPATH=bindings/python/src python -m unittest discover -s bindings/python/tests`
- `npm test --prefix bindings/wasm`

When parity fixtures are added, run the native-vs-binding and
lite-vs-python-`nirs4all` parity suites before publishing.

## Boundaries

- The native source of truth for PLS/NIRS methods is `nirs4all-methods`.
- File parsing belongs to `nirs4all-formats`.
- Dataset assembly belongs to `nirs4all-io`.
- Dataset catalog metadata belongs to `nirs4all-datasets`.
- Graph planning and leakage-safe ML orchestration belong to `dag-ml` and
  `dag-ml-data`.

Bindings translate idiomatic host objects to those upstream contracts only.
