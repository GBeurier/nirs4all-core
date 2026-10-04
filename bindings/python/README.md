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

`read_archive_v2_payloads(path)` returns the native-validated `manifest` and
exact opaque `members` (a path-to-bytes mapping). Archive V2 also transports
the explicitly declared RAW Methods RolePipeline family: signed DAG-ML
packages retain every captured N4ME state, binding and controller trust
requirement in SHA-addressed `artifacts/<sha256>.json` members. Existing N4MM
archives retain their original profile and declarations.

Storage validation does not authorize a controller or interpret the models.
Pass this inventory to DAG-ML's `validate_archive_v2_portable_payloads` before
using its replay driver, with explicit trusted manifests and a signed current
cohort. The full Python SDK exposes `write_portable_predictor_archive_v2`,
`read_portable_predictor_archive_v2` and `replay_portable_predictor_archive_v2`
for that composition. This adds portable transport, not N-D encoders or a
generalization of the existing callback-free N4MM replay functions.

## Archive V3 host view

`nirs4all_core.read_archive_v3_view(path)` invokes the Rust Archive V3 reader
and returns validated replay references and N4MM inventory. It does not parse
ZIP members in Python or execute a replay.

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
transformers and selectors, then one regressor or classifier) in the native
Methods role pipeline (`n4m.roles.RolePipeline`) and reads/writes the
`nirs4all.n4m.trained_pipeline.v8` envelope, so a pipeline trained in any
binding predicts identically here:

```python
import nirs4all_core as n4core

fitted = n4core.N4mRolePipeline.fit_recipe(recipe, X_train_frame, y_train)
fitted.to_json("trained-v8.json")
predictions = n4core.N4mRolePipeline.from_json("trained-v8.json").predict(X_new_frame)
```

Every target column reaches the steps that need `y`. DataFrame column names are
stored (`feature_names` in the envelope) and a DataFrame with renamed or
reordered columns is refused; arrays are positional. The native import refuses
states that contradict the recipe. A state that embeds training rows (kernel
PLS, LW-PLS, ...) is written only with `to_json(..., allow_training_rows=True)`
and flagged `contains_training_rows`. Envelopes written before these two fields
still load. The import also refuses an `n_features` that is not a positive JSON
integer equal to the native width, a column name holding NUL, and a
`class_names` table that is not a non-empty list of unique strings or finite
numbers labelling every fitted class id (index = id); missing or non-finite
labels are refused at fit. `recipe` is a copy of the recipe the states attest,
which is also the one exported. Core 0.4.1's Methods extra requires
`nirs4all-methods` 1.3.2 or later (ABI 2.17). The role envelope's native format
remains compatible with ABI 2.14.
