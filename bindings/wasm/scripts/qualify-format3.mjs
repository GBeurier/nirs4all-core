// Actual Core Rust/WASM projection + owner Methods WASM replays native-built
// format-3 archives. Run only against an explicitly staged local source cohort.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { replayMethodsArchiveV2 } from '../src/archive-v2.js';

const [artifacts, methodsDist] = process.argv.slice(2);
assert.ok(artifacts && methodsDist, 'usage: qualify-format3.mjs ARTIFACTS METHODS_DIST');
const methods = await import(pathToFileURL(path.join(path.resolve(methodsDist), 'index.js')));
await methods.loadModule();
assert.ok(methods.abiVersion()[1] >= 20, 'format3 requires actual owner ABI2.20');
const module = methods.getModule();
const calls = new Map();
const original = module.ccall.bind(module);
module.ccall = (symbol, ...args) => {
  calls.set(symbol, (calls.get(symbol) ?? 0) + 1);
  return original(symbol, ...args);
};
for (const name of ['raw', 'pipeline']) {
  const scenario = JSON.parse(await readFile(path.join(artifacts, `format3-${name}.scenario.json`), 'utf8'));
  const archive = await readFile(path.join(artifacts, `format3-${name}.n4a`));
  const dataset = { X: scenario.x, rows: scenario.x.length, cols: scenario.x[0].length, sampleIds: scenario.sample_ids };
  const result = await replayMethodsArchiveV2(archive, dataset, { methods });
  assert.equal(result.fallback, false);
  assert.equal(result.engine, 'nirs4all-methods-wasm');
  assert.equal(result.nativePredictorDescriptor.format_version, 3);
  assert.ok(result.nativePredictorDescriptor.writer_abi.minor >= 20);
  assert.equal(result.nativePredictorDescriptor.pipeline != null, scenario.pipeline);
  assert.deepEqual(result.sampleIds, scenario.sample_ids);
  assert.deepEqual(result.targetNames, scenario.target_names);
  assert.equal(result.data.length, scenario.expected.flat().length);
  result.data.forEach((value, index) => assert.ok(Math.abs(value - scenario.expected.flat()[index]) < 1e-9));
  const tampered = await readFile(path.join(artifacts, `format3-${name}.tampered.n4a`));
  const importsBefore = calls.get('n4m_model_import_from_buffer');
  await assert.rejects(replayMethodsArchiveV2(tampered, dataset, { methods }));
  assert.equal(calls.get('n4m_model_import_from_buffer'), importsBefore, 'Core must reject tamper before native model import');
  const older = { ...methods, abiVersion: () => [2, 19, 0] };
  await assert.rejects(replayMethodsArchiveV2(archive, dataset, { methods: older }), /requires Methods ABI 2\.20/);
  assert.equal(calls.get('n4m_model_import_from_buffer'), importsBefore, 'unsupported runtime must fail before import');
}
assert.equal(calls.get('n4m_model_import_from_buffer'), 2);
for (const fit of ['n4m_estimators_pls_fit', 'n4m_wasm_pls_fit']) assert.equal(calls.get(fit) ?? 0, 0);
console.log('Core native WASM format3 raw/pipeline: numerical parity, exact descriptors, ABI refusal and pre-import tamper refusal passed');
