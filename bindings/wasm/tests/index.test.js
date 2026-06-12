import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';

import {
  importUpstream,
  loadPipelineDefinition,
  loadPortableStack,
  methods,
  portableClassNames,
  upstream,
  upstreams,
} from '../src/index.js';

const fixtureUrl = new URL('../../../tests/parity/fixtures/portable_methods_pipeline.json', import.meta.url);
const yamlFixtureUrl = new URL('../../../tests/parity/fixtures/portable_methods_pipeline.yaml', import.meta.url);

test('expected upstreams are registered', () => {
  assert.deepEqual(
    upstreams.map((item) => item.key),
    ['dag_ml', 'dag_ml_data', 'formats', 'io', 'datasets', 'methods'],
  );
});

test('upstream lookup returns metadata', () => {
  assert.equal(upstream('methods').role, 'Portable C ABI PLS/NIRS numerical engine');
  assert.deepEqual(upstream('methods').candidates, ['@nirs4all/methods-wasm']);
  assert.deepEqual(upstream('formats').candidates, ['nirs4all-formats-wasm']);
  assert.deepEqual(upstream('datasets').candidates, ['@nirs4all/nirs4all-datasets-wasm']);
  assert.equal(upstream('missing'), null);
});

test('domain proxies expose keys', () => {
  assert.equal(methods.key, 'methods');
});

test('upstream imports fail explicitly when optional wasm packages are absent', async () => {
  await assert.rejects(() => importUpstream('missing'), /Unknown nirs4all upstream/);
  await assert.rejects(() => methods.import(), /nirs4all upstream 'methods' is not installed/);
  await assert.rejects(() => loadPortableStack(['methods']), /nirs4all upstream 'methods' is not installed/);
});

test('nirs4all JSON and YAML pipeline syntax normalize to the same portable definition', () => {
  const jsonPipeline = loadPipelineDefinition(readFileSync(fixtureUrl, 'utf8'));
  const yamlPipeline = loadPipelineDefinition(readFileSync(yamlFixtureUrl, 'utf8'));

  assert.deepEqual(jsonPipeline, yamlPipeline);
  assert.deepEqual(
    portableClassNames(jsonPipeline),
    [
      'nirs4all.operators.splitters.KennardStoneSplitter',
      'nirs4all.operators.transforms.StandardNormalVariate',
      'nirs4all.operators.transforms.SavitzkyGolay',
      'sklearn.cross_decomposition.PLSRegression',
    ],
  );
  assert.deepEqual(jsonPipeline.pipeline.at(-1)._range_, [2, 11, 2]);
});

test('all shared parity fixtures keep JSON and YAML in lockstep', () => {
  const fixtureDir = new URL('../../../tests/parity/fixtures/', import.meta.url);
  const names = readdirSync(fixtureDir)
    .filter((name) => name.startsWith('portable_') && name.endsWith('.json'))
    .map((name) => name.slice(0, -'.json'.length))
    .sort();

  assert.ok(names.length >= 4);
  for (const name of names) {
    const jsonPipeline = loadPipelineDefinition(readFileSync(new URL(`${name}.json`, fixtureDir), 'utf8'));
    const yamlPipeline = loadPipelineDefinition(readFileSync(new URL(`${name}.yaml`, fixtureDir), 'utf8'));
    assert.deepEqual(jsonPipeline, yamlPipeline);
    assert.ok(portableClassNames(jsonPipeline).length > 0);
  }
});

test('steps alias and direct arrays match the nirs4all loader surface', () => {
  const definition = loadPipelineDefinition(readFileSync(fixtureUrl, 'utf8'));

  assert.deepEqual(loadPipelineDefinition({ steps: definition.pipeline }).pipeline, definition.pipeline);
  assert.deepEqual(loadPipelineDefinition(definition.pipeline).pipeline, definition.pipeline);
});
