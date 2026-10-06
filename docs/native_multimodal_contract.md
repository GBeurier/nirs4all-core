# Native multimodal workflow profile

`run_multimodal` (Python), `runMultimodal` (Node/Octave facade), and
`nirs4all_run_multimodal` (R) transport a public IO dataset v2, catalog-native
pipeline recipe, and ordered source policies to Core. IO owns source alignment,
relations, target masks and source assembly; Methods owns feature computation and
model state; DAG owns fold-scoped fitting, OOF scoring, selection and full refit.
Bindings perform no numerical imputation or fitting.

The finite qualified profile combines complete rank-2 dense numeric sources with
`ragged_series` sources. Source policies must name every input source in IO order:

```json
[
  {"source_id":"spectra","encoder":"identity","missing_policy":"reject"},
  {"source_id":"dynamics","encoder":"ragged_summary","missing_policy":"zero_with_indicator"}
]
```

`identity` accepts complete dense numeric rows. `ragged_summary` executes the
Methods procedure `utilities.ragged_summary`: channel-wise mean, population
standard deviation, minimum and maximum, then sequence length, duration and a
presence indicator. Time coordinates are finite and strictly increasing within
each present sequence. Empty sequences require explicit absence. The default
`reject` policy refuses absence. The opt-in `zero_with_indicator` policy encodes
an absent sequence with zeros and indicator zero; present sequences retain
indicator one. There is no implicit padding or learned imputation. The shared
Methods packed-matrix contract refuses a cohort with no observed packed rows.

The final native regressor or classifier is instantiated independently for each target.
`target_mask=true` means observed. IO normalizes masked placeholders, while DAG
filters them out of supervised fitting and OOF metrics. Fold transformers use
only fold training features, and target observation never makes a validation
row eligible for fitting. A target with no observed training rows fails closed.
Complete multi-target pipelines remain available through `run_pipeline`.
Dependent groups and origins require explicit leakage-safe folds. Sample IDs,
target names, masks, mask fingerprints, source schemas and projection provenance
are carried through native training and replay. Native lineage reports supervised
fit and observed-row counts; training influence remains a conservative fold
training superset when target observations are partial.

Transport is `nirs4all.native-multimodal.v1` JSON containing one
`nirs4all.native-pipeline.v1` package per target. It captures recipes, policies,
raw source schemas and projection fingerprints. Export is atomic and refuses
overwrite. Load validates native package/outcome closure and exact recipe-to-graph
correspondence. Node/R/Octave retain original JSON bytes to preserve uint64 values.
This profile is separate from `.n4a` and does not promise arbitrary SDK/DL model
portability. Prediction takes target-free IO v2 data, validates captured source
schemas, recomputes stateless native projections and imports fitted N4ME states
without FIT or REFIT. CPU CLI hosts are required for this multimodal profile;
browser generic pipelines have a separate qualification.

Qualification uses independent NumPy ragged summaries and sklearn
StandardScaler/Ridge and binary PLS-LDA models per target, explicit partial masks and missing
sequences, grouped-fold complete multi-target pipelines, binary PLS-LDA, and
fresh-process cold replay. Malformed offsets, presence, time coordinates,
nonfinite features, unobserved targets and altered captured policies are refused.
Licensed MATLAB execution is outside this profile; Octave exercises its thin
transport facade.
