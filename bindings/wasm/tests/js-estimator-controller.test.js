import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import * as dagMl from 'dag-ml-wasm';
import {
  createDagMlNodeResult,
  createJsEstimatorController,
  createN4mModelController,
  createRandomForestController,
} from '../src/index.js';
import { requireMethodsArtifact } from './methods-artifact.js';

dagMl.initSync({ module: readFileSync(fileURLToPath(
  new URL('../node_modules/dag-ml-wasm/dag_ml_wasm_bg.wasm', import.meta.url),
)) });

const ids = Array.from({ length: 12 }, (_, index) => `s${index}`);
const X = ids.map((_, index) => [index, index % 3]);
const y = ids.map((_, index) => 2 * index + (index % 3));

function fixture() {
  const foldSet = JSON.parse(dagMl.kfold_split_json(
    JSON.stringify({ n_splits: 3, shuffle: true, seed: 42 }),
    JSON.stringify(ids),
    'outer',
  ));
  const artifact = JSON.parse(dagMl.compile_pipeline_dsl_artifact_json(JSON.stringify({
    id: 'test-js-controller',
    pipeline: [{ sources: ['x'] }, { split: { type: 'KFold', n_splits: 3 } },
      { model: 'RandomForest', params: { n_estimators: 12 } }],
    root_seed: 42,
  })));
  artifact.campaign_template.split_invocation.fold_set = foldSet;
  return { foldSet, artifact };
}

function planFor(controller, artifact) {
  const manifest = JSON.stringify([controller.manifest]);
  const plan = dagMl.build_execution_plan_json('plan:js-test', JSON.stringify(artifact.graph),
    JSON.stringify(artifact.campaign_template), manifest);
  return { plan, manifest };
}

function execute(controller, plan, manifest, phase) {
  return JSON.parse(dagMl.execute_execution_plan_phase_json(plan, manifest, 'run:js-test',
    42, phase, controller.invoke));
}

test('generic JS estimator follows native DAG-ML folds, phases and exact seed', () => {
  const { foldSet, artifact } = fixture();
  const fitCohorts = [];
  const seeds = [];
  const controller = createJsEstimatorController({
    dagMl, controllerId: 'controller:js.test', foldSet,
    dataset: { sampleIds: ids, X, y },
    createEstimator({ exactSeed }) {
      seeds.push(exactSeed);
      return {
        fit(rows, targets) {
          fitCohorts.push(rows.map((row) => row[0]));
          this.mean = targets.reduce((sum, value) => sum + value, 0) / targets.length;
        },
        predict(rows) { return rows.map(() => this.mean); },
      };
    },
  });
  const { plan, manifest } = planFor(controller, artifact);
  const cv = execute(controller, plan, manifest, 'FIT_CV');
  assert.equal(cv.length, 3);
  for (let index = 0; index < cv.length; index++) {
    const result = cv[index];
    const fold = foldSet.folds.find((entry) => entry.fold_id === result.lineage.fold_id);
    assert.deepEqual(result.predictions[0].sample_ids, fold.validation_sample_ids);
    assert.deepEqual(fitCohorts[index], fold.train_sample_ids.map((id) => Number(id.slice(1))));
    assert.equal(result.predictions[0].partition, 'validation');
    assert.equal(typeof seeds[index], 'string');
    assert.match(seeds[index], /^\d+$/);
  }
  assert.deepEqual(new Set(cv.flatMap((result) => result.predictions[0].sample_ids)), new Set(ids));
  execute(controller, plan, manifest, 'REFIT');
  assert.deepEqual(fitCohorts.at(-1), foldSet.sample_ids.map((id) => Number(id.slice(1))));
  controller.setPredictionDataset({ sampleIds: ['future:0'], X: [[15, 0]] });
  const prediction = execute(controller, plan, manifest, 'PREDICT')[0].predictions[0];
  assert.deepEqual(prediction.sample_ids, ['future:0']);
  assert.equal(prediction.values[0][0], y.reduce((sum, value) => sum + value, 0) / y.length);
  assert.equal(prediction.partition, 'final');
});

test('random forest regression is reproducible and its model reloads', async () => {
  const { foldSet, artifact } = fixture();
  const options = { dagMl, foldSet, dataset: { sampleIds: ids, X, y } };
  const first = await createRandomForestController(options);
  const second = await createRandomForestController(options);
  const a = planFor(first, artifact);
  const b = planFor(second, artifact);
  const cvA = execute(first, a.plan, a.manifest, 'FIT_CV');
  const cvB = execute(second, b.plan, b.manifest, 'FIT_CV');
  assert.deepEqual(cvA.map((result) => result.predictions), cvB.map((result) => result.predictions));
  execute(first, a.plan, a.manifest, 'REFIT');
  const future = { sampleIds: ['f0', 'f1'], X: [[3, 0], [9, 0]] };
  const prediction = first.predict(future);
  assert.equal(prediction.length, 2);
  assert.ok(prediction.every(Number.isFinite));
  second.importModel(JSON.parse(JSON.stringify(first.exportModel())));
  assert.deepEqual(second.predict(future), prediction);
  assert.throws(() => second.predict({ sampleIds: ['f'], X: [[1]] }), /feature count/);
});

test('random forest classification yields numeric class predictions', async () => {
  const { foldSet } = fixture();
  const controller = await createRandomForestController({
    dagMl, foldSet, task: 'classification',
    dataset: { sampleIds: ids, X, y: ids.map((_, index) => Number(index >= 6)) },
  });
  controller.fitFull({ n_estimators: 8 }, '123456789123456789');
  const result = controller.predict({ sampleIds: ['f0', 'f1'], X: [[0, 0], [11, 2]] });
  assert.ok(result.every((value) => value === 0 || value === 1));
});

test('n4m controller preserves direct Methods WASM model predictions per DAG-ML fold', async (t) => {
  const artifactPath = requireMethodsArtifact(t);
  if (!artifactPath) return;
  const methods = await import(artifactPath.indexUrl.href);
  await methods.loadModule();
  const { foldSet, artifact } = fixture();
  const controller = createN4mModelController({
    dagMl, methods, modelType: 'PLSRegression', foldSet,
    dataset: { sampleIds: ids, X, y },
  });
  const { plan, manifest } = planFor(controller, artifact);
  const results = execute(controller, plan, manifest, 'FIT_CV');
  for (const result of results) {
    const fold = foldSet.folds.find((entry) => entry.fold_id === result.lineage.fold_id);
    const train = fold.train_sample_ids.map((id) => Number(id.slice(1)));
    const validation = fold.validation_sample_ids.map((id) => Number(id.slice(1)));
    const matrix = (indices) => ({
      data: Float64Array.from(indices.flatMap((index) => X[index])),
      rows: indices.length, cols: X[0].length,
    });
    const direct = methods.fitModel('PLSRegression', matrix(train), {
      data: Float64Array.from(train.map((index) => y[index])), rows: train.length, cols: 1,
    }, 1, []);
    const expected = methods.predictModel(direct, matrix(validation));
    const actual = result.predictions[0].values.map((row) => row[0]);
    assert.ok(actual.every((value, index) => Math.abs(value - expected.data[index]) < 1e-12));
  }
  execute(controller, plan, manifest, 'REFIT');
  const restored = createN4mModelController({
    dagMl, methods, modelType: 'PLSRegression', foldSet,
    dataset: { sampleIds: ids, X, y },
  });
  restored.importModel(JSON.parse(JSON.stringify(controller.exportModel())));
  assert.deepEqual(restored.predict({ sampleIds: ['future'], X: [[13, 1]] }),
    controller.predict({ sampleIds: ['future'], X: [[13, 1]] }));
});

test('rejects a FoldSet that does not match the dataset', () => {
  const { foldSet } = fixture();
  assert.throws(() => createJsEstimatorController({
    dagMl, controllerId: 'controller:js.test',
    dataset: { sampleIds: ids.slice(1), X: X.slice(1), y: y.slice(1) },
    foldSet, createEstimator: () => ({}),
  }), /sample IDs must exactly match/);
});

test('shared NodeResult helper leaves the seed for native DAG-ML injection', () => {
  const task = {
    node_plan: { node_id: 'model:0', controller_id: 'controller:js.test',
      controller_version: '1.0.0', params_fingerprint: 'fingerprint' },
    run_id: 'run:0', phase: 'FIT_CV', variant_id: 'variant:base',
    fold_id: 'fold0', branch_path: [],
  };
  const result = createDagMlNodeResult(task, {
    sampleIds: ['s1'], values: [[1.5]], targetNames: ['y'],
  });
  assert.equal(result.lineage.seed, null);
  assert.deepEqual(result.predictions[0].sample_ids, ['s1']);
});
