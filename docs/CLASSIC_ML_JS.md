# Classical ML adapters for the browser

The `nirs4all/classic-ml` entry keeps classical JavaScript ML dependencies out
of the base portable runtime. The loader is asynchronous once, before a run;
the controller factory, fit and predict callbacks are synchronous. DAG-ML WASM
currently requires this: its JS callback must return a node-result string
immediately. Blocking a Promise on the browser worker would deadlock work that
needs that worker's event loop.

## Qualified scope

| Library | Controller support | Host estimator support | Model storage |
| --- | --- | --- | --- |
| ml.js (`ml` 8.x) | Random forest/decision tree regression and classification; KNN classification | Same, plus PCA fit/transform | Its own JSON; same library/version required for reliable replay |
| scikitjs 1.24 | Synchronous decision tree regression and classification | Trees, transformers such as StandardScaler, and async estimators such as LinearRegression | Its own async JSON serializer; controller export/import are async |
| Kanaries ML 1.1 | None yet | None yet | Candidate for a later qualification |

The shared façade follows the practical `BaseEstimator`/`TransformerMixin`
shape (`getParams`, `setParams`, `clone`, `fit`, `predict`/`transform`,
`fitTransform`) without claiming numerical parity with scikit-learn. ml.js
provides matrix, decomposition and statistics modules, but this is a selected
toolbox rather than SciPy compatibility. A JSON ML model is never labeled as a
portable native n4m artifact.

For [Kanaries ML](https://github.com/Kanaries/ml), the published 1.1 package has no runtime dependencies and has
a direct `BaseEstimator` / `TransformerBase` API, random forests and PCA. Its
repository has numerous tests and a CI job that checks fixtures pinned to
scikit-learn. It is also young and has little independent adoption. A local
round-trip of a random forest through `toJSON` / `loadModel` reproduced its own
predictions, but that is insufficient to claim cross-language or scientific
parity. Its options use camelCase (`nEstimators`, `randomState`); snake_case
arguments were silently ignored in a probe. We therefore defer an adapter until
parameter rejection, independently generated numerical references, artifact
compatibility, and browser bundle size are qualified.

TensorFlow.js is optional and loaded only through scikitjs; it is not required
for ml.js controllers or n4m pipelines. TensorFlow.js 3.x is needed by the
current scikitjs release. `createScikitJsAsyncController()` awaits fit,
prediction and serialization for any class exposed by scikitjs, and returns
the same NodeResult shape for a host that supplies a native DAG-ML NodeTask.
It does **not** plug into `execute_campaign_phase_json()`: that WASM entrypoint
requires its callback to return a JSON string synchronously. Blocking the
same browser worker while awaiting a Promise would prevent its microtasks from
running. A future native async phase driver is required for asynchronous
estimators to participate in DAG-ML-managed CV. Do not substitute host-made
fold loops for DAG-ML's seed, fold and lineage authority.

## Capability inventory (0.3.35)

| Area | JS/WASM package | nirs4all-web pipeline nodes | Remaining gap |
| --- | --- | --- | --- |
| Portable n4m | Methods-backed pipeline and serialized PLS replay, strict Python oracle gate | n4m preprocessing and 25 model nodes | Archive V2 replay remains the qualified single-predictor Methods subset |
| ml.js supervised | Random forest and CART regression/classification; KNN classification, all with synchronous DAG-ML controllers | Five nodes: two forests, two trees, KNN classifier | Other ml.js models need independent validation before binding |
| ml.js transforms | PCA fit/transform/inverseTransform host adapter | No ml.js PCA pipeline node; the Explore PCA view is separate | Pipeline transformer controller and fitted-state replay |
| scikitjs synchronous | Tree regression/classification DAG-ML controllers; StandardScaler host transformer | No scikitjs nodes | Browser dependency/bundle and artifact integration |
| scikitjs asynchronous | Generic host estimator and awaiting controller; LinearRegression fit/predict/serialization exercised | No async training nodes | Native DAG-ML asynchronous callback/phase driver |
| Kanaries ML | Evaluated, no adapter | None | Parameter, numerical, artifact and browser qualification |
| TensorFlow.js / Torch / ONNX | TensorFlow.js only as optional scikitjs backend; no neural-network controller | None | Dedicated framework and portable-model work |

The package exposes a selected ml.js toolbox, not a complete sklearn or SciPy
implementation. Only n4m-backed portable models have the cross-language binary
and scientific parity claim; ml.js and scikitjs artifacts stay library-specific.

A production Vite browser probe loaded scikitjs and TensorFlow.js 3.21, fit a
decision tree, and predicted `[1]` for input `[[1]]`. Its generated JavaScript
chunks totalled about 3.26 MB uncompressed (about 0.65 MB gzip), with no new
WASM file. This is a load-size observation, not a benchmark of training time or
broader algorithm accuracy.
