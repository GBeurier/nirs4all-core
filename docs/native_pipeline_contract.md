# Native pipeline JSON contract

Python `run_pipeline`, R `nirs4all_run_pipeline`, Node `runPipeline`, and MATLAB/Octave `runPipeline` transport a public dataset plus a recipe to the native Core CLI. A recipe contains ordered `steps` (method_id, declared role, parameter map) and `candidates` (final model parameter maps). Native Methods resolves catalog identities and DAG executes fold training, OOF selection, full refit, and cold prediction. Transformer and selector state is fitted within each training fold.

The finite qualified CPU corpus starts with raw PLS and standard_scale→Ridge, including complete multiple targets and declared grouped folds. Native catalog classifier dispatch is available; classification corpus and browser generic pipelines are qualified separately. Classification labels outside exact f32 integer representation are refused by IO rather than rounded.

Dependent origins and groups require explicitly declared folds. Native IO/DAG validate identity and reject group or origin overlap. Prediction provides explicit fresh sample IDs and executes PREDICT from captured N4ME states without fitting.

Export writes an atomic, non-overwriting JSON file. Its schema is `nirs4all.native-pipeline.v1`, version 1, containing exact config, native TrainingOutcome and portable Package V2. The config is checked against signed campaign recipe metadata; package fingerprints, refit artifact families and outcome links are native-validated before load/export/predict. R, Node and MATLAB preserve package JSON bytes so uint64 fingerprints are never rounded through host JSON numbers. This envelope has its own JSON transport; generic Archive V2/V3 export is a separate qualification profile.

The Node API currently requires a native CLI host; it refuses browser execution. Python, R and MATLAB bindings contain no numerical learning implementation. Arbitrary SDK Python estimators and arbitrary DL weights are outside this contract.

Explicit Methods library paths supplied at training or loading are retained by the in-memory CPU facades for prediction and retraining. A per-call path overrides that runtime. Exported native model JSON contains no host library path; a new host supplies its own runtime when loading.
