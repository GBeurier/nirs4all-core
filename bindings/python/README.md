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

## Native archive bridge

`nirs4all_core.read_portable_predictor_package_v2(path)` invokes the embedded
Rust Archive V2 reader and returns the exact validated DAG-ML Package V2 bytes.
It does not parse ZIP members in Python, deserialize the package, or execute a
prediction. Pass the returned bytes to DAG-ML's typed package/replay surface;
the aggregate remains only the container and integrity boundary.

`replay_methods_archive_v2(...)` and `replay_methods_archive_v3(...)` provide
the callback-free execution path. Rust validates the complete archive before
DAG-ML parses the signed request and numeric Methods inputs or opens the
invocation-local N4MM runtime. These functions do not accept Python callbacks,
estimator handles, pickle, or joblib sidecars; unsupported host controllers are
refused rather than hydrated implicitly.

For calibrated scalar Package V2 archives,
`replay_methods_archive_v2_conformal_presentation_v1(...)` returns the exact
self-validating presentation built by DAG-ML from the native replay. The
Python layer only transports strict JSON; it does not calculate quantiles,
interval endpoints, fingerprints, or sample joins.

For named multi-target outputs,
`replay_methods_archive_v2_conformal_presentation_v2(...)` returns the
additive, archive-bound `ConformalPresentationV2`. It preserves predictor,
archive, calibration and presentation fingerprints and applies the same
no-recalculation rule. V1 remains the scalar compatibility surface.

Archive replay accepts raw PLS N4MM format 1 and the exact embedded format 2
`SNV(ddof=0) -> Savitzky-Golay(mode=interp) -> PLS` profile. Format 2 requires
its typed ABI 2.5 descriptor and never falls back to Python preprocessing.
Training an IO `DatasetPackage` into Archive V2 is currently a Rust aggregate
surface, not a Python API.

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

## Generic n4m role recipes (trained envelope v8)

Any Methods estimator is a recipe step through the language-neutral token
`"n4m:<catalog method id>"` (or `{"class": "n4m:<id>", "params": {...}}`),
shared with the full Python `nirs4all`, the R package, the Rust binding and the
npm package. `load_pipeline_definition` accepts these tokens when they resolve
in the Methods manifest (`n4m.roles.method_class`); `n4m_role_capabilities()`
lists the usable steps from that manifest.

`N4mRolePipeline` fits such a recipe (sample filters on training rows only,
transformers and selectors, then one regressor or classifier) and reads/writes
the `nirs4all.n4m.trained_pipeline.v8` envelope, so a pipeline trained in any
binding predicts identically here:

```python
import nirs4all_core as n4core

fitted = n4core.N4mRolePipeline.fit_recipe(recipe, X_train, y_train)
fitted.to_json("trained-v8.json")
predictions = n4core.N4mRolePipeline.from_json("trained-v8.json").predict(X_new)
```

It requires `nirs4all-methods` with ABI 2.13 estimator roles (1.0.22 or later).

