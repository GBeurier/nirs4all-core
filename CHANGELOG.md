# Changelog

All notable changes to **nirs4all-core** (formerly **nirs4all-lite**) are
documented here. The project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The Rust
crate `[package]` version in `bindings/rust/nirs4all/Cargo.toml` is the
single source of truth; `scripts/bump_version.sh` propagates it to every other
binding manifest.

## [Unreleased]

RC V1 topology: this unreleased train combines the first `LOCK-GOV` facade
slice (additive) with the **Python distribution rename
`nirs4all-lite` → `nirs4all-core`** (Phase R1 of `docs/CORE_RENAME.md`,
executed by RC-A on the RC V1 control-board decision).

### Changed (RC V1 rename)

- Python distribution renamed `nirs4all-lite` → **`nirs4all-core`**
  (`bindings/python/pyproject.toml`). The canonical import root stays
  `nirs4all_lite`; the wheel still ships `nirs4all_lite` + `n4a` +
  `nirs4all_core`, so no import breaks. Rust/npm/R/MATLAB names are unaffected
  (already the bare `nirs4all`).
- `release_topology_manifest()` schema bumped to
  `nirs4all-core.release-topology.v2`: `aggregate.id = "nirs4all-core"`
  (`legacy_id = "nirs4all-lite"`), `python.distribution = "nirs4all-core"`
  with `legacy_distribution_status = "superseded"`, install rows flipped, and
  the source/SBOM artifact renamed `nirs4all-core-source-sbom`.
- Release workflows build/validate/publish under the new name
  (`nirs4all_core-*` wheel, `nirs4all-core-<version>-src.*` source prefix,
  PyPI project `nirs4all-core`). The `nirs4all-core` PyPI Trusted Publisher
  registration, the GitHub repo rename, and the RTD slug rename remain
  pending external admin actions (`docs/CORE_RENAME.md` Phase R2); the legacy
  `nirs4all-lite` PyPI project stays installable and must never be yanked.
- User-facing diagnostics across the five bindings now say
  "nirs4all-core portable subset".

First safe `LOCK-GOV` slice — **additive only**, no legacy import removed.

### Added

- Python `n4a` import facade — a brand-aligned root (`import n4a`) that
  re-exports the full `nirs4all_lite` public surface and adds no behavior.
- Python `nirs4all_core` import alias for the `nirs4all-lite` → `nirs4all-core`
  direction; re-exports `nirs4all_lite`. (Introduced additively; the
  distribution rename above landed later in the same unreleased train.)
- `docs/NAMING.md` documenting the per-language aggregate names, the lite→core
  direction, the facades, and the `n4a` token disambiguation (`n4a` import vs
  `.n4a` bundle extension vs `n4a-datasets` CLI) for `GOV-004`.
- `bindings/python/tests/test_facade.py` proving surface parity, object
  identity, `__getattr__` passthrough, and that legacy `nirs4all_lite` imports
  and the full-`nirs4all` coexistence rule are preserved.

### Fixed

- Removed the stale `License :: OSI Approved :: MIT License` trove classifier
  from the Python `pyproject.toml`; the SPDX `License-Expression`
  (`CeCILL-2.1 OR AGPL-3.0-or-later`) is authoritative (PEP 639). The wheel
  metadata is no longer self-contradictory.

## [0.2.0] - 2026-06-14

**Breaking** (pre-1.0 minor bump, 0.1.0 → 0.2.0) — coordinated with the breaking
**nirs4all-methods 1.0.0** (C ABI 2.0 + the `n4m.<role>` namespace). `nirs4all-lite`
re-exports the methods surface, so consumers must move to the methods 1.0.0 / ABI-2 surface.

### Changed (breaking)

- Re-exports the ABI-2 `nirs4all-methods` surface. The Python aggregate now
  imports methods through the new `n4m.<role>` namespace
  (e.g. `n4m.transform.scatter`, `n4m.transform.smoothing`,
  `n4m.model_selection.splitters`) instead of the old flat `n4m.sklearn.*`
  layout.
- The Rust/WASM bindings load the ABI-2 C symbols: `n4m_pp_*` preprocessing
  entry points are now `n4m_transform_*`, and `n4m_split_*` selection entry
  points are now `n4m_model_selection_*`.
- Pinned `nirs4all-methods >= 1.0.0` (was `>= 0.99.0`) in the Python
  `methods`, `all`, and bundled-aggregate extras.

### Versioning

- Bumped the lite project version `0.1.0` → `0.2.0` across every packaging
  manifest: the Rust crate (source of truth), the WASM `package.json` /
  `package-lock.json`, the Python `pyproject.toml`, and the R `DESCRIPTION`.
  The MATLAB/Octave archive version derives from the Rust crate version at
  build time.
