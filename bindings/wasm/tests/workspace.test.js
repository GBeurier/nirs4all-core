import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { openWorkspace } from '../src/workspace.js';
const fixture = process.env.NIRS4ALL_WORKSPACE_FIXTURE;

test('SDK SQLite/Parquet workspace snapshot queries native results and exports fit-free lifecycle', { skip: !fixture }, async () => {
  const indexBytes = fs.readFileSync(path.join(fixture, 'workspace.json'));
  const index = JSON.parse(indexBytes);
  const members = Object.fromEntries(Object.keys(index.files).map(name => [name, fs.readFileSync(path.join(fixture, name))]));
  const workspace = await openWorkspace(indexBytes, members);
  const run = workspace.runs()[0].runId;
  const view = JSON.parse(fs.readFileSync(path.join(fixture, index.runs[run].path, 'result_view.json')));
  assert.deepEqual(workspace.predictions(run), view.predictions);
  assert.equal(workspace.compare(run).length, view.score_set.reports.length);
  const snapshot = workspace.export();
  members['store.sqlite'][0] ^= 1;
  await assert.rejects(openWorkspace(indexBytes, members), /integrity/);
  const loaded = await openWorkspace(snapshot.indexBytes, snapshot.members);
  assert.deepEqual(loaded.predictions(run), workspace.predictions(run));
  workspace.close();
  assert.equal(workspace.closed, true);
  assert.throws(() => workspace.predictions(run), /closed/);
  assert.throws(() => workspace.export(), /closed/);
  const bad = JSON.parse(indexBytes); bad.files['../escape'] = bad.files['store.sqlite'];
  await assert.rejects(openWorkspace(new TextEncoder().encode(JSON.stringify(bad)), snapshot.members), /member path/);
});
