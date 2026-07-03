<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/horizontal-dark.svg">
    <img alt="nirs4all-core" src="assets/brand/horizontal.svg" width="440">
  </picture>
</p>

# nirs4all-core

`nirs4all-core` (formerly `nirs4all-lite`) is the low-level, portable nirs4all
distribution. It aggregates:

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
| Python | `nirs4all-core` | `nirs4all_lite` |
| Rust | `nirs4all` | `nirs4all` |
| JavaScript/WASM | `nirs4all` | `nirs4all` |
| R | `nirs4all` | `library(nirs4all)` |
| MATLAB/Octave | `nirs4all` | `+nirs4all` namespace |

The Python distribution is `nirs4all-core` (RC V1 rename from `nirs4all-lite`);
it cannot use the bare `nirs4all` name because the full Python `nirs4all`
library owns it. Other language bindings use `nirs4all`. The canonical Python
import root stays `nirs4all_lite` for compatibility.

The Python aggregate also exposes two **additive, non-shadowing** import facades
(see [`docs/NAMING.md`](docs/NAMING.md)): the brand root `n4a` (`import n4a`) and
the `nirs4all_core` alias matching the distribution name. Both re-export
`nirs4all_lite` verbatim and add no behavior.

## Public surface

Every binding exposes the upstream domains directly:

- `formats`
- `io`
- `datasets`
- `methods`
- `dag_ml`
- `dag_ml_data`

Pipelines built by `nirs4all-core` are expected to compose those domains, not
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
current portable fixtures use the nirs4all examples syntax for Kennard-Stone,
SNV, Savitzky-Golay, and a PLS `n_components` sweep via `_range_`/`param`.
Python, Rust, JavaScript/WASM, R, and MATLAB/Octave expose this parser contract.
Savitzky-Golay keeps the full Python nirs4all default boundary behavior
(`mode: "interp"`) and also preserves explicit methods-backed SciPy modes
(`mirror`, `constant`, `nearest`, `wrap`, `interp`) plus `cval`.

JavaScript/WASM, Python, Rust, R, and MATLAB/Octave execute the initial portable
subset through `nirs4all-methods` and compare the same four JSON/YAML fixtures
against the full Python `nirs4all` oracle. The JavaScript/WASM binding
additionally returns a serialized PLS model and exposes
`predictPortablePipeline()` so browser clients can reuse the selected portable
pipeline without reimplementing the preprocessing or prediction path. The
MATLAB/Octave execution path delegates to the upstream `+pls4all` MEX shims and
is strict-parity gated in CI. See [`docs/PARITY.md`](docs/PARITY.md).

## Repository layout

```text
bindings/
  python/      # Python distribution: nirs4all-core
  rust/        # Rust crate: nirs4all
  wasm/        # npm/WASM package: nirs4all
  r/           # R package skeleton: nirs4all
  matlab/      # MATLAB/Octave namespace and portable execution facade
compat/        # Upstream registry and compatibility metadata
docs/          # Architecture, binding, parity, and release contracts
tests/parity/  # Cross-runtime parity fixture plan
```

## Current status

This repository is now a buildable aggregate scaffold. It exposes the upstream
domains in each target language, builds package artifacts for Python, npm,
R, MATLAB/Octave, and Rust, and wires CI gates for those targets. The numerical
and parsing behavior is still delegated to the upstream packages; `nirs4all-core`
does not vendor or reimplement those engines.

## Local checks

```bash
make test-v1-surfaces
make test
cargo test --workspace
PYTHONPATH=bindings/python/src python -m unittest discover -s bindings/python/tests
npm test --prefix bindings/wasm
```

`make test-v1-surfaces` is the public V1 surface gate for Python, R, and
JavaScript/WASM. It runs the Python unittest suite, the WASM npm tests, and the
R surface/upstream/pipeline checks when `R` and `Rscript` are installed; local
workstations without them print a skip/risk message instead. `make test-r` is
the separate local `R CMD check --no-manual bindings/r` gate.

Strict Python-vs-full-`nirs4all` execution parity needs local
`nirs4all-methods` Python bindings and libn4m:

```bash
PYTHONPATH=bindings/python/src:/path/to/nirs4all-methods/bindings/python/src \
PLS4ALL_LIB_PATH=/path/to/libn4m.so \
NIRS4ALL_LITE_REQUIRE_METHODS_PARITY=1 \
python -m unittest bindings/python/tests/test_execution_parity.py -v
```

Strict Rust-vs-full-`nirs4all` execution parity needs a local libn4m build:

```bash
NIRS4ALL_METHODS_LIB=/path/to/libn4m.so \
LD_LIBRARY_PATH=/path/to/libn4m-directory \
NIRS4ALL_LITE_REQUIRE_METHODS_PARITY=1 \
cargo test -p nirs4all rust_binding_execution_matches_full_python_nirs4all_oracle -- --nocapture
```

Strict R-vs-full-`nirs4all` execution parity needs an installed `n4m` R binding
with the portable preprocessing and splitter surface:

```bash
make test-r-parity
```

Strict MATLAB/Octave-vs-full-`nirs4all` execution parity needs the
`nirs4all-methods` `+pls4all` MEX shims on the Octave/MATLAB path:

```bash
make test-matlab-parity
```

`make build` produces the language artifacts when the required toolchains are
installed. R and MATLAB/Octave checks require local R/Octave installations; CI
also runs those gates.

## License

`nirs4all-core` is dual-licensed open-source — **`CeCILL-2.1 OR AGPL-3.0-or-later`** (your choice) —
with an optional **commercial license** for closed-source / SaaS use. For any commercial use, contact
<nirs4all-admin@cirad.fr>. As an aggregate it re-exports sibling libraries that carry their own
licenses (the sibling crates currently use CeCILL-2.1 OR AGPL-3.0-or-later).
See [`LICENSING.md`](LICENSING.md), [`LICENSES/`](LICENSES/), and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
