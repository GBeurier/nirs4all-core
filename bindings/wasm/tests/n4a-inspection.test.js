import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { inspectN4aContainer } from '../src/index.js';
import { writeN4aContainer } from '../src/n4a.js';

const fixture = new URL('../../rust/nirs4all/tests/parity/fixtures/n4a_host_opaque_v1/', import.meta.url);
async function archive() {
  const manifest = fs.readFileSync(new URL('manifest.json', fixture), 'utf8');
  const members = JSON.parse(fs.readFileSync(new URL('members.json', fixture), 'utf8'))
    .map(({ path, file }) => ({ path, bytes: new Uint8Array(fs.readFileSync(new URL(file, fixture))) }));
  return writeN4aContainer(manifest, members);
}
test('real Core container reports exact inventory and leaves unknown owners opaque', async () => {
  const { archiveBytes, receipt } = await archive();
  const session = await inspectN4aContainer(new Blob([archiveBytes]));
  try {
    const report = session.report();
    assert.equal(report.byteLength, String(archiveBytes.length));
    assert.equal(report.view.readability.container_sha256, receipt.container_sha256);
    assert.equal(report.view.executability, 'not_evaluated');
    for (const member of receipt.members) assert.equal(report.memberByteLengths[member.path], String(member.byte_length));
    assert.equal(JSON.parse(report.manifestText).schema, 'nirs4all.n4a.container.v1');
    for (const artifact of report.view.readability.manifest.artifacts) {
      const preview = await session.preview(artifact.id);
      assert.equal(preview.status, artifact.storage.kind === 'reference' ? 'unresolved' : 'opaque');
    }
  } finally { session.close(); }
  assert.throws(() => session.report(), /closed/);
  await assert.rejects(session.preview('anything'), /closed/);
});
test('exact owner/schema/version routing, lazy read sharing and port member scope', async () => {
  const { archiveBytes } = await archive();
  const manifest = JSON.parse(fs.readFileSync(new URL('manifest.json', fixture), 'utf8'));
  const artifact = manifest.artifacts.find(item => item.storage.kind === 'embedded');
  let calls = 0;
  const port = { owner: artifact.owner, contentType: artifact.content_type.id, version: artifact.content_type.version,
    async inspect(bytes, incoming, options) {
      calls++;
      assert.ok(bytes.length > 0);
      assert.equal(incoming.id, artifact.id);
      assert.throws(() => options.readMember('not-owned'), /only read members/);
      await new Promise(resolve => setTimeout(resolve, 5));
      return { kind: 'document', title: 'Owner report', status: 'not_evaluated', detail: 'Report only', fields: [], tables: [] };
    } };
  const session = await inspectN4aContainer(archiveBytes, { previewers: [port] });
  try {
    assert.equal(calls, 0);
    const [a, b] = await Promise.all([session.preview(artifact.id), session.preview(artifact.id)]);
    assert.equal(calls, 1); assert.deepEqual(a, b); assert.equal(a.artifactId, artifact.id);
    await Promise.all([session.preview(artifact.id, { maxRows: 1 }), session.preview(artifact.id, { maxRows: 2 })]);
    assert.equal(calls, 3, 'different display windows must not reuse an in-flight preview');
    await assert.rejects(session.preview(artifact.id, { maxRows: 0 }), /positive integer/);
    await assert.rejects(session.preview('missing'), /Unknown/);
  } finally { session.close(); }
  await assert.rejects(inspectN4aContainer(archiveBytes, { previewers: [port, port] }), /Duplicate/);
  const wrong = await inspectN4aContainer(archiveBytes, { previewers: [{ ...port, version: port.version + 1 }] });
  try { assert.equal((await wrong.preview(artifact.id)).status, 'opaque'); assert.equal(calls, 3); } finally { wrong.close(); }
});
test('tampered input is refused before any owner preview', async () => {
  const { archiveBytes } = await archive();
  archiveBytes[Math.floor(archiveBytes.length / 2)] ^= 1;
  await assert.rejects(inspectN4aContainer(archiveBytes), /refusal|unsupported/);
});
