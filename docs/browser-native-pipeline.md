# Catalog-native browser pipelines

`runBrowserPipeline` executes a linear native Methods recipe through DAG-ML's
WASM training service. Each transform is fitted inside its own training fold;
DAG validates the folds, selects the OOF candidate and orchestrates the full-data
refit. Core does not implement fitting, selection or prediction numerics.

```javascript
import { runBrowserPipeline, loadBrowserPipeline } from 'nirs4all';

const pipeline = {
  steps: [
    { method_id: 'preprocessing.scaling.standard_scale', role: 'transformer', params: {} },
    { method_id: 'models.regularized.ridge', role: 'regressor', params: {} },
  ],
  candidates: [{ alpha: 0.1 }, { alpha: 1 }],
};
const model = await runBrowserPipeline(trainingDataset, { pipeline, sourceId: 'spectra' });
const exportedText = model.export();
const restored = await loadBrowserPipeline(exportedText);
const prediction = await restored.predict(targetFreePredictionDataset);
```

IO's public dataset owns row alignment and target shape. This profile accepts one
complete numeric source, one or more fully observed regression targets, or one
integer classification target. The last step is a native regressor/classifier;
preceding steps are catalog-native transformers/selectors. Parameters are checked
against the live Methods manifest before any FIT. Browser integer parameters must be exactly representable safe JavaScript integers; CPU recipes with larger int64 parameters are refused by this profile. Training-row retention and
additional fit inputs (weights, blocks, domain, groups as model inputs) are refused.

Groups require explicit fold IDs. Native leakage validation checks their split;
repetition and independent-unit overrides require a different qualified profile.
The browser qualification covers StandardScale → Ridge with multiple targets and
grouped folds, and binary PLS-LDA label prediction. It does not establish parity
for every method in the catalog or every classifier probability surface.

Every refitted node retains its own opaque N4ME state and native descriptor in
`nirs4all.native-pipeline.v1`. The exact JSON string is the transport: do not parse
and stringify it, because native uint64 seeds can exceed JavaScript's exact range.
The shared Rust envelope validator checks package/outcome closure before loading.
Native Methods import checks method identity, capabilities and planned parameters
again before prediction. Hydrated state is released even when replay fails.

The CPU and browser consumers use the same pipeline envelope and N4ME states.
`NativePipeline.load(exportedText, {cli})` exposes the native CPU CLI transport in
Node; Python/R/MATLAB use the native CPU facade. Prediction performs no FIT.
This envelope is not an Archive V2 `.n4a`; its artifact kind is `n4m_estimator`.
The existing bounded workflow `.n4a` remains a separate documented format.

This API needs the matching Core, DAG WASM and IO matrix-projection distribution
cohort. A source checkout qualified with local upstream patches is development
evidence, not proof that an earlier public version exposes the new API.
