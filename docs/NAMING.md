# Naming & aggregate facade (LOCK-GOV)

This page records the governed naming of the portable aggregate and the
**additive** import facades introduced by the first `LOCK-GOV` slice. It is the
Python-facing companion to the ecosystem governance decisions (`GOV-003`
per-language source-of-truth names, `GOV-004` alias/token policy).

## Aggregate package names (current, RC V1)

The aggregate is published under the bare `nirs4all` name in every host language
**except Python**, where the full `nirs4all` modelling library already owns that
import root. To avoid colliding with it, the Python aggregate ships as
`nirs4all-core` (canonical import `nirs4all_lite`).

| Target | Distribution / external name | Import / module name |
| --- | --- | --- |
| Python | `nirs4all-core` | `nirs4all_lite` |
| Rust | `nirs4all` | `nirs4all` |
| JavaScript/WASM | `nirs4all` | `nirs4all` |
| R | `nirs4all` | `library(nirs4all)` |
| MATLAB/Octave | `nirs4all` | `+nirs4all` namespace |

## Applied: `nirs4all-lite` → `nirs4all-core` (RC V1)

The governance direction promoted the aggregate from `lite` to
**`nirs4all-core`** (SW2 `GOV-003`, A13 `DEC-GOV-002`, RC V1 control board).
This is a *concept* rename, **not** a single literal rename across ecosystems:
Rust/npm/R/MATLAB already use the bare `nirs4all` name and are unaffected. Only
the Python distribution name changed.

The RC V1 head applies the rename in package metadata (Phase R1 of
[`CORE_RENAME.md`](CORE_RENAME.md)):

- The Python distribution is **`nirs4all-core`**. `nirs4all-lite` is the legacy
  distribution name; per the rename runbook it stays installable on PyPI and its
  final release becomes a thin alias that depends on `nirs4all-core` (Phase R2,
  registry action — never yank existing versions).
- The **canonical import root stays `nirs4all_lite`** so every existing import
  keeps working. Inverting canonicity (making `nirs4all_core` the implementation
  and `nirs4all_lite` the alias) is deliberately deferred: the ecosystem
  release-lock reads `bindings/python/src/nirs4all_lite/_topology.py` by path
  and must be updated in lockstep with any such move.
- The **additive `nirs4all_core` import facade** matches the distribution name.
  Its advertised public contract is deliberately core-only: inspection,
  validation, capability reporting, release topology, and facade access.
  Execution helpers from `nirs4all_lite` stay reachable through compatibility
  passthrough, but they are not part of `nirs4all_core.__all__`.
- GitHub repo and Read the Docs now use `nirs4all-core`. The remaining external
  admin action is the PyPI Trusted Publisher for `nirs4all-core`, plus the final
  `nirs4all-lite` alias release.

```python
import nirs4all_core          # alias matching the distribution name (additive)
import nirs4all_lite          # canonical import — unchanged
assert nirs4all_core.upstreams is nirs4all_lite.upstreams
assert "run_portable_pipeline" not in nirs4all_core.__all__
```

## The `n4a` facade (additive brand root)

`n4a` is a short, brand-aligned Python import root (`n4a` = "nirs4all") that
re-exports the full public surface of the aggregate. It adds no behavior; it is
a facade over `nirs4all_lite`.

```python
import n4a

n4a.upstream_status()
plan = n4a.parse_execution_plan(config)
```

Both facades are **additive and non-shadowing**: they never remove or rename the
`nirs4all_lite` surface, and — per the Python strategic-path rule — they do
**not** define a top-level `nirs4all` Python module, so the full `nirs4all`
library and the aggregate continue to coexist.

The package exposes `release_topology_manifest()` as a machine-checkable summary
of this contract (schema `nirs4all-core.release-topology.v2`). It records that
the current Python distribution is `nirs4all-core`, that `nirs4all-lite` is the
superseded legacy name, and that the `nirs4all_core` import contract is not an
execution engine.

## `n4a` token disambiguation (GOV-004)

The `n4a` stem is used in three different layers of the ecosystem. They are
intentionally consistent (`n4a` = "nirs4all") but must not be conflated:

| Token | Layer | Meaning |
| --- | --- | --- |
| `n4a` | Python import | The aggregate **import facade** documented here (`import n4a`). |
| `.n4a` | File extension | The pipeline/bundle **file format** produced by the full `nirs4all` library. |
| `n4a-datasets` | Console script | The **CLI** shipped by `nirs4all-datasets`. |

A `.n4a` file is data, `n4a-datasets` is an executable, and `n4a` is an import
root. None is an alias of another.
