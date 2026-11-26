# Release Plan

Release artifacts should be built from the same upstream lock:

- Rust crate: `nirs4all`
- Python wheel/sdist: `nirs4all-core`
- npm package: `nirs4all`
- R source package: `nirs4all`, released separately from `nirs4all-r`
- MATLAB/Octave archive: `nirs4all-matlab-octave`
- WASM bundle consumed by `nirs4all-web`

The Rust, JavaScript/WASM, and MATLAB/Octave artifacts published as
`nirs4all` are target-language releases of this `nirs4all-core` aggregate, not
separate host-language reimplementations. They stay delegating consumers of the
shared upstream packages and `nirs4all-methods`.
The R consumer uses the same upstream cohort and is maintained and published
from its separate `nirs4all-r` repository.

Before release:

1. Pin upstream versions or SHAs in `compat/upstreams.toml`.
2. Build each binding from the same lock.
3. Run upstream binding parity gates.
4. Run aggregate cross-language parity gates.
5. Run equivalent-pipeline checks against full Python `nirs4all`.
6. Verify external operator capability levels: metadata-only operators must not
   be marketed as executable, and executable operators must have parity fixtures.
7. Verify the Python release topology manifest: the current distribution is
   `nirs4all-core`, `nirs4all_core` is the canonical import, `n4a` is the only
   additive Python facade, and execution exports are delegated to upstream
   projects.
8. Publish artifacts and record provenance in the release notes.

`nirs4all_core.release_topology_manifest()` is the aggregate-side consumer
contract for ecosystem release manifests (schema
`nirs4all-core.release-topology.v2`). It records the current `nirs4all-core`
Python distribution, per-registry aggregate artifact rows, explicit V1
Python/R/JavaScript-WASM surface gates, Python facade namespaces, optional
upstream policy (notably external `nirs4all-datasets`), and
license/SBOM/`nirs4all-methods` C ABI pointers. Central release tooling should
consume these fields instead of re-deriving topology from prose.

Local artifact commands:

```bash
make test-v1-surfaces
make test
make build-python
make build-npm
make build-matlab
cargo package -p nirs4all
```

`make test-v1-surfaces` covers the core-owned Python, WASM, Rust, and
MATLAB/Octave surfaces. R checks and CRAN/R-universe validation now run in
`nirs4all-r` rather than this core repository.

Run the owner and affected binding/parity checks for the candidate cohort and
retain their actual commands, exits, dependency identities and limits. Native
Windows coverage must use actual Windows artifacts; a WSL ELF process is Linux
evidence. Current hosted workflows build Windows wheels and run their package
smokes. A Windows package smoke does not establish full scientific parity.

Since Core 0.5.0, the historical machine-specific local receipt is no longer a
publication prerequisite. `qualification/policy.json`,
`compat/local-qualification.json`, their retained evidence and the verifier
describe their original source/cohort. They remain available for historical
inspection:

```bash
python scripts/verify_local_qualification.py --project core --receipt compat/local-qualification.json --root .
```

The verifier rejects changed source scopes and stale evidence; it must not be
made to relabel historical results as qualification of a new cohort. Current
publication channels enforce their package, version, source and authentication
guards and bounded platform smokes. Record scientific checks and any unexecuted
platform or parity checks explicitly in each release's evidence.

Run the complete Python suite with `python -m pytest bindings/python/tests`;
`unittest discover` alone omits the native workspace and CLI function tests.
Stage actual experiment/browser/native archive fixtures and the native runtime
before the suite so the affected native cases execute rather than skip.
Windows JavaScript parity can use the complete `test:js` suite against the
WASM candidate produced by GitHub; building Rust on Windows locally
is not required merely to execute that candidate.

Every CI run uploads the build outputs as artifacts (`rust-crate`, `python-*`,
`npm-wasm` and `matlab-octave`).

Tagged releases are cut by five dedicated workflows — `release-python.yml`,
`release-npm.yml`, `release-crates.yml`, `release-matlab.yml`,
`release-source.yml`. On a non-pre-release tag `vX.Y.Z` they publish PyPI
`nirs4all-core` (OIDC Trusted Publishing, environment `pypi`), npm `nirs4all`
(`NPM_TOKEN`), crates.io `nirs4all` (`CARGO_REGISTRY_TOKEN`), and attach the
MATLAB/Octave zip and the source + SBOM bundle to the Release. The R product is
released independently from `nirs4all-r`.
Pre-release tags build/attach but publish to no registry; `workflow_dispatch`
runs every workflow in dry-run mode. The version source of truth is the Rust
crate manifest, propagated by `scripts/bump_version.sh`. See
[`PUBLISHING.md`](PUBLISHING.md).

## Cross-repository version constraints

Versions are `X.Y.Z`. A **patch** (`Z`) carries fixes and purely additive,
backward-compatible features; existing outputs, wire formats, schemas and ABI
symbols stay unchanged. A **minor** (`Y`) carries any compatibility break:
changed/removed API signature, native ABI break or ABI version change,
wire/byte/persisted-schema, `.n4a` container/inventory or payload, or shared
fixture change, renamed ids, or changed numerical results of existing
operations. `X` is the product version. This repository classifies each of its
bumps.

nirs4all packages from different repositories accept every patch of one tested
series, `>=X.Y[.Z],<X.(Y+1)`: Rust `"0.4"` (1.x: `">=1.4.1, <1.5"`, never
`"1.4"`), Python `>=0.4,<0.5` (1.x: `>=1.4.1,<1.5`), npm `^0.4.0` (1.x:
`~1.4.1`, never `^1.4.0`), R a lower bound plus a documented series. A patch
floor is allowed when a fix or feature from that patch is required; moving to a
new minor is an explicit, tested consumer decision. Core's owner requirements are
declared in
`bindings/rust/nirs4all/Cargo.toml` and `bindings/wasm-native/Cargo.toml`
(`dag-ml*` `"0.5"`, `dag-ml-data*` `"0.2"`, `n4m` `"0.5"`, `nirs4all-formats`
`"0.3"`, `nirs4all-io*` `"0.4"`), the Python extras (`dag-ml>=0.5,<0.6`,
`dag-ml-data>=0.2,<0.3`, `nirs4all-formats>=0.3,<0.4`, `nirs4all-io>=0.4,<0.5`,
`nirs4all-datasets>=0.4,<0.5`, `nirs4all-methods`/`pls4all>=1.5.0,<1.6`) and the npm
peers (DMD `^0.2.0`, Formats `^0.3.0`, IO and Datasets `^0.4.0`, DAG `^0.5.0`, Methods `~1.5.0`). Exact, reproducible versions live
only in `Cargo.lock`, `bindings/wasm/package-lock.json`, the `compat/upstreams.toml`
parity refs and the release order, so an owner patch within its selected
series (for example a compatible IO 0.4 patch) needs no Core release. The crates and bindings of this repository
stay in lockstep through `scripts/bump_version.sh`.

Native Methods libraries
are accepted by the runtime ABI check, not by the package version: the
current candidate cohort uses Methods 1.5.0 (ABI 2.24, N4MM format 3), and
a library lacking a required ABI is refused at runtime. The full policy is
`docs/contracts/release/VERSION_CONSTRAINTS.md` in `nirs4all-ecosystem`.

Enforcement: `bindings/python/tests/test_release_topology.py` checks owner
requirements in the Rust, Python and npm manifests, including a required patch
floor inside one tested Python series and tilde ranges for 1.x npm owners;
`cargo --locked` and `npm ci` keep the exact cohort; `.n4a` container and
inventory fixtures are checked by the Rust and binding suites. A guard that
fails when a contract file changes without a minor bump is not automated yet.
