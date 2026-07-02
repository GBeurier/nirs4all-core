# Installation

`nirs4all-core` ships one aggregate surface across five host languages. Each
binding installs through that language's native registry and delegates numerical,
parsing, and dataset work to the upstream packages. Install only the upstream
extras you need — the aggregate itself adds no engines.

:::{note}
`nirs4all-datasets` is **external and optional everywhere**. It is never bundled
into the default aggregate; opt in explicitly per binding (see each section
below).
:::

## Python

Distribution name `nirs4all-core`, imported as `nirs4all_lite`.

```bash
pip install nirs4all-core
```

The base install pulls in only `PyYAML`. The upstream engines are optional
extras, so you choose what to bring in:

```bash
# Individual upstreams
pip install "nirs4all-core[methods]"   # nirs4all-methods + scikit-learn
pip install "nirs4all-core[formats]"   # nirs4all-formats
pip install "nirs4all-core[io]"        # nirs4all-io
pip install "nirs4all-core[dag-ml]"    # dag-ml
pip install "nirs4all-core[dag-ml-data]"

# Bundled aggregate = methods + formats + io + dag-ml + dag-ml-data
pip install "nirs4all-core[all]"

# Datasets is excluded from [all]; opt in explicitly
pip install "nirs4all-core[datasets]"

# Everything, including the optional datasets catalog
pip install "nirs4all-core[everything]"
```

Requires Python 3.11 or newer.

## Rust

Crate name `nirs4all` (published from `bindings/rust/nirs4all`).

```bash
cargo add nirs4all
```

The bundled aggregate is `methods + formats + io + dag-ml + dag-ml-data`. The
`nirs4all-datasets` surface is gated behind an off-by-default Cargo feature that
only un-gates the datasets API (it pulls in no extra compiled dependency):

```toml
[dependencies]
nirs4all = { version = "0.1", features = ["datasets"] }
```

## JavaScript / WASM

npm package name `nirs4all` (published from `bindings/wasm`).

```bash
npm install nirs4all
```

It exposes typed ESM APIs and browser-safe WASM loaders, and delegates execution
to the `nirs4all-methods` WASM artifact. `nirs4all-web` consumes this package; UI
code does not live here.

## R

Package name `nirs4all` (built from `bindings/r`). The upstream ecosystem
bindings are not on mainstream CRAN yet, so the natural channel today is
**R-universe**:

```r
install.packages(
  "nirs4all",
  repos = c(
    "https://gbeurier.r-universe.dev",
    "https://cloud.r-project.org"
  )
)
```

It is a pure-R package (no compilation) that `Imports` only `jsonlite` and
`yaml`. The upstream bindings (`nirs4allformats`, `nirs4allio`,
`nirs4alldatasets`, `n4m`, `dagmldata`) are `Suggests`, resolved from R-universe
via `Additional_repositories`.

## MATLAB / Octave

The MATLAB/Octave binding ships the `+nirs4all` namespace as a zip attached to
the GitHub Release (`nirs4all-matlab-octave-<version>.zip`). Unzip it and add the
directory to your path:

```matlab
addpath('/path/to/nirs4all-matlab-octave')
```

The public subset is Octave-safe. Strict-parity execution additionally requires
the `nirs4all-methods` `+pls4all` MEX shims on the MATLAB/Octave path.
