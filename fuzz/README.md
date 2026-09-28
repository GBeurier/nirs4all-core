# Archive V2 fuzz target

`archive_v2_bytes` sends at most 2 MiB of hostile input to Core's canonical
in-memory Archive V2 parser and validator. It does not invoke Python or duplicate
ZIP, manifest, inventory, or payload validation.

With `cargo-fuzz` and a nightly Rust toolchain already installed, check or run it
from the repository root:

```console
cargo +nightly fuzz check archive_v2_bytes
cargo +nightly fuzz run archive_v2_bytes -- -max_len=2097152
```

Generated corpora and crash artifacts are intentionally untracked. Qualifying
SEC-001 still requires a separately recorded fuzz campaign and corpus review.
