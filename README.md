<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/horizontal-dark.svg">
    <img alt="nirs4all-core" src="assets/brand/horizontal.svg" width="440">
  </picture>
</p>

# nirs4all-core

`nirs4all-core` is the portable aggregate publication of the low-level
nirs4all stack. It aggregates:

- `dag-ml`
- `dag-ml-data`
- `nirs4all-formats`
- `nirs4all-io`
- `nirs4all-datasets`
- `nirs4all-methods`

It must not add independent numerical, parsing, or pipeline logic. The upstream
projects stay the source of truth; this repository provides the canonical
aggregate package surface, native bindings, release glue, and parity checks.
The core-owned non-Python package names stay `nirs4all`; the R product is now
maintained in the separate `nirs4all-r` repository. All bindings consume shared
upstream packages rather than duplicating parsers, IO, orchestration, or kernels.

## Package names

| Target | External name | Import/module name |
| --- | --- | --- |
| Python | `nirs4all-core` | `nirs4all_core` |
| Rust | `nirs4all` | `nirs4all` |
| JavaScript/WASM | `nirs4all` | `nirs4all` |
| R (external product) | `nirs4all` | `library(nirs4all)` from `nirs4all-r` |
| MATLAB/Octave | `nirs4all` | `+nirs4all` namespace |

The Python distribution is `nirs4all-core`; it cannot use the bare `nirs4all`
name because the full Python `nirs4all` library owns it. Other language
bindings use `nirs4all`. The canonical Python import root is
`nirs4all_core`.

The Rust crate, npm package, and MATLAB/Octave namespace named `nirs4all`
remain core-owned release identities. The R product has its own lifecycle in
`nirs4all-r`, while sharing DAG-ML and Methods contracts.

The canonical source repository for core-owned artifacts is
`GBeurier/nirs4all-core`; `GBeurier/nirs4all-r` owns the R package.

That shared non-Python name is a packaging identity, not a claim that every
upstream domain has a runtime binding in every language. The full six-domain
aggregate is recorded as metadata and exposed through re-export/load hooks where
the host ecosystem has a real upstream package.

The Python aggregate also exposes the **additive, non-shadowing** brand facade
`n4a` (`import n4a`; see [`docs/NAMING.md`](docs/NAMING.md)). It re-exports
`nirs4all_core` verbatim and adds no behavior.

## Public surface

The aggregate registry tracks the same upstream domains everywhere:

- `formats`
- `io`
- `datasets`
- `methods`
- `dag_ml`
- `dag_ml_data`

Pipelines built by `nirs4all-core` are expected to compose those domains, not
reimplement them. For example, a binding should make it possible to reach the
formats and methods layers from the top-level `nirs4all` package when matching
upstream runtime bindings are installed. Domains without a host binding remain
metadata-only and must fail explicitly if requested as executable capabilities.

Current runtime coverage is intentionally uneven: JavaScript/WASM records npm
peer candidates for every domain; the separate R product reaches upstream R
packages, including the `dagml` process-local loss/metric registry; MATLAB/Octave
has runtime candidates for DAG-ML local registries through `+dagml` and methods
through `+n4m`, while `dag_ml_data`, `formats`, `io`, and `datasets` remain
metadata-only.

External operator support must stay execution-gated. When an upstream executor
can plan or call an external operator, a binding may add an idiomatic host
adapter for that operator. Those adapters are future/gated work, not a current
availability claim. Until the execution path exists, bindings must report the
capability as unavailable instead of shipping a fake local implementation. See
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
MATLAB/Octave execution path delegates to the upstream `+n4m` MEX shims and
is strict-parity gated in CI. See [`docs/PARITY.md`](docs/PARITY.md).

## Repository layout

```text
bindings/
  python/      # Python distribution: nirs4all-core
  rust/        # Rust crate: nirs4all
  wasm/        # npm/WASM package: nirs4all
  matlab/      # MATLAB/Octave namespace and portable execution facade
compat/        # Upstream registry and compatibility metadata
docs/          # Architecture, binding, parity, and release contracts
tests/parity/  # Cross-runtime parity fixture plan
```

## Current status

This repository is a buildable aggregate with a bounded native Archive V2
slice. Rust, Python, and JavaScript/WASM can validate and replay portable
Methods archives according to their documented binding limits: raw PLS uses
N4MM format 1, while the embedded
`SNV(ddof=0) -> Savitzky-Golay(mode=interp) -> PLS` pipeline uses format 2 and
ABI 2.5. Rust can also train one selected dense IO `DatasetPackage` source into
Archive V2 and produce a calibrated archive from a disjoint package; Rust and
Python expose the identity-bound multi-target conformal presentation V2 without
recalculating calibration in the host.

The broader aggregate surface remains intentionally uneven across Python, npm,
MATLAB/Octave, and Rust. Runtime execution is limited to upstream bindings
that actually exist in each host; numerical and parsing behavior stays
delegated to those upstream packages, and `nirs4all-core` does not vendor or
reimplement their engines.

## Local checks

```bash
make test-v1-surfaces
make test
cargo test --workspace
PYTHONPATH=bindings/python/src python -m unittest discover -s bindings/python/tests
npm test --prefix bindings/wasm
```

`make test-v1-surfaces` covers the core-owned Python, Rust, JavaScript/WASM,
and MATLAB/Octave surfaces. The R product runs its checks in `nirs4all-r`.

Strict Python-vs-full-`nirs4all` execution parity needs local
`nirs4all-methods` Python bindings and libn4m:

```bash
PYTHONPATH=bindings/python/src:/path/to/nirs4all-methods/bindings/python/src \
PLS4ALL_LIB_PATH=/path/to/libn4m.so \
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 \
python -m unittest bindings/python/tests/test_execution_parity.py -v
```

Strict Rust-vs-full-`nirs4all` execution parity needs a local libn4m build:

```bash
NIRS4ALL_METHODS_LIB=/path/to/libn4m.so \
LD_LIBRARY_PATH=/path/to/libn4m-directory \
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 \
cargo test -p nirs4all rust_binding_execution_matches_full_python_nirs4all_oracle -- --nocapture
```

Strict R-vs-Python parity is tested in `nirs4all-r` against the portable
JSON/YAML fixtures retained under `tests/parity` here.

Strict MATLAB/Octave-vs-full-`nirs4all` execution parity needs the
`nirs4all-methods` `+n4m` MEX shims on the Octave/MATLAB path:

```bash
make test-matlab-parity
```

`make build` produces core-owned language artifacts when the required toolchains
are installed. R checks and publication run in `nirs4all-r`.

## License

`nirs4all-core` is dual-licensed open-source — **`CECILL-2.1 OR AGPL-3.0-or-later`** (your choice) —
with an optional **commercial license** for closed-source / SaaS use. For any commercial use, contact
<nirs4all-admin@cirad.fr>. As an aggregate it re-exports sibling libraries that carry their own
licenses (the sibling crates currently use CECILL-2.1 OR AGPL-3.0-or-later).
See [`LICENSING.md`](LICENSING.md), [`LICENSES/`](LICENSES/), and [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
