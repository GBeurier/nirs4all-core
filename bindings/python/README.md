# Python Binding

Distribution name: `nirs4all-core`

Import name: `nirs4all_core`

This binding intentionally avoids the `nirs4all` import name so it can be
installed next to the full Python `nirs4all` package during parity checks.
The canonical source repository is `nirs4all-core`; only the Python
distribution carries the `-core` suffix because the production `nirs4all`
Python package already owns the bare name.

An additive import facade is available for governed topology work:

- `n4a` mirrors the full `nirs4all_core` aggregate surface.

## Archive V2 bridge

`nirs4all_core.read_portable_predictor_package_v2(path)` invokes the embedded
Rust Archive V2 reader and returns the exact validated DAG-ML Package V2 bytes.
It does not parse ZIP members in Python, deserialize the package, or execute a
prediction. Pass the returned bytes to DAG-ML's typed package/replay surface;
the aggregate remains only the container and integrity boundary.

## Portable Execution

`nirs4all_core.run_portable_pipeline(source, dataset)` executes the shared
portable JSON/YAML subset through the `nirs4all-methods` Python bindings:

- `KennardStoneSplitter`
- `StandardNormalVariate` / `SNV`
- `SavitzkyGolay`
- `sklearn.cross_decomposition.PLSRegression`
- `_range_` sweeps over `n_components`

Savitzky-Golay defaults to `mode="interp"` for full Python nirs4all parity and
preserves explicit methods-backed modes (`mirror`, `constant`, `nearest`,
`wrap`, `interp`) plus `cval`.

The aggregate does not implement numerical kernels. Install the optional
methods extra, or make `n4m` and `pls4all` importable, before calling it:

```bash
python -m pip install "nirs4all-core[methods]"
```

The strict local parity gate compares all shared fixtures against the full
Python `nirs4all` oracle and reports max prediction/RMSE deltas on failure:

```bash
PYTHONPATH=bindings/python/src:/path/to/nirs4all-methods/bindings/python/src \
N4M_LIB_PATH=/path/to/libn4m.so \
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 \
python -m unittest bindings/python/tests/test_execution_parity.py -v
```
