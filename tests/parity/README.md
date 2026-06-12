# Parity Fixtures

This directory holds cross-runtime parity fixtures for `nirs4all-lite`.

The `portable_*.json` / `portable_*.yaml` pairs use the same `pipeline:`
envelope as the full Python `nirs4all` `examples/pipeline_samples` syntax.
The initial shared cases are:

- SNV + PLS;
- Savitzky-Golay + PLS;
- Kennard-Stone + SNV + PLS;
- Kennard-Stone + SNV + Savitzky-Golay + PLS `n_components` sweep via
  `_range_` / `param`.

Each fixture should include:

- input data or a DOI/catalog reference;
- a pipeline descriptor;
- expected outputs and tolerances;
- upstream version metadata;
- the full Python `nirs4all` comparison result when the pipeline is equivalent.

Do not add generated private datasets or draft-paper material here.
