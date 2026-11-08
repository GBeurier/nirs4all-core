# Architecture

`nirs4all-core` is an aggregate distribution, not a new computational layer.

## Source-of-truth map

| Domain | Owner | nirs4all-core responsibility |
| --- | --- | --- |
| Vendor file parsing | `nirs4all-formats` | expose readers and records |
| Dataset assembly | `nirs4all-io` | expose loading/configuration adapters |
| Dataset catalog | `nirs4all-datasets` | expose catalog access and provenance |
| Numerical methods | `nirs4all-methods` | expose method bindings and pipeline nodes |
| DAG execution and scientific evaluation | `dag-ml` | expose graph planning, scoring definitions/context, folds, selection, lineage and replay |
| Runtime data contracts | `dag-ml-data` | expose sample/origin/group relationships and leakage constraints |
| Reusable frontend presentation and interactions | `nirs4all-ui` | provide owner results to views through product adapters |
| Recipe catalog content | `nirs4all-repository` | expose versioned recipes and execution requirements |

Methods numerical metric kernels and DAG-ML scoring must agree through shared
definition fixtures. A kernel value alone has no fold, target-reduction or
selection context. Conformal calibration stays in DAG-ML under ADR-20. UI
renders authoritative scores and IO proposals; it does not infer datasets or
recompute scientific results. Bindings and product shells retain transport and
frontend interactions, with semantic recipe conversion in compiler adapters.

The [n4a compatibility contract](N4A_COMPATIBILITY.md) separates container
versions from owner artifact schemas and executable operations.

## Binding surface

Each host binding should provide:

- a top-level `nirs4all` surface, except Python where the import is
  `nirs4all_core`;
- a shared registry for upstream domains: `formats`, `io`, `datasets`,
  `methods`, `dag_ml`, and `dag_ml_data`;
- runtime accessors only for upstream bindings that exist in the host language;
- host-native pipeline composition only where execution is backed by upstream
  engines and parity gates;
- no fallback reimplementation when an upstream binding is missing.

The registry is broader than current runtime coverage. JavaScript/WASM publishes
npm peer candidates for all six domains, Rust records crate/package metadata for
the aggregate, and R includes `dagml` for local DAG-ML loss/metric functions.
MATLAB/Octave similarly exposes the `+dagml` local registry and `+n4m` methods;
its other domain rows remain metadata-only.

## Strategic Python path

The full Python `nirs4all` V1 library consumes the native stack through its
declared DAG-ML/Core execution paths and retains Python-owned controllers.
The aggregate imports as `nirs4all_core`, so both packages coexist. The bounded
`run_portable_pipeline` convenience API is separate from full-library DAG-ML
execution; its selection-score contract is documented in [Parity](PARITY.md).
