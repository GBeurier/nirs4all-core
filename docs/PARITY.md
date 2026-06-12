# Parity Strategy

`nirs4all-lite` needs parity gates because it is intended to become the portable
core behind multiple host-language APIs.

## Tiers

1. **Upstream native vs upstream binding**: each upstream project proves its own
   binding parity first, especially `nirs4all-methods`.
2. **nirs4all-lite native vs binding**: lite pipelines produce identical
   results across Rust, Python, R, MATLAB/Octave, and WASM within declared
   tolerance.
3. **nirs4all-lite vs full Python nirs4all**: equivalent pipelines match the
   current Python library before the lite binding can replace any core path.

## Fixture policy

Fixtures must record:

- dataset identity and provenance;
- pipeline descriptor;
- upstream versions or commit SHAs;
- numeric tolerance and dtype;
- platform/runtime notes when relevant.

Golden outputs should be generated from the owning upstream implementation, not
from host-language rewrites.

Pipeline definition fixtures must use the full Python `nirs4all` JSON/YAML
syntax. The parser-level contract accepts JSON/YAML paths, JSON/YAML text,
direct step lists, `pipeline`, and `steps` in every target binding, but
canonical fixtures should keep the same `pipeline` envelope used in
`nirs4all/examples/pipeline_samples`. Execution parity is a separate gate: a
definition that parses is not considered validated until native lite and every
target binding match the full Python `nirs4all` result for the portable operator
subset.

External operators follow the same rule. A language binding can expose operator
metadata before execution support exists, but an operator cannot be marked
`execute-local`, `execute-remote`, or `parity-validated` until a fixture proves
that the host-language adapter delegates to the same upstream behavior.

## Initial parity targets

- PLS pipelines backed by `nirs4all-methods`.
- Format load/record conversion backed by `nirs4all-formats`.
- Dataset materialization backed by `nirs4all-io`.
- Catalog resolution backed by `nirs4all-datasets`.
- DAG execution descriptors backed by `dag-ml` and `dag-ml-data`.
- External operator adapters once the relevant executor can plan or call them.
