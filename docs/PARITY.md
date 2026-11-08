# Parity Strategy

`nirs4all-core` needs parity gates because it is intended to become the portable
core behind multiple host-language APIs.

## Tiers

1. **Upstream native vs upstream binding**: each upstream project proves its own
   binding parity first, especially `nirs4all-methods`.
2. **nirs4all-core native vs core-owned bindings**: portable aggregate pipelines
   produce identical results across Rust, Python, MATLAB/Octave, and WASM within
   declared tolerance. The external `nirs4all-r` product uses the same oracle.
3. **nirs4all-core vs full Python nirs4all**: equivalent pipelines match the
   current Python library before this binding can replace any core path.

## Static surface parity (no runtime required)

### Retained MATLAB/Octave portable execution contract

The retained Core portable runner is MATLAB/Octave-only. Its bounded
KS/SNV/SG/PLS route accepts one optional splitter anywhere before the final PLS model, including after SNV/SG preprocessing declarations. It
preserves the relative transform order and applies transforms separately to the
resulting train and validation rows. Ambiguous steps,
duplicate splitters and steps after a model are refused before runtime loading.
Integer parameters accept finite integral numbers or numeric strings in the
signed 32-bit range; booleans and lossy conversions are refused. Component
counts are positive. Inclusive `_range_` sweeps have a positive stride,
ascending bounds and at most 10,000 variants. Their terminal addition never
overflows, including a single `2147483647` candidate. Methods still validates
whether a component count is valid for the actual matrix.

`tests/parity/fixtures/execution_contract_cases.json` is the shared positive
and negative input corpus. The copy shipped in Rust is checked for byte
equality by the Python gate; `nirs4all-r` pins its copy by checksum and tests
every positive/negative case. This complements the numerical oracle fixtures.

The runner's scores describe **training** when there is no splitter and
**selection_validation** when Kennard-Stone reserves rows. The selected
component minimizes the same reported RMSE, so this result provides **no
independent test estimate after selection**. MATLAB/Octave returns
`evaluation = {scope, independent_test: false}`. The former JS comparison and
other-language reference records remain historical. Existing `testIndices` identifies
the scored rows for compatibility; its name does not establish independence.
Predictions, variants and split indices retain their established numerical
contract. Full Python `nirs4all.run` / DAG-ML validation and independent test
paths are separate APIs and are not replaced by this convenience runner.

### Registered operator surface

Before any numeric gate runs, the public *surface* must be identical across
bindings. `bindings/python/tests/test_cross_language_surface.py` proves, in pure
Python by reading the binding sources, that the upstream registry (keys + role
strings) is identical across the **four core-owned** bindings — Python, WASM,
MATLAB/Octave, and Rust — plus the machine-readable `compat/upstreams.toml`.
`bindings/python/tests/test_capability_matrix.py` additionally proves that the
two native execution profiles in `compat/capabilities.toml` match the generated
host data and the entrypoints and producers in the sources (see
[`EXECUTION_PROFILES.md`](EXECUTION_PROFILES.md)). That registry is a metadata
contract, not numerical parity.

Because these gates need no Node/Octave/`cargo` toolchain, surface drift in any
core-owned binding is caught in the required Python suite even on machines where the other
runtimes are unavailable. They run in `make test-python-v1-surfaces`.

## Fixture policy

Fixtures must record:

- dataset identity and provenance;
- pipeline descriptor;
- upstream versions or commit SHAs;
- numeric tolerance and dtype;
- platform/runtime notes when relevant.

Golden outputs should be generated from the owning upstream implementation, not
from host-language rewrites.

Historical portable definition fixtures retain the full Python `nirs4all`
JSON/YAML syntax and the `pipeline` envelope used in
`nirs4all/examples/pipeline_samples`. The JS loader retains JSON/YAML text,
direct arrays and `pipeline` / `steps` envelopes for manifest-resolved
`n4m:<method id>` roles; it refuses the old Python class aliases. Parsing is
separate from execution qualification: only equivalent declared protocols may
be compared, with their training, selection and refit populations identified.

External operators follow the same rule. A language binding can expose operator
metadata before execution support exists, but an operator is not claimed as
executable until a fixture proves that the host-language adapter delegates to
the same upstream behavior.

## Initial parity targets

- PLS pipelines backed by `nirs4all-methods`.
- Format load/record conversion backed by `nirs4all-formats`.
- Dataset materialization backed by `nirs4all-io`.
- Catalog resolution backed by `nirs4all-datasets`.
- DAG execution descriptors backed by `dag-ml` and `dag-ml-data`.
- External operator adapters once the relevant executor can plan or call them.

## Current execution gates

The retained MATLAB/Octave portable runner compares four unchanged JSON/YAML
fixtures against the full Python oracle: SNV + PLS; Savitzky-Golay + PLS;
Kennard-Stone + SNV + PLS; and Kennard-Stone + SNV + Savitzky-Golay with a
component sweep. Its `nirs4all.runPortablePipeline()` route, count guard, score
scope and MEX delegation remain unchanged. Savitzky-Golay defaults to `interp`
and retains explicit `mirror`, `constant`, `nearest`, `wrap`, `interp` and `cval`.

`tests/parity/expected/portable_python_oracle.json` retains its original
`scripts/parity/generate_python_oracle.py` provenance (full Python nirs4all
operators plus sklearn PLS), split indices and declared tolerances. The old JS
runner comparison, including selected coefficient replay, is archived verbatim
as `.txt` under `tests/parity/reference/core-js-runner/`. It is not an active
production qualification or a protocol-equivalent replacement for DAG refit.

Active JS `bindings/wasm/tests/parity.test.js` preserves MBPLS and corrected
GroupSparsePLS held-out references through Methods `fitModel` / `predictModel`
with their original vectors and `<1e-10` tolerance.
`bindings/wasm/tests/native-augmentation.test.js` preserves the 22 native/R
augmentation cases at seed 42, exact inputs and `1e-10 * max(1, abs(expected))`
tolerance. Current Python/R v8 envelope replay and named-refit checks remain
unchanged (`1e-12` historical replay; `1e-9` versioned fresh fit).

Historical Python/R/Rust comparison commands below are retained as reference
navigation; they do not restore the removed JS execution API.

The historical oracle generation command is recorded for provenance; retirement
does not regenerate it:

```bash
PYTHONPATH=/path/to/nirs4all \
  /path/to/nirs4all/.venv/bin/python \
  scripts/parity/generate_python_oracle.py
```

Run the retained WASM numerical and role parity gates against a matched
`nirs4all-methods` JS/WASM artifact. The Core tests need a complete Methods JS
dist containing `index.js`, `n4m.js`, and `n4m.wasm`. The artifact-dependent
checks skip by default when absent; strict mode requires the artifact. Use the preflight to
turn an incomplete local Methods checkout into an explicit failure without
requiring Node or Emscripten to be available in the Core repo:

```bash
make check-wasm-methods-artifact \
  NIRS4ALL_METHODS_ROOT=../RC-v1-methods
```

Build/stage the Methods package from a checkout with Node/npm and Emscripten
(`emcc`/`emcmake`) available:

```bash
cd /path/to/nirs4all-methods
cmake --preset emscripten
cmake --build --preset emscripten --target n4m_wasm --parallel
cd bindings/js
npm ci
npm run build
npm run stage:wasm
```

Set `NIRS4ALL_METHODS_JS_DIST` when the methods checkout is not the default
sibling. Set `NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1` to make a missing methods
artifact fail instead of skip:

```bash
NIRS4ALL_METHODS_JS_DIST=/path/to/nirs4all-methods/bindings/js/dist \
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 \
npm test --prefix bindings/wasm
```

The equivalent strict make target is:

```bash
make test-wasm-parity-strict \
  NIRS4ALL_METHODS_ROOT=/path/to/nirs4all-methods
```

Historical Python reference command, with `nirs4all-methods` importable and a
local libn4m build:

```bash
PYTHONPATH=bindings/python/src:/path/to/nirs4all-methods/bindings/python/src \
N4M_LIB_PATH=/path/to/libn4m.so \
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 \
python -m unittest bindings/python/tests/test_execution_parity.py -v
```

Historical Rust reference command, with a local libn4m build:

```bash
NIRS4ALL_METHODS_LIB=/path/to/libn4m.so \
LD_LIBRARY_PATH=/path/to/libn4m-directory \
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 \
cargo test -p nirs4all rust_binding_execution_matches_full_python_nirs4all_oracle -- --nocapture
```

Historical R reference command in the separate `nirs4all-r` repository,
using the shared JSON/YAML fixtures and Python oracle retained here:

```bash
Rscript ../nirs4all-r/tests/portable-json-yaml.R
```

Run the strict MATLAB/Octave parity gate after building the `nirs4all-methods`
`+n4m` MEX shims:

```bash
NIRS4ALL_CORE_PARITY_ORACLE=$PWD/tests/parity/expected/portable_python_oracle.json \
NIRS4ALL_CORE_PARITY_FIXTURES=$PWD/tests/parity/fixtures \
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 \
octave --quiet --eval "addpath('bindings/matlab/tests'); parity"
```
