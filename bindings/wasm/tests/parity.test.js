import assert from 'node:assert/strict';
import test from 'node:test';

import { requireMethodsArtifact } from './methods-artifact.js';

function maxAbsDiff(actual, expected) {
  assert.equal(actual.length, expected.length);
  let max = 0;
  for (let i = 0; i < actual.length; i += 1) {
    max = Math.max(max, Math.abs(actual[i] - expected[i]));
  }
  return max;
}

test('MBPLS Methods WASM matches independent R/Python held-out oracle', async (t) => {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) return;
  const methods = await import(artifact.indexUrl.href);
  await methods.loadModule();
  const rows = 21, cols = 12;
  const X = new Float64Array(rows * cols);
  const y = new Float64Array(rows);
  for (let i = 0; i < rows; i += 1) {
    for (let j = 0; j < cols; j += 1) {
      X[i * cols + j] = Math.sin((i + 1) * (j + 1) / 9)
        + Math.cos((i + 1) + (j + 1) / 7) + (i + 1) * (j + 1) / 100;
    }
    y[i] = 1.3 + 0.7 * X[i * cols + 1] - 0.4 * X[i * cols + 5];
  }
  const fitted = methods.fitModel('MBPLS',
    { data: X, rows, cols }, { data: y, rows, cols: 1 }, 2, [4, 4, 4]);
  const heldout = new Float64Array(3 * cols);
  for (const [row, source] of [1, 7, 16].entries()) {
    for (let j = 0; j < cols; j += 1) heldout[row * cols + j] = X[source * cols + j] + 0.031;
  }
  const predicted = methods.predictModel(fitted, { data: heldout, rows: 3, cols });
  assert.ok(maxAbsDiff(predicted.data,
    [1.3614391588922699, 2.033212108151359, 0.7661914180346159]) < 1e-10);
});

test('GroupSparsePLS Methods WASM matches corrected Python n4m held-out oracle', async (t) => {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) return;
  const methods = await import(artifact.indexUrl.href);
  await methods.loadModule();
  const rows = 21, cols = 8;
  const X = new Float64Array(rows * cols);
  const y = new Float64Array(rows);
  for (let i = 0; i < rows; i += 1) {
    for (let j = 0; j < cols; j += 1) {
      X[i * cols + j] = Math.sin((i + 1) * (j + 1) / 9)
        + Math.cos((i + 1) + (j + 1) / 7) + (i + 1) * (j + 1) / 100;
    }
    y[i] = 1.3 + 0.7 * X[i * cols + 1] - 0.4 * X[i * cols + 5];
  }
  const heldout = new Float64Array(3 * cols);
  for (const [row, source] of [1, 7, 16].entries()) {
    for (let j = 0; j < cols; j += 1) heldout[row * cols + j] = X[source * cols + j] + 0.031;
  }
  const groups = [0, 0, 1, 1, 2, 2, 3, 3];
  const oracles = [
    [0, [0.9908421322687999, 2.2185030364862124, 0.936452057620222]],
    [0.15, [1.0634702655693697, 1.8270584733785862, 1.0614347191399331]],
  ];
  for (const [lambda, expected] of oracles) {
    const fitted = methods.fitModel('GroupSparsePLS',
      { data: X, rows, cols }, { data: y, rows, cols: 1 }, 2, [lambda, ...groups]);
    const predicted = methods.predictModel(fitted, { data: heldout, rows: 3, cols });
    assert.ok(maxAbsDiff(predicted.data, expected) < 1e-10,
      `GroupSparsePLS lambda=${lambda} differs from Python n4m oracle`);
  }
});
