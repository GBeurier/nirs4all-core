import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import * as dagMl from 'dag-ml-wasm';
import {
  createMlJsController, createMlJsEstimator, createMlJsPca,
  createScikitJsAsyncController, createScikitJsController, createScikitJsEstimator, loadMlJs, loadScikitJs,
} from 'nirs4all/classic-ml';

dagMl.initSync({ module: readFileSync(fileURLToPath(
  new URL('../node_modules/dag-ml-wasm/dag_ml_wasm_bg.wasm', import.meta.url),
)) });

const sampleIds = Array.from({ length: 12 }, (_, index) => `s${index}`);
const X = sampleIds.map((_, index) => [index, index % 3]);
const y = sampleIds.map((_, index) => 2 * index + (index % 3));

function planFor(controller) {
  const foldSet = JSON.parse(dagMl.kfold_split_json(
    JSON.stringify({ n_splits: 3, shuffle: true, seed: 42 }), JSON.stringify(sampleIds), 'outer',
  ));
  const artifact = JSON.parse(dagMl.compile_pipeline_dsl_artifact_json(JSON.stringify({
    id: 'ml-adapters',
    pipeline: [{ sources: ['x'] }, { split: { type: 'KFold', n_splits: 3 } },
      { model: 'DecisionTree', params: {} }],
    root_seed: 42,
  })));
  artifact.campaign_template.split_invocation.fold_set = foldSet;
  const manifests = JSON.stringify([controller.manifest]);
  const plan = dagMl.build_execution_plan_json('plan:ml-adapters', JSON.stringify(artifact.graph),
    JSON.stringify(artifact.campaign_template), manifests);
  return { foldSet, plan, manifests };
}

function execute(controller, plan, manifests, phase) {
  return JSON.parse(dagMl.execute_execution_plan_phase_json(
    plan, manifests, 'run:ml-adapters', 42, phase, controller.invoke,
  ));
}

test('ml.js exposes sklearn-style fit/predict and PCA transform without local numerics', async () => {
  const ml = await loadMlJs();
  const tree = createMlJsEstimator({ ml, estimatorName: 'DecisionTreeRegressor' });
  assert.deepEqual(tree.getParams(), {});
  tree.setParams({ minNumSamples: 1 }).fit(X, y);
  const predicted = tree.predict(X);
  assert.equal(predicted.length, X.length);
  assert.ok(predicted.every(Number.isFinite));
  const restored = createMlJsEstimator({ ml, estimatorName: 'DecisionTreeRegressor' })
    .load(JSON.parse(JSON.stringify(tree.toJSON())));
  assert.deepEqual(restored.predict(X), predicted);

  const pca = createMlJsPca({ ml }).fit(X);
  const projection = pca.transform(X);
  assert.equal(projection.length, X.length);
  assert.equal(projection[0].length, X[0].length);
  assert.deepEqual(createMlJsPca({ ml }).load(JSON.parse(JSON.stringify(pca.toJSON()))).transform(X), projection);
  const recovered = pca.inverseTransform(projection);
  assert.ok(recovered.every((row, i) => row.every((value, j) => Math.abs(value - X[i][j]) < 1e-8)));
});

test('ml.js random forest regression and classification survive JSON round trips', async () => {
  const ml = await loadMlJs();
  const cases = [
    ['RandomForestRegressor', y, { nEstimators: 7, seed: 19, noOOB: true }],
    ['RandomForestClassifier', y.map((value) => Number(value > 11)), { nEstimators: 7, seed: 19, noOOB: true }],
    ['KNeighborsClassifier', y.map((value) => Number(value > 11)), { k: 3 }],
  ];
  for (const [estimatorName, target, params] of cases) {
    const model = createMlJsEstimator({ ml, estimatorName, params }).fit(X, target);
    const predicted = model.predict(X);
    assert.equal(predicted.length, X.length);
    assert.ok(predicted.every(Number.isFinite));
    const restored = createMlJsEstimator({ ml, estimatorName }).load(JSON.parse(JSON.stringify(model.toJSON())));
    assert.deepEqual(restored.predict(X), predicted, `${estimatorName} restore`);
    if (estimatorName === 'KNeighborsClassifier') {
      const artifact = structuredClone(model.toJSON());
      const workerRestored = createMlJsEstimator({ ml, estimatorName }).load(artifact);
      assert.deepEqual(workerRestored.predict([[-1, 0], [13, 1]]), model.predict([[-1, 0], [13, 1]]));
      assert.doesNotThrow(() => JSON.stringify(artifact), 'loading must not add circular parent links');
    }
  }
});

test('ml.js classifiers survive worker structuredClone before new-sample prediction', async () => {
  const ml = await loadMlJs();
  const future = [[-1, 0], [13, 1]];
  for (const estimatorName of ['DecisionTreeClassifier', 'RandomForestClassifier']) {
    const model = createMlJsEstimator({ ml, estimatorName,
      params: { nEstimators: 7, seed: 19, noOOB: true } })
      .fit(X, y.map((value) => Number(value > 11)));
    const expected = model.predict(future);
    const cloned = structuredClone(model.toJSON());
    const restored = createMlJsEstimator({ ml, estimatorName }).load(cloned);
    assert.deepEqual(restored.predict(future), expected, `${estimatorName} worker transfer`);
    assert.deepEqual(createMlJsEstimator({ ml, estimatorName })
      .load(JSON.parse(JSON.stringify(cloned))).predict(future), expected,
    `${estimatorName} worker transfer and JSON persistence`);
  }
});

test('ml.js tree controller uses native DAG-ML folds and serialized model', async () => {
  const ml = await loadMlJs();
  const foldSet = JSON.parse(dagMl.kfold_split_json(
    JSON.stringify({ n_splits: 3, shuffle: true, seed: 42 }), JSON.stringify(sampleIds), 'outer',
  ));
  const controller = createMlJsController({ ml, estimatorName: 'DecisionTreeRegressor',
    dagMl, foldSet, dataset: { sampleIds, X, y } });
  assert.equal(typeof controller.then, 'undefined');
  const { plan, manifests } = planFor(controller);
  const cv = execute(controller, plan, manifests, 'FIT_CV');
  assert.equal(cv.length, 3);
  for (const result of cv) {
    const fold = foldSet.folds.find((entry) => entry.fold_id === result.lineage.fold_id);
    assert.deepEqual(result.predictions[0].sample_ids, fold.validation_sample_ids);
  }
  execute(controller, plan, manifests, 'REFIT');
  const next = { sampleIds: ['future'], X: [[5, 2]] };
  const expected = controller.predict(next);
  const restored = createMlJsController({ ml, estimatorName: 'DecisionTreeRegressor',
    dagMl, foldSet, dataset: { sampleIds, X, y } });
  restored.importModel(JSON.parse(JSON.stringify(controller.exportModel())));
  assert.deepEqual(restored.predict(next), expected);
});

test('scikitjs loads with TensorFlow.js and preserves sync tree plus async linear model', async () => {
  const sk = await loadScikitJs();
  const tree = createScikitJsEstimator({ scikitJs: sk, estimatorName: 'DecisionTreeRegressor' });
  assert.equal(typeof tree.fit(X, y)?.then, 'undefined');
  const predicted = tree.predict(X);
  assert.equal(predicted.length, X.length);
  assert.ok(predicted.every(Number.isFinite));
  const artifact = await tree.toJSON();
  const restored = await createScikitJsEstimator({ scikitJs: sk, estimatorName: 'DecisionTreeRegressor' })
    .load(artifact);
  assert.deepEqual(restored.predict(X), predicted);

  const linear = createScikitJsEstimator({ scikitJs: sk, estimatorName: 'LinearRegression' });
  const trained = linear.fit(X, y);
  assert.equal(typeof trained?.then, 'function');
  await trained;
  const linearPredictions = linear.predict(X);
  assert.equal(linearPredictions.length, X.length);
  assert.ok(linearPredictions.every(Number.isFinite));
});

test('scikitjs StandardScaler follows TransformerMixin-style fit/transform and reload', async () => {
  const sk = await loadScikitJs();
  const scaler = createScikitJsEstimator({ scikitJs: sk, estimatorName: 'StandardScaler' });
  const transformed = scaler.fitTransform(X);
  assert.equal(transformed.length, X.length);
  assert.equal(transformed[0].length, X[0].length);
  for (let column = 0; column < X[0].length; column += 1) {
    const mean = transformed.reduce((sum, row) => sum + row[column], 0) / transformed.length;
    assert.ok(Math.abs(mean) < 1e-6);
  }
  const artifact = await scaler.toJSON();
  const restored = await createScikitJsEstimator({ scikitJs: sk, estimatorName: 'StandardScaler' })
    .load(artifact);
  const again = restored.transform(X);
  assert.ok(again.every((row, i) => row.every((value, j) => Math.abs(value - transformed[i][j]) < 1e-6)));
});

test('scikitjs sync tree controller runs DAG-ML, while async classes are refused', async () => {
  const sk = await loadScikitJs();
  const foldSet = JSON.parse(dagMl.kfold_split_json(
    JSON.stringify({ n_splits: 3, shuffle: true, seed: 42 }), JSON.stringify(sampleIds), 'outer',
  ));
  const options = { scikitJs: sk, dagMl, foldSet, dataset: { sampleIds, X, y } };
  assert.throws(() => createScikitJsController({ ...options, estimatorName: 'LinearRegression' }),
    /synchronous DAG-ML WASM callback/);
  const controller = createScikitJsController({ ...options, estimatorName: 'DecisionTreeRegressor' });
  assert.equal(typeof controller.then, 'undefined');
  const { plan, manifests } = planFor(controller);
  const cv = execute(controller, plan, manifests, 'FIT_CV');
  assert.equal(cv.length, 3);
  execute(controller, plan, manifests, 'REFIT');
  assert.throws(() => controller.exportModel(), /exportModelAsync/);
  const artifact = await controller.exportModelAsync();
  const restored = createScikitJsController({ ...options, estimatorName: 'DecisionTreeRegressor' });
  assert.throws(() => restored.importModel(artifact), /importModelAsync/);
  await restored.importModelAsync(artifact);
  const future = { sampleIds: ['future'], X: [[9, 1]] };
  assert.deepEqual(restored.predict(future), controller.predict(future));
});

test('scikitjs async controller awaits fit, prediction, and model restoration', async () => {
  const sk = await loadScikitJs();
  const foldSet = JSON.parse(dagMl.kfold_split_json(
    JSON.stringify({ n_splits: 3, shuffle: true, seed: 42 }), JSON.stringify(sampleIds), 'outer',
  ));
  const options = { scikitJs: sk, estimatorName: 'LinearRegression', dagMl, foldSet,
    dataset: { sampleIds, X, y } };
  const controller = createScikitJsAsyncController(options);
  const controllerId = controller.manifest.controller_id;
  const fold = foldSet.folds[0];
  const task = { phase: 'FIT_CV', run_id: 'run:async', fold_id: fold.fold_id,
    variant_id: 'base', branch_path: [], node_plan: {
      node_id: 'model:async', controller_id: controllerId,
      controller_version: '1.0.0', params: {}, params_fingerprint: 'test',
    } };
  const pendingFold = controller.invokeAsync(controllerId, JSON.stringify(task), '42');
  assert.equal(typeof pendingFold.then, 'function');
  const foldResult = JSON.parse(await pendingFold);
  assert.deepEqual(foldResult.predictions[0].sample_ids, fold.validation_sample_ids);
  assert.equal(foldResult.predictions[0].values.length, fold.validation_sample_ids.length);
  assert.ok(foldResult.predictions[0].values.every(([value]) => Number.isFinite(value)));

  await controller.fitFull({}, '42');
  const future = { sampleIds: ['future'], X: [[13, 1]] };
  const expected = await controller.predict(future);
  const artifact = await controller.exportModel();
  const restored = createScikitJsAsyncController(options);
  await restored.importModel(JSON.parse(JSON.stringify(artifact)));
  assert.deepEqual(await restored.predict(future), expected);
  await assert.rejects(restored.predict({ sampleIds: ['wrong'], X: [[1]] }), /feature count/);
});
