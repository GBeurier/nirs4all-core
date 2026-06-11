# Release Plan

Release artifacts should be built from the same upstream lock:

- Rust crate: `nirs4all`
- Python wheel/sdist: `nirs4all-lite`
- npm package: `nirs4all`
- R source package: `nirs4all`
- MATLAB/Octave archive: `nirs4all`
- WASM bundle consumed by `nirs4all-web`

Before release:

1. Pin upstream versions or SHAs in `compat/upstreams.toml`.
2. Build each binding from the same lock.
3. Run upstream binding parity gates.
4. Run lite cross-language parity gates.
5. Run equivalent-pipeline checks against full Python `nirs4all`.
6. Publish artifacts and record provenance in the release notes.

Local artifact commands:

```bash
make test
make build-python
make build-npm
make build-r
make build-matlab
cargo package -p nirs4all
```

`R CMD build/check`, Octave smoke tests, and CRAN/R-universe validation require
R/Octave toolchains. They are part of CI because they may not be available on
every development workstation.

Every CI run uploads the build outputs as artifacts (`rust-crate`, `python-*`,
`npm-wasm`, `r-source`, and `matlab-octave`). Tagged releases attach the same
artifact families to the GitHub Release.
