import assert from 'node:assert/strict';
import test from 'node:test';

import { loadArchiveV2Native, replayMethodsArchiveV2 } from '../src/index.js';

const dataset = {
  X: [[1.5, 0.5], [3.5, 1.5]],
  rows: 2,
  cols: 2,
  sampleIds: ['predict.0', 'predict.1'],
};

test('Archive V2 native surface is a Rust/WASM validator', async () => {
  const native = await loadArchiveV2Native();
  assert.equal(typeof native.ValidatedMethodsArchiveV2, 'function');
  assert.throws(
    () => new native.ValidatedMethodsArchiveV2(new Uint8Array([0x4e, 0x34, 0x61])),
    /Core Archive V2 refusal/,
  );
});

test('invalid Archive V2 refuses before Methods is observed', async () => {
  let methodsObserved = false;
  const methods = new Proxy({}, {
    get() {
      methodsObserved = true;
      throw new Error('Methods must not be observed');
    },
  });
  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array([0x50, 0x4b, 0x03, 0x04]), dataset, { methods }),
    /Core Archive V2 refusal/,
  );
  assert.equal(methodsObserved, false);
});

test('host matrix and identity contracts refuse before archive validation', async () => {
  let nativeObserved = false;
  const archiveNative = new Proxy({}, {
    get() {
      nativeObserved = true;
      throw new Error('archive validator must not be observed');
    },
  });
  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array(), {
      ...dataset,
      X: [[Number.NaN, 0.5], [3.5, 1.5]],
    }, { archiveNative }),
    /finite-value contract/,
  );
  assert.equal(nativeObserved, false);

  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array(), {
      ...dataset,
      sampleIds: ['predict.0', 'predict.0'],
    }, { archiveNative }),
    /distinct bounded identity strings/,
  );
  assert.equal(nativeObserved, false);

  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array(), dataset, { archiveNative, fallback: true }),
    /options have unknown fields: fallback/,
  );
  assert.equal(nativeObserved, false);
});
