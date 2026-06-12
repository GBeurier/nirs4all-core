# R Binding

R package name: `nirs4all`

The R binding should expose formula/data-frame friendly wrappers while delegating
all computation to upstream bindings.

The portable execution gate is available as
`nirs4all_run_portable_pipeline()`. It delegates Kennard-Stone, SNV,
Savitzky-Golay, and PLS to the installed `n4m` methods binding and is validated
against the same full Python `nirs4all` oracle as the Python and WASM bindings.
