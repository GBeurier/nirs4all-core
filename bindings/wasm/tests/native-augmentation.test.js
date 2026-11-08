import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { parseTrainAugmentation } from '../src/native-augmentation.js';
import { requireMethodsArtifact } from './methods-artifact.js';

const augmentationCases = JSON.parse(readFileSync(new URL(
  '../../../tests/parity/fixtures/native_x_augmentation_cases.json', import.meta.url,
), 'utf8'));

function augmentationStep(kind, values, seed = 42) {
  return { train_augmentation: { class: 'n4m.NativeXAugmentation', params: { kind, values, seed } } };
}

test('native X augmentation parses the closed 22-kind R contract', () => {
  assert.equal(augmentationCases.cases.length, 22);
  for (const [kind, methodsKind, values] of augmentationCases.cases) {
    const parsed = parseTrainAugmentation(augmentationStep(kind, values));
    assert.deepEqual(parsed, { kind, methodsKind, values, seed: 42 });
  }
  for (const [kind, values, seed] of [
    ['mixup', [0.5], 42],
    ['GaussianNoise', [0.03], 42],
    ['gaussian_noise', [], 42],
    ['gaussian_noise', [Infinity], 42],
    ['gaussian_noise', [0.03], -1],
    ['gaussian_noise', [0.03], 2 ** 53],
    ['gaussian_noise', [0.03], 1.5],
  ]) {
    assert.throws(() => parseTrainAugmentation(augmentationStep(kind, values, seed)));
  }
});

test('native X augmentation matches Methods for all 22 kinds and frozen R oracles', async (t) => {
  const artifact = requireMethodsArtifact(t);
  if (!artifact) return;
  const methods = await import(artifact.indexUrl.href);
  if (typeof methods.augmentNative !== 'function') {
    t.skip('local Methods artifact predates ABI 2.11');
    return;
  }
  await methods.loadModule();
  const X = { rows: 8, cols: 32, data: Float64Array.from(
    Array.from({ length: 8 * 32 }, (_, i) => 1 + 0.02 * (i % 32)),
  ) };
  for (const [kind, methodsKind, values] of augmentationCases.cases) {
    const parsed = parseTrainAugmentation(augmentationStep(kind, values));
    const actual = methods.augmentNative(parsed.methodsKind, X,
      parsed.values, parsed.seed).data;
    const repeat = methods.augmentNative(methodsKind, X, values, 42).data;
    assert.deepEqual(actual, repeat, `${kind} native kind mapping and seed`);
    const nativeOracle = augmentationCases.native_oracle[kind];
    assert.ok(nativeOracle, `${kind} native oracle exists`);
    const close = (a, b) => Math.abs(a - b) <= 1e-10 * Math.max(1, Math.abs(b));
    assert.ok(close(actual.reduce((a, b) => a + b, 0), nativeOracle[0]), `${kind} native sum`);
    assert.ok(close(actual.reduce((a, b) => a + b * b, 0), nativeOracle[1]), `${kind} native sumsq`);
    const oracle = augmentationCases.r_oracle[kind];
    if (oracle) {
      assert.ok(close(actual.reduce((a, b) => a + b, 0), oracle[0]), `${kind} R sum`);
      assert.ok(close(actual.reduce((a, b) => a + b * b, 0), oracle[1]), `${kind} R sumsq`);
      oracle[2].forEach((value, i) => assert.ok(close(actual[i], value), `${kind} R prefix ${i}`));
    }
  }
});

