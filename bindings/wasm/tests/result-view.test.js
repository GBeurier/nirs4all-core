import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { openExperiment, trainingResultView } from '../src/result-view.js';

const fixture = process.env.NIRS4ALL_EXPERIMENT_FIXTURE;

async function inputs() {
  const index = new Uint8Array(await readFile(resolve(fixture, 'experiment.json')));
  const inventory = JSON.parse(new TextDecoder().decode(index));
  const names = ['result_view.json', ...Object.keys(inventory.results).map((name) => `results/${name}`)];
  if (inventory.model_archive) names.push('model.n4a');
  const members = Object.fromEntries(await Promise.all(names.map(async (name) => [name, new Uint8Array(await readFile(resolve(fixture, name)))])));
  return { index, members };
}

test('native experiment projection preserves candidate scopes and query isolation', { skip: !fixture }, async () => {
  const { index, members } = await inputs();
  assert.ok(JSON.parse(new TextDecoder().decode(index)).model_archive, 'Real model fixture required for prediction closure');
  const experiment = await openExperiment(index, members);
  const projection = JSON.parse(new TextDecoder().decode(members['result_view.json']));
  assert.equal(experiment.validationLevel, 'hashed_native_projection');
  assert.equal(experiment.modelPredictionClosure, true);
  assert.deepEqual(experiment.unattestedDisplayFields, ['dataset', 'task_type']);
  const first = experiment.predictions()[0];
  assert.deepEqual(first, projection.predictions[0]);
  first.sample_ids[0] = 'changed';
  first.y_pred[0] = 999;
  assert.deepEqual(experiment.predictions()[0], projection.predictions[0]);
  const comparisons = experiment.compare();
  assert.deepEqual(comparisons[0].metrics, projection.score_set.reports[0].metrics);
  comparisons[0].metrics.changed = 999;
  assert.equal(experiment.compare()[0].metrics.changed, undefined);
  assert.throws(() => experiment.predictions({ variantId: 'absent' }), /Unknown variant/);
  assert.throws(() => experiment.compare({ variantId: 'absent' }), /Unknown variant/);
});

async function rehash(index, members) {
  const inventory = JSON.parse(new TextDecoder().decode(index));
  const { createHash } = await import('node:crypto');
  const hash = value => createHash('sha256').update(value).digest('hex');
  for (const name of Object.keys(inventory.results)) inventory.results[name] = hash(members[`results/${name}`]);
  inventory.result_view_sha256 = hash(members['result_view.json']);
  return new TextEncoder().encode(JSON.stringify(inventory));
}

test('rehashed forged prediction projection cannot claim native model closure', { skip: !fixture }, async () => {
  for (const field of ['y_pred', 'y_true', 'sample_ids', 'sample_indices', 'target_names', 'refit_context']) {
    const { index, members } = await inputs();
    const view = JSON.parse(new TextDecoder().decode(members['result_view.json']));
    const row = view.predictions.find(row => row.y_true.length);
    if (field === 'y_pred' || field === 'y_true') row[field][0] += 1000;
    else if (field === 'sample_ids' || field === 'sample_indices') [row[field][0], row[field][1]] = [row[field][1], row[field][0]];
    else if (field === 'target_names') row[field][0] = 'forged-target';
    else row[field] = 'forged-context';
    members['result_view.json'] = new TextEncoder().encode(JSON.stringify(view));
    await assert.rejects(openExperiment(await rehash(index, members), members), /predictions differ/, field);
  }
});

test('winner must be native and model-free validation never claims model closure', { skip: !fixture }, async () => {
  const { index, members } = await inputs();
  const plain = JSON.parse(new TextDecoder().decode(index));
  plain.model_archive = null;
  const result = await openExperiment(new TextEncoder().encode(JSON.stringify(plain)), members);
  assert.equal(result.modelPredictionClosure, false);
  const manifest = JSON.parse(new TextDecoder().decode(members['results/manifest.json']));
  const view = JSON.parse(new TextDecoder().decode(members['result_view.json']));
  delete manifest.selected_variant_id;
  view.manifest = manifest;
  members['results/manifest.json'] = new TextEncoder().encode(JSON.stringify(manifest));
  members['result_view.json'] = new TextEncoder().encode(JSON.stringify(view));
  await assert.rejects(openExperiment(await rehash(index, members), members), /manifest selected_variant_id/);
});

test('exact member tampering is rejected before querying', { skip: !fixture }, async () => {
  const { index, members } = await inputs();
  members['results/predictions.parquet'][0] ^= 1;
  await assert.rejects(openExperiment(index, members), /hash mismatch/);
});

test('training projection uses signed original order and refuses caller permutations', { skip: !fixture }, async () => {
  const { execFileSync } = await import('node:child_process');
  const { mkdtemp, rm } = await import('node:fs/promises');
  const { tmpdir } = await import('node:os');
  assert.ok(process.env.NIRS4ALL_CORE_CLI && process.env.NIRS4ALL_MODEL_FIXTURE);
  const directory = await mkdtemp(resolve(tmpdir(), 'nirs4all-signed-result-order-'));
  try {
    const output = resolve(directory, 'loaded.json');
    execFileSync(process.env.NIRS4ALL_CORE_CLI, ['workflow-load', '--archive', process.env.NIRS4ALL_MODEL_FIXTURE, '--output', output]);
    const outcome = JSON.parse(await readFile(output, 'utf8')).training_outcome;
    const signed = outcome.effective_plan.campaign.metadata.input_sample_ids;
    assert.notDeepEqual(signed, [...signed].sort(), 'Fixture must exercise s1/s10 public-order hazard');
    const view = trainingResultView(outcome);
    for (const row of view.predictions()) {
      assert.deepEqual(row.sample_indices, row.sample_ids.map(id => signed.indexOf(id)));
    }
    assert.deepEqual(trainingResultView(outcome, signed).predictions(), view.predictions());
    assert.throws(() => trainingResultView(outcome, [...signed].reverse()), /signed input sample order/);
    const missing = structuredClone(outcome);
    delete missing.effective_plan.campaign.metadata.input_sample_ids;
    assert.throws(() => trainingResultView(missing), /Signed input sample identities/);
    assert.throws(() => trainingResultView(missing, signed), /Signed input sample identities/);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
