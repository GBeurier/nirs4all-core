import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';

import * as nirs4all from '../src/index.js';
import {
  dagMl,
  dagMlData,
  capabilityManifest,
  datasets,
  executionProfiles,
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
  localImplementationRegistry,
  loadMethods,
  loadMethodsWasm,
  loadPipelineDefinition,
  loadPortableStack,
  loadArchiveV2Native,
  methods,
  methodsWasm,
  parseExecutionPlan,
  portableClassNames,
  predictPortablePipeline,
  replayMethodsArchiveV2,
  runPortablePipeline,
  runtimeSurfaces,
  upstream,
  upstreams,
} from '../src/index.js';

const fixtureUrl = new URL('../../../tests/parity/fixtures/portable_methods_pipeline.json', import.meta.url);
const yamlFixtureUrl = new URL('../../../tests/parity/fixtures/portable_methods_pipeline.yaml', import.meta.url);

test('public V1 WASM surface exports expected names', () => {
  assert.deepEqual(
    Object.keys(nirs4all).sort(),
    [
      'BrowserNativePipeline',
      'NativeMultimodal',
      'NativePipeline',
      'loadBrowserPipeline',
      'predictBrowserPipeline',
      'runBrowserPipeline',
      'runMultimodal',
      'runPipeline',
      'openWorkspace',
      'BrowserTuningResult',
      'CalibratedWorkflow',
      'MultimodalPredictor',
      'N4aContainer',
      'NativeTuningResult',
      'Workflow',
      'calibrate',
      'conformalMetrics',
      'dataset',
      'exportCalibrated',
      'exportWorkflow',
      'generate',
      'load',
      'loadBrowserTuning',
      'loadCalibrated',
      'loadTuning',
      'openExperiment',
      'predict',
      'predictCalibrated',
      'predictMultimodalArchive',
      'retrain',
      'robustness',
      'run',
      'tune',
      'tuneBrowser',
      'dagMl',
      'dagMlData',
      'capabilityManifest',
      'executionProfiles',
      'createAsyncJsEstimatorController',
      'createDagMlNodeResult',
      'createDagMlModelManifest',
      'createJsEstimatorController',
      'createN4mModelController',
      'createRandomForestController',
      'datasets',
      'formats',
      'importUpstream',
      'inspectMethodsArchiveV2Predictors',
      'inspectN4aBytes',
      'io',
      'loadDagMl',
      'loadArchiveV2Native',
      'loadDagMlData',
      'loadDagMlDataWasm',
      'loadDagMlWasm',
      'loadDataIoWasm',
      'loadDatasets',
      'loadDatasetsWasm',
      'loadFormats',
      'loadIo',
      'localImplementationRegistry',
      'loadMethods',
      'loadMethodsWasm',
      'loadPipelineDefinition',
      'loadPortableStack',
      'methods',
      'methodsWasm',
      'N4M_ROLE_PREFIX',
      'N4M_TRAINED_PIPELINE_SCHEMA',
      'N4mRolePipeline',
      'n4mRoleCapabilities',
      'openN4aContainer',
      'parseExecutionPlan',
      'portableClassNames',
      'predictN4aNative',
      'predictPortablePipeline',
      'produceN4aNative',
      'readPortableArchiveV2',
      'replayMethodsArchiveV2',
      'retrainN4aNative',
      'runPortablePipeline',
      'runtimeSurfaces',
      'upstream',
      'upstreams',
      'writeN4aContainer',
      'writeN4aStream',
      'calibrateNativePackage',
      'writeNativePredictorPackage',
      'writeNativeRefitPackage',
    ].sort(),
  );
  assert.equal(nirs4all.loadPipelineDefinition, loadPipelineDefinition);
  assert.equal(nirs4all.runPortablePipeline, runPortablePipeline);
  assert.equal(nirs4all.predictPortablePipeline, predictPortablePipeline);
  assert.equal(nirs4all.loadArchiveV2Native, loadArchiveV2Native);
  assert.equal(nirs4all.replayMethodsArchiveV2, replayMethodsArchiveV2);
  assert.equal(nirs4all.runtimeSurfaces, runtimeSurfaces);
  assert.equal(nirs4all.capabilityManifest, capabilityManifest);
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
  assert.deepEqual(upstream('methods').candidates, ['@nirs4all/methods']);
  assert.deepEqual(upstream('formats').candidates, ['@nirs4all/formats-wasm']);
  assert.deepEqual(upstream('io').candidates, ['@nirs4all/io-wasm']);
  assert.deepEqual(upstream('datasets').candidates, ['@nirs4all/datasets-wasm']);
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

test('local implementation registry delegates to the DAG-ML constructor', async () => {
  class Registry {
    register_loss() {}
    register_metric() {}
    bind_training_loss() {}
  }
  const registry = await localImplementationRegistry({ LocalImplementationRegistry: Registry });
  assert.ok(registry instanceof Registry);
  assert.equal(typeof registry.register_loss, 'function');
  assert.equal(typeof registry.register_metric, 'function');
  assert.equal(typeof registry.bind_training_loss, 'function');
});

test('local implementation registry rejects an old DAG-ML module', async () => {
  await assert.rejects(
    () => localImplementationRegistry({}),
    /does not expose LocalImplementationRegistry.*upgrade dag-ml-wasm/s,
  );
});

test('local implementation registry rejects an old registry surface', async () => {
  class Registry {}
  await assert.rejects(
    () => localImplementationRegistry({ LocalImplementationRegistry: Registry }),
    /does not expose register_loss, register_metric, bind_training_loss.*upgrade dag-ml-wasm/s,
  );
});

test('public upstream loaders report the correct missing upstream', async () => {
  await assert.rejects(loadFormats, /upstream 'formats'.*@nirs4all\/formats-wasm/s);
  await assert.rejects(loadIo, /upstream 'io'.*@nirs4all\/io-wasm/s);
  await assert.rejects(loadDatasets, /upstream 'datasets'.*@nirs4all\/datasets-wasm/s);
  await assert.rejects(loadMethods, /upstream 'methods'.*@nirs4all\/methods/s);
  assert.equal(typeof (await loadDagMl()).derive_controller_manifest_json, 'function');
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

test('R-shaped train augmentation normalizes from JSON and YAML without widening other runtimes', () => {
  const source = {
    pipeline: [
      { train_augmentation: {
        class: 'n4m.NativeXAugmentation',
        params: { kind: 'gaussian_noise', values: [0.03], seed: 42 },
      } },
      { model: { class: 'sklearn.cross_decomposition.PLSRegression', params: { n_components: 2 } } },
    ],
  };
  assert.deepEqual(loadPipelineDefinition(JSON.stringify(source)),
    loadPipelineDefinition('pipeline:\n  - train_augmentation:\n      class: n4m.NativeXAugmentation\n      params:\n        kind: gaussian_noise\n        values: [0.03]\n        seed: 42\n  - model:\n      class: sklearn.cross_decomposition.PLSRegression\n      params:\n        n_components: 2\n'));
  assert.throws(() => loadPipelineDefinition({ pipeline: [
    { class: 'n4m.NativeXAugmentation' }, source.pipeline[1],
  ] }), /outside the current/);
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

const profilesFixtureUrl = new URL('../../../tests/parity/expected/capability_profiles.json', import.meta.url);
const READ_PROFILES = () => JSON.parse(readFileSync(profilesFixtureUrl, 'utf8'));
const CANONICAL = 'native-n4a-canonical';
const CONTAINER = 'n4a-container';
const NATIVE_PACKAGE = 'native-methods-package';
const PROFILE_IDS = ['native-workflow', 'native-conformal', NATIVE_PACKAGE, CANONICAL, CONTAINER];
const SURFACES = ['python', 'javascript_wasm', 'rust', 'matlab_octave'];
// MATLAB/Octave declares no entrypoint in the native package profile, so its profile surfaces are explicit.
const PROFILE_SURFACES = { [NATIVE_PACKAGE]: ['python', 'javascript_wasm', 'rust'] };
const surfacesOf = (id) => PROFILE_SURFACES[id] ?? SURFACES;
const OPERATIONS = ['compile', 'fit', 'predict', 'import', 'export', 'produce', 'inspect', 'calibrate', 'retrain', 'read', 'write', 'update'];
const S = 'supported';
const U = 'unsupported';
// Exact operation declaration, statuses in SURFACES order.
// The canonical V1 actions are explicit unsupported cells in the older profiles; the canonical
// profile declares compile/fit/import/export unsupported (no aliases) and calibration on Rust only.
// read/write/update (and the owner-neutral inspect) belong to the n4a-container storage profile alone.
const NO_CANONICAL = { produce: [U, U, U, U], inspect: [U, U, U, U], calibrate: [U, U, U, U], retrain: [U, U, U, U] };
const NO_STORAGE = { read: [U, U, U, U], write: [U, U, U, U], update: [U, U, U, U] };
const EXPECTED_STATUS = {
  'native-workflow': { compile: [U, U, S, U], fit: [S, S, S, S], predict: [S, S, S, S], import: [S, S, U, S], export: [S, S, U, S], ...NO_CANONICAL, ...NO_STORAGE },
  'native-conformal': { compile: [U, U, U, U], fit: [S, S, S, S], predict: [S, S, S, S], import: [S, S, U, S], export: [S, S, U, S], ...NO_CANONICAL, ...NO_STORAGE },
  [NATIVE_PACKAGE]: {
    compile: [U, U, U, U], fit: [U, U, U, U], predict: [S, S, S, U], import: [S, S, S, U], export: [S, S, S, U], ...NO_CANONICAL, ...NO_STORAGE,
  },
  [CANONICAL]: {
    compile: [U, U, U, U], fit: [U, U, U, U], predict: [S, S, S, S], import: [U, U, U, U], export: [U, U, U, U],
    produce: [S, S, S, S], inspect: [S, S, S, S], calibrate: [U, U, S, U], retrain: [S, S, S, S], ...NO_STORAGE,
  },
  [CONTAINER]: {
    compile: [U, U, U, U], fit: [U, U, U, U], predict: [U, U, U, U], import: [U, U, U, U], export: [U, U, U, U],
    produce: [U, U, U, U], inspect: [S, S, S, S], calibrate: [U, U, U, U], retrain: [U, U, U, U],
    read: [S, S, S, S], write: [S, S, S, S], update: [S, S, S, S],
  },
};
const EXPECTED_WASM_SYMBOLS = {
  'native-workflow': {
    fit: ['runBrowserPipeline'],
    predict: ['BrowserNativePipeline.predict', 'predictBrowserPipeline'],
    import: ['loadBrowserPipeline'],
    export: ['BrowserNativePipeline.export'],
  },
  'native-conformal': { fit: ['calibrate'], predict: ['predictCalibrated'], import: ['loadCalibrated'], export: ['exportCalibrated'] },
  [NATIVE_PACKAGE]: { predict: ['replayMethodsArchiveV2'], import: ['readPortableArchiveV2'], export: ['writeNativePredictorPackage', 'writeNativeRefitPackage'] },
  [CANONICAL]: { predict: ['predictN4aNative'], produce: ['produceN4aNative'], inspect: ['inspectN4aBytes'], retrain: ['retrainN4aNative'] },
  [CONTAINER]: { inspect: ['openN4aContainer'], read: ['openN4aContainer'], write: ['writeN4aContainer'], update: ['writeN4aContainer'] },
};
const profileById = (profiles, id) => profiles.profiles.find((item) => item.id === id);

test('execution profiles equal the generated TOML-owner fixture and the manifest copy', () => {
  const profiles = executionProfiles();
  assert.deepEqual(profiles, READ_PROFILES());
  assert.equal(profiles.schema, 'nirs4all-core.execution-profiles.v1');
  assert.equal(profiles.version, 1);
  assert.deepEqual(profiles.runtime_surfaces, SURFACES);
  assert.deepEqual(profiles.operations, OPERATIONS);
  assert.deepEqual(runtimeSurfaces, SURFACES);
  const manifest = capabilityManifest();
  assert.deepEqual(Object.keys(manifest).sort(), ['aggregate', 'executionProfiles', 'runtimeSurfaces', 'schema']);
  assert.equal(manifest.schema, 'nirs4all-core.capabilities.v1');
  assert.deepEqual(manifest.runtimeSurfaces, SURFACES);
  assert.deepEqual(manifest.executionProfiles, profiles);
});

test('exactly the native profiles are supported, with version 1 and their content types', () => {
  const profiles = executionProfiles();
  assert.deepEqual(profiles.profiles.map((item) => item.id), PROFILE_IDS);
  assert.deepEqual(profiles.profiles.map((item) => item.version), [1, 1, 1, 1, 1]);
  assert.deepEqual(profileById(profiles, 'native-workflow').content_types, [
    { schema: 'nirs4all.native-pipeline.v1', owner: 'nirs4all-core', carried_in: 'native pipeline JSON envelope' },
  ]);
  assert.deepEqual(profileById(profiles, 'native-conformal').content_types, [
    { schema: 'nirs4all.calibrated.v1', owner: 'nirs4all-core', carried_in: 'Core calibration wrapper JSON, only on the content_routes that name this schema: the Rust calibrate_archive return value, the CLI fit/load responses and the MATLAB struct. The JS/WASM host object and the Python host object carry no schema id; import/export transfer the native DAG Package V2 (scientific package schema2) carried in the owner-neutral Core V1 container (physical schema1) via native-methods-package@1' },
  ]);
  assert.ok(profileById(profiles, 'native-conformal').dependencies.includes('dag_ml'));
  // No retired scope can be reached through the public surface.
  const text = JSON.stringify(capabilityManifest());
  for (const term of ['legacy', 'nirs4all-lite', 'portable_pipeline', 'runtime_contract', 'parity-validated', 'conformal_store']) {
    assert.equal(text.includes(term), false, term);
  }
});

test('operation declarations are exact and unsupported entries carry reasons', () => {
  const profiles = executionProfiles();
  for (const [id, table] of Object.entries(EXPECTED_STATUS)) {
    const profile = profileById(profiles, id);
    assert.deepEqual(profile.surfaces, surfacesOf(id));
    assert.deepEqual(Object.keys(profile.operations), OPERATIONS);
    for (const operation of OPERATIONS) {
      const cells = profile.operations[operation];
      assert.deepEqual(Object.keys(cells), SURFACES);
      assert.deepEqual(SURFACES.map((surface) => cells[surface].status), table[operation], `${id}.${operation}`);
      for (const surface of SURFACES) {
        const cell = cells[surface];
        if (cell.status === U) {
          assert.ok(cell.reason.length > 0);
          assert.equal(cell.entrypoints, undefined);
        } else {
          assert.ok(cell.entrypoints.length > 0);
          assert.equal(cell.reason, undefined);
        }
      }
    }
  }
});

test('supported WASM entrypoints are real exports with the declared symbols', () => {
  const profiles = executionProfiles();
  let checked = 0;
  for (const profile of profiles.profiles) {
    for (const operation of OPERATIONS) {
      const cell = profile.operations[operation].javascript_wasm;
      if (cell.status !== S) continue;
      assert.deepEqual(cell.entrypoints.map((item) => item.symbol), EXPECTED_WASM_SYMBOLS[profile.id][operation]);
      for (const { symbol, source } of cell.entrypoints) {
        assert.ok(source.startsWith('bindings/wasm/src/'), source);
        const [owner, method] = symbol.split('.');
        if (method === undefined) {
          assert.equal(typeof nirs4all[owner], 'function', symbol);
        } else {
          assert.equal(typeof nirs4all[owner]?.prototype?.[method], 'function', symbol);
        }
        checked += 1;
      }
    }
  }
  assert.equal(checked, 21);
});

test('browser requirements name WASM builds only and every surface has its own requirements', () => {
  const profiles = executionProfiles();
  for (const profile of profiles.profiles) {
    assert.deepEqual(Object.keys(profile.runtime_requirements), surfacesOf(profile.id));
    const browser = profile.runtime_requirements.javascript_wasm.join(' ');
    assert.match(browser, /WASM builds/);
    assert.match(browser, /no native library path and no CLI/);
    assert.equal(profile.runtime_requirements.rust.some((row) => row.includes('CLI') && !row.includes('no CLI')), false);
    // The envelope/V2 profiles shell out to the CLI on Python and MATLAB/Octave; the canonical profile
    // uses the Python native extension (no CLI) and the CLI only on MATLAB/Octave; so does n4a-container.
    const cliSurfaces = profile.id === NATIVE_PACKAGE ? [] : [CANONICAL, CONTAINER].includes(profile.id) ? ['matlab_octave'] : ['python', 'matlab_octave'];
    for (const surface of cliSurfaces) {
      assert.ok(profile.runtime_requirements[surface].some((row) => row.includes('nirs4all-core-archive CLI')), surface);
    }
  }
});

test('current browser-native exports stay public next to the profile entrypoints', () => {
  for (const name of ['runBrowserPipeline', 'loadBrowserPipeline', 'predictBrowserPipeline']) {
    assert.equal(typeof nirs4all[name], 'function', name);
  }
  assert.equal(typeof nirs4all.BrowserNativePipeline, 'function');
  for (const method of ['predict', 'export', 'retrain', 'compare']) {
    assert.equal(typeof nirs4all.BrowserNativePipeline.prototype[method], 'function', method);
  }
});

test('the removed capability APIs are not exported and refuse access', () => {
  for (const name of ['runtimeContracts', 'artifactContracts', 'controllerCapabilities',
    'requiredKeywordRegistryEntries', 'portableOperatorClasses']) {
    assert.equal(name in nirs4all, false, name);
  }
});

test('execution profiles are returned as independent copies of frozen data', () => {
  const first = executionProfiles();
  first.profiles[0].operations.fit.javascript_wasm.status = U;
  first.profiles[1].content_types[0].schema = 'mutated';
  delete first.external_federation;
  const manifest = capabilityManifest();
  manifest.executionProfiles.profiles.length = 0;
  manifest.runtimeSurfaces.push('r');
  assert.deepEqual(executionProfiles(), READ_PROFILES());
  assert.deepEqual(capabilityManifest().executionProfiles, READ_PROFILES());
  assert.deepEqual(capabilityManifest().runtimeSurfaces, SURFACES);
  assert.equal(Object.isFrozen(nirs4all.runtimeSurfaces), true);
});

test('content is declared per route: JS/WASM never carries the wrapper schema id', () => {
  const profiles = executionProfiles();
  assert.deepEqual(profileById(profiles, 'native-workflow').content_routes, []);
  const routes = profileById(profiles, 'native-conformal').content_routes;
  assert.equal(routes.length, 21);
  const wasm = routes.filter((route) => route.surface === 'javascript_wasm');
  assert.deepEqual(wasm.map((route) => [route.operation, route.route, route.carries]), [
    ['fit', 'public_return', 'host_object'], ['fit', 'archive_transfer', 'native_archive_v2'],
    ['import', 'public_return', 'host_object'], ['import', 'archive_transfer', 'native_archive_v2'],
    ['export', 'archive_transfer', 'native_archive_v2'],
  ]);
  assert.ok(wasm.every((route) => route.schema === undefined));
  assert.deepEqual(wasm.find((route) => route.route === 'public_return').fields, ['archive', 'calibration']);
  for (const route of routes.filter((item) => item.route === 'archive_transfer')) {
    assert.equal(route.archive_schema_version, 2);
    assert.equal(route.calibration_in_archive, 'PortablePredictorPackage.conformal_calibration');
    assert.equal(route.schema, undefined);
  }
  assert.deepEqual([...new Set(routes.filter((route) => route.schema).map((route) => route.schema))], ['nirs4all.calibrated.v1']);
  // Every route names a supported public entrypoint of its operation and surface.
  for (const route of routes) {
    const cell = profileById(profiles, 'native-conformal').operations[route.operation][route.surface];
    assert.equal(cell.status, 'supported');
    assert.ok(cell.entrypoints.some((entry) => entry.symbol === route.entrypoint), JSON.stringify(route));
  }
});

test('R is externally federated, outside the Core runtime surfaces, and references only the native profiles', () => {
  const profiles = executionProfiles();
  assert.equal(profiles.runtime_surfaces.includes('r'), false);
  assert.equal(runtimeSurfaces.includes('r'), false);
  assert.equal(profiles.external_federation.length, 1);
  const [r] = profiles.external_federation;
  assert.equal(r.surface, 'r');
  assert.equal(r.core_runtime_surface, false);
  assert.equal(r.status, 'externally-federated');
  assert.equal(r.repository, 'nirs4all-r');
  assert.deepEqual(r.profiles, PROFILE_IDS);
  assert.ok(r.entrypoints.every((entry) => PROFILE_IDS.includes(entry.profile)));
});
