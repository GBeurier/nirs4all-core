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

Complete scientific qualification runs locally on Linux/WSL and native Windows.
Keep the full unit, native-versus-binding, full-SDK oracle, WASM/Octave and
cross-language end-to-end commands above; moving them out of GitHub does not
remove their release requirement. Native Windows coverage uses actual Windows
artifacts, not a WSL ELF process. Licensed MATLAB and explicitly deferred
platforms retain their existing separate scope.

Before publication, retain direct process commands/exits, hardware/toolchain
identities, logs/reports and the source/dependency/fixture/helper fingerprints
for each mandatory gate. The reviewed policy is `qualification/policy.json`;
`compat/local-qualification.json` references actual evidence under
`qualification/`. Verify it with:

```bash
python scripts/verify_local_qualification.py --project core --receipt compat/local-qualification.json --root .
```

The verifier rejects stale scoped inputs, changed evidence, missing hosts/gates,
unapproved skips and failed numerical observations. Historical results can be
inherited only through their retained original input manifest and exact
continuity evidence; the current Git SHA alone does not qualify old logs.

GitHub builds packages and runs bounded package/platform smokes. Dry-run builds
may precede local Windows qualification so their actual Windows artifacts can
be tested locally. Every publication channel checks the complete source-bound
local evidence before upload; no receipt is required merely to build a private
candidate. Source-only workflow/documentation changes do not invalidate an
unchanged numerical gate's reviewed input scope.

Run the complete Python suite with `python -m pytest bindings/python/tests`;
`unittest discover` alone omits the native workspace and CLI function tests.
Stage actual experiment/browser/native archive fixtures and the native runtime
before the suite so required qualification cases execute rather than skip.
Windows JavaScript parity can use the complete `test:js` suite against the
qualified WASM candidate produced by GitHub; building Rust on Windows locally
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

nirs4all packages from different repositories depend on each other at **minor**
granularity, never more precisely: Rust `"0.4"` (`>=0.4.0, <0.5.0`; `"1.3"` for
1.x), Python `>=0.4,<0.5` (`>=1.3,<2` for 1.x), npm `^0.4.0` (`^1.3.0`), R
`pkg (>= 0.4.0), pkg (< 0.5.0)`. Core's owner requirements follow it in
`bindings/rust/nirs4all/Cargo.toml` and `bindings/wasm-native/Cargo.toml`
(`dag-ml*` `"0.4"`, `dag-ml-data*` `"0.2"`, `n4m` `"0.4"`, `nirs4all-formats`
`"0.2"`, `nirs4all-io*` `"0.3"`), the Python extras (`dag-ml>=0.4,<0.5`,
`dag-ml-data>=0.2,<0.3`, `nirs4all-formats>=0.2,<0.3`, `nirs4all-io>=0.3,<0.4`,
`nirs4all-datasets>=0.3,<0.4`, `nirs4all-methods`/`pls4all>=1.3,<2`) and the npm
peers (`^0.2.0`, `^0.3.0`, `^0.4.0`, `^1.3.0`). Exact, reproducible versions live
only in `Cargo.lock`, `bindings/wasm/package-lock.json`, the `compat/upstreams.toml`
parity refs and the release order, so an owner patch (for example IO 0.3.6 or
dag-ml 0.4.3) needs no Core release. The crates and bindings of this repository
stay in lockstep through `scripts/bump_version.sh`.

In return, a **patch** release of Core is backward compatible and changes no
wire format, byte contract, persisted schema, `.n4a` container/inventory or
payload semantics, native ABI or shared fixture; any such change needs at least
a **minor** release (for 0.x, every minor is breaking). Native Methods libraries
are accepted by the runtime ABI check, not by the package version: the
current cohort is qualified with Methods 1.3.18 (ABI 2.23, N4MM format 3), and
a library lacking a required ABI is refused at runtime. The full policy is
`docs/contracts/release/VERSION_CONSTRAINTS.md` in `nirs4all-ecosystem`.

Enforcement: `bindings/python/tests/test_release_topology.py` refuses an owner
requirement more precise than the minor in the Rust, Python and npm manifests;
`cargo --locked` and `npm ci` keep the exact cohort; `.n4a` container and
inventory fixtures are checked by the Rust and binding suites. A guard that
fails when a contract file changes without a minor bump is not automated yet.
