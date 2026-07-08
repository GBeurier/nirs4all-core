# Architecture

`nirs4all-core` is an aggregate distribution, not a new computational layer.

## Source-of-truth map

| Domain | Owner | nirs4all-core responsibility |
| --- | --- | --- |
| Vendor file parsing | `nirs4all-formats` | expose readers and records |
| Dataset assembly | `nirs4all-io` | expose loading/configuration adapters |
| Dataset catalog | `nirs4all-datasets` | expose catalog access and provenance |
| Numerical methods | `nirs4all-methods` | expose method bindings and pipeline nodes |
| DAG execution | `dag-ml` | expose graph planning/execution contracts |
| Data contracts | `dag-ml-data` | expose sample-aligned schemas |

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
the aggregate, R currently has package candidates for `dag_ml_data`, `formats`,
`io`, `datasets`, and `methods` but not `dag_ml`, and MATLAB/Octave only has a
runtime candidate for `methods` through `+n4m`. The other MATLAB/Octave
domain rows are metadata-only.

## Strategic Python path

The Python binding is expected to become good enough to replace the core of the
full Python `nirs4all` library later. Until that migration is explicit,
`nirs4all-core` must avoid importing itself as `nirs4all` in Python so both
packages can coexist during parity checks.
