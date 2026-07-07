# Changelog

All notable changes to **nirs4all-core** are documented here. The project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The Rust
crate `[package]` version in `bindings/rust/nirs4all/Cargo.toml` is the
single source of truth; `scripts/bump_version.sh` propagates it to every other
binding manifest.

## [Unreleased]

## [0.3.1] - 2026-07-08

### Fixed

- Removed remaining documentation wording that implied a maintained
  `nirs4all-lite` public alias. The V1 target has no legacy alias release.

## [0.3.0] - 2026-07-07

### Changed

- Made `nirs4all_core` the canonical Python implementation package for
  `nirs4all-core`.
- Kept `n4a` as the additive brand facade over `nirs4all_core`.
- Removed public legacy alias rows from the release topology and wheel package
  list.
- Standardized strict parity gates on `NIRS4ALL_CORE_*` environment variables.

## [0.2.12] - 2026-07-07

RC16 runtime-contract honesty release.

### Added

- Added `runtime_contracts` / `runtimeContracts` across Python, R, Rust,
  MATLAB/Octave, and JavaScript/WASM capability manifests.
- Declared portable pipeline execution and standalone serialized-model
  prediction as separate custom-host contracts.
- Guarded the manifest so only JavaScript/WASM currently claims
  `predictPortablePipeline()` parity for serialized selected-model replay.

## [0.2.11] - 2026-07-07

RC15 changelog catch-up release.

### Fixed

- Added the missing changelog entries for the `0.2.9` and `0.2.10` release
  candidates so source releases document the custom-host and lockfile fixes.

## [0.2.10] - 2026-07-07

RC14 lockfile consistency release.

### Fixed

- Bumped the tracked root `Cargo.lock` package entry alongside the Rust crate
  version after the `0.2.9` source tree still reported `nirs4all = 0.2.8` in
  the lockfile.
- Extended `scripts/bump_version.sh --check` so future release bumps also
  validate the root Cargo lockfile's local `nirs4all` package version.

## [0.2.9] - 2026-07-07

RC13 custom-host capability manifest release.

### Added

- Added the cross-language V1 capability manifest for custom app hosts across
  Python, WASM/JavaScript, R, Rust, and MATLAB/Octave bindings.
- Exposed stable controller capability IDs for Kennard-Stone splitting, SNV,
  Savitzky-Golay, PLS regression, and the portable methods pipeline.
- Documented runtime surfaces and the custom-host manifest contract.

### Changed

- Synchronized `nirs4all-web` with the vendored core WASM custom-host surface.

## [0.2.8] - 2026-07-07

RC12 methods package-name alignment.

### Changed

- Updated JavaScript/WASM upstream metadata from `@nirs4all/methods-wasm` to
  the V1 package `@nirs4all/methods`.
- Advertised MATLAB/Octave methods as `+n4m` with `+pls4all` compatibility.

### Fixed

- Republished the core aggregate surface after `0.2.7` was tagged against the
  older methods WASM package name.

## [0.2.7] - 2026-07-06

RC11 core aggregate release head.

### Fixed

- Normalized the Python package license expression to the canonical SPDX
  casing `CECILL-2.1 OR AGPL-3.0-or-later`.

### Added

- Custom-host composition documentation for using the `nirs4all` WASM package
  with host-provided UI/runtime layers.

### Fixed

- Stabilized the R multimodal roundtrip E2E environment checks without changing
  the runtime contract.

## [0.2.6] - 2026-07-06

RC V1 package-name and upstream compatibility hardening.

### Changed

- Locked core Rust dependencies and merged the core main gates into the RC
  release train.
- Accepted the published/scoped WASM upstream package-name variants used by the
  release train.

## [0.2.5] - 2026-07-06

RC V1 topology: this historical train combined the first `LOCK-GOV` facade
slice (additive) with the Python distribution rename from the retired
`nirs4all-lite` name to `nirs4all-core` (Phase R1 of `docs/CORE_RENAME.md`,
executed by RC-A on the RC V1 control-board decision).

### Changed (RC V1 rename)

- Python distribution renamed from the retired `nirs4all-lite` name to
  **`nirs4all-core`** (`bindings/python/pyproject.toml`). This 0.2.x train
  briefly kept transitional Python import compatibility; that compatibility
  surface is superseded by the 0.3.0 canonical `nirs4all_core` + `n4a` package.
  Rust/npm/R/MATLAB names are unaffected (already the bare `nirs4all`).
- `release_topology_manifest()` schema bumped to
  `nirs4all-core.release-topology.v2`: `aggregate.id = "nirs4all-core"`,
  `python.distribution = "nirs4all-core"`, install rows flipped, and the
  source/SBOM artifact renamed `nirs4all-core-source-sbom`.
- Release workflows build/validate/publish under the new name
  (`nirs4all_core-*` wheel, `nirs4all-core-<version>-src.*` source prefix,
  PyPI project `nirs4all-core`). The V1 RC target does not publish or maintain
  a public `nirs4all-lite` compatibility release.
- User-facing diagnostics across the five bindings now say
  "nirs4all-core portable subset".

First safe `LOCK-GOV` slice — **additive only**, no legacy import removed.

### Added

- Python `n4a` import facade — a brand-aligned root (`import n4a`) that
  re-exports the aggregate public surface and adds no behavior.
- Python `nirs4all_core` import root for the `nirs4all-core` aggregate.
  (Introduced additively; it became canonical in the 0.3.0 naming cleanup.)
- `docs/NAMING.md` documenting the per-language aggregate names, the lite→core
  direction, the facades, and the `n4a` token disambiguation (`n4a` import vs
  `.n4a` bundle extension vs `n4a-datasets` CLI) for `GOV-004`.
- `bindings/python/tests/test_facade.py` proving surface parity, object
  identity, `__getattr__` passthrough, and full-`nirs4all` coexistence.

### Fixed

- Removed the stale `License :: OSI Approved :: MIT License` trove classifier
  from the Python `pyproject.toml`; the SPDX `License-Expression`
  (`CECILL-2.1 OR AGPL-3.0-or-later`) is authoritative (PEP 639). The wheel
  metadata is no longer self-contradictory.

## [0.2.0] - 2026-06-14

**Breaking** (pre-1.0 minor bump, 0.1.0 → 0.2.0) — coordinated with the breaking
**nirs4all-methods 1.0.0** (C ABI 2.0 + the `n4m.<role>` namespace). The
then-current aggregate re-exported the methods surface, so consumers had to move
to the methods 1.0.0 / ABI-2 surface.

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
