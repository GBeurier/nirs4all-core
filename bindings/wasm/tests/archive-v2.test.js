import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import path from 'node:path';

import { resolveMethodsArtifact } from './methods-artifact.js';
import { MultimodalPredictor } from '../src/multimodal.js';

import {
  inspectMethodsArchiveV2Predictors,
  loadArchiveV2Native,
  calibrateNativePackage,
  readPortableArchiveV2,
  writeN4aContainer,
  writeNativePredictorPackage,
  writeNativeRefitPackage,
  replayMethodsArchiveV2,
} from '../src/index.js';

const dataset = {
  X: [[1.5, 0.5], [3.5, 1.5]],
  rows: 2,
  cols: 2,
  sampleIds: ['predict.0', 'predict.1'],
};

test('Archive V2 native surface is a Rust/WASM validator', async () => {
  const native = await loadArchiveV2Native();
  assert.equal(typeof native.ValidatedMethodsArchiveV2, 'function');
  assert.equal(typeof native.ValidatedMethodsArchiveV2.with_entrypoint, 'function');
  assert.throws(
    () => native.ValidatedMethodsArchiveV2.with_entrypoint(new Uint8Array([0x4e, 0x34, 0x61]), undefined, undefined),
    /n4a physical refusal: archive is shorter than a ZIP end record/,
  );
});

test('portable inventory keeps native-first corruption refusal', async () => {
  await assert.rejects(
    readPortableArchiveV2(new Uint8Array([0x50, 0x4b, 0x03, 0x04])),
    /archive|ZIP|zip/i,
  );
});

test('neutral V1 writer preserves traversal, duplicate and missing member refusals', async () => {
  const fixture = new URL('../../rust/nirs4all/tests/parity/fixtures/n4a_host_opaque_v1/', import.meta.url);
  const manifest = fs.readFileSync(new URL('manifest.json', fixture), 'utf8');
  const members = JSON.parse(fs.readFileSync(new URL('members.json', fixture), 'utf8'))
    .map(({ path, file }) => ({ path, bytes: new Uint8Array(fs.readFileSync(new URL(file, fixture))) }));
  await assert.rejects(writeN4aContainer(manifest, [...members, members[0]]), /manifest refusal/);
  await assert.rejects(writeN4aContainer(manifest, [{ ...members[0], path: '../escape.json' }, ...members.slice(1)]),
    /manifest refusal|physical refusal|unsafe/);
  await assert.rejects(writeN4aContainer(manifest, members.slice(1)), /manifest refusal/);
});

test('native package writers refuse malformed host inputs before Core is called', async () => {
  const bytes = new Uint8Array([123, 125]);
  await assert.rejects(writeNativePredictorPackage('', bytes, bytes), /container id/);
  await assert.rejects(writeNativePredictorPackage('archive:test', '{}', bytes), /must be an ArrayBuffer or byte view/);
  await assert.rejects(writeNativeRefitPackage('archive:test', '{}'), /must be an ArrayBuffer or byte view/);
  await assert.rejects(writeNativeRefitPackage('archive:test', bytes, { memoryBudgetBytes: -1 }), /memoryBudgetBytes/);
});

test('calibration carry refuses malformed host inputs before Core is called', async () => {
  const bytes = new Uint8Array([123, 125]);
  await assert.rejects(calibrateNativePackage(bytes, bytes, bytes, ''), /container id/);
  await assert.rejects(calibrateNativePackage('base', bytes, bytes, 'archive:test'), /must be an ArrayBuffer or byte view/);
  await assert.rejects(calibrateNativePackage(bytes, bytes, bytes, 'archive:test', { entrypoint: '' }), /entrypoint/);
});

test('native package reader keeps Core-first refusal for an old physical ZIP', async () => {
  await assert.rejects(
    readPortableArchiveV2(new Uint8Array([0x50, 0x4b, 0x03, 0x04]), { entrypoint: 'predict-external' }),
    /archive|ZIP|zip|n4a/i,
  );
});

test('standalone descriptor inspection preserves Core-first refusal', async () => {
  await assert.rejects(
    inspectMethodsArchiveV2Predictors(new Uint8Array([0x50, 0x4b, 0x03, 0x04])),
    /n4a physical refusal: archive is shorter than a ZIP end record/,
  );
});

test('invalid Archive V2 refuses before Methods is observed', async () => {
  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array([0x50, 0x4b, 0x03, 0x04]), dataset),
    /n4a physical refusal: archive is shorter than a ZIP end record/,
  );
});

test('host matrix and identity contracts refuse before archive validation', async () => {
  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array(), {
      ...dataset,
      X: [[Number.NaN, 0.5], [3.5, 1.5]],
    }),
    /finite-value contract/,
  );

  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array(), {
      ...dataset,
      sampleIds: ['predict.0', 'predict.0'],
    }),
    /distinct bounded identity strings/,
  );

  const neverIterate = {
    *[Symbol.iterator]() {
      throw new Error('sample ID iterable must not be consumed');
    },
  };
  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array(), {
      ...dataset,
      sampleIds: neverIterate,
    }),
    /sample IDs must be an array matching the row count/,
  );
});

test('runtime dependency injection cannot bypass Core or Methods authority', async () => {
  let injectedObserved = false;
  const injected = new Proxy({}, {
    get() {
      injectedObserved = true;
      throw new Error('injected dependency must never be observed');
    },
  });
  await assert.rejects(
    replayMethodsArchiveV2(new Uint8Array([0x00]), dataset, {
      archiveNative: injected,
      methods: injected,
    }),
    /n4a physical refusal: archive is shorter than a ZIP end record/,
  );
  assert.equal(injectedObserved, false);
});


test('Archive V2 shape validation retains safe dimensions and checked cell products', async () => {
  for (const [rows, cols] of [
    [0, 2], [-1, 2], [1.5, 2], [Infinity, 2], [NaN, 2],
    [1, 0], [1, -1], [1, 1.5], [1, Infinity], [1, NaN],
    [Number.MAX_SAFE_INTEGER + 1, 1], [1, Number.MAX_SAFE_INTEGER + 1],
    [Number.MAX_SAFE_INTEGER, 2],
  ]) {
    await assert.rejects(
      replayMethodsArchiveV2(new Uint8Array(), { X: [], rows, cols }),
      /positive safe integer dimensions and cell count/,
    );
  }
});

// Root selects a genuinely trained Package2 and its recorded native scenario.
// The descriptor only points to retained artifacts; it never creates an oracle.
async function retainedReplayFixture(t, variable) {
  const descriptorPath = process.env[variable];
  if (!descriptorPath) {
    if (variable === 'NIRS4ALL_CORE_CAP_REPLAY_FIXTURE') {
      assert.notEqual(process.env.NIRS4ALL_CORE_REQUIRE_METHODS_PARITY, '1',
        `${variable} must select an accepted retained Package2 fixture for this gate`);
    }
    t.skip(`${variable} is unset; real Package2 replay is unqualified`);
    return null;
  }
  const descriptor = JSON.parse(fs.readFileSync(descriptorPath, 'utf8'));
  assert.equal(typeof descriptor.archive, 'string');
  assert.equal(typeof descriptor.scenario, 'string');
  assert.ok(Number.isFinite(descriptor.tolerance) && descriptor.tolerance >= 0,
    'fixture descriptor requires an explicit owner-approved absolute tolerance');
  const directory = path.dirname(path.resolve(descriptorPath));
  const archive = fs.readFileSync(path.resolve(directory, descriptor.archive));
  const scenario = JSON.parse(fs.readFileSync(path.resolve(directory, descriptor.scenario), 'utf8'));
  assert.ok(Array.isArray(scenario.x) && scenario.x.length > 0);
  const cols = scenario.x[0].length;
  assert.ok(Number.isSafeInteger(cols) && cols > 0);
  assert.ok(scenario.x.every((row) => Array.isArray(row) && row.length === cols
    && row.every(Number.isFinite)));
  assert.equal(scenario.sample_ids.length, scenario.x.length);
  assert.ok(Array.isArray(scenario.target_names) && scenario.target_names.length > 0);
  assert.equal(scenario.expected.length, scenario.x.length);
  assert.ok(scenario.expected.every((row) => Array.isArray(row)
    && row.length === scenario.target_names.length && row.every(Number.isFinite)));
  const artifact = resolveMethodsArtifact();
  assert.deepEqual(artifact.missing, [], `stage the qualified Methods artifact at ${artifact.distPath}`);
  const methods = await import(artifact.indexUrl.href);
  return { archive, scenario, cols, tolerance: descriptor.tolerance,
    options: { methods, entrypoint: descriptor.entrypoint } };
}

test('real Package2 replay exceeds former row and cell caps at its trained width', async (t) => {
  const fixture = await retainedReplayFixture(t, 'NIRS4ALL_CORE_CAP_REPLAY_FIXTURE');
  if (!fixture) return;
  const { archive, scenario, cols, tolerance, options } = fixture;
  const rows = Math.max(4097, Math.floor(2_097_152 / cols) + 1, scenario.x.length);
  const X = new Float64Array(rows * cols);
  const sampleIds = Array.from({ length: rows }, (_, row) => `cap.${row}`);
  // Repeat retained rows in reverse order, together with their recorded owner
  // predictions. One full public replay call exercises both former ceilings.
  for (let row = 0; row < rows; row += 1) {
    X.set(scenario.x[scenario.x.length - 1 - (row % scenario.x.length)], row * cols);
  }
  assert.ok(rows > 4096 && rows * cols > 2_097_152);
  const result = await replayMethodsArchiveV2(archive, { X, rows, cols, sampleIds }, options);
  assert.equal(result.engine, 'nirs4all-methods-wasm');
  assert.equal(result.fallback, false);
  assert.equal(result.rows, rows);
  assert.equal(result.cols, scenario.target_names.length);
  assert.equal(result.nativePredictorDescriptor.dimensions.n_features, cols);
  assert.deepEqual(result.sampleIds, sampleIds);
  assert.deepEqual(result.targetNames, scenario.target_names);
  assert.equal(result.data.length, rows * result.cols);
  for (let row = 0; row < rows; row += 1) {
    const expected = scenario.expected[scenario.x.length - 1 - (row % scenario.x.length)];
    for (let target = 0; target < result.cols; target += 1) {
      assert.ok(Math.abs(result.data[row * result.cols + target] - expected[target]) <= tolerance,
        `recorded owner prediction differs at row ${row}, target ${target}`);
    }
  }
  await assert.rejects(
    replayMethodsArchiveV2(archive, {
      X: new Float64Array(cols + 1), rows: 1, cols: cols + 1, sampleIds: ['width.mismatch'],
    }, options),
    /Archive model expects \d+ features; received \d+/,
  );
});

test('real Package2 replay preserves target names longer than 128 characters', async (t) => {
  const fixture = await retainedReplayFixture(t, 'NIRS4ALL_CORE_LONG_TARGET_REPLAY_FIXTURE');
  if (!fixture) return;
  const { archive, scenario, cols, tolerance, options } = fixture;
  assert.ok(scenario.target_names.some((name) => name.length > 128),
    'select a retained Package2 with a genuinely long declared target name');
  const result = await replayMethodsArchiveV2(archive, {
    X: scenario.x, rows: scenario.x.length, cols, sampleIds: scenario.sample_ids,
  }, options);
  assert.deepEqual(result.targetNames, scenario.target_names);
  assert.deepEqual(result.sampleIds, scenario.sample_ids);
  assert.equal(result.data.length, scenario.expected.length * scenario.target_names.length);
  result.data.forEach((value, index) => {
    const expected = scenario.expected[Math.floor(index / result.cols)][index % result.cols];
    assert.ok(Math.abs(value - expected) <= tolerance);
  });
});

test('multimodal load retains empty-state and byte-integrity refusals', async () => {
  const methods = { MultimodalPipeline: { fromState() {
    assert.fail('invalid envelope must refuse before the owner state importer');
  } } };
  for (const state of [[], [256], [-1], [0.5], [NaN], [Infinity]]) {
    await assert.rejects(MultimodalPredictor.load({
      schema: 'nirs4all.multimodal-predictor.v1', schema_version: 1,
      recipe: {}, source_schemas: {}, target_names: ['target'], state,
    }, { io: {}, methods }), /Invalid multimodal predictor envelope or native byte state/);
  }
});
