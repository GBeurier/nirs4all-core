# Architecture

`nirs4all-lite` is an aggregate distribution, not a new computational layer.

## Source-of-truth map

| Domain | Owner | nirs4all-lite responsibility |
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
  `nirs4all_lite`;
- direct access to upstream domains: `formats`, `io`, `datasets`, `methods`,
  `dag_ml`, and `dag_ml_data`;
- host-native pipeline composition that delegates to upstream engines;
- no fallback reimplementation when an upstream binding is missing.

## Strategic Python path

The Python binding is expected to become good enough to replace the core of the
full Python `nirs4all` library later. Until that migration is explicit,
`nirs4all-lite` must avoid importing itself as `nirs4all` in Python so both
packages can coexist during parity checks.
