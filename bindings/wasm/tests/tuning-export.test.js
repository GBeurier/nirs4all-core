import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { NativeTuningResult } from '../src/tuning.js';

test('failed tuning exports preserve destination availability and existing content', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'nirs4all-export-test-'));
  try {
    const target = path.join(root, 'export');
    const model = new NativeTuningResult(path.join(root, 'missing.n4a'), {}, { seed: 0 });
    await assert.rejects(model.export(target), { code: 'ENOENT' });
    await assert.rejects(fs.stat(target), { code: 'ENOENT' });
    const source = path.join(root, 'model.n4a');
    await fs.writeFile(source, 'native archive');
    model.archivePath = source;
    model.outcome = { training_outcome: { outcome_fingerprint: 'fingerprint' }, archive_sha256: 'digest' };
    await model.export(target);
    assert.equal(await fs.readFile(path.join(target, 'model.n4a'), 'utf8'), 'native archive');
    await assert.rejects(model.export(target), { code: 'EEXIST' });
    assert.equal(await fs.readFile(path.join(target, 'model.n4a'), 'utf8'), 'native archive');
    assert.deepEqual((await fs.readdir(root)).sort(), ['export', 'model.n4a']);
  } finally { await fs.rm(root, { recursive: true, force: true }); }
});
