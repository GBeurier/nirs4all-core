# nirs4all

Rust aggregate surface for `nirs4all-core`.

This crate exposes the low-level nirs4all upstream domains from one package:
`dag-ml`, `dag-ml-data`, `nirs4all-formats`, `nirs4all-io`,
`nirs4all-datasets`, and `nirs4all-methods`.

It does not implement parsers, dataset loaders, DAG scheduling, or numerical
methods. Those capabilities remain owned by the upstream crates and bindings.

For the shared portable parity subset, call
`run_portable_pipeline_with_library()` with a `libn4m` path. The Rust binding
loads the C ABI dynamically and compares against the same full Python
`nirs4all` oracle as the Python, R, and JavaScript/WASM bindings.
