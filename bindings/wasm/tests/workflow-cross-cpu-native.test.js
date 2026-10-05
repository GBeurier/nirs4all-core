import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import * as dag from 'dag-ml-wasm';
import { Workflow, load, predict, retrain } from '../src/workflow.js';

const archivePath = process.env.NIRS4ALL_CPU_ARCHIVE;
const datasetPath = process.env.NIRS4ALL_CPU_PREDICT_DATASET;
const methodsPath = process.env.NIRS4ALL_METHODS_JS;
const ioPath = process.env.NIRS4ALL_IO_JS;
const qualified = Boolean(archivePath && datasetPath && methodsPath && ioPath);

test('public raw C-native archive load and cold WASM prediction retain native ABI schema', { skip: !qualified }, async () => {
  const methods = await import(pathToFileURL(methodsPath)), io = await import(pathToFileURL(ioPath));
  dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
  const options = { dagMl: dag, methods, io };
  const archiveBytes = new Uint8Array(fs.readFileSync(archivePath));
  const dataset = JSON.parse(fs.readFileSync(datasetPath));
  dataset.dataset.y = null; dataset.dataset.target_mask = null;
  dataset.dataset.partitions.dtype = '<U7';
  dataset.dataset.partitions.values = dataset.dataset.sample_ids.map(() => 'predict');
  const workflow = await load(archiveBytes, options);
  const constructed = new Workflow(archiveBytes, workflow.outcome, workflow.config, 'wasm_role_pipeline');
  await assert.rejects(retrain(constructed, dataset, options), /C-native N4MM retraining requires a native CPU host/);
  await assert.rejects(retrain(workflow, dataset, options), /C-native N4MM retraining requires a native CPU host/);
  const originalFit = methods.RolePipeline.prototype.fit;
  methods.RolePipeline.prototype.fit = () => { throw new Error('C-native replay cannot fit a host pipeline'); };
  let predicted;
  try { predicted = await predict(workflow, dataset, options); }
  finally { methods.RolePipeline.prototype.fit = originalFit; }
  assert.equal(predicted.schema, 'nirs4all.core.archive-v2-replay.v1');
  assert.equal(predicted.engine, 'nirs4all-methods-wasm');
  assert.equal(predicted.fallback, false);
  assert.equal(predicted.rows, dataset.dataset.sample_ids.length);
  assert.equal(predicted.cols, 1);
  assert.ok(predicted.data.every(Number.isFinite));
  assert.equal(Object.hasOwn(predicted, 'outputs'), false);
  const badUnits = structuredClone(dataset); badUnits.dataset.sources[0].axis_units.wavelength = 'different-unit';
  await assert.rejects(predict(workflow, badUnits, options), /schema differs/);
  const badColumns = structuredClone(dataset); badColumns.dataset.sources[0].feature_names = ['b', 'a', 'c', 'd', 'e', 'f', 'g'];
  await assert.rejects(predict(workflow, badColumns, options), /schema differs/);
  const badAxes = structuredClone(dataset); badAxes.dataset.sources[0].axes = ['sample', 'feature'];
  await assert.rejects(predict(workflow, badAxes, options));
  await assert.rejects(load(archiveBytes, { ...options, preprocessing: 'msc' }), /Only snv_savgol/);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'nirs4all-cold-public-'));
  try {
    const copiedArchive = path.join(directory, 'model.n4a'); fs.writeFileSync(copiedArchive, archiveBytes);
    const script = `import fs from 'node:fs';
      import * as dag from ${JSON.stringify(import.meta.resolve('dag-ml-wasm'))};
      import * as methods from ${JSON.stringify(pathToFileURL(methodsPath).href)};
      import * as io from ${JSON.stringify(pathToFileURL(ioPath).href)};
      import {load,predict} from ${JSON.stringify(new URL('../src/workflow.js', import.meta.url).href)};
      dag.initSync({module:fs.readFileSync(${JSON.stringify(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm')).pathname)})});
      await methods.loadModule();
      methods.RolePipeline.prototype.fit=()=>{throw Error('Cold replay cannot fit')};
      const native=methods.getModule(), original=native.ccall;
      native.ccall=function(name,...args){if(name.includes('_fit'))throw Error('Cold native ABI cannot fit');return original.call(this,name,...args)};
      const options={dagMl:dag,methods,io};
      const w=await load(new Uint8Array(fs.readFileSync(${JSON.stringify(copiedArchive)})),options);
      const result=await predict(w,${JSON.stringify(dataset)},options);
      console.log(JSON.stringify({schema:result.schema,data:result.data,sampleIds:result.sampleIds}));`;
    const cold = spawnSync(process.execPath, ['--input-type=module', '-e', script], { cwd: directory, encoding: 'utf8' });
    assert.equal(cold.status, 0, cold.stderr);
    const actual = JSON.parse(cold.stdout.trim());
    assert.equal(actual.schema, predicted.schema);
    assert.deepEqual(actual.data, predicted.data);
    assert.deepEqual(actual.sampleIds, predicted.sampleIds);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
