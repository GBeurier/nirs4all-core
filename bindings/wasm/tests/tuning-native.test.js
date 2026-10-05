import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { execFileSync } from 'node:child_process';
import { tune, loadTuning } from '../src/tuning.js';

const fixture = process.env.NIRS4ALL_TUNING_DATASET, methodsLibrary = process.env.N4M_LIBRARY_PATH;
const qualified = Boolean(fixture && methodsLibrary && process.env.NIRS4ALL_CORE_CLI);
test('Node HPO retains completed trials, exports, reopens and predicts cold', { skip: !qualified }, async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'nirs4all-node-hpo-'));
  try {
    const data = JSON.parse(fs.readFileSync(fixture)), options = { methodsLibrary, trials: 2, seed: 91 };
    const first = await tune(data, { ...options, archive: path.join(directory, 'first.n4a'), runId: 'run:node:hpo:first' });
    const resumed = await first.resume(data, { trials: 4, archive: path.join(directory, 'resumed.n4a'), runId: 'run:node:hpo:resumed' });
    assert.deepEqual(resumed.trials().slice(0, 2), first.trials());
    assert.equal(resumed.trials().length, 4);
    assert.equal(resumed.compare().length, 4);
    const inputIds = data.dataset.sample_ids;
    assert.equal(inputIds.indexOf('s10'), 10);
    assert.notDeepEqual(inputIds, [...inputIds].sort());
    assert.deepEqual(resumed.outcome.training_outcome.effective_plan.campaign.metadata.input_sample_ids, inputIds);
    function assertPublicSampleIndices(result) {
      const rows = result.predictions();
      assert.ok(rows.some(row => row.sample_ids.includes('s10')));
      for (const row of rows) {
        assert.deepEqual(row.sample_indices, row.sample_ids.map(id => inputIds.indexOf(id)));
      }
    }
    assertPublicSampleIndices(resumed);
    const destination = await resumed.export(path.join(directory, 'export'));
    const loaded = await loadTuning(destination, { methodsLibrary });
    assertPublicSampleIndices(loaded);
    const x = data.dataset.sources[0].array.values.slice(0, 3).map(row => row.map(Math.fround));
    const replay = await loaded.predict(x, { sampleIds: ['cold:0', 'cold:1', 'cold:2'] });
    assert.deepEqual(replay.outputs[0].predictions[0].sample_ids, ['cold:0', 'cold:1', 'cold:2']);
    assert.ok(replay.lineage.every(line => line.phase === 'PREDICT'));
    const final = resumed.outcome.training_outcome.outputs[0].predictions[0];
    const values = new Map(final.sample_ids.map((id, index) => [id, final.values[index]]));
    assert.deepEqual(replay.outputs[0].predictions[0].values, data.dataset.sample_ids.slice(0, 3).map(id => values.get(id)));
    const record = JSON.parse(fs.readFileSync(path.join(destination, 'tuning.json'))); record.config.seed++;
    fs.writeFileSync(path.join(destination, 'tuning.json'), JSON.stringify(record));
    await assert.rejects(loadTuning(destination), /differs/);
    await assert.rejects(first.resume(data, { trials: 4, seed: 92, archive: path.join(directory, 'bad.n4a') }));
    assert.equal(fs.existsSync(path.join(directory, 'bad.n4a')), false);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

test('cross-host native u64 study loads fail explicitly beyond the JS exact seed range', { skip: !qualified }, async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'nirs4all-node-u64-'));
  try {
    const archive = path.join(directory, 'model.n4a'), output = path.join(directory, 'outcome.json');
    execFileSync(process.env.NIRS4ALL_CORE_CLI, ['tuning-run', '--input', fixture, '--source-id', 'spectra',
      '--trials', '1', '--seed', '9007199254740993', '--sampler', 'random', '--metric', 'rmse',
      '--methods-library', methodsLibrary, '--archive', archive, '--run-id', 'run:node:u64:cross-host', '--output', output]);
    const outcome = JSON.parse(fs.readFileSync(output));
    const saved = { schema: 'nirs4all.tuning.v1', config: { source_id: 'spectra', trials: 1,
      seed: 0, sampler: 'random', metric: 'rmse' }, archive_sha256: outcome.archive_sha256,
      training_outcome_fingerprint: outcome.training_outcome.outcome_fingerprint };
    // Preserve the native u64 JSON token exactly, as a Python export does.
    fs.writeFileSync(path.join(directory, 'tuning.json'), JSON.stringify(saved).replace('"seed":0', '"seed":9007199254740993'));
    await assert.rejects(loadTuning(directory), /exact integer range/);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
