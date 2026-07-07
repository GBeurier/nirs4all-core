# MATLAB/Octave Binding

Namespace: `+nirs4all`

The canonical source repository is `nirs4all-core`; the shipped MATLAB/Octave
surface keeps the bare `nirs4all` namespace and is attached to releases as
`nirs4all-matlab-octave-<version>.zip`.

The MATLAB/Octave binding exposes the top-level `nirs4all` aggregate namespace.
It parses the same portable JSON/YAML pipeline envelope as the other bindings
and delegates numerical execution to the upstream `nirs4all-methods` `+pls4all`
MEX shims.

Only `methods` has a MATLAB/Octave runtime candidate today (`+pls4all`). The
other upstream domains are kept in `nirs4all.upstreams()` as aggregate metadata
and do not advertise npm/WASM package names or MATLAB/Octave execution support.

## Surface

- `nirs4all.loadPipelineDefinition(source)` accepts JSON/YAML text, a
  JSON/YAML path, a direct step list, `pipeline`, or `steps`.
- `nirs4all.portableClassNames(definition)` returns the operator classes in the
  definition.
- `nirs4all.runPortablePipeline(source, dataset)` executes the portable
  Kennard-Stone/SNV/Savitzky-Golay/PLS subset through `pls4all`.
- `nirs4all.runtimeContracts()` distinguishes parity-validated pipeline
  execution from standalone serialized-model prediction. MATLAB/Octave does not
  expose the WASM replay-predict contract yet.
- `nirs4all.upstreams()` and `nirs4all.requireUpstream(key)` expose the
  aggregate upstream registry.

`dataset` is a struct with `X`, `y`, and optionally `rows`/`cols` when `X` is
flat. JSON-decoded numeric matrices and nested numeric cells are accepted so the
same oracle fixtures work in MATLAB and Octave.

Savitzky-Golay defaults to `mode = 'interp'` for full nirs4all parity and
preserves explicit methods-backed modes (`mirror`, `constant`, `nearest`,
`wrap`, `interp`) plus `cval`.

## Requirements

Build and put the `nirs4all-methods` MATLAB/Octave binding on the path first:

```matlab
addpath('/path/to/nirs4all-methods/bindings/matlab')
```

Then add this binding:

```matlab
addpath('/path/to/nirs4all-core/bindings/matlab')
```

## Checks

Parser smoke:

```bash
octave --quiet --eval "run('bindings/matlab/tests/smoke.m')"
```

Strict execution parity against the full Python `nirs4all` oracle:

```bash
NIRS4ALL_CORE_REQUIRE_METHODS_PARITY=1 make test-matlab-parity
```
