import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { parseExecutionPlan, predictPortablePipeline, runPortablePipeline } from '../src/index.js';
import { requireMethodsArtifact } from './methods-artifact.js';

const fixtureUrl = new URL('../../../tests/parity/fixtures/portable_methods_pipeline.json', import.meta.url);

test('portable parsers share bounded positive and negative contract cases', () => {
  const cases = JSON.parse(readFileSync(new URL('../../../tests/parity/fixtures/execution_contract_cases.json', import.meta.url), 'utf8'));
  for (const item of cases.invalid) {
    assert.throws(() => parseExecutionPlan(item), undefined, item.name);
  }
  for (const item of cases.valid) {
    assert.deepEqual(parseExecutionPlan(item).nComponents, item.components, item.name);
  }
});

function deterministicNoise(row, col) {
  let state = ((row + 1) * 73856093) ^ ((col + 1) * 19349663);
  state >>>= 0;
  state = (1664525 * state + 1013904223) >>> 0;
  return state / 4294967295 - 0.5;
}

function makeDataset(rows = 40, cols = 28) {
  const X = new Float64Array(rows * cols);
  const y = new Float64Array(rows);
  for (let r = 0; r < rows; r += 1) {
    const phase = r / 5;
    let target = 0;
    for (let c = 0; c < cols; c += 1) {
      const wavelength = 900 + c * 8;
      const value =
        0.6 * Math.sin(phase + c / 7)
        + 0.25 * Math.cos(r / 6 - c / 11)
        + 0.002 * wavelength
        + ((r % 4) - 1.5) * 0.03
        + 0.12 * deterministicNoise(r, c)
        + 0.03 * Math.sin(((r + 1) * (c + 2)) / 13);
      X[r * cols + c] = value;
      target += value * (c < cols / 2 ? 0.04 : -0.025) + 0.01 * deterministicNoise(c, r);
    }
    y[r] = target + 0.2 * Math.sin(r / 3) + r * 0.015;
  }
  return { X, y, rows, cols };
}

function maxAbsDiff(actual, expected) {
  assert.equal(actual.length, expected.length);
  let max = 0;
  for (let i = 0; i < actual.length; i += 1) {
    max = Math.max(max, Math.abs(actual[i] - expected[i]));
  }
  return max;
}

test('portable execution plan recognizes the shared nirs4all fixture', () => {
  const fixture = readFileSync(fixtureUrl, 'utf8');
  const plan = parseExecutionPlan(fixture);

  assert.equal(plan.splitter.type, 'KennardStone');
  assert.deepEqual(plan.preprocessing.map((step) => step.type), ['StandardNormalVariate', 'SavitzkyGolay']);
  assert.deepEqual(plan.preprocessing[1].params, [11, 2, 0, 4, 0]);
  assert.deepEqual(plan.nComponents, [2, 4, 6, 8, 10]);
});

test('portable execution plan keeps nirs4all Savitzky-Golay defaults', () => {
  const plan = parseExecutionPlan({
    pipeline: [
      { class: 'nirs4all.operators.transforms.SavitzkyGolay', params: { window_length: 11 } },
      {
        model: {
          class: 'sklearn.cross_decomposition.PLSRegression',
          params: { n_components: 2 },
        },
      },
    ],
  });

  assert.deepEqual(plan.preprocessing[0].params, [11, 3, 0, 4, 0]);
});

test('portable execution plan preserves Savitzky-Golay mode and cval', () => {
  const plan = parseExecutionPlan({
    pipeline: [
      {
        class: 'nirs4all.operators.transforms.SavitzkyGolay',
        params: { window_length: 11, mode: 'constant', cval: 7.25 },
      },
      {
        model: {
          class: 'sklearn.cross_decomposition.PLSRegression',
          params: { n_components: 2 },
        },
      },
    ],
  });

  assert.deepEqual(plan.preprocessing[0].params, [11, 3, 0, 1, 7.25]);
});

test('MSC recipe restores training state across JSON and never fits validation data', async () => {
  const fits = [];
  const created = [];
  const methods = {
    ppCreate(type) {
      created.push(type);
      assert.equal(type, 'MSC');
      return { type, state: null };
    },
    ppFit(op, data, rows, cols) {
      fits.push(Array.from(data));
      op.state = Float64Array.from({ length: cols }, (_, col) => {
        let sum = 0;
        for (let row = 0; row < rows; row += 1) sum += data[row * cols + col];
        return sum / rows;
      });
    },
    ppGetState: (op) => op.state,
    ppSetState(op, state) { op.state = state; },
    ppTransform(op, data, rows, cols) {
      assert.ok(op.state, 'preprocessing must have training state before transform');
      return Float64Array.from(data, (value, index) => value - op.state[index % cols]);
    },
    ppDestroy() {},
    computeSplitIndices: () => ({ trainIndices: [0, 1], testIndices: [2, 3] }),
    fitPls: () => ({
      coefficients: Float64Array.of(1, 0), xMean: Float64Array.of(0, 0),
      yMean: Float64Array.of(0), intercept: null, n_features: 2, n_targets: 1,
    }),
    predictPls: (_model, X) => ({
      data: Float64Array.from({ length: X.rows }, (_, row) => X.data[row * X.cols]),
      rows: X.rows,
      cols: 1,
    }),
  };
  const source = { pipeline: [
    { class: 'nirs4all.operators.splitters.KennardStoneSplitter' },
    { class: 'n4m.MSC' },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression', params: { n_components: 1 } } },
  ] };
  const dataset = { X: [1, 2, 3, 4, 100, 200, 300, 400], y: [0, 0, 98, 298], rows: 4, cols: 2 };
  const fitted = await runPortablePipeline(source, dataset, { methods });
  assert.deepEqual(fitted.preprocessing, [{ type: 'MSC', params: [], state: [2, 3] }]);
  assert.deepEqual(fits, [[1, 2, 3, 4]]);
  assert.deepEqual(fitted.selected.predictions, [98, 298]);

  const replayed = JSON.parse(JSON.stringify(fitted));
  const validation = await predictPortablePipeline(replayed, { X: [100, 200, 300, 400], rows: 2, cols: 2 }, { methods });
  assert.deepEqual(validation.data, fitted.selected.predictions);
  assert.deepEqual(created, ['MSC', 'MSC']);
  assert.deepEqual(fits, [[1, 2, 3, 4]], 'prediction must not re-fit a stateful operator');

  replayed.preprocessing[0].state = [2];
  await assert.rejects(
    predictPortablePipeline(replayed, { X: [100, 200], rows: 1, cols: 2 }, { methods }),
    /state length 1 does not match 2 features/,
  );
  replayed.preprocessing[0].state = [2, Number.NaN];
  await assert.rejects(
    predictPortablePipeline(replayed, { X: [100, 200], rows: 1, cols: 2 }, { methods }),
    /invalid fitted state/,
  );
  delete replayed.preprocessing[0].state;
  await assert.rejects(
    predictPortablePipeline(replayed, { X: [100, 200], rows: 1, cols: 2 }, { methods }),
    /requires fitted state/,
  );
});

test('MSC Python aliases use the Methods MSC token and reject unsupported parameters', () => {
  for (const className of [
    'nirs4all.operators.transforms.MSC',
    'nirs4all.operators.transforms.MultiplicativeScatterCorrection',
    'nirs4all.operators.transforms.nirs.MultiplicativeScatterCorrection',
  ]) {
    const plan = parseExecutionPlan({ pipeline: [
      { class: className, params: { scale: false, copy: true } },
      { model: { class: 'sklearn.cross_decomposition.PLSRegression' } },
    ] });
    assert.deepEqual(plan.preprocessing, [{ type: 'MSC', params: [] }]);
  }
  assert.throws(() => parseExecutionPlan({ pipeline: [
    { class: 'n4m.MSC', params: { reference: [1, 2] } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression' } },
  ] }), /Unsupported MSC parameter/);
});

test('eight affine model aliases dispatch and replay through Methods', async () => {
  const cases = [
    ['Ridge', { lambda: 2 }, [2]],
    ['RidgePLS', { ridge_lambda: 3 }, [3]],
    ['RobustPLS', { huber_k: 1.5, max_irls_iter: 7 }, [1.5, 7]],
    ['CPPLS', { gamma: 0.4 }, [0.4]],
    ['SparseSIMPLS', { sparsity_lambda: 0.02 }, [0.02]],
    ['ECR', { alpha: 0.7 }, [0.7]],
    ['ContinuumRegression', { tau: 0.3 }, [0.3]],
    ['MIRPLS', {}, []],
  ];
  for (const [type, params, vector] of cases) {
    const calls = [];
    const methods = {
      fitModel(token, X, Y, components, values) {
        calls.push(['fit', token, components, values]);
        assert.equal(X.rows, Y.rows);
        return { coefficients: Float64Array.of(1, 0), xMean: Float64Array.of(0, 0),
          yMean: Float64Array.of(0), intercept: type === 'Ridge' ? Float64Array.of(2) : null,
          n_features: 2, n_targets: 1 };
      },
      predictModel(model, X) {
        calls.push(['predict', model.intercept == null ? null : Array.from(model.intercept)]);
        return { data: Float64Array.from({ length: X.rows }, (_, row) => X.data[row * X.cols]),
          rows: X.rows, cols: 1 };
      },
    };
    const source = { pipeline: [{ model: { class: `n4m.${type}`, params: { ...params, n_components: 2 } } }] };
    const input = { X: [1, 2, 3, 4, 5, 6], y: [1, 3, 5], rows: 3, cols: 2 };
    const plan = parseExecutionPlan(source);
    assert.equal(plan.modelType, type);
    assert.deepEqual(plan.modelParams, vector);
    const fitted = await runPortablePipeline(source, input, { methods });
    assert.equal(fitted.model.type, type);
    assert.deepEqual(fitted.model.params, vector);
    assert.deepEqual(fitted.selected.predictions, [1, 3, 5]);
    const replay = JSON.parse(JSON.stringify(fitted));
    const predicted = await predictPortablePipeline(replay, { X: input.X, rows: 3, cols: 2 }, { methods });
    assert.deepEqual(predicted.data, fitted.selected.predictions);
    assert.deepEqual(calls[0], ['fit', type, 2, vector]);
    assert.equal(calls.filter(([name]) => name === 'predict').length, 2);
  }
});

test('affine model recipes reject unsupported or lossy parameters', () => {
  for (const [type, params, pattern] of [
    ['Ridge', { alpha: 1 }, /Unsupported Ridge parameter/],
    ['RidgePLS', { ridge_lambda: 'NaN' }, /ridge_lambda must be finite/],
    ['RobustPLS', { max_irls_iter: 2.5 }, /max_irls_iter must be an integer/],
    ['MIRPLS', { tau: 0.5 }, /Unsupported MIRPLS parameter/],
  ]) {
    assert.throws(() => parseExecutionPlan({ pipeline: [
      { model: { class: `n4m.${type}`, params } },
    ] }), pattern);
  }
});

test('SPA learns only on training rows and replays sorted selected columns', async () => {
  const calls = [];
  const methods = {
    computeSplitIndices: () => ({ trainIndices: [0, 1], testIndices: [2, 3] }),
    selectSpa(X, Y, topK, components) {
      calls.push(['select', Array.from(X.data), Array.from(Y.data), topK, components]);
      return BigInt64Array.of(2n, 0n);
    },
    fitPls(X) {
      calls.push(['fit', Array.from(X.data), X.cols]);
      return { coefficients: Float64Array.of(1, 0), xMean: Float64Array.of(0, 0),
        yMean: Float64Array.of(0), intercept: null, n_features: 2, n_targets: 1 };
    },
    predictPls(_model, X) {
      calls.push(['predict', Array.from(X.data), X.cols]);
      return { data: Float64Array.from({ length: X.rows }, (_, row) => X.data[row * X.cols]),
        rows: X.rows, cols: 1 };
    },
  };
  const source = { pipeline: [
    { class: 'nirs4all.operators.splitters.KennardStoneSplitter' },
    { class: 'n4m.SPA', params: { top_k: 2, n_components: 1 } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression', params: { n_components: 1 } } },
  ] };
  const dataset = { X: [1, 10, 2, 3, 20, 4, 5, 30, 6, 7, 40, 8],
    y: [1, 3, 5, 7], rows: 4, cols: 3 };
  const fitted = await runPortablePipeline(source, dataset, { methods });
  assert.deepEqual(calls[0], ['select', [1, 10, 2, 3, 20, 4], [1, 3], 2, 1]);
  assert.deepEqual(calls[1], ['fit', [1, 2, 3, 4], 2]);
  assert.deepEqual(fitted.preprocessing, [{ type: 'SPA', params: [2, 1], state: [2, 0] }]);
  assert.deepEqual(fitted.selected.predictions, [5, 7]);
  const replay = JSON.parse(JSON.stringify(fitted));
  const result = await predictPortablePipeline(replay, { X: dataset.X, rows: 4, cols: 3 }, { methods });
  assert.deepEqual(result.data, [1, 3, 5, 7]);
  assert.equal(calls.filter(([name]) => name === 'select').length, 1);
  assert.deepEqual(calls.at(-1), ['predict', [1, 2, 3, 4, 5, 6, 7, 8], 2]);

  for (const state of [[0, 0], [0, 3], [0], [0, 1.5], null]) {
    replay.preprocessing[0].state = state;
    await assert.rejects(
      predictPortablePipeline(replay, { X: dataset.X, rows: 4, cols: 3 }, { methods }),
      /SPA/,
    );
  }
});

test('SPA preserves native ranking while projecting ascending columns after JSON replay', async () => {
  const projected = [];
  const methods = {
    selectSpa: () => BigInt64Array.of(8n, 3n, 1n),
    fitPls(X) {
      projected.push(Array.from(X.data));
      return { coefficients: Float64Array.of(1, 0, 0), xMean: Float64Array.of(0, 0, 0),
        yMean: Float64Array.of(0), intercept: null, n_features: 3, n_targets: 1 };
    },
    predictPls(_model, X) {
      projected.push(Array.from(X.data));
      return { data: Float64Array.from({ length: X.rows }, (_, row) => X.data[row * X.cols]),
        rows: X.rows, cols: 1 };
    },
  };
  const source = { pipeline: [
    { class: 'n4m.SPA', params: { top_k: 3, n_components: 1 } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression', params: { n_components: 1 } } },
  ] };
  const dataset = { X: Array.from({ length: 36 }, (_, index) => index + 1),
    y: [2, 11, 20, 29], rows: 4, cols: 9 };
  const fitted = await runPortablePipeline(source, dataset, { methods });
  assert.deepEqual(fitted.preprocessing[0].state, [8, 3, 1]);
  const expected = [2, 4, 9, 11, 13, 18, 20, 22, 27, 29, 31, 36];
  assert.deepEqual(projected[0], expected);
  const replay = JSON.parse(JSON.stringify(fitted));
  await predictPortablePipeline(replay, { X: dataset.X, rows: 4, cols: 9 }, { methods });
  assert.deepEqual(projected.at(-1), expected);
  assert.deepEqual(replay.preprocessing[0].state, [8, 3, 1]);
});

test('SPA recipes reject missing, unsupported, and out-of-range top_k', async () => {
  for (const params of [{}, { top_k: 0 }, { top_k: 1.5 }, { top_k: 2, seed: 1 }]) {
    assert.throws(() => parseExecutionPlan({ pipeline: [
      { class: 'n4m.SPA', params },
      { model: { class: 'sklearn.cross_decomposition.PLSRegression' } },
    ] }), /SPA/);
  }
  const source = { pipeline: [
    { class: 'n4m.SPA', params: { top_k: 4 } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression' } },
  ] };
  await assert.rejects(runPortablePipeline(source, {
    X: [1, 2, 3, 4, 5, 6], y: [1, 2, 3], rows: 3, cols: 2,
  }, { methods: {} }), /SPA top_k 4 exceeds 2 features/);
  const rankSource = { pipeline: [
    { class: 'nirs4all.operators.splitters.KennardStoneSplitter' },
    { class: 'n4m.SPA', params: { top_k: 1, n_components: 2 } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression' } },
  ] };
  await assert.rejects(runPortablePipeline(rankSource, {
    X: [1, 2, 3, 4, 5, 6], y: [1, 2, 3], rows: 3, cols: 2,
  }, { methods: { computeSplitIndices: () => ({ trainIndices: [0, 1], testIndices: [2] }) } }),
  /SPA n_components 2 exceeds train rank limit 1/);
});

test('generic Selector parses all native names and rejects lossy parameters', () => {
  const names = ['spa_select', 'cars_select', 'interval_select', 'stability_select',
    'uve_select', 'random_frog_select', 'scars_select', 'ga_select', 'pso_select',
    'vissa_select', 'shaving_select', 'bve_select', 't2_select', 'wvc_select',
    'wvc_threshold_select', 'emcuve_select', 'randomization_select', 'bipls_select',
    'sipls_select', 'rep_select', 'ipw_select', 'st_select', 'iriv_select',
    'irf_select', 'vip_spa_select'];
  const required = { spa_select: { top_k: 2 }, stability_select: { top_k: 2 },
    wvc_select: { top_k: 2 }, random_frog_select: { top_k: 2, seed: 0 },
    ipw_select: { top_k: 2 }, irf_select: { top_k: 2, seed: 0 },
    vip_spa_select: { top_k: 2 }, uve_select: { noise_seed: 0 },
    emcuve_select: { noise_seed: 0 }, randomization_select: { randomization_seed: 0 },
    scars_select: { seed: 0 }, ga_select: { seed: 0 }, pso_select: { seed: 0 },
    vissa_select: { seed: 0 }, iriv_select: { seed: 0 },
    t2_select: { alpha_thresholds: [0.05] }, st_select: { thresholds: [0.1] } };
  const source = (method, method_params = required[method] ?? {}) => ({ pipeline: [
    { class: 'n4m.Selector', params: { method, n_components: 1, method_params } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression', params: { n_components: 1 } } },
  ] });
  for (const method of names) {
    const plan = parseExecutionPlan(source(method));
    assert.equal(plan.preprocessing[0].params.method, method);
  }
  for (const [method, params] of [
    ['other', {}], ['spa_select', {}], ['spa_select', { top_k: 1.5 }],
    ['spa_select', { top_k: 2, bogus: 1 }], ['t2_select', { alpha_thresholds: [] }],
    ['wvc_select', { top_k: 2, normalize: 1 }],
    ['uve_select', { noise_seed: 9007199254740992 }],
  ]) assert.throws(() => parseExecutionPlan(source(method, params)), /Selector|parameter|top_k|thresholds|noise_seed/);
});

test('generic Selector fits on training rows, preserves rank, and replays sorted projection', async () => {
  const calls = [];
  const methods = {
    computeSplitIndices: () => ({ trainIndices: [0, 1, 2, 3], testIndices: [4, 5] }),
    selectVariables(method, X, Y, components, params) {
      calls.push(['select', method, X.rows, Array.from(X.data), Array.from(Y.data), components, params]);
      return BigInt64Array.of(8n, 3n, 1n);
    },
    fitPls(X) {
      calls.push(['fit', Array.from(X.data)]);
      return { coefficients: Float64Array.of(1, 0, 0), xMean: Float64Array.of(0, 0, 0),
        yMean: Float64Array.of(0), intercept: null, n_features: 3, n_targets: 1 };
    },
    predictPls(_model, X) {
      calls.push(['predict', Array.from(X.data)]);
      return { data: Float64Array.from({ length: X.rows }, (_, row) => X.data[row * X.cols]),
        rows: X.rows, cols: 1 };
    },
  };
  const source = { pipeline: [
    { class: 'nirs4all.operators.splitters.KennardStoneSplitter' },
    { class: 'n4m.Selector', params: { method: 'wvc_select', n_components: 1,
      method_params: { top_k: 3, normalize: false } } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression', params: { n_components: 1 } } },
  ] };
  const dataset = { X: Array.from({ length: 54 }, (_, i) => i + 1),
    y: [2, 11, 20, 29, 38, 47], rows: 6, cols: 9 };
  const fitted = await runPortablePipeline(source, dataset, { methods });
  assert.deepEqual(calls[0], ['select', 'wvc_select', 4,
    dataset.X.slice(0, 36), dataset.y.slice(0, 4), 1, { top_k: 3, normalize: false }]);
  assert.deepEqual(fitted.preprocessing[0].state, [8, 3, 1]);
  assert.deepEqual(calls[1], ['fit', [2, 4, 9, 11, 13, 18, 20, 22, 27, 29, 31, 36]]);
  const replay = JSON.parse(JSON.stringify(fitted));
  await predictPortablePipeline(replay, { X: dataset.X, rows: 6, cols: 9 }, { methods });
  assert.deepEqual(calls.at(-1), ['predict', [2, 4, 9, 11, 13, 18, 20, 22, 27,
    29, 31, 36, 38, 40, 45, 47, 49, 54]]);
  assert.equal(calls.filter(([name]) => name === 'select').length, 1);
  assert.deepEqual(replay.preprocessing[0].state, [8, 3, 1]);
  for (const state of [[1, 1], [9], [], null]) {
    replay.preprocessing[0].state = state;
    await assert.rejects(predictPortablePipeline(replay, dataset, { methods }), /Selector/);
  }
});

test('generic Selector checks train rank, plan rows, and top_k before native calls', async () => {
  const source = (method, method_params, components = 1) => ({ pipeline: [
    { class: 'n4m.Selector', params: { method, n_components: components, method_params } },
    { model: { class: 'sklearn.cross_decomposition.PLSRegression', params: { n_components: 1 } } },
  ] });
  const dataset = { X: [1, 2, 3, 4, 5, 6], y: [1, 2, 3], rows: 3, cols: 2 };
  await assert.rejects(runPortablePipeline(source('spa_select', { top_k: 3 }),
    dataset, { methods: {} }), /top_k/);
  await assert.rejects(runPortablePipeline(source('spa_select', { top_k: 1 }, 3),
    dataset, { methods: {} }), /training rank/);
  await assert.rejects(runPortablePipeline(source('cars_select', {}),
    dataset, { methods: {} }), /at least 4 training rows/);
});

test('older stateless preprocessing remains replayable without fitted state', async () => {
  let fits = 0;
  const methods = {
    ppCreate: () => ({}),
    ppFit() { fits += 1; },
    ppTransform: (_op, data) => Float64Array.from(data),
    ppDestroy() {},
    predictPls: (_model, X) => ({ data: Float64Array.of(X.data[0]), rows: X.rows, cols: 1 }),
  };
  const model = {
    coefficients: [1, 0], xMean: [0, 0], yMean: [0], intercept: null,
    n_features: 2, n_targets: 1,
  };
  const result = await predictPortablePipeline(
    { preprocessing: [
      { type: 'StandardNormalVariate', params: [] },
      { type: 'SavitzkyGolay', params: [3, 1, 0, 4, 0] },
    ], model },
    { X: [7, 8], rows: 1, cols: 2 },
    { methods },
  );
  assert.deepEqual(result.data, [7]);
  assert.equal(fits, 0);
});

test('portable execution plan rejects lossy operator parameter coercions', () => {
  assert.throws(() => parseExecutionPlan({
    pipeline: [
      { class: 'nirs4all.operators.transforms.SavitzkyGolay', params: { window_length: 10.5 } },
      {
        model: {
          class: 'sklearn.cross_decomposition.PLSRegression',
          params: { n_components: 2 },
        },
      },
    ],
  }), /window_length must be an integer/);

  assert.throws(() => parseExecutionPlan({
    pipeline: [
      {
        model: {
          class: 'sklearn.cross_decomposition.PLSRegression',
          params: { n_components: 1.5 },
        },
      },
    ],
  }), /n_components must be an integer/);

  assert.throws(() => parseExecutionPlan({
    pipeline: [
      {
        model: { class: 'sklearn.cross_decomposition.PLSRegression' },
        param: 'n_components',
        _range_: [0, 4, 2],
      },
    ],
  }), /n_components range start must be >= 1/);

  assert.throws(() => parseExecutionPlan({
    pipeline: [
      {
        model: { class: 'sklearn.cross_decomposition.PLSRegression' },
        param: 'n_components',
        _range_: [4, 2, 1],
      },
    ],
  }), /start must be <= stop/);
});

test('portable WASM execution delegates the shared pipeline to nirs4all-methods', async (t) => {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) {
    return;
  }

  const methods = await import(artifact.indexUrl.href);
  const dataset = makeDataset();
  const result = await runPortablePipeline(readFileSync(fixtureUrl, 'utf8'), dataset, { methods });

  assert.equal(result.name, 'portable_methods_pipeline');
  assert.deepEqual(result.evaluation, { scope: 'selection_validation', independent_test: false });
  assert.equal(result.split.kind, 'KennardStone');
  assert.equal(result.variants.length, 5);
  assert.equal(result.targets.length, result.split.testIndices.length);
  assert.equal(result.selected.rmse, Math.min(...result.variants.map((item) => item.rmse)));
  assert.equal(result.model.type, 'PLSRegression');
  assert.equal(result.model.n_components, result.selected.n_components);
  assert.ok(result.selected.predictions.every(Number.isFinite));

  const predicted = await predictPortablePipeline(result, { X: dataset.X, rows: dataset.rows, cols: dataset.cols }, { methods });
  assert.equal(predicted.rows, dataset.rows);
  assert.equal(predicted.cols, 1);
  const heldOut = result.split.testIndices.map((index) => predicted.data[index]);
  assert.ok(maxAbsDiff(heldOut, result.selected.predictions) <= 1e-10);
});
