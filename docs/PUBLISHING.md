# Publishing Checklist

Package names to reserve or create:

| Target | Registry | Name | Artifact |
| --- | --- | --- | --- |
| Python | PyPI | `nirs4all-lite` | wheel + sdist from `bindings/python` |
| JavaScript/WASM | npm | `nirs4all` | npm tarball from `bindings/wasm` |
| Rust | crates.io | `nirs4all` | crate from `bindings/rust/nirs4all` |
| R | CRAN / R-universe | `nirs4all` | source tarball from `bindings/r` |
| MATLAB/Octave | GitHub Releases | `nirs4all-matlab-octave-<version>.zip` | zip from `bindings/matlab` |

## Python / PyPI

Build:

```bash
python -m pip install build twine
python -m build bindings/python --outdir dist/python
python -m twine check dist/python/*
```

Publish after the PyPI project `nirs4all-lite` exists and trusted publishing or
an API token is configured:

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

## R / CRAN

Build and check:

```bash
mkdir -p dist/r
cd dist/r && R CMD build ../../bindings/r && cd ../..
R CMD check --as-cran dist/r/nirs4all_*.tar.gz
```

Submit the resulting `nirs4all_<version>.tar.gz` through CRAN's submission
workflow. `nirs4all` is an aggregate package, so CRAN submission should wait
until every declared CRAN dependency is either on CRAN or moved to Suggests /
runtime discovery.

## R-universe

R-universe is configured through `GBeurier/GBeurier.r-universe.dev`.
`packages.json` tracks this package as:

```json
{
  "package": "nirs4all",
  "url": "https://github.com/GBeurier/nirs4all-lite",
  "subdir": "bindings/r"
}
```

Publish only after `R CMD check` is green in CI.

## MATLAB/Octave

Build:

```bash
scripts/build-matlab-package.sh dist/matlab
```

Attach the zip to the GitHub Release. A `.mltbx` can be added later when the
MATLAB toolbox metadata is ready; the current portable artifact is Octave-safe.
