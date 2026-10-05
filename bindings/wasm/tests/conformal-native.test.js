import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import * as dag from 'dag-ml-wasm';
import { load, run, workflowReplay } from '../src/workflow.js';
import { calibrate, predictCalibrated, conformalMetrics, exportCalibrated, loadCalibrated } from '../src/conformal.js';
import { robustness } from '../src/robustness.js';
import { loadArchiveV2Native, readPortableArchiveV2 } from '../src/archive-v2.js';

const methodsPath = process.env.NIRS4ALL_METHODS_JS, ioPath = process.env.NIRS4ALL_IO_JS;
const qualified = Boolean(methodsPath && ioPath);

function cohort(prefix, observed = true) {
  const X = Array.from({ length: 12 }, (_, row) => Array.from({ length: 7 }, (_, col) => Math.sin((row + 1) * (col + 1) * 0.13) + row * 0.11 + col * 0.2));
  const ids = X.map((_, i) => `${prefix}${String(i).padStart(2, '0')}`);
  const y = X.map((row, i) => 2 * row[0] - row[2] + i * 0.13);
  return { schema: 'nirs4all.dataset.v1', schema_version: 1, origin_ids: ids, fold_ids: ids.map(() => null), dataset: {
    schema: 'nirs4all.multimodal-dataset', schema_version: 1, name: 'conformal-fixture', sample_ids: ids,
    sources: [{ name: 'spectra', sample_ids: ids, representation_id: 'signal_1d', axes: ['sample', 'wavelength'],
      feature_names: null, axis_units: {}, axis_coordinates: {}, array: { dtype: 'float64', shape: [12, 7], values: X } }],
    y: observed ? { dtype: 'float64', shape: [12], values: y } : null, groups: null,
    partitions: { dtype: '<U7', shape: [12], values: ids.map(() => observed ? 'train' : 'predict') },
  } };
}

async function assertIndependentPhysicalRefusals(model, record, truth, options) {
  const rows = record.dataset.sample_ids.length;
  const archive = await readPortableArchiveV2(model.archive);
  const pkg = JSON.parse(new TextDecoder().decode(archive.members[archive.manifest.replay.portable_predictor_package.member_path]));
  const trainingIds = pkg.effective_plan.fold_set.sample_ids;
  const repetitionOnly = structuredClone(record);
  repetitionOnly.dataset.repetition_ids = Array(rows).fill('rep:0');
  assert.throws(() => options.io.dataset(repetitionOnly), /repetition_ids require independent_unit_ids/);
  for (const enrich of [
    value => { value.dataset.groups = { dtype: 'object', shape: [rows], values: Array(rows).fill(trainingIds[0]) }; },
    value => { value.origin_ids = Array(rows).fill(trainingIds[0]); },
    value => { value.dataset.independent_unit_ids = Array.from({ length: rows }, (_, row) => trainingIds[row % trainingIds.length]); },
    value => {
      value.dataset.independent_unit_ids = [...value.dataset.sample_ids];
      value.dataset.repetition_ids = Array(rows).fill('rep:0');
    },
  ]) {
    const enriched = structuredClone(record); enrich(enriched);
    // These are valid IO datasets. Uncertainty's narrower public profile must
    // reject them rather than erase declared identity metadata during replay.
    const normalized = options.io.dataset(enriched).toJSON();
    assert.deepEqual(normalized.origin_ids, enriched.origin_ids);
    assert.deepEqual(normalized.dataset.groups, enriched.dataset.groups);
    for (const key of ['independent_unit_ids', 'repetition_ids'])
      if (Object.hasOwn(enriched.dataset, key)) assert.deepEqual(normalized.dataset[key], enriched.dataset[key]);
    await assert.rejects(predictCalibrated(model, enriched, options), /independent physical/);
    await assert.rejects(robustness(model, enriched, truth, options), /independent physical/);
  }
}

test('native calibrate/replay/metrics/cold archive reload and frozen Gaussian audits', { skip: !qualified }, async () => {
  const methods = await import(pathToFileURL(methodsPath)), io = await import(pathToFileURL(ioPath));
  dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
  const options = { dagMl: dag, methods, io, components: [1, 2], runId: 'run:conformal:wasm' };
  const workflow = await run(cohort('train.'), options);
  const originalFit = methods.RolePipeline.prototype.fit;
  methods.RolePipeline.prototype.fit = () => { throw new Error('Calibration or replay must never FIT'); };
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'nirs4all-calibration-'));
  try {
    const calibrated = await calibrate(workflow, cohort('cal.'), { ...options, coverages: [0.8, 0.9] });
    const prediction = await predictCalibrated(calibrated, cohort('test.', false), options);
    assert.deepEqual(prediction.sample_ids, cohort('test.', false).dataset.sample_ids);
    assert.equal(prediction.interval_block.intervals.length, 2);
    const truth = { sample_ids: prediction.sample_ids, values: cohort('test.').dataset.y.values.map(y => [y]) };
    const metrics = await conformalMetrics(calibrated, prediction, truth);
    assert.equal(metrics.coverages.length, 2);
    await assertIndependentPhysicalRefusals(calibrated, cohort('test.', false), truth, options);
    const aliasCalibration = cohort('fresh.cal.'); aliasCalibration.origin_ids = Array(12).fill('train.00');
    assert.deepEqual(io.dataset(aliasCalibration).toJSON().origin_ids, aliasCalibration.origin_ids);
    await assert.rejects(calibrate(workflow, aliasCalibration, options), /independent physical/);
    const file = path.join(directory, 'calibrated.n4a'); fs.writeFileSync(file, exportCalibrated(calibrated));
    const loaded = await loadCalibrated(fs.readFileSync(file), options);
    const again = await predictCalibrated(loaded, cohort('test.', false), options);
    assert.deepEqual(again.interval_block.intervals, prediction.interval_block.intervals);
    const report = await robustness(loaded, cohort('test.', false), truth, options);
    const repeated = await robustness(loaded, cohort('test.', false), truth, options);
    assert.equal(report.mode, 'clean_frozen');
    assert.deepEqual(report.scenarios[1].point_predictions, repeated.scenarios[1].point_predictions);
    assert.notDeepEqual(report.scenarios[0].point_predictions, report.scenarios[1].point_predictions);
    await assert.rejects(calibrate(workflow, cohort('train.'), options), /overlap/);
    await assert.rejects(predictCalibrated(loaded, cohort('cal.', false), options), /overlap/);
    const bad = cohort('bad.'); bad.dataset.sources[0].axis_units.wavelength = 'nm';
    await assert.rejects(calibrate(workflow, bad, options), /schema/);
    const script = `import fs from 'node:fs'; import * as dag from ${JSON.stringify(import.meta.resolve('dag-ml-wasm'))};
      import * as methods from ${JSON.stringify(pathToFileURL(methodsPath).href)}; import * as io from ${JSON.stringify(pathToFileURL(ioPath).href)};
      import {loadCalibrated,predictCalibrated} from ${JSON.stringify(new URL('../src/conformal.js', import.meta.url).href)};
      dag.initSync({module:fs.readFileSync(${JSON.stringify(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm')).pathname)})});
      const options={dagMl:dag,methods,io};const model=await loadCalibrated(fs.readFileSync(process.argv[1]),options);
      methods.RolePipeline.prototype.fit=()=>{throw Error('cold replay FIT forbidden')};
      console.log(JSON.stringify((await predictCalibrated(model,${JSON.stringify(cohort('test.', false))},options)).interval_block.intervals));`;
    const cold = spawnSync(process.execPath, ['--input-type=module', '-e', script, file], { encoding: 'utf8' });
    assert.equal(cold.status, 0, cold.stderr);
    assert.deepEqual(JSON.parse(cold.stdout), prediction.interval_block.intervals);
  } finally { methods.RolePipeline.prototype.fit = originalFit; fs.rmSync(directory, { recursive: true, force: true }); }
});

test('shared C-native calibrated archive replays actual N4MM intervals and noise in WASM',
  { skip: !qualified || !process.env.NIRS4ALL_BLOCK5_FIXTURE }, async () => {
    const fixture = process.env.NIRS4ALL_BLOCK5_FIXTURE;
    const methods = await import(pathToFileURL(methodsPath)), io = await import(pathToFileURL(ioPath));
    dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
    let inspections = 0;
    const injectedMethods = { ...methods, inspectN4mm(...args) { inspections++; return methods.inspectN4mm(...args); } };
    const options = { dagMl: dag, methods: injectedMethods, io };
    const model = await loadCalibrated(fs.readFileSync(path.join(fixture, 'shared-calibrated.n4a')), options);
    const input = JSON.parse(fs.readFileSync(path.join(fixture, 'independent.json')));
    const reference = JSON.parse(fs.readFileSync(path.join(fixture, 'shared-prediction.json')));
    const record = cohort('test.', false);
    record.dataset.sample_ids = input.sample_ids; record.origin_ids = input.sample_ids;
    record.dataset.sources[0].sample_ids = input.sample_ids; record.dataset.sources[0].array.values = input.x;
    const prediction = await predictCalibrated(model, record, options);
    assert.ok(inspections > 0, 'C-native conformal replay must use the supplied Methods runtime');
    assert.equal(prediction.execution, 'callback_free_methods_n4mm');
    assert.deepEqual(prediction.sample_ids, input.sample_ids);
    const actual = prediction.point_prediction.values.flat(), expected = reference.point_prediction.values.flat();
    for (let i = 0; i < actual.length; i++) assert.ok(Math.abs(actual[i] - expected[i]) < 1e-10);
    assert.equal(prediction.interval_block.intervals.length, reference.interval_block.intervals.length);
    prediction.interval_block.intervals.forEach((interval, coverage) => {
      interval.cells.flat().forEach((cell, i) => {
        const expectedCell = reference.interval_block.intervals[coverage].cells.flat()[i];
        assert.equal(cell.status, expectedCell.status);
        if (cell.status === 'finite') {
          assert.ok(Math.abs(cell.lower - expectedCell.lower) < 1e-10);
          assert.ok(Math.abs(cell.upper - expectedCell.upper) < 1e-10);
        }
      });
    });
    const metrics = await conformalMetrics(model, prediction, { sample_ids: input.sample_ids, values: input.y.map(y => [y]) });
    assert.equal(metrics.coverages.length, 2);
    await assertIndependentPhysicalRefusals(model, record,
      { sample_ids: input.sample_ids, values: input.y.map(y => [y]) }, options);
    const missing = structuredClone(record);
    missing.dataset.source_alignment = 'left';
    missing.dataset.sources[0].sample_ids = missing.dataset.sources[0].sample_ids.slice(1);
    missing.dataset.sources[0].array.shape[0]--;
    missing.dataset.sources[0].array.values = missing.dataset.sources[0].array.values.slice(1);
    assert.equal(io.dataset(missing).toJSON().dataset.sources[0].presence_mask.values[0], false);
    await assert.rejects(predictCalibrated(model, missing, options), /complete numeric source/);
    await assert.rejects(robustness(model, missing,
      { sample_ids: input.sample_ids, values: input.y.map(y => [y]) }, options), /complete numeric source/);
    const report = await robustness(model, record, { sample_ids: input.sample_ids, values: input.y.map(y => [y]) }, options);
    assert.equal(report.execution, 'callback_free_methods_n4mm');
    assert.equal(report.mode, 'clean_frozen');
    assert.notDeepEqual(report.scenarios[0].point_predictions, report.scenarios[1].point_predictions);
    const nativeReport = JSON.parse(fs.readFileSync(path.join(fixture, 'shared-robustness.json')));
    const noise = report.scenarios[1].point_predictions.flat(), expectedNoise = nativeReport.scenarios[1].point_predictions.flat();
    noise.forEach((value, i) => assert.ok(Math.abs(value - expectedNoise[i]) < 1e-10));
    const repeated = await robustness(model, record, { sample_ids: input.sample_ids, values: input.y.map(y => [y]) }, options);
    assert.deepEqual(report.scenarios[1].point_predictions, repeated.scenarios[1].point_predictions);
  });

test('uncalibrated C-native workflow audits observed and Gaussian points after cold load without FIT',
  { skip: !qualified || !process.env.NIRS4ALL_BLOCK5_FIXTURE }, async () => {
    const fixture = process.env.NIRS4ALL_BLOCK5_FIXTURE;
    const methods = await import(pathToFileURL(methodsPath)), io = await import(pathToFileURL(ioPath));
    dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
    let inspections = 0;
    const injectedMethods = { ...methods, inspectN4mm(...args) { inspections++; return methods.inspectN4mm(...args); } };
    const options = { dagMl: dag, methods: injectedMethods, io };
    const input = JSON.parse(fs.readFileSync(path.join(fixture, 'independent.json')));
    const reference = JSON.parse(fs.readFileSync(path.join(fixture, 'shared-uncalibrated-robustness.json')));
    const record = cohort('test.', false);
    record.dataset.sample_ids = input.sample_ids; record.origin_ids = input.sample_ids;
    record.dataset.sources[0].sample_ids = input.sample_ids; record.dataset.sources[0].array.values = input.x;
    const truth = { sample_ids: input.sample_ids, values: input.y.map(y => [y]) };
    const file = path.join(fixture, 'shared-model.n4a');
    const model = await load(fs.readFileSync(file), options);
    assert.equal(model.outcome.conformal_calibration, undefined);
    await methods.loadModule();
    const module = methods.getModule(), originalCall = module.ccall;
    const originalFit = methods.RolePipeline.prototype.fit;
    module.ccall = function(name, ...args) {
      if (/^n4m_(model|role_pipeline|estimator)_fit/.test(name)) throw new Error('Frozen audit model FIT forbidden');
      return originalCall.call(this, name, ...args);
    };
    methods.RolePipeline.prototype.fit = () => { throw new Error('Frozen audit RolePipeline FIT forbidden'); };
    let report;
    try {
      report = await robustness(model, record, truth, options);
      assert.equal(inspections, 2, 'Both native scenarios must use the supplied Methods runtime');
      assert.equal(report.calibration_fingerprint, null);
      assert.equal(report.audit_only, true);
      assert.equal(report.mode, 'clean_frozen');
      assert.equal(report.execution, 'callback_free_methods_n4mm');
      report.scenarios.forEach((scenario, index) => {
        assert.equal(scenario.intervals, null);
        assert.equal(typeof scenario.point_prediction_fingerprint, 'string');
        const expected = reference.scenarios[index];
        scenario.point_predictions.flat().forEach((point, row) => assert.ok(Math.abs(point - expected.point_predictions.flat()[row]) < 1e-10));
        for (const metric of ['mae', 'rmse']) assert.ok(Math.abs(scenario.metrics.metrics[metric] - expected.metrics.metrics[metric]) < 1e-10);
      });
      assert.notDeepEqual(report.scenarios[0].point_predictions, report.scenarios[1].point_predictions);
      const repeated = await robustness(model, record, truth, options);
      assert.deepEqual(repeated.scenarios[1].point_predictions, report.scenarios[1].point_predictions);
      const bad = structuredClone(record); bad.dataset.sources[0].axis_units.wavelength = 'nm';
      await assert.rejects(robustness(model, bad, truth, options), /schema/);
      const overlap = cohort('train.', false);
      overlap.dataset.sample_ids = model.outcome.effective_plan.fold_set.sample_ids;
      overlap.origin_ids = overlap.dataset.sample_ids; overlap.dataset.sources[0].sample_ids = overlap.dataset.sample_ids;
      await assert.rejects(robustness(model, overlap, { ...truth, sample_ids: overlap.dataset.sample_ids }, options), /overlap/);
    } finally { module.ccall = originalCall; methods.RolePipeline.prototype.fit = originalFit; }
    const script = `import fs from 'node:fs';import * as dag from ${JSON.stringify(import.meta.resolve('dag-ml-wasm'))};
      import * as methods from ${JSON.stringify(pathToFileURL(methodsPath).href)};import * as io from ${JSON.stringify(pathToFileURL(ioPath).href)};
      import {load} from ${JSON.stringify(new URL('../src/workflow.js', import.meta.url).href)};
      import {robustness} from ${JSON.stringify(new URL('../src/robustness.js', import.meta.url).href)};
      dag.initSync({module:fs.readFileSync(${JSON.stringify(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm')).pathname)})});
      await methods.loadModule();const module=methods.getModule(),original=module.ccall;
      module.ccall=function(name,...args){if(/^n4m_(model|role_pipeline|estimator)_fit/.test(name))throw Error('cold model FIT forbidden');return original.call(this,name,...args)};
      methods.RolePipeline.prototype.fit=()=>{throw Error('cold RolePipeline FIT forbidden')};
      const options={dagMl:dag,methods,io},model=await load(fs.readFileSync(process.argv[1]),options);
      console.log(JSON.stringify(await robustness(model,${JSON.stringify(record)},${JSON.stringify(truth)},options)));`;
    const cold = spawnSync(process.execPath, ['--input-type=module', '-e', script, file], { encoding: 'utf8' });
    assert.equal(cold.status, 0, cold.stderr);
    assert.deepEqual(JSON.parse(cold.stdout).scenarios, report.scenarios);
  });


test('public calibration joins nonlexical IDs and independently permuted truth natively', { skip: !qualified }, async () => {
  const methods = await import(pathToFileURL(methodsPath)), io = await import(pathToFileURL(ioPath));
  dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
  const options = { dagMl: dag, methods, io, components: [1, 2], runId: 'run:calibration:nonlexical' };
  const training = cohort('nonlexical.train.');
  const workflow = await run(training, options);
  const calibration = cohort('cal.');
  const ids = calibration.dataset.sample_ids.map((_, i) => `cal.s${i}`);
  calibration.origin_ids = ids; calibration.dataset.sample_ids = ids;
  calibration.dataset.sources[0].sample_ids = ids;
  const originalFit = methods.RolePipeline.prototype.fit;
  methods.RolePipeline.prototype.fit = () => { throw Error('Calibration must never FIT'); };
  try {
    const calibrated = await calibrate(workflow, calibration, { ...options, coverages: [0.8, 0.9] });
    assert.deepEqual(calibrated.calibration.sample_ids, [...ids].sort());
    const details = await workflowReplay(workflow, calibration, options, true);
    const native = await loadArchiveV2Native();
    const archive = await readPortableArchiveV2(workflow.archive);
    const source = new TextDecoder().decode(archive.members[archive.manifest.replay.training_artifacts.training_outcome.member_path]);
    const capture = truth => JSON.parse(native.calibrate_workflow_replay_json(source, details.replayJson,
      JSON.stringify(details.relations), JSON.stringify(truth), '[0.8,0.9]', '"error"'));
    const permuted = structuredClone(details.truth);
    permuted.sample_ids.reverse(); permuted.values.reverse(); permuted.validity_masks.reverse();
    assert.deepEqual(capture(permuted).calibration.quantiles, calibrated.calibration.quantiles);
    const missingNames = structuredClone(permuted); delete missingNames.target_names;
    assert.throws(() => capture(missingNames), /missing field.*target_names/);
    const missingMasks = structuredClone(permuted); delete missingMasks.validity_masks;
    assert.throws(() => capture(missingMasks), /missing field.*validity_masks/);
    assert.throws(() => capture({ sample_ids: permuted.sample_ids, values: permuted.values }), /missing field/);
    assert.throws(() => capture({ ...permuted, target_names: null }), /invalid type/);
    assert.throws(() => capture({ ...permuted, validity_masks: null }), /invalid type/);
    const wrongSet = structuredClone(details.truth); wrongSet.sample_ids[0] = 'unknown.calibration';
    assert.throws(() => capture(wrongSet), /sample IDs differ/);
    const duplicate = structuredClone(details.truth); duplicate.sample_ids[0] = duplicate.sample_ids[1];
    assert.throws(() => capture(duplicate), /duplicate sample IDs/);
    const wrongNames = structuredClone(details.truth); wrongNames.target_names = ['wrong'];
    assert.throws(() => capture(wrongNames), /target schema/);
    const wrongWidth = structuredClone(details.truth); wrongWidth.values[0].push(1);
    assert.throws(() => capture(wrongWidth), /complete finite truth/);
    const masked = structuredClone(details.truth); masked.validity_masks[0][0] = false;
    assert.throws(() => capture(masked), /complete finite truth/);
    const maskedDataset = structuredClone(calibration);
    maskedDataset.dataset.target_mask = { dtype: 'bool', shape: [12], values: ids.map((_, i) => i !== 0) };
    await assert.rejects(calibrate(workflow, maskedDataset, options), /complete finite truth/);
    const renamedDataset = structuredClone(calibration); renamedDataset.dataset.target_names = ['wrong'];
    await assert.rejects(calibrate(workflow, renamedDataset, options), /target schema/);
    const loaded = await loadCalibrated(exportCalibrated(calibrated), options);
    assert.deepEqual(loaded.calibration.quantiles, calibrated.calibration.quantiles);
  } finally { methods.RolePipeline.prototype.fit = originalFit; }
});
