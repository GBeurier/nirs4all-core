import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';

import * as nirs4all from '../src/index.js';
import {
  dagMl,
  dagMlData,
  datasets,
  formats,
  importUpstream,
  io,
  loadDagMl,
  loadDagMlData,
  loadDagMlDataWasm,
  loadDagMlWasm,
  loadDataIoWasm,
  loadDatasets,
  loadDatasetsWasm,
  loadFormats,
  loadIo,
  loadMethods,
  loadMethodsWasm,
  loadPipelineDefinition,
  loadPortableStack,
  methods,
  methodsWasm,
  portableClassNames,
  portableOperatorClasses,
  predictPortablePipeline,
  runPortablePipeline,
  upstream,
  upstreams,
} from '../src/index.js';

const fixtureUrl = new URL('../../../tests/parity/fixtures/portable_methods_pipeline.json', import.meta.url);
const yamlFixtureUrl = new URL('../../../tests/parity/fixtures/portable_methods_pipeline.yaml', import.meta.url);

test('public V1 WASM surface exports expected names', () => {
  assert.deepEqual(
    Object.keys(nirs4all).sort(),
    [
      'dagMl',
      'dagMlData',
      'datasets',
      'formats',
      'importUpstream',
      'io',
      'loadDagMl',
      'loadDagMlData',
      'loadDagMlDataWasm',
      'loadDagMlWasm',
      'loadDataIoWasm',
      'loadDatasets',
      'loadDatasetsWasm',
      'loadFormats',
      'loadIo',
      'loadMethods',
      'loadMethodsWasm',
      'loadPipelineDefinition',
      'loadPortableStack',
      'methods',
      'methodsWasm',
      'parseExecutionPlan',
      'portableClassNames',
      'portableOperatorClasses',
      'predictPortablePipeline',
      'runPortablePipeline',
      'upstream',
      'upstreams',
    ].sort(),
  );
  assert.equal(nirs4all.loadPipelineDefinition, loadPipelineDefinition);
  assert.equal(nirs4all.runPortablePipeline, runPortablePipeline);
  assert.equal(nirs4all.predictPortablePipeline, predictPortablePipeline);
  assert.equal(nirs4all.portableOperatorClasses, portableOperatorClasses);
  assert.equal(nirs4all.methodsWasm, methodsWasm);
});

test('expected upstreams are registered', () => {
  assert.deepEqual(
    upstreams.map((item) => item.key),
    ['dag_ml', 'dag_ml_data', 'formats', 'io', 'datasets', 'methods'],
  );
});

test('upstream lookup returns metadata', () => {
  assert.equal(upstream('methods').role, 'Portable C ABI PLS/NIRS numerical engine');
  assert.deepEqual(upstream('methods').candidates, ['@nirs4all/methods-wasm']);
  assert.deepEqual(upstream('formats').candidates, ['@nirs4all/formats-wasm', 'nirs4all-formats-wasm']);
  assert.deepEqual(upstream('io').candidates, ['@nirs4all/io-wasm', 'nirs4all-io-wasm']);
  assert.deepEqual(upstream('datasets').candidates, [
    '@nirs4all/datasets-wasm',
    '@nirs4all/nirs4all-datasets-wasm',
  ]);
  assert.equal(upstream('missing'), null);
});

test('domain proxies expose keys', () => {
  assert.equal(formats.key, 'formats');
  assert.equal(io.key, 'io');
  assert.equal(datasets.key, 'datasets');
  assert.equal(methods.key, 'methods');
  assert.equal(dagMl.key, 'dag_ml');
  assert.equal(dagMlData.key, 'dag_ml_data');
});

test('public upstream loaders map to the declared V1 upstreams', () => {
  assert.equal(typeof loadFormats, 'function');
  assert.equal(typeof loadIo, 'function');
  assert.equal(typeof loadDatasets, 'function');
  assert.equal(typeof loadMethods, 'function');
  assert.equal(typeof loadDagMl, 'function');
  assert.equal(typeof loadDagMlData, 'function');
  assert.equal(typeof loadMethodsWasm, 'function');
  assert.equal(typeof loadDagMlWasm, 'function');
  assert.equal(typeof loadDagMlDataWasm, 'function');
  assert.equal(typeof loadDatasetsWasm, 'function');
  assert.equal(typeof loadDataIoWasm, 'function');
});

test('public upstream loaders report the correct missing upstream', async () => {
  await assert.rejects(loadFormats, /upstream 'formats'.*@nirs4all\/formats-wasm, nirs4all-formats-wasm/s);
  await assert.rejects(loadIo, /upstream 'io'.*@nirs4all\/io-wasm, nirs4all-io-wasm/s);
  await assert.rejects(
    loadDatasets,
    /upstream 'datasets'.*@nirs4all\/datasets-wasm, @nirs4all\/nirs4all-datasets-wasm/s,
  );
  await assert.rejects(loadMethods, /upstream 'methods'.*@nirs4all\/methods-wasm/s);
  await assert.rejects(loadDagMl, /upstream 'dag_ml'.*dag-ml-wasm/s);
  await assert.rejects(loadDagMlData, /upstream 'dag_ml_data'.*dag-ml-data-wasm/s);
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
