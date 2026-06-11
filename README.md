# nirs4all-lite

`nirs4all-lite` is the low-level, portable nirs4all distribution. It aggregates:

- `dag-ml`
- `dag-ml-data`
- `nirs4all-formats`
- `nirs4all-io`
- `nirs4all-datasets`
- `nirs4all-methods`

It must not add independent numerical, parsing, or pipeline logic. The upstream
projects stay the source of truth; this repository provides a canonical package
surface, native bindings, release glue, and parity checks.

## Package names

| Target | External name | Import/module name |
| --- | --- | --- |
| Python | `nirs4all-lite` | `nirs4all_lite` |
| Rust | `nirs4all` | `nirs4all` |
| JavaScript/WASM | `nirs4all` | `nirs4all` |
| R | `nirs4all` | `library(nirs4all)` |
| MATLAB/Octave | `nirs4all` | `+nirs4all` namespace |

Python keeps the `nirs4all-lite` distribution name to avoid colliding with the
full Python `nirs4all` library. Other language bindings use `nirs4all`.

## Public surface

Every binding exposes the upstream domains directly:

- `formats`
- `io`
- `datasets`
- `methods`
- `dag_ml`
- `dag_ml_data`

Pipelines built by `nirs4all-lite` are expected to compose those domains, not
reimplement them. For example, a binding should make it possible to reach the
formats and methods layers from the top-level `nirs4all` package.

## Repository layout

```text
bindings/
  python/      # Python distribution: nirs4all-lite
  rust/        # Rust crate: nirs4all
  wasm/        # npm/WASM package: nirs4all
  r/           # R package skeleton: nirs4all
  matlab/      # MATLAB/Octave namespace skeleton
compat/        # Upstream registry and compatibility metadata
docs/          # Architecture, binding, parity, and release contracts
tests/parity/  # Cross-runtime parity fixture plan
```

## Current status

This repository is the initial split scaffold. It contains testable binding
manifests and upstream loader surfaces, but not yet the compiled upstream
artifacts or cross-language release builds.

## Local checks

```bash
cargo test --workspace
PYTHONPATH=bindings/python/src python -m unittest discover -s bindings/python/tests
npm test --prefix bindings/wasm
```

R and MATLAB/Octave skeletons are intentionally metadata-only until packaging
and runtime gates are wired.
