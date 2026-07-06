# R Binding

R package name: `nirs4all`

The canonical source repository is `nirs4all-core`; the R publication keeps the
bare `nirs4all` name because the lite->core rename only changes the Python
distribution identity.

The R package publishes the aggregate registry and delegates only to upstream R
bindings that exist and are installed. Domains without an R binding, currently
`dag_ml`, remain metadata-only until the upstream package is published. Future
formula/data-frame controllers are gated on those upstream execution paths.

The portable execution gate is available as
`nirs4all_run_portable_pipeline()`. It delegates Kennard-Stone, SNV,
Savitzky-Golay, and PLS to the installed `n4m` methods binding and is validated
against the same full Python `nirs4all` oracle as the Python and WASM bindings.
Savitzky-Golay defaults to `mode = "interp"` for full nirs4all parity and
preserves explicit methods-backed modes plus `cval`.
