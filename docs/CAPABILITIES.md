# Capability Matrix

This page reports, honestly, what each language binding of the nirs4all aggregate
(shipping as `nirs4all-core`, formerly `nirs4all-lite`) can actually
*do* — not what it advertises. The capability vocabulary is the ladder defined in
[`OPERATORS.md`](OPERATORS.md):

`metadata` → `plan` → `execute-local` → `execute-remote` → `parity-validated`.

The machine-readable source of truth is [`compat/capabilities.toml`](../compat/capabilities.toml).
Every claim below is enforced against the binding sources and parity gate files
by `bindings/python/tests/test_capability_matrix.py`, so the table cannot
over-claim: a binding may not say `execute-local` without a real run symbol, nor
`parity-validated` without a real parity gate.

## Portable operator subset

The aggregate itself executes exactly one operator subset — Kennard-Stone split,
SNV, Savitzky-Golay, and PLS regression — and it does so by **delegating all
numerics to the `methods` upstream** (`nirs4all-methods` / `libn4m` / `+n4m`
with `+pls4all` compatibility / `n4m`). It never re-implements a kernel. The same nine class aliases are
declared identically in all five bindings (proven by
`test_cross_language_surface.py`).

| Language | Level | Run entry point | Numerics reached via | Parity gate |
| --- | --- | --- | --- | --- |
| Python | `parity-validated` | `run_portable_pipeline()` | nirs4all-methods Python (`n4m`/`pls4all`) | `bindings/python/tests/test_execution_parity.py` |
| Rust | `parity-validated` | `run_portable_pipeline_with_library()` | caller-supplied `libn4m` (`NIRS4ALL_METHODS_LIB`) | `cargo test` `rust_binding_execution_matches_full_python_nirs4all_oracle` |
| JavaScript/WASM | `parity-validated` | `runPortablePipeline()` / `predictPortablePipeline()` | `@nirs4all/methods` | `bindings/wasm/tests/parity.test.js` |
| R | `parity-validated` | `nirs4all_run_portable_pipeline()` | nirs4all-methods R (`n4m`/`pls4all`) | `bindings/r/tests/parity.R` |
| MATLAB/Octave | `parity-validated` | `nirs4all.runPortablePipeline()` | `+n4m` MATLAB/Octave MEX shims with `+pls4all` compatibility | `bindings/matlab/tests/parity.m` |

"`parity-validated`" here is **conditional on the `methods` upstream being
present**. Without it, every binding degrades honestly:

- the parser/inspection surface (`load_pipeline_definition`,
  `portable_class_names`, `parse_execution_plan`) still works — this is the
  `plan` level;
- the run entry point raises a clear "capability unavailable" style error
  (e.g. R's "does not expose …", MATLAB's `nirs4all:MissingMethods`, the Rust
  loader error, the strict-parity skip guarded by
  `NIRS4ALL_LITE_REQUIRE_METHODS_PARITY`), never a silent local re-implementation.

The shared numeric oracle is
`tests/parity/expected/portable_python_oracle.json`, generated from the full
Python `nirs4all` library (see [`PARITY.md`](PARITY.md)).

## Upstream domains

The other upstream domains — `formats`, `io`, `datasets`, `dag_ml`,
`dag_ml_data` — are re-exported through **lazy import proxies/loaders only**. The
aggregate does not wrap or execute their operators, so its own capability over
them is `metadata`; the real execution capability is whatever the installed
upstream provides. This is recorded as `metadata` rather than dressed up as
aggregate execution.

This metadata is shared across package names, but runtime candidates are
language-specific. R currently has no declared `dag_ml` package candidate, and
MATLAB/Octave only advertises `methods` through `+n4m` and `+pls4all`; its other domain
rows are metadata-only and must not be read as npm/WASM package names or
MATLAB/Octave runtime support.

| Domain | Aggregate level | Notes |
| --- | --- | --- |
| `formats` | `metadata` | lazy re-export; execution = upstream-provided |
| `io` | `metadata` | lazy re-export; execution = upstream-provided |
| `datasets` | `metadata` | optional/external; lazy re-export |
| `dag_ml` | `metadata` | lazy re-export; no R binding declared yet |
| `dag_ml_data` | `metadata` | lazy re-export; execution = upstream-provided |

## Why this matters for the release

The RC stop condition is explicit: *do not fake unsupported execution in a
language binding; report capability levels honestly.* This matrix + the
enforcement test are that guarantee. If a future change adds, say, a browser
`execute-remote` path or a new operator, the ledger and its test must be updated
in lockstep, and the test will fail until the claim is backed by a real symbol
and gate.
