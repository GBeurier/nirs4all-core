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

For Kanaries ML, the published 1.1 package has no runtime dependencies and has
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
current scikitjs release. Asynchronous scikitjs models remain host-only until
DAG-ML offers an asynchronous WASM callback path.

A production Vite browser probe loaded scikitjs and TensorFlow.js 3.21, fit a
decision tree, and predicted `[1]` for input `[[1]]`. Its generated JavaScript
chunks totalled about 3.26 MB uncompressed (about 0.65 MB gzip), with no new
WASM file. This is a load-size observation, not a benchmark of training time or
broader algorithm accuracy.
