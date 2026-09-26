import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  N4M_TRAINED_PIPELINE_SCHEMA,
  N4mRolePipeline,
  loadPipelineDefinition,
  n4mRoleCapabilities,
  parseExecutionPlan,
  portableClassNames,
} from '../src/index.js';
import { requireMethodsArtifact } from './methods-artifact.js';

const fixtures = new URL('../../../tests/parity/', import.meta.url);
const read = (path) => JSON.parse(readFileSync(new URL(path, fixtures), 'utf8'));
const pythonTrained = read('fixtures/n4m_roles_v8_python_trained.json');
const rTrained = read('fixtures/n4m_roles_v8_r_trained.json');
const rOracle = read('expected/n4m_roles_v8_r_trained_oracle.json');

const matrix = (rows) => ({ X: rows, rows: rows.length, cols: rows[0].length });
const maxAbsDiff = (actual, expected) => {
  assert.equal(actual.length, expected.length);
  return Math.max(...actual.map((value, i) => Math.abs(value - expected[i])));
};

async function loadMethods(t) {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) return null;
  const methods = await import(artifact.indexUrl.href);
  if (typeof methods.methodClass !== 'function') {
    const message = 'the Methods JS/WASM build predates ABI 2.13 estimator roles';
    if (process.env.NIRS4ALL_CORE_REQUIRE_METHODS_PARITY === '1') throw new Error(message);
    t.skip(message);
    return null;
  }
  await methods.loadModule();
  return methods;
}

test('n4m:<method id> tokens resolve through the Methods manifest', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const recipe = pythonTrained.regression.envelope.recipe;

  assert.deepEqual(portableClassNames(recipe), [
    'n4m:filters.y_outlier', 'n4m:preprocessing.scatter.snv', 'n4m:filters.variance', 'n4m:models.pls.cppls',
  ]);
  assert.deepEqual(loadPipelineDefinition(recipe, { methods }).pipeline, recipe.pipeline);
  assert.throws(
    () => loadPipelineDefinition({ pipeline: ['n4m:not.a.method'] }, { methods }),
    /outside the current nirs4all-core portable subset: n4m:not\.a\.method$/,
  );
  assert.throws(() => parseExecutionPlan(loadPipelineDefinition(recipe, { methods })), /N4mRolePipeline\.fit/);

  const capabilities = await n4mRoleCapabilities({ methods });
  const cppls = capabilities.find((item) => item.token === 'n4m:models.pls.cppls');
  assert.deepEqual(cppls.roles, ['regressor']);
  assert.ok(cppls.parameters.includes('n_components'));
  assert.ok(capabilities.every((item) => !item.roles.includes('splitter')));
});

test('WASM replays Python- and R-trained v8 envelopes exactly', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const xTest = matrix(pythonTrained.x_test);

  const regression = await N4mRolePipeline.fromJSON(pythonTrained.regression.envelope, { methods });
  const predicted = regression.predict(xTest);
  assert.equal(predicted.cols, 1);
  assert.ok(maxAbsDiff(predicted.data, pythonTrained.regression.predict) <= 1e-12);
  assert.deepEqual(regression.toJSON(), pythonTrained.regression.envelope);
  regression.dispose();

  const classification = await N4mRolePipeline.fromJSON(
    JSON.stringify(pythonTrained.classification.envelope), { methods });
  assert.deepEqual(classification.predict(xTest).labels, pythonTrained.classification.predict);
  classification.dispose();

  const fromR = await N4mRolePipeline.fromJSON(rTrained, { methods });
  assert.ok(maxAbsDiff(fromR.predict(matrix(rOracle.x_test)).data, rOracle.predict) <= 1e-12);
  fromR.dispose();
});

test('JS-trained v8 envelopes round trip and match the Python fits', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const xTrain = pythonTrained.x_train;
  const xTest = matrix(pythonTrained.x_test);

  for (const [name, tolerance] of [['regression', 1e-9], ['classification', 0]]) {
    const { envelope, y_train: yTrain, predict: expected } = pythonTrained[name];
    const fitted = await N4mRolePipeline.fit(envelope.recipe, { ...matrix(xTrain), y: yTrain }, { methods });
    const text = JSON.stringify(fitted);
    const document = JSON.parse(text);
    assert.equal(document.schema, N4M_TRAINED_PIPELINE_SCHEMA);
    assert.deepEqual(document.states.map((state) => state.method_id), envelope.states.map((state) => state.method_id));

    const replayed = await N4mRolePipeline.fromJSON(text, { methods });
    const live = fitted.predict(xTest);
    assert.deepEqual(replayed.predict(xTest), live);
    if (name === 'regression') {
      assert.ok(maxAbsDiff(live.data, expected) <= tolerance);
      const retrained = await replayed.retrain({ ...matrix(xTrain), y: yTrain }, { methods });
      assert.deepEqual(retrained.predict(xTest), live);
      retrained.dispose();
    } else {
      assert.deepEqual(live.labels, expected);
      assert.deepEqual(document.states.at(-1).class_names, ['high', 'low']);
    }
    fitted.dispose();
    replayed.dispose();
  }
});

test('v8 envelopes refuse tampered or mismatched states', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const envelope = structuredClone(pythonTrained.regression.envelope);

  await assert.rejects(N4mRolePipeline.fromJSON({ ...envelope, schema: 'nirs4all.n4m.trained_pipeline.v7' }, { methods }),
    /Unsupported trained n4m pipeline envelope/);
  await assert.rejects(N4mRolePipeline.fromJSON({ ...envelope, states: envelope.states.slice(1) }, { methods }),
    /states do not match/);
  const tampered = structuredClone(envelope);
  tampered.states[0].sha256 = '0'.repeat(64);
  await assert.rejects(N4mRolePipeline.fromJSON(tampered, { methods }), /fails its checksum/);
  const swapped = structuredClone(envelope);
  swapped.recipe.pipeline[1] = 'n4m:preprocessing.scatter.msc';
  await assert.rejects(N4mRolePipeline.fromJSON(swapped, { methods }), /does not match its recipe step/);
});
