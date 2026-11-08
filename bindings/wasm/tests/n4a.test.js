import assert from 'node:assert/strict';
import test from 'node:test';

import { inspectN4aBytes, predictN4aNative, produceN4aNative, retrainN4aNative } from '../src/n4a.js';

// Boundary checks run before any WASM module is loaded. Exchange behaviour is
// qualified by tests/n4a-exchange-driver.mjs under the Rust exchange gate.
const bytes = new Uint8Array([1]);
const recipe = { steps: [], candidates: [] };

test('n4a actions accept exact bytes only', async () => {
  await assert.rejects(inspectN4aBytes('archive'), /exact bytes/);
  await assert.rejects(produceN4aNative('{}', { sourceId: 'spectra', recipe }), /exact bytes/);
  await assert.rejects(produceN4aNative({ dataset: {} }, { sourceId: 'spectra', recipe }), /exact bytes/);
  await assert.rejects(retrainN4aNative(bytes, '{}'), /exact bytes/);
  await assert.rejects(retrainN4aNative('archive', bytes), /exact bytes/);
  await assert.rejects(predictN4aNative(bytes, { entrypoint: 'predict-external', sourceId: 'spectra', record: '{}' }), /exact bytes/);
});

test('n4a actions refuse missing identifiers and recipes explicitly', async () => {
  await assert.rejects(produceN4aNative(bytes, { recipe }), /sourceId/);
  await assert.rejects(produceN4aNative(bytes, { sourceId: 'spectra' }), /recipe/);
  await assert.rejects(predictN4aNative(bytes, { sourceId: 'spectra' }), /entrypoint/);
  await assert.rejects(predictN4aNative(bytes, { entrypoint: 'predict' }), /sourceId/);
  await assert.rejects(predictN4aNative(bytes, null), /Options must be an object/);
});
