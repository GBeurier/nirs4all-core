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

`open_native_package(path, *, entrypoint=None, operation="predict-external",
memory_budget_bytes=None)` retains the selected Rust proof for Package2/3.
Readers and replay adapters accept a path, retained `N4aFile` or `NativePackage`.
`read_archive_v2_payloads` returns the exact package/outcome bytes, selected
verified member map and neutral Core V1 manifest/view; logical package URIs are
not physical member paths. Core validates the selected DAG scientific closure,
while runtime controller trust and execution remain explicit owner operations.

`write_native_predictor_package(path, outcome_bytes, package_bytes,
container_id=...)` and `write_native_refit_package(path, package_bytes,
container_id=...)` publish new Core V1 containers from exact DAG root bytes.
They preserve all supported native state families and refuse replacing an
existing destination. Package3 child storage invents neither parent evidence nor
scores. Calibration uses the immutable-base native operation and retains
unrelated content. There is no old physical writer or runtime conversion path.

## Owner-neutral `.n4a` exchange

Core puts no content-size cap on a `.n4a`. The only bound is the optional
`memory_budget_bytes=` of a call: the working memory you authorize for Core's
own allocations, never the payload that stays on disk. `None` (the default)
means only the allocator and the host constrain the call; it is not a claim
about machine RAM, and on an overcommitting Linux host a hard guarantee needs an
explicit budget or cgroup/ulimit limits. A budget below the 4096-byte streaming
chunk is refused. Allocations made by IO, DAG and Methods inside a native action
are outside this budget. Every refusal (budget, corruption, changed file) is a
`ValueError`.

Two handles, one validated Core index, no Python ZIP code:

- **File-backed (large archives).** `open_n4a_file(path, *, memory_budget_bytes=None)`
  verifies the whole file in one sequential pass and keeps only the index in
  memory. The returned `N4aFile` has `view()`, `manifest_bytes()`,
  `artifact_member_paths(id)`, `member(path)`, `open_member(path)`,
  `copy_member_to(path, dest)`, `copy_archive_to(dest)` and `close()` (it is a
  context manager). `open_member` returns a non-seekable binary stream with
  `read`/`readinto`, so `io.BufferedReader(stream)` works. Nothing is trusted from
  the open: the file is not locked, so each read hashes the bytes it actually
  returns and compares them with the receipt. The call that would deliver a
  member's last byte raises `ValueError` and withholds those bytes if the file
  changed or shrank; bytes delivered earlier are unverified, and a stream
  abandoned early has verified nothing (`stream.verified`). `copy_*_to` write
  into any binary file-like object as they verify, so on `ValueError` the
  destination holds unverified bytes and must be discarded.
- **In memory.** `open_n4a_container(archive, *, memory_budget_bytes=None)`
  validates caller bytes once and returns an immutable `N4aContainer` with
  `view()`, `manifest_bytes()`, `member(path)`, `artifact_members(id)` and
  `archive_bytes()`. It keeps one copy of `archive`. Whoever asks for `bytes`
  (`member`, `member` on a file handle, `archive_bytes`, the writer's result)
  pays that materialization, which is admitted against the budget and allocated
  fallibly; use streams or `write_n4a_file` to avoid it.

Writers: `write_n4a_container(manifest_json, members, base=None)` returns the
new container as `bytes`. `write_n4a_file(destination, manifest_json, members,
*, base=None)` streams straight to disk: a member is `(archive_path, bytes)` or
`(archive_path, os.PathLike)`, a path member is read by Core twice (digest and
write passes, never held in memory) and a source that changes between the passes
is refused. `destination` is staged and published without replacing anything,
and the result holds the `container_sha256`, `manifest_sha256` and `byte_length`
computed while writing. With `base` (an `N4aFile` or `N4aContainer`, never
modified) the members are replacements and every declared member that is not
replaced keeps its bytes from the base; omitting a member from the manifest
deletes it. Manifests are exact JSON text and Core derives every size and
SHA-256, so placeholders are accepted. A duplicate or undeclared member is
refused by Core and refusals are `ValueError`.

Members are `bytes` and are never deserialized: a stored host model (including a
pickle) is only exchanged, and `view()["executability"]` is always
`"not_evaluated"`. Reading is not execution; running a host model is an explicit
action of its owner or the SDK outside Core. `inspect_n4a_bytes` keeps refusing
anything but a closed native-produced model, and its view (like the native
produce and retrain views) has exactly three keys: `manifest`, `receipt` and
`training`. Native actions stay RAM-materialized and accept the same
`memory_budget_bytes=`.

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
which is also the one exported. Core 0.5.0's Methods extra requires
`nirs4all-methods` 1.3.13 or later (ABI 2.20). The role envelope's native format
remains compatible with ABI 2.14.
