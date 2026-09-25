# Publishing Checklist

Package names to reserve or create:

| Target | Registry | Name | Artifact | Workflow |
| --- | --- | --- | --- | --- |
| Python | PyPI | `nirs4all-core` | wheel + sdist from `bindings/python` | `release-python.yml` |
| JavaScript/WASM | npm | `nirs4all` | npm package from `bindings/wasm` | `release-npm.yml` |
| Rust | crates.io | `nirs4all` | crate from `bindings/rust/nirs4all` | `release-crates.yml` |
| MATLAB/Octave | GitHub Releases | `nirs4all-matlab-octave-<version>.zip` | zip from `bindings/matlab` | `release-matlab.yml` |
| Source + SBOM | GitHub Releases | `nirs4all-core-<version>-src.*` | git-archive + CycloneDX + SHA256SUMS | `release-source.yml` |

All artifacts in this table are cut from `GBeurier/nirs4all-core`. The R
package `nirs4all` is published separately from `GBeurier/nirs4all-r`.

The core-owned non-Python `nirs4all` publications are still bindings of the
`nirs4all-core` aggregate. Their packaging and release notes must describe them
as target-language surfaces over the shared upstream packages and
`nirs4all-methods`, never as independent full `nirs4all` implementations with
duplicated parsing, IO, orchestration, or numerical logic.

## How releases are cut

The single source of truth for the version is the **Rust crate**
(`bindings/rust/nirs4all/Cargo.toml`); `scripts/bump_version.sh` propagates it to
the Python and npm manifests (with the spelling each ecosystem needs) and
`scripts/bump_version.sh --check` fails CI on drift.

On a **non-pre-release tag `vX.Y.Z`** the core release workflows run and:

* publish **PyPI `nirs4all-core`** via **OIDC Trusted Publishing** (GitHub
  environment `pypi`, `id-token: write` — no API token),
* publish **npm `nirs4all`** (needs the `NPM_TOKEN` secret),
* publish **crates.io `nirs4all`** (needs the `CARGO_REGISTRY_TOKEN` secret),
* build + attach the **MATLAB/Octave** zip and the **source + SBOM** bundle to
  the GitHub Release.

Across those registry publications, the architecture stays fixed: Python uses
the `nirs4all-core` distribution name because the full Python `nirs4all`
package already owns the bare name, while Rust/npm/MATLAB publish as
`nirs4all` for their host ecosystems but still consume the same aggregate lock
and upstream engines.

A **pre-release tag** (anything with a `-`, e.g. `v0.1.0-alpha.1`) builds and
attaches artifacts but **publishes to no registry**. `workflow_dispatch` runs
every workflow in dry-run mode (build/validate only).

The PyPI Trusted Publisher must be created once by the maintainer for project
`nirs4all-core` with: owner `GBeurier`, repo `nirs4all-core`, workflow
`release-python.yml`, environment `pypi`. No public legacy alias release is
part of the RC target.

`nirs4all-datasets` is **external/optional everywhere** and is never bundled: a
Python extra (`nirs4all-core[datasets]`, excluded from `[all]`), an optional npm peer
dependency, and an off-by-default Cargo feature (`datasets`).

## Python / PyPI

Build:

```bash
python -m pip install build twine
python -m build bindings/python --outdir dist/python
python -m twine check dist/python/*
```

Manual fallback publish only after the PyPI project `nirs4all-core` exists and
the Trusted Publisher is configured; the normal release path remains
`release-python.yml` via OIDC:

```bash
python -m twine upload dist/python/*
```

## npm

Build:

```bash
npm test --prefix bindings/wasm
npm pack ./bindings/wasm --pack-destination dist/npm
```

Publish after the npm package `nirs4all` is owned by the project:

```bash
npm publish dist/npm/nirs4all-*.tgz --access public
```

## Rust / crates.io

Validate:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo package -p nirs4all
```

Publish after the crate name `nirs4all` is reserved:

```bash
cargo publish -p nirs4all
```

## R / CRAN / R-universe

`nirs4all-r` owns the R source package, version, CRAN submission materials,
R-universe entry, tests, and release workflow. Core retains shared portable
JSON/YAML fixtures under `tests/parity`; it no longer publishes or versions an
R package. See `GBeurier/nirs4all-r` for the build and check commands.

## MATLAB/Octave

Build:

```bash
scripts/build-matlab-package.sh dist/matlab
```

`release-matlab.yml` runs `make test-matlab-parity` against the pinned
`nirs4all-methods` ref before attaching the zip to the GitHub Release. A
`.mltbx` can be added later when the MATLAB toolbox metadata is ready; the
current portable artifact is Octave-safe.
