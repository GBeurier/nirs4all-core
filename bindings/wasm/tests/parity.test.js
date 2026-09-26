import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { predictPortablePipeline, runPortablePipeline } from '../src/index.js';
import { requireMethodsArtifact } from './methods-artifact.js';

const oracleUrl = new URL('../../../tests/parity/expected/portable_python_oracle.json', import.meta.url);
const fixtureDir = new URL('../../../tests/parity/fixtures/', import.meta.url);

function maxAbsDiff(actual, expected) {
  assert.equal(actual.length, expected.length);
  let max = 0;
  for (let i = 0; i < actual.length; i += 1) {
    max = Math.max(max, Math.abs(actual[i] - expected[i]));
  }
  return max;
}

test('portable WASM execution matches the full Python nirs4all oracle', async (t) => {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) {
    return;
  }

  const methods = await import(artifact.indexUrl.href);
  const oracle = JSON.parse(readFileSync(oracleUrl, 'utf8'));
  const dataset = {
    X: Float64Array.from(oracle.dataset.X),
    y: Float64Array.from(oracle.dataset.y),
    rows: oracle.dataset.rows,
    cols: oracle.dataset.cols,
  };
  const tol = oracle.metadata.tolerances;

  for (const expected of oracle.cases) {
    const fixture = readFileSync(new URL(`${expected.name}.json`, fixtureDir), 'utf8');
    const actual = await runPortablePipeline(fixture, dataset, { methods });
    if (actual.split.kind !== 'all') {
      const alternate = JSON.parse(fixture);
      const splitter = alternate.pipeline.shift();
      alternate.pipeline.splice(alternate.pipeline.length - 1, 0, splitter);
      assert.deepEqual(await runPortablePipeline(alternate, dataset, { methods }), actual);
    }
    assert.deepEqual(actual.evaluation, {
      scope: actual.split.kind === 'all' ? 'training' : 'selection_validation',
      independent_test: false,
    });

    assert.deepEqual(actual.split, expected.split, `${expected.name}: split mismatch`);
    assert.ok(
      maxAbsDiff(actual.targets, expected.targets) <= tol.targets_abs,
      `${expected.name}: target diff is too large`,
    );
    assert.equal(actual.variants.length, expected.variants.length, `${expected.name}: variant count mismatch`);
    for (let i = 0; i < expected.variants.length; i += 1) {
      assert.equal(actual.variants[i].n_components, expected.variants[i].n_components);
      assert.ok(
        Math.abs(actual.variants[i].rmse - expected.variants[i].rmse) <= tol.rmse_abs,
        `${expected.name}: RMSE diff for n_components=${expected.variants[i].n_components}`,
      );
      assert.ok(
        maxAbsDiff(actual.variants[i].predictions, expected.variants[i].predictions) <= tol.predictions_abs,
        `${expected.name}: prediction diff for n_components=${expected.variants[i].n_components}`,
      );
    }
    assert.equal(actual.selected.n_components, expected.selected.n_components, `${expected.name}: selected component mismatch`);
    assert.equal(actual.model.n_components, expected.selected.n_components, `${expected.name}: fitted model component mismatch`);

    const predicted = await predictPortablePipeline(actual, {
      X: dataset.X,
      rows: dataset.rows,
      cols: dataset.cols,
    }, { methods });
    const heldOut = actual.split.testIndices.map((index) => predicted.data[index]);
    assert.ok(
      maxAbsDiff(heldOut, actual.selected.predictions) <= tol.predictions_abs,
      `${expected.name}: portable predict diff for selected model`,
    );
  }
});

test('MBPLS WASM recipe matches independent R/Python held-out oracle', async (t) => {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) return;
  const methods = await import(artifact.indexUrl.href);
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
  const recipe = { pipeline: [{ model: { class: 'n4m.MBPLS', params: {
    n_components: 2, block_sizes: [4, 4, 4],
  } } }] };
  const fitted = await runPortablePipeline(recipe, { X, y, rows, cols }, { methods });
  assert.equal(fitted.model.type, 'MBPLS');
  assert.deepEqual(fitted.model.params, [4, 4, 4]);
  const heldout = new Float64Array(3 * cols);
  for (const [row, source] of [1, 7, 16].entries()) {
    for (let j = 0; j < cols; j += 1) heldout[row * cols + j] = X[source * cols + j] + 0.031;
  }
  const predicted = await predictPortablePipeline(JSON.parse(JSON.stringify(fitted)),
    { X: heldout, rows: 3, cols }, { methods });
  assert.ok(maxAbsDiff(predicted.data,
    [1.3614391588922699, 2.033212108151359, 0.7661914180346159]) < 1e-10);
});
