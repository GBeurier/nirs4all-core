// Generic n4m role recipes and the cross-language trained envelope v8.
//
// The negative envelope cases (recipe/state mismatch, empty pipeline, feature
// permutation, training rows without opt-in, multi-target routing) replay the
// Methods shared fixture n4m_role_pipeline_methods.json and are identical in
// the Python, JS/WASM and Rust suites, as are the label-table, n_features, NUL
// column-name, ragged-shape and attested-recipe mutations.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
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
const pythonNamed = read('fixtures/n4m_roles_v8_python_named.json');
const shared = read('fixtures/n4m_role_pipeline_methods.json');
const rTrained = read('fixtures/n4m_roles_v8_r_trained.json');
const rOracle = read('expected/n4m_roles_v8_r_trained_oracle.json');
const cases = Object.fromEntries(shared.cases.map((item) => [item.name, item]));

const matrix = (rows, featureNames) => ({ X: rows, rows: rows.length, cols: rows[0].length, featureNames });
const maxAbsDiff = (actual, expected) => {
  assert.equal(actual.length, expected.length);
  return Math.max(...actual.map((value, i) => Math.abs(value - expected[i])));
};
const escape = (text) => new RegExp(text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));

// A v8 envelope of Methods fixture states: {method_id, n4me_base64, contains_training_rows}
// or bare base64 bytes, then labelled with their recipe step.
function envelope(steps, states, featureNames, classNames) {
  const stateful = steps.map((step) => step.class.slice(4)).filter((id) => !id.startsWith('filters.'));
  const entries = states.slice(0, stateful.length).map((state, i) => {
    const entry = typeof state === 'string' ? { method_id: stateful[i], n4me_base64: state } : { ...state };
    entry.sha256 = createHash('sha256').update(Buffer.from(entry.n4me_base64, 'base64')).digest('hex');
    return entry;
  });
  if (classNames) entries.at(-1).class_names = classNames;
  const document = { schema: N4M_TRAINED_PIPELINE_SCHEMA, recipe: { pipeline: steps }, n_features: shared.x_train[0].length };
  if (featureNames) document.feature_names = featureNames;
  document.states = entries;
  return document;
}

// The envelope without its N4ME bytes (they record the writing ABI).
function withoutBytes(document) {
  const copy = structuredClone(document);
  copy.states.forEach((state) => {
    delete state.n4me_base64;
    delete state.sha256;
  });
  return copy;
}

async function loadMethods(t) {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) return null;
  const methods = await import(artifact.indexUrl.href);
  if (typeof methods.RolePipeline !== 'function') {
    const message = 'the Methods JS/WASM build predates ABI 2.14 role pipelines';
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

test('WASM replays envelopes written before the additive fields', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const xTest = matrix(pythonTrained.x_test);

  const regression = await N4mRolePipeline.fromJSON(pythonTrained.regression.envelope, { methods });
  const predicted = regression.predict(xTest);
  assert.equal(predicted.cols, 1);
  assert.ok(maxAbsDiff(predicted.data, pythonTrained.regression.predict) <= 1e-12);
  assert.equal(regression.featureNames, undefined);
  const rewritten = regression.toJSON();
  assert.equal('feature_names' in rewritten, false);
  assert.deepEqual(rewritten.states.map((state) => state.contains_training_rows), [false, false, false]);
  rewritten.states.forEach((state) => delete state.contains_training_rows);
  assert.deepEqual(withoutBytes(rewritten), withoutBytes(pythonTrained.regression.envelope));
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
    const { envelope: expectedEnvelope, y_train: yTrain, predict: expected } = pythonTrained[name];
    const fitted = await N4mRolePipeline.fit(expectedEnvelope.recipe, { ...matrix(xTrain), y: yTrain }, { methods });
    const text = JSON.stringify(fitted);
    const document = JSON.parse(text);
    assert.ok(document.states.every((state) => state.contains_training_rows === false));
    document.states.forEach((state) => delete state.contains_training_rows);
    assert.deepEqual(withoutBytes(document), withoutBytes(expectedEnvelope));

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
    }
    fitted.dispose();
    replayed.dispose();
  }
});

test('the Python-trained named envelope keeps column identity and training rows', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const names = pythonNamed.feature_names;
  const replayed = await N4mRolePipeline.fromJSON(pythonNamed.envelope, { methods });
  assert.deepEqual(replayed.featureNames, names);
  assert.ok(maxAbsDiff(replayed.predict(matrix(pythonNamed.x_test, names)).data, pythonNamed.predict) <= 1e-12);
  assert.throws(() => replayed.toJSON(), /retains training rows/);
  assert.throws(() => JSON.stringify(replayed), /retains training rows/);
  const rewritten = replayed.toJSON({ allowTrainingRows: true });
  assert.deepEqual(rewritten.feature_names, names);
  assert.deepEqual(withoutBytes(rewritten), withoutBytes(pythonNamed.envelope));
  replayed.dispose();

  const refit = await N4mRolePipeline.fit(
    pythonNamed.envelope.recipe, { ...matrix(pythonNamed.x_train, names), y: pythonNamed.y_train }, { methods });
  assert.deepEqual(refit.featureNames, names);
  assert.ok(maxAbsDiff(refit.predict(matrix(pythonNamed.x_test, names)).data, pythonNamed.predict) <= 1e-9);
  refit.dispose();
});

test('WASM replays the Methods shared pipelines', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const xTest = matrix(shared.x_test, shared.feature_names);
  const { regression, classification } = shared;
  const fromRegression = await N4mRolePipeline.fromJSON(
    envelope(regression.steps, regression.states, shared.feature_names), { methods });
  assert.ok(maxAbsDiff(fromRegression.predict(xTest).data, regression.predict) <= 1e-9);
  fromRegression.dispose();
  const fromClassification = await N4mRolePipeline.fromJSON(
    envelope(classification.steps, classification.states, shared.feature_names, classification.class_names), { methods });
  assert.deepEqual(fromClassification.predict(xTest).labels, classification.predict);
  assert.deepEqual(fromClassification.toJSON().states.at(-1).class_names, classification.class_names);
  fromClassification.dispose();
});

test('multi-target Y reaches supervised transformers (F06)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const item = cases.multi_target_supervised_transformer;
  const fitted = await N4mRolePipeline.fit({ pipeline: item.steps }, { ...matrix(shared.x_train), y: shared.y2_train }, { methods });
  const predicted = fitted.predict(matrix(shared.x_test));
  assert.equal(predicted.cols, 2);
  assert.ok(maxAbsDiff(predicted.data, item.predict.flat()) <= 1e-9);
  const replayed = await N4mRolePipeline.fromJSON(JSON.stringify(fitted), { methods });
  assert.deepEqual(replayed.predict(matrix(shared.x_test)), predicted);
  fitted.dispose();
  replayed.dispose();
});

test('invalid recipes are refused (F05)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  for (const name of ['empty_recipe', 'wrong_role_order', 'missing_terminal']) {
    await assert.rejects(
      N4mRolePipeline.fit({ pipeline: cases[name].steps }, { ...matrix(shared.x_train), y: shared.y_train }, { methods }),
      escape(cases[name].message), name);
  }
  await assert.rejects(N4mRolePipeline.fromJSON(envelope([], []), { methods }), /at least one step/);
  await assert.rejects(
    N4mRolePipeline.fit({ pipeline: ['sklearn.cross_decomposition.PLSRegression'] }, { ...matrix(shared.x_train), y: shared.y_train }, { methods }),
    /n4m:<method id> steps only/);
});

test('states that contradict the recipe are refused (F05)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  for (const name of ['recipe_param_differs_from_state', 'method_mismatch', 'state_count_mismatch']) {
    await assert.rejects(
      N4mRolePipeline.fromJSON(envelope(cases[name].steps, cases[name].states), { methods }), escape(cases[name].message), name);
  }
  const { steps, states } = shared.regression;
  const contradicting = structuredClone(states);
  contradicting[0].contains_training_rows = true;
  await assert.rejects(N4mRolePipeline.fromJSON(envelope(steps, contradicting), { methods }), /contains_training_rows/);
});

test('permuted or missing columns are refused (F03)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const { steps, states } = shared.regression;
  const pipeline = await N4mRolePipeline.fromJSON(envelope(steps, states, shared.feature_names), { methods });
  const permuted = cases.feature_name_permutation;
  assert.throws(() => pipeline.predict(matrix(shared.x_test, permuted.feature_names)), escape(permuted.message));
  assert.throws(() => pipeline.predict(matrix(shared.x_test.map((row) => row.slice(0, -1)))), escape(cases.width_mismatch.message));
  pipeline.dispose();

  const fitted = await N4mRolePipeline.fit({ pipeline: steps }, { ...matrix(shared.x_train, shared.feature_names), y: shared.y_train }, { methods });
  const replayed = await N4mRolePipeline.fromJSON(JSON.stringify(fitted), { methods });
  assert.deepEqual(replayed.featureNames, shared.feature_names);
  assert.throws(() => replayed.predict(matrix(shared.x_test, permuted.feature_names)), /reordered/);
  fitted.dispose();
  replayed.dispose();
});

test('training rows need the export opt-in (F10)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const item = cases.training_rows_without_opt_in;
  const fitted = await N4mRolePipeline.fit({ pipeline: item.steps }, { ...matrix(shared.x_train), y: shared[item.y] }, { methods });
  assert.throws(() => fitted.toJSON(), escape(item.message));
  const document = fitted.toJSON({ allowTrainingRows: true });
  assert.deepEqual(document.states.map((state) => state.contains_training_rows), [false, true]);
  const replayed = await N4mRolePipeline.fromJSON(document, { methods });
  assert.deepEqual(replayed.predict(matrix(shared.x_test)), fitted.predict(matrix(shared.x_test)));
  fitted.dispose();
  replayed.dispose();
});

test('tampered or foreign envelopes are refused', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const source = pythonTrained.regression.envelope;

  await assert.rejects(N4mRolePipeline.fromJSON({ ...source, schema: 'nirs4all.n4m.trained_pipeline.v7' }, { methods }),
    /Unsupported trained n4m pipeline envelope/);
  const tampered = structuredClone(source);
  tampered.states[0].sha256 = '0'.repeat(64);
  await assert.rejects(N4mRolePipeline.fromJSON(tampered, { methods }), /fails its checksum/);
  const relabelled = structuredClone(source);
  relabelled.states[0].method_id = 'preprocessing.scatter.msc';
  await assert.rejects(N4mRolePipeline.fromJSON(relabelled, { methods }), /does not match its recipe step/);
  await assert.rejects(N4mRolePipeline.fromJSON({ ...source, n_features: 3 }, { methods }), /n_features/);
});

// The Python-trained classification envelope with its label table replaced.
function withClassNames(source, classNames) {
  const document = structuredClone(source);
  document.states.at(-1).class_names = classNames;
  return document;
}

test('label tables that contradict the states are refused (R05)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const source = pythonTrained.classification.envelope;
  const xTest = matrix(pythonTrained.x_test);
  for (const [classNames, message] of [
    [[], 'class_names must be a non-empty list of labels'],
    ['high', 'class_names must be a non-empty list of labels'],
    [null, 'class_names must be a non-empty list of labels'],
    [['only'], 'class id 1 has no entry in class_names (1 labels)'],
    [['same', 'same'], 'class label "same" is duplicated'],
    [[1.5, 1.5], 'class label 1.5 is duplicated'],
    [['high', 2], 'class_names mixes strings and numbers'],
    [['high', null], 'class label null is not a string or a finite number'],
    [[true, false], 'class label true is not a string or a finite number'],
    [['high', Number.NaN], 'class label NaN is not a string or a finite number'],
  ]) {
    await assert.rejects(N4mRolePipeline.fromJSON(withClassNames(source, classNames), { methods }),
      escape(message), JSON.stringify(classNames));
  }

  // Index = class id: a longer table keeps the slots of labels a filter removed,
  // and finite numbers are labels too.
  const longer = await N4mRolePipeline.fromJSON(withClassNames(source, ['a', 'b', 'c']), { methods });
  assert.equal(longer.predict(xTest).labels.length, xTest.rows);
  longer.dispose();
  const numeric = await N4mRolePipeline.fromJSON(JSON.stringify(withClassNames(source, [0.5, 1.5])), { methods });
  assert.deepEqual(numeric.predict(xTest).labels,
    pythonTrained.classification.predict.map((label) => (label === 'high' ? 0.5 : 1.5)));
  assert.deepEqual(numeric.toJSON().states.at(-1).class_names, [0.5, 1.5]);
  numeric.dispose();

  // States fitted on class ids 10 and 20 have no entry in a two-label table.
  const ids = pythonTrained.classification.y_train.map((label) => (label === 'high' ? 10 : 20));
  const byIds = await N4mRolePipeline.fit(source.recipe, { ...matrix(pythonTrained.x_train), y: ids }, { methods });
  const document = byIds.toJSON();
  assert.equal('class_names' in document.states.at(-1), false);
  byIds.dispose();
  await assert.rejects(N4mRolePipeline.fromJSON(withClassNames(document, ['high', 'low']), { methods }),
    /class id 10 has no entry in class_names \(2 labels\)/);
  await assert.rejects(
    N4mRolePipeline.fromJSON(withClassNames(pythonTrained.regression.envelope, ['high', 'low']), { methods }),
    /class_names label the classes of a final classifier/);
});

test('envelope widths are positive JSON integers (R16)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const source = pythonTrained.regression.envelope;
  for (const [value, shown] of [[24.9, '24.9'], ['24', '"24"'], [true, 'true'], [0, '0'], [-24, '-24'], [null, 'null']]) {
    await assert.rejects(N4mRolePipeline.fromJSON({ ...source, n_features: value }, { methods }),
      escape(`n_features must be a positive JSON integer, got ${shown}`), String(value));
  }
  const { n_features: _, ...missing } = source;
  await assert.rejects(N4mRolePipeline.fromJSON(missing, { methods }), /n_features must be a positive JSON integer/);
});

test('column names with NUL are refused before the native call', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const { steps, states } = shared.regression;
  const names = [...shared.feature_names];
  names[3] = 'nm1006\0';
  await assert.rejects(N4mRolePipeline.fromJSON(envelope(steps, states, names), { methods }), /contains a NUL character/);
  await assert.rejects(
    N4mRolePipeline.fit({ pipeline: steps }, { ...matrix(shared.x_train, names), y: shared.y_train }, { methods }),
    /contains a NUL character/);
  const pipeline = await N4mRolePipeline.fromJSON(envelope(steps, states, shared.feature_names), { methods });
  assert.throws(() => pipeline.predict(matrix(shared.x_test, names)), /contains a NUL character/);
  pipeline.dispose();
});

test('ragged or contradictory X and Y are refused before flattening (R04, F02)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const recipe = { pipeline: [{ class: 'n4m:models.regularized.ridge', params: { alpha: 1 } }] };
  const X = shared.x_train;
  const y = shared.y2_train;
  const fit = (dataset) => N4mRolePipeline.fit(recipe, { ...matrix(X), y, ...dataset }, { methods });
  (await fit({})).dispose();

  // Rows of widths 1 and 3 keep the n*q total but shift every later sample.
  const ragged = structuredClone(y);
  ragged[1] = [10];
  ragged[2] = [20, 21, 22];
  await assert.rejects(fit({ y: ragged }), /y row 1 has 1 targets but row 0 has 2/);
  await assert.rejects(fit({ y: y.slice(1) }), /y has 35 rows but X has 36/);
  await assert.rejects(fit({ y: [...y, y[0]] }), /y has 37 rows but X has 36/);
  await assert.rejects(fit({ y: y.map(() => []) }), /y rows must hold at least one target/);
  await assert.rejects(fit({ y: y.map((row) => row[0]).slice(1) }), /y must have length 36/);

  const extraColumn = X.map((row) => [...row, 123]);
  await assert.rejects(fit({ X: extraColumn }), /Dataset X row 0 has 13 values but declares 12 columns/);
  const raggedX = structuredClone(X);
  raggedX[5] = raggedX[5].slice(1);
  await assert.rejects(fit({ X: raggedX }), /Dataset X row 5 has 11 values but declares 12 columns/);
  await assert.rejects(fit({ X: X.slice(1) }), /Dataset X has 35 rows but declares 36/);
  await assert.rejects(fit({ X: [...X, X[0]] }), /Dataset X has 37 rows but declares 36/);

  const fitted = await fit({});
  assert.throws(() => fitted.predict({ X: shared.x_test.map((row) => [...row, 0]), rows: shared.x_test.length, cols: 12 }),
    /Dataset X row 0 has 13 values but declares 12 columns/);
  fitted.dispose();
});

test('the exported recipe is the one the states attest (R14)', async (t) => {
  const methods = await loadMethods(t);
  if (!methods) return;
  const recipe = structuredClone(pythonTrained.regression.envelope.recipe);
  const fitted = await N4mRolePipeline.fit(recipe, { ...matrix(pythonTrained.x_train), y: pythonTrained.regression.y_train }, { methods });
  recipe.pipeline.at(-1).params.n_components = 1;
  fitted.recipe.pipeline.at(-1).params.n_components = 1;
  const text = JSON.stringify(fitted);
  assert.deepEqual(JSON.parse(text).recipe, pythonTrained.regression.envelope.recipe);
  const replayed = await N4mRolePipeline.fromJSON(text, { methods });
  assert.deepEqual(replayed.predict(matrix(pythonTrained.x_test)), fitted.predict(matrix(pythonTrained.x_test)));
  fitted.dispose();
  replayed.dispose();
});
