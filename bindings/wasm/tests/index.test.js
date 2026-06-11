import assert from 'node:assert/strict';
import test from 'node:test';

import { methods, upstream, upstreams } from '../src/index.js';

test('expected upstreams are registered', () => {
  assert.deepEqual(
    upstreams.map((item) => item.key),
    ['dag_ml', 'dag_ml_data', 'formats', 'io', 'datasets', 'methods'],
  );
});

test('upstream lookup returns metadata', () => {
  assert.equal(upstream('methods').role, 'Portable C ABI PLS/NIRS numerical engine');
  assert.equal(upstream('missing'), null);
});

test('domain proxies expose keys', () => {
  assert.equal(methods.key, 'methods');
});
