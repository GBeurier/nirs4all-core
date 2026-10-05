import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import * as dag from 'dag-ml-wasm';
import { run, predict, retrain, exportWorkflow, load, workflowRecipe } from '../src/workflow.js';

const methodsPath = process.env.NIRS4ALL_METHODS_JS;
const ioPath = process.env.NIRS4ALL_IO_JS;
const qualified = Boolean(methodsPath && ioPath);

function record(X, y, ids) {
  return { schema: 'nirs4all.dataset.v1', schema_version: 1, origin_ids: ids, fold_ids: ids.map(() => null), dataset: {
    schema: 'nirs4all.multimodal-dataset', schema_version: 1, name: 'workflow-fixture', sample_ids: ids,
    sources: [{ name: 'spectra', sample_ids: ids, representation_id: 'signal_1d', axes: ['sample', 'wavelength'],
      feature_names: null, axis_units: {}, axis_coordinates: {}, array: { dtype: 'float64', shape: [X.length, 7], values: X } }],
    y: y ? { dtype: 'float64', shape: [y.length], values: y } : null, groups: null,
    partitions: { dtype: '<U7', shape: [ids.length], values: ids.map(() => y ? 'train' : 'predict') },
  } };
}

test('native signed unsafe-seed archive is refused at public load', {
  skip: !process.env.NIRS4ALL_UNSAFE_SEED_ARCHIVE,
}, async () => {
  dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
  await assert.rejects(load(new Uint8Array(fs.readFileSync(process.env.NIRS4ALL_UNSAFE_SEED_ARCHIVE)), { dagMl: dag }),
    /seed must be a nonnegative safe integer/);
});

test('native candidates select OOF, refit, export, fresh replay and retrain', { skip: !qualified }, async () => {
  const methods = await import(pathToFileURL(methodsPath)), io = await import(pathToFileURL(ioPath));
  dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
  const X = Array.from({ length: 12 }, (_, row) => Array.from({ length: 7 }, (_, col) => Math.sin((row + 1) * (col + 1) * 0.13) + row * 0.11 + col * 0.2));
  const y = X.map((row, i) => 2 * row[0] - row[2] + i * 0.13), ids = X.map((_, i) => `s${i}`);
  const options = { dagMl: dag, methods, io, components: [1, 2], seed: 17, folds: 3 };
  assert.throws(() => { workflowRecipe.steps[1].params.window_length = 7; }, TypeError);
  const originalFit = methods.RolePipeline.prototype.fit;
  let invalidFitCalls = 0;
  methods.RolePipeline.prototype.fit = () => {
    invalidFitCalls++;
    throw new Error('Invalid public seed or fold count reached native FIT');
  };
  try {
    for (const seed of [Number.MAX_SAFE_INTEGER + 1, -1, 0.5, NaN, Infinity, '91']) {
      await assert.rejects(run(record(X, y, ids), { ...options, seed }), /seed must be a nonnegative safe integer/);
    }
    for (const folds of [0, 1, 2.5, Number.MAX_SAFE_INTEGER + 1]) {
      await assert.rejects(run(record(X, y, ids), { ...options, folds }), /folds must be a safe integer/);
    }
    assert.equal(invalidFitCalls, 0);
  } finally { methods.RolePipeline.prototype.fit = originalFit; }
  const fittedRows = [];
  methods.RolePipeline.prototype.fit = function (matrix, ...rest) {
    fittedRows.push(matrix.rows);
    return originalFit.call(this, matrix, ...rest);
  };
  let workflow;
  try { workflow = await run(record(X, y, ids), options); }
  finally { methods.RolePipeline.prototype.fit = originalFit; }
  assert.equal(workflow.outcome.variant_oof_averages.length, 1);
  assert.equal(workflow.outcome.effective_plan.variants.length, 2);
  assert.equal(workflow.outcome.oof_averages[0].predictions.unit_ids.length, 12);
  assert.equal(workflow.outcome.execution_bundle.selections && Object.keys(workflow.outcome.execution_bundle.selections).length, 1);
  assert.equal(workflow.outcome.execution_bundle.refit_artifacts.length, 1);
  assert.deepEqual(fittedRows.sort((a, b) => a - b), [8, 8, 8, 8, 8, 8, 8, 8, 8, 12]);
  assert.equal(workflow.summary().variantIds.length, 2);
  assert.ok(workflow.predictions().length > 0);
  const heldout = record(X.slice(0, 2), null, ['fresh1', 'fresh2']);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'nirs4all-workflow-'));
  try {
    const exported = exportWorkflow(workflow), filename = path.join(directory, 'workflow.json');
    fs.writeFileSync(filename, JSON.stringify(exported));
    workflow = null;
    const loaded = await load(JSON.parse(fs.readFileSync(filename)));
    const rawLoaded = await load(loaded.archive, options);
    assert.deepEqual(rawLoaded.config, loaded.config);
    await assert.rejects(load(loaded.archive, { ...options, preprocessing: 'msc' }), /Only snv_savgol/);
    methods.RolePipeline.prototype.fit = () => { throw new Error('Prediction must never fit'); };
    let replay;
    try { replay = await predict(loaded, heldout, options); }
    finally { methods.RolePipeline.prototype.fit = originalFit; }
    assert.deepEqual(replay.outputs[0].predictions[0].sample_ids, ['fresh1', 'fresh2']);
    assert.ok(replay.outputs[0].predictions[0].values.flat().every(Number.isFinite));
    const badSchema = structuredClone(heldout); badSchema.dataset.sources[0].axis_units.wavelength = 'nm';
    await assert.rejects(predict(loaded, badSchema, options), /schema differs/);
    const reordered = structuredClone(heldout); reordered.dataset.sources[0].feature_names = ['b', 'a', 'c', 'd', 'e', 'f', 'g'];
    await assert.rejects(predict(loaded, reordered, options), /schema differs/);
    const script = `import fs from 'node:fs';
      import * as dag from ${JSON.stringify(import.meta.resolve('dag-ml-wasm'))};
      import * as methods from ${JSON.stringify(pathToFileURL(methodsPath).href)};
      import * as io from ${JSON.stringify(pathToFileURL(ioPath).href)};
      import {load,predict} from ${JSON.stringify(new URL('../src/workflow.js', import.meta.url).href)};
      dag.initSync({module:fs.readFileSync(${JSON.stringify(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm')).pathname)})});
      methods.RolePipeline.prototype.fit=()=>{throw Error('Fresh replay cannot fit')};
      const w=await load(fs.readFileSync(${JSON.stringify(filename)},'utf8'));
      const out=await predict(w,${JSON.stringify(heldout)},{dagMl:dag,methods,io});
      console.log(JSON.stringify(out.outputs[0].predictions[0].values));`;
    const fresh = spawnSync(process.execPath, ['--input-type=module', '-e', script], { encoding: 'utf8' });
    assert.equal(fresh.status, 0, fresh.stderr);
    assert.deepEqual(JSON.parse(fresh.stdout.trim()), replay.outputs[0].predictions[0].values);
    const newRun = await retrain(loaded, record(X, y.map(value => value + 1), ids), {
      ...options, runId: undefined, components: undefined, seed: undefined, folds: undefined, sourceId: undefined });
    assert.notEqual(newRun.outcome.outcome_fingerprint, loaded.outcome.outcome_fingerprint);
    assert.notEqual(newRun.outcome.run_id, loaded.outcome.run_id);
    assert.deepEqual(newRun.config.components, loaded.config.components);
    assert.equal(newRun.config.seed, 17);
    assert.equal(newRun.config.folds, 3);
    const bad = structuredClone(exported); bad.archive[0] ^= 1;
    await assert.rejects(load(bad));
    const badOutcome = structuredClone(exported); badOutcome.outcome.selected_variant_id = 'variant:invented';
    await assert.rejects(load(badOutcome), /differs/);
    const badConfig = structuredClone(exported); badConfig.config.components = [3, 4];
    await assert.rejects(load(badConfig), /differs/);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
