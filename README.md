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

External operator support must stay execution-gated. When an upstream executor
can plan or call an external operator, each language binding should expose that
operator through native host idioms rather than a raw lowest-common-denominator
API. Until the execution path exists, bindings must report the capability as
unavailable instead of shipping a fake local implementation. See
[`docs/OPERATORS.md`](docs/OPERATORS.md).

## Pipeline definitions

The lightweight parser accepts the same definition envelope as the full Python
`nirs4all.pipeline.PipelineConfigs`: a direct list of steps, a mapping with
`pipeline`, a mapping with `steps`, a JSON/YAML path, or JSON/YAML text. The
current portable fixture uses the nirs4all examples syntax for Kennard-Stone,
SNV, Savitzky-Golay, and a PLS `n_components` sweep via `_range_`/`param`.
Python, Rust, JavaScript/WASM, R, and MATLAB/Octave expose this parser contract.

This is a syntax contract only. Numerical execution and lite-vs-full-`nirs4all`
equivalence still require the parity gates described in
[`docs/PARITY.md`](docs/PARITY.md).

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

This repository is now a buildable aggregate scaffold. It exposes the upstream
domains in each target language, builds package artifacts for Python, npm,
R, MATLAB/Octave, and Rust, and wires CI gates for those targets. The numerical
and parsing behavior is still delegated to the upstream packages; `nirs4all-lite`
does not vendor or reimplement those engines.

## Local checks

```bash
make test
cargo test --workspace
PYTHONPATH=bindings/python/src python -m unittest discover -s bindings/python/tests
npm test --prefix bindings/wasm
```

`make build` produces the language artifacts when the required toolchains are
installed. R and MATLAB/Octave checks require local R/Octave installations; CI
also runs those gates.
