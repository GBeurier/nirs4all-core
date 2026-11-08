# Native package V1 transport fuzz target

`native_package_bytes` sends hostile input to the shared neutral V1 container
and selected native Package2/3 scientific closure loader. It does not invoke Python or duplicate
ZIP, manifest, inventory, or payload validation.

With `cargo-fuzz` and a nightly Rust toolchain already installed, check or run it
from the repository root:

```console
cargo +nightly fuzz check native_package_bytes
cargo +nightly fuzz run native_package_bytes -- -max_len=2097152
```

The example campaign length limit is a fuzzer policy, not a V1 content-size
limit. The target declares no intrinsic payload or entry-count ceiling.

Generated corpora and crash artifacts are intentionally untracked. Qualifying
SEC-001 still requires a separately recorded fuzz campaign and corpus review.
