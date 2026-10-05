import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import * as dag from 'dag-ml-wasm';
import { generate } from '../src/tuning.js';
import { tuneBrowser, loadBrowserTuning } from '../src/browser-tuning.js';

const methodsPath = process.env.NIRS4ALL_METHODS_JS, ioPath = process.env.NIRS4ALL_IO_JS;
const fixture = process.env.NIRS4ALL_TUNING_DATASET;
const qualified = Boolean(methodsPath && ioPath && fixture);

test('WASM generator keeps constraints, shaped choices and native seeded identity', { skip: !qualified }, async () => {
  const choices = { shape: [[2, 3], [4, 5]], selection: ['one', 'two'] }, constraints = {
    exclude: [[{ dimension: 'shape', label: 'choice:1' }, { dimension: 'selection', label: 'choice:1' }]],
  };
  const all = await generate(choices, { constraints, seed: 17 });
  const sampled = await generate(choices, { strategy: 'random', count: 2, constraints, seed: 17 });
  assert.equal(all.length, 3); assert.equal(sampled.length, 2);
  assert.ok(all.every(variant => typeof variant.seed === 'string' && /^\d+$/.test(variant.seed)));
  assert.deepEqual(sampled, await generate(choices, { strategy: 'random', count: 2, constraints, seed: 17 }));
  assert.ok(sampled.every(variant => all.some(other => other.variant_id === variant.variant_id)));
});

test('browser native HPO resumes without old fits and cold model replay never fits', { skip: !qualified }, async () => {
  const methods = await import(pathToFileURL(methodsPath)), io = await import(pathToFileURL(ioPath));
  dag.initSync({ module: fs.readFileSync(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm'))) });
  const data = JSON.parse(fs.readFileSync(fixture));
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'nirs4all-browser-tuning-'));
  const snapshotPath = path.join(directory, 'checkpoint.json'), exportPath = path.join(directory, 'tuning.json');
  const options = { dagMl: dag, methods, io, seed: 91, sampler: 'random',
    persist: snapshot => fs.writeFileSync(snapshotPath, JSON.stringify(snapshot)) };
  const originalFit = methods.RolePipeline.prototype.fit;
  let fits = 0;
  methods.RolePipeline.prototype.fit = function (...args) { fits++; return originalFit.apply(this, args); };
  try {
    for (const persist of [async () => {}, () => ({ then() { throw new Error('Persistence must never be awaited'); } })]) {
      await assert.rejects(tuneBrowser(data, { ...options, trials: 1, persist }), /persistence must be synchronous/);
      assert.equal(fits, 0, 'Invalid persistence must be rejected before the first native FIT');
    }
    const first = await tuneBrowser(data, { ...options, trials: 2, runId: 'run:browser:hpo:first' });
    assert.equal(fits, 5);
    const initialFitCalls = fits;
    fits = 0;
    const resumed = await tuneBrowser(data, { ...options, trials: 4, checkpoint: JSON.parse(fs.readFileSync(snapshotPath)), runId: 'run:browser:hpo:resumed' });
    assert.equal(fits, 5);
    const resumedFitCalls = fits;
    assert.deepEqual(resumed.trials().slice(0, 2), first.trials());
    assert.equal(resumed.trials().length, 4);
    const changed = structuredClone(data); changed.dataset.sources[0].array.values[0][0] += 1;
    await assert.rejects(first.resume(changed, { ...options, trials: 4 }));
    assert.equal(fits, 5);
    await assert.rejects(first.resume(data, { ...options, trials: 4, seed: 92 }));
    assert.equal(fits, 5);
    fits = 0;
    const continuous = await tuneBrowser(data, { ...options, trials: 4, runId: 'run:browser:hpo:continuous' });
    const continuousFitCalls = fits;
    assert.deepEqual(continuous.trials(), resumed.trials());
    const exported = await resumed.export(options); fs.writeFileSync(exportPath, JSON.stringify(exported));
    let coldFitCalls = 0;
    methods.RolePipeline.prototype.fit = () => { coldFitCalls++; throw new Error('Cold native tuning replay cannot fit'); };
    const loaded = await loadBrowserTuning(JSON.parse(fs.readFileSync(exportPath)), options);
    const heldout = structuredClone(data), ids = ['cold:browser:0', 'cold:browser:1', 'cold:browser:2'];
    heldout.origin_ids = ids; heldout.fold_ids = ids.map(() => null); heldout.dataset.sample_ids = ids;
    heldout.dataset.sources[0].sample_ids = ids; heldout.dataset.sources[0].array.shape[0] = 3;
    heldout.dataset.sources[0].array.values = heldout.dataset.sources[0].array.values.slice(0, 3);
    heldout.dataset.y = null; heldout.dataset.partitions = { dtype: '<U7', shape: [3], values: ids.map(() => 'predict') };
    const replay = await loaded.predict(heldout, options);
    const prediction = replay.replay_outcome.outputs[0].prediction;
    assert.deepEqual(prediction.sample_ids, ids); assert.ok(prediction.values.flat().every(Number.isFinite));
    assert.equal(coldFitCalls, 0);
    if (process.env.NIRS4ALL_TUNING_AUDIT) {
      fs.mkdirSync(process.env.NIRS4ALL_TUNING_AUDIT, { recursive: true });
      for (const [name, value] of Object.entries({ 'browser-export.json': exported,
        'browser-first-search.json': first.search, 'browser-resumed-search.json': resumed.search,
        'browser-first-checkpoint.json': first.snapshot, 'browser-resumed-checkpoint.json': resumed.snapshot,
        'browser-cold-replay.json': replay, 'browser-fit-budget.json': {
          initialTrials: first.trials().length, initialFitCalls,
          resumedTotalTrials: resumed.trials().length, resumedFitCalls,
          retainedTrials: resumed.trials().slice(0, first.trials().length).length,
          continuousTrials: continuous.trials().length, continuousFitCalls,
          coldPredictFitCalls: coldFitCalls, counterOwner: 'Methods RolePipeline.fit',
          phaseBreakdown: 'not_observed' } })) {
        fs.writeFileSync(path.join(process.env.NIRS4ALL_TUNING_AUDIT, name), JSON.stringify(value));
      }
    }
    const bad = structuredClone(exported); bad.config.seed++;
    await assert.rejects(loadBrowserTuning(bad, options), /options differ|contract mismatch/);
    const damaged = structuredClone(exported); damaged.snapshot.n4mopt[0] ^= 1;
    await assert.rejects(loadBrowserTuning(damaged, options));
    const unsafe = structuredClone(exported); unsafe.config.seed = 2 ** 53;
    await assert.rejects(loadBrowserTuning(unsafe, options), /exact integer range/);
    const script = `import fs from 'node:fs';
      import * as dag from ${JSON.stringify(import.meta.resolve('dag-ml-wasm'))};
      import * as methods from ${JSON.stringify(pathToFileURL(methodsPath).href)};
      import * as io from ${JSON.stringify(pathToFileURL(ioPath).href)};
      import {loadBrowserTuning} from ${JSON.stringify(new URL('../src/browser-tuning.js', import.meta.url).href)};
      dag.initSync({module:fs.readFileSync(${JSON.stringify(new URL(import.meta.resolve('dag-ml-wasm/dag_ml_wasm_bg.wasm')).pathname)})});
      methods.RolePipeline.prototype.fit=()=>{throw Error('Fresh browser replay cannot fit')};
      const options={dagMl:dag,methods,io};
      const result=await loadBrowserTuning(JSON.parse(fs.readFileSync(${JSON.stringify(exportPath)})),options);
      const replay=await result.predict(${JSON.stringify(heldout)},options);
      console.log(JSON.stringify(replay.replay_outcome.outputs[0].prediction.values));`;
    const fresh = spawnSync(process.execPath, ['--input-type=module', '-e', script], { encoding: 'utf8' });
    assert.equal(fresh.status, 0, fresh.stderr);
    assert.deepEqual(JSON.parse(fresh.stdout.trim()), prediction.values);
  } finally { methods.RolePipeline.prototype.fit = originalFit; fs.rmSync(directory, { recursive: true, force: true }); }
});
