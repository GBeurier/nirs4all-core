import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { loadArchiveV2Native } from '../src/archive-v2.js';
import { N4aContainer, openN4aContainer, predictN4aNative, retrainN4aNative, writeN4aContainer, writeN4aStream } from '../src/n4a.js';

// Dedicated gate for generic `.n4a` storage exchange over the shared Core
// fixture `n4a_host_opaque_v1` (raw members plus `members.json`, `manifest.json`,
// `update/*` and `expected.json`; there is no ready-made `.n4a`). The fixture is
// required: a missing or incomplete fixture fails every fixture test, never
// skips it. Override its location with NIRS4ALL_N4A_HOST_OPAQUE_FIXTURE.
//
// Needs the built Core WASM bridge (`npm run build:native`) but no Methods or
// DAG-ML runtime: nothing here executes, deserializes or trusts a host payload.

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const REFUSAL = /n4a (?:limit|memory|physical|manifest|integrity) refusal|n4a unsupported format/;
const MEMORY = /n4a (?:limit|memory) refusal/;
const decode = (bytes) => new TextDecoder().decode(bytes);
const same = (actual, expected) => assert.deepEqual([...actual], [...expected]);

const fixtureDir = process.env.NIRS4ALL_N4A_HOST_OPAQUE_FIXTURE
  ?? fileURLToPath(new URL('../../rust/nirs4all/tests/parity/fixtures/n4a_host_opaque_v1', import.meta.url));

function loadFixture() {
  assert.ok(fs.existsSync(fixtureDir), `Core fixture n4a_host_opaque_v1 is required but missing: ${fixtureDir}`);
  const text = (relative) => fs.readFileSync(`${fixtureDir}/${relative}`, 'utf8');
  const listing = (prefix) => JSON.parse(text(`${prefix}members.json`)).map(({ path, file }) => ({
    path, bytes: new Uint8Array(fs.readFileSync(`${fixtureDir}/${prefix}${file}`)),
  }));
  const expected = JSON.parse(text('expected.json'));
  const fixture = {
    expected,
    manifestText: text('manifest.json'),
    manifest: JSON.parse(text('manifest.json')),
    members: listing(''),
    update: { manifestText: text('update/manifest.json'), manifest: JSON.parse(text('update/manifest.json')), members: listing('update/') },
  };
  // The files must agree with the fixture's own recorded per-member identities.
  assert.equal(expected.members.length, fixture.members.length);
  for (const { path, byte_length: length, sha256: digest } of expected.members) {
    const member = fixture.members.find((item) => item.path === path);
    assert.ok(member, `members.json lists ${path}`);
    assert.equal(member.bytes.length, length, path);
    assert.equal(sha256(member.bytes), digest, path);
  }
  return fixture;
}

const memberIdentities = (receipt) => Object.fromEntries(receipt.members.map((m) => [m.path, [m.byte_length, m.sha256]]));
const expectedIdentities = (list) => Object.fromEntries(list.map((m) => [m.path, [m.byte_length, m.sha256]]));
const expectedMember = (fixture, path) => fixture.expected.members.find((item) => item.path === path);
const bytesOf = (fixture, path) => fixture.members.find((item) => item.path === path).bytes;

async function refuses(promise, pattern = REFUSAL) {
  await assert.rejects(promise, (error) => {
    assert.ok(error instanceof Error, 'a refusal is an Error, not a bare string');
    assert.match(error.message, pattern);
    return true;
  });
}

// Every opened container is closed even when an assertion fails.
async function withContainer(archiveBytes, action) {
  const container = await openN4aContainer(archiveBytes);
  try { return await action(container); } finally { container.close(); }
}

const writeBase = (fixture) => writeN4aContainer(fixture.manifestText, fixture.members);

test('storage actions accept exact bytes and valid arguments only', async () => {
  const manifest = {};
  const members = [{ path: 'host/x.bin', bytes: new Uint8Array([1]) }];
  await refuses(openN4aContainer('archive'), /exact bytes/);
  await refuses(openN4aContainer({}), /exact bytes/);
  await refuses(writeN4aContainer(manifest, members[0]), /members must be an array/);
  await refuses(writeN4aContainer(manifest, [{ path: 'host/x.bin', bytes: 'text' }]), /exact bytes/);
  await refuses(writeN4aContainer(manifest, [{ path: '', bytes: members[0].bytes }]), /path/);
  await refuses(writeN4aContainer(42, members), /manifest/);
  await refuses(writeN4aContainer(manifest, members, { base: {} }), /N4aContainer/);
  assert.throws(() => new N4aContainer({}, {}), /openN4aContainer/);
});

test('the public entry point exports the storage API', async () => {
  const index = await import('../src/index.js');
  assert.equal(index.openN4aContainer, openN4aContainer);
  assert.equal(index.writeN4aContainer, writeN4aContainer);
  assert.equal(index.N4aContainer, N4aContainer);
});

test('constructs the fixture container with Core-derived identities', async (t) => {
  const fixture = loadFixture();
  const { archiveBytes, receipt } = await writeBase(fixture);
  assert.ok(archiveBytes instanceof Uint8Array);
  assert.equal(receipt.byte_length, archiveBytes.length);
  assert.equal(receipt.container_sha256, sha256(archiveBytes));
  assert.deepEqual(memberIdentities(receipt), expectedIdentities(fixture.expected.members));

  // The dedicated exchange gate requires the owner-generated golden.
  t.diagnostic(`measured WASM container: ${receipt.byte_length} bytes, sha256 ${receipt.container_sha256}, manifest sha256 ${receipt.manifest_sha256}`);
  const golden = fixture.expected.container;
  assert.match(golden.container_sha256 ?? '', /^[0-9a-f]{64}$/, 'owner container golden is required');
  assert.ok(Number.isInteger(golden.byte_length) && golden.byte_length > 0, 'owner length golden is required');
  assert.match(golden.manifest_sha256 ?? '', /^[0-9a-f]{64}$/, 'owner manifest golden is required');
  assert.equal(receipt.container_sha256, golden.container_sha256);
  assert.equal(receipt.byte_length, golden.byte_length);
  assert.equal(receipt.manifest_sha256, golden.manifest_sha256);

  // Manifest as text, exact bytes or object, and members in any order, give one container.
  const reversed = await writeN4aContainer(fixture.manifestText, [...fixture.members].reverse());
  same(reversed.archiveBytes, archiveBytes);
  const fromBytes = await writeN4aContainer(new TextEncoder().encode(fixture.manifestText), fixture.members);
  assert.equal(fromBytes.receipt.container_sha256, receipt.container_sha256);
  const fromObject = await writeN4aContainer(fixture.manifest, fixture.members);
  assert.equal(fromObject.receipt.container_sha256, receipt.container_sha256);
});

test('archives and reads every fixture member, artifact and manifest byte-exactly', async () => {
  const fixture = loadFixture();
  const { archiveBytes, receipt } = await writeBase(fixture);
  await withContainer(archiveBytes, (container) => {
    assert.equal(container.closed, false);
    const view = container.view();
    assert.equal(view.readability.storage, 'verified');
    assert.equal(view.readability.container_sha256, receipt.container_sha256);
    assert.equal(view.readability.manifest_sha256, receipt.manifest_sha256);
    assert.equal(view.readability.byte_length, archiveBytes.length);
    assert.equal(view.executability, 'not_evaluated');
    // Host-owned payloads stay opaque; nothing is validated, resolved as native or trusted.
    assert.equal(view.readability.training_view.status, 'not_available');
    for (const artifact of view.readability.semantics.artifacts ?? []) assert.notEqual(artifact.content, 'validated', artifact.artifact_id);

    const manifestBytes = container.manifestBytes();
    assert.equal(sha256(manifestBytes), receipt.manifest_sha256);
    const stored = JSON.parse(decode(manifestBytes));
    assert.equal(stored.container_id, fixture.manifest.container_id);
    assert.deepEqual(expectedIdentities(stored.members), expectedIdentities(fixture.expected.members));

    for (const { path, bytes } of fixture.members) {
      const read = container.member(path);
      same(read, bytes);
      assert.equal(sha256(read), expectedMember(fixture, path).sha256, path);
    }
    assert.equal(container.member('host/absent.bin'), undefined);
    assert.equal(container.member('manifest.json'), undefined, 'the manifest is not an inventoried member');

    for (const artifact of fixture.manifest.artifacts) {
      const members = container.artifactMembers(artifact.id);
      if (artifact.storage.kind === 'reference') {
        assert.deepEqual(members, [], `${artifact.id} is a reference artifact and owns no member`);
        continue;
      }
      assert.deepEqual(members.map((m) => m.path), artifact.storage.member_paths, artifact.id);
      for (const { path, bytes } of members) same(bytes, bytesOf(fixture, path));
    }
    assert.equal(container.artifactMembers('artifact:unknown'), undefined);

    same(container.archiveBytes(), archiveBytes);
  });
});

test('returned bytes are independent copies of an immutable container', async () => {
  const fixture = loadFixture();
  const { archiveBytes } = await writeBase(fixture);
  const input = Uint8Array.from(archiveBytes);
  await withContainer(input, (container) => {
    input.fill(0); // the caller's buffer is not shared with the handle
    container.member('host/model.bin').fill(0);
    container.archiveBytes().fill(0);
    container.manifestBytes().fill(0);
    same(container.member('host/model.bin'), bytesOf(fixture, 'host/model.bin'));
    same(container.archiveBytes(), archiveBytes);
  });
});

test('the fixture update replaces, retains and deletes against an untouched base', async (t) => {
  const fixture = loadFixture();
  const { update, expected } = fixture;
  const base = await writeBase(fixture);
  const replacement = update.members.find((m) => m.path === 'host/model.bin');
  await withContainer(base.archiveBytes, async (container) => {
    // Nothing replaced: the base's own manifest reproduces the base exactly.
    const unchanged = await writeN4aContainer(container.manifestBytes(), [], { base: container });
    same(unchanged.archiveBytes, base.archiveBytes);

    const updated = await writeN4aContainer(update.manifestText, update.members, { base: container });
    assert.notEqual(updated.receipt.container_sha256, base.receipt.container_sha256);
    t.diagnostic(`measured WASM update container: ${updated.receipt.byte_length} bytes, sha256 ${updated.receipt.container_sha256}`);
    const golden = expected.update.container;
    if (golden.container_sha256 !== null) assert.equal(updated.receipt.container_sha256, golden.container_sha256);
    if (golden.byte_length !== null) assert.equal(updated.receipt.byte_length, golden.byte_length);
    if (golden.manifest_sha256 !== null) assert.equal(updated.receipt.manifest_sha256, golden.manifest_sha256);

    // The base handle and its bytes are unchanged and still usable.
    assert.equal(container.closed, false);
    same(container.archiveBytes(), base.archiveBytes);
    for (const { path, bytes } of fixture.members) same(container.member(path), bytes);

    const [replaced] = expected.update.replaced;
    assert.equal(sha256(replacement.bytes), replaced.sha256);
    assert.equal(replacement.bytes.length, replaced.byte_length);
    await withContainer(updated.archiveBytes, (next) => {
      assert.equal(JSON.parse(decode(next.manifestBytes())).container_id, update.manifest.container_id);
      same(next.member(replaced.path), replacement.bytes);
      assert.equal(sha256(next.member(replaced.path)), replaced.sha256);
      for (const path of expected.update.retained_from_base) {
        same(next.member(path), bytesOf(fixture, path));
        assert.equal(sha256(next.member(path)), expectedMember(fixture, path).sha256, path);
      }
      for (const path of expected.update.deleted) assert.equal(next.member(path), undefined, path);
      assert.deepEqual(
        Object.keys(memberIdentities(updated.receipt)).sort(),
        [replaced.path, ...expected.update.retained_from_base].sort(),
      );
    });
  });
});

test('write and update refusals come from Core', async () => {
  const fixture = loadFixture();
  const { members, manifest, update, expected } = fixture;
  const { unknown_member: unknownMember, undeclared_replacement: undeclared } = expected.refusals;

  // Fresh write: declared bytes absent, undeclared input, duplicate input, bad path.
  await refuses(writeN4aContainer(fixture.manifestText, members.slice(1)), /manifest refusal/);
  await refuses(writeN4aContainer(fixture.manifestText, [...members, { path: undeclared, bytes: members[0].bytes }]), /manifest refusal/);
  await refuses(writeN4aContainer(fixture.manifestText, [...members, members[0]]), /manifest refusal/);
  await refuses(writeN4aContainer(fixture.manifestText, [{ path: '../escape.bin', bytes: members[0].bytes }, ...members.slice(1)]), REFUSAL);

  // Manifest: unsupported versions, unknown field, invalid JSON.
  await refuses(writeN4aContainer({ ...manifest, schema_version: 2 }, members), /unsupported format/);
  await refuses(writeN4aContainer({ ...manifest, minimum_reader_version: 2 }, members), /unsupported format/);
  await refuses(writeN4aContainer({ ...manifest, unexpected: true }, members), REFUSAL);
  await refuses(writeN4aContainer('{"schema":', members), REFUSAL);

  const { archiveBytes } = await writeBase(fixture);
  await withContainer(archiveBytes, async (base) => {
    // Update: a declared member neither the base nor the replacements hold.
    const missing = structuredClone(update.manifest);
    missing.members.push({ path: unknownMember, byte_length: 0, sha256: '0'.repeat(64) });
    missing.artifacts.find((a) => a.id === 'model').storage.member_paths.push(unknownMember);
    await refuses(writeN4aContainer(missing, update.members, { base }), /manifest refusal/);
    // Update: an undeclared replacement, and a duplicate replacement.
    await refuses(writeN4aContainer(update.manifestText, [...update.members, { path: undeclared, bytes: members[0].bytes }], { base }), /manifest refusal/);
    await refuses(writeN4aContainer(update.manifestText, [...update.members, ...update.members], { base }), /manifest refusal/);
    // Refusals leave the base open and unchanged.
    assert.equal(base.closed, false);
    same(base.archiveBytes(), archiveBytes);
  });
});

test('open refuses tampered, truncated and foreign bytes', async () => {
  const fixture = loadFixture();
  const { archiveBytes } = await writeBase(fixture);
  const model = bytesOf(fixture, 'host/model.bin');
  const at = Buffer.from(archiveBytes).indexOf(Buffer.from(model));
  assert.ok(at > 0, 'the stored profile keeps payload bytes verbatim');

  const payload = Uint8Array.from(archiveBytes);
  payload[at + model.length - 2] ^= 0x01;
  await refuses(openN4aContainer(payload));

  const manifestAt = Buffer.from(archiveBytes).indexOf(Buffer.from(fixture.manifest.container_id));
  assert.ok(manifestAt > 0);
  const manifest = Uint8Array.from(archiveBytes);
  manifest[manifestAt + 10] ^= 0x01;
  await refuses(openN4aContainer(manifest));

  await refuses(openN4aContainer(archiveBytes.slice(0, archiveBytes.length - 1)));
  await refuses(openN4aContainer(archiveBytes.slice(0, 64)));
  await refuses(openN4aContainer(new Uint8Array(0)));
  await refuses(openN4aContainer(Uint8Array.from(Buffer.from('not a zip archive'))));
  await refuses(openN4aContainer(model)); // an opaque payload is not a container
});

test('close frees the handle deterministically and refuses further use', async () => {
  const fixture = loadFixture();
  const { archiveBytes } = await writeBase(fixture);
  const container = await openN4aContainer(archiveBytes);
  container.close();
  assert.equal(container.closed, true);
  container.close(); // idempotent
  for (const call of [
    () => container.view(), () => container.manifestBytes(), () => container.member('host/model.bin'),
    () => container.artifactMembers('model'), () => container.archiveBytes(),
  ]) assert.throws(call, /closed/);
  await refuses(writeN4aContainer(fixture.manifestText, [], { base: container }), /closed/);
});

test('readable is not executable: the native predict plan refuses the host entrypoint', async () => {
  const fixture = loadFixture();
  const { archiveBytes } = await writeBase(fixture);
  const native = await loadArchiveV2Native();
  // Constructed directly: this loads no Methods or DAG-ML runtime.
  let refused;
  try {
    const plan = new native.N4aPredictPlan(archiveBytes, fixture.expected.refusals.entrypoint_id, undefined, 'spectra', 'predict', true, 'request:storage-test');
    plan.free();
  } catch (error) {
    refused = String(error?.message ?? error);
  }
  assert.ok(refused, 'predict must not accept a container outside the native model profile');
  await withContainer(archiveBytes, (container) => assert.equal(container.view().executability, 'not_evaluated'));
});

// ---------------------------------------------------------------------------
// Public predict/retrain refuse an opaque container before any runtime loads.
// ---------------------------------------------------------------------------

// Counts every touch of the Methods/DAG-ML side. A refusal must leave it empty.
function stubRuntime() {
  const calls = [];
  class Controller { constructor() { calls.push('controller.new'); } }
  return {
    calls,
    options: {
      methods: { loadModule: async () => { calls.push('methods.loadModule'); } },
      dagMl: {
        execute_training_json() { calls.push('dag.execute'); },
        replay_training_package_json() { calls.push('dag.replay'); },
      },
      estimatorAdapter: { N4mWasmEstimatorControllers: Controller },
    },
  };
}
const RUNTIME_TEXT = /not installed|runtime is unavailable|does not expose the canonical/;

test('public predictN4aNative refuses the opaque host container before loading Methods', async () => {
  const fixture = loadFixture();
  const { archiveBytes } = await writeBase(fixture);
  const request = { entrypoint: fixture.expected.refusals.entrypoint_id, sourceId: 'spectra' };
  const stub = stubRuntime();
  await assert.rejects(predictN4aNative(archiveBytes, { ...request, ...stub.options }), (error) => {
    assert.ok(error instanceof Error);
    assert.doesNotMatch(error.message, RUNTIME_TEXT);
    return true;
  });
  assert.deepEqual(stub.calls, [], 'no runtime object may be touched before Core refuses');
  // Default runtime selection: the refusal is Core's, whether or not Methods is installed.
  await assert.rejects(predictN4aNative(archiveBytes, request), (error) => {
    assert.doesNotMatch(String(error?.message ?? error), RUNTIME_TEXT);
    return true;
  });
});

test('public retrainN4aNative refuses the opaque host container before loading Methods', async () => {
  const fixture = loadFixture();
  const { archiveBytes } = await writeBase(fixture);
  const record = new Uint8Array([1]);
  const stub = stubRuntime();
  await assert.rejects(retrainN4aNative(archiveBytes, record, stub.options), (error) => {
    assert.ok(error instanceof Error);
    assert.doesNotMatch(error.message, RUNTIME_TEXT);
    return true;
  });
  assert.deepEqual(stub.calls, [], 'no runtime object may be touched before Core refuses');
  await assert.rejects(retrainN4aNative(archiveBytes, record), (error) => {
    assert.doesNotMatch(String(error?.message ?? error), RUNTIME_TEXT);
    return true;
  });
});

// The fixture holds no native model, so a plan or facts object can never be built from it.
// Cleanup and ordering are therefore checked on the real n4a.js source loaded over a fake
// Core bridge (its two imports are replaced by data: modules; nothing else is changed).
const dataUrl = (code) => `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`;
async function loadWithFakeBridge() {
  const original = fs.readFileSync(new URL('../src/n4a.js', import.meta.url), 'utf8');
  const bridge = "from './archive-v2.js'", index = "from './index.js'";
  assert.ok(original.includes(bridge) && original.includes(index), 'n4a.js imports changed; update this harness');
  const source = original
    .replace(bridge, `from '${dataUrl('export const loadArchiveV2Native = async () => globalThis.__n4aFake.native;')}'`)
    .replace(index, `from '${dataUrl('export const loadDagMlWasm = async () => globalThis.__n4aFake.dag; export const loadMethodsWasm = async () => globalThis.__n4aFake.methods;')}'`);
  return import(dataUrl(source));
}

function fakeCore(log, { refuse = false } = {}) {
  const guard = (name) => { log.push(`${name}.new`); if (refuse) throw 'n4a refusal: fake owner refusal'; };
  class Plan { constructor() { guard('plan'); } free() { log.push('plan.free'); } }
  class TrainPlan { free() { log.push('trainplan.free'); } }
  class Facts {
    constructor() { guard('facts'); }
    plan() { log.push('facts.plan'); return new TrainPlan(); }
    free() { log.push('facts.free'); }
  }
  return { N4aPredictPlan: Plan, N4aTrainPlan: TrainPlan, N4aRetrainFacts: Facts, inspect_n4a_bytes() {}, sha256_bytes() {} };
}

async function withFakeBridge(setup, action) {
  const log = [];
  const previous = globalThis.__n4aFake;
  const fake = { native: fakeCore(log, setup.core), dag: setup.dag ?? {}, methods: {
    loadModule: async () => { log.push('methods.loadModule'); if (setup.loadFails) throw new Error('methods load failed'); },
  } };
  globalThis.__n4aFake = fake;
  try {
    const n4a = await loadWithFakeBridge();
    return await action(n4a, log, { estimatorAdapter: { N4mWasmEstimatorControllers: class {} } });
  } finally {
    if (previous === undefined) delete globalThis.__n4aFake; else globalThis.__n4aFake = previous;
  }
}

const PREDICT = { entrypoint: 'e', sourceId: 's' };
const BYTES = new Uint8Array([1]);

test('the predict plan is built before the runtime loads and is freed when the runtime rejects', async () => {
  // dagMl lacks the DAG entry points, so runtime() rejects after Methods loaded.
  await withFakeBridge({}, async (n4a, log, runtime) => {
    await assert.rejects(n4a.predictN4aNative(BYTES, { ...PREDICT, ...runtime }), /does not expose the canonical/);
    assert.deepEqual(log, ['plan.new', 'methods.loadModule', 'plan.free']);
  });
  await withFakeBridge({ loadFails: true }, async (n4a, log, runtime) => {
    await assert.rejects(n4a.predictN4aNative(BYTES, { ...PREDICT, ...runtime }), /methods load failed/);
    assert.deepEqual(log, ['plan.new', 'methods.loadModule', 'plan.free']);
  });
  // A Core refusal ends the call before any runtime object is touched, and surfaces as an Error.
  await withFakeBridge({ core: { refuse: true } }, async (n4a, log, runtime) => {
    await assert.rejects(n4a.predictN4aNative(BYTES, { ...PREDICT, ...runtime }), (error) => {
      assert.ok(error instanceof Error);
      assert.match(error.message, /fake owner refusal/);
      return true;
    });
    assert.deepEqual(log, ['plan.new']);
  });
});

test('the retrain facts are read before the runtime loads and are freed when the runtime rejects', async () => {
  await withFakeBridge({}, async (n4a, log, runtime) => {
    await assert.rejects(n4a.retrainN4aNative(BYTES, BYTES, runtime), /does not expose the canonical/);
    assert.deepEqual(log, ['facts.new', 'methods.loadModule', 'facts.free'], 'the facts are never planned without a runtime');
  });
  await withFakeBridge({ loadFails: true }, async (n4a, log, runtime) => {
    await assert.rejects(n4a.retrainN4aNative(BYTES, BYTES, runtime), /methods load failed/);
    assert.deepEqual(log, ['facts.new', 'methods.loadModule', 'facts.free']);
  });
  await withFakeBridge({ core: { refuse: true } }, async (n4a, log, runtime) => {
    await assert.rejects(n4a.retrainN4aNative(BYTES, BYTES, runtime), /fake owner refusal/);
    assert.deepEqual(log, ['facts.new']);
  });
});

// ---------------------------------------------------------------------------
// Streaming storage: external random-access sources, verified chunks, streamed
// writes and the optional budget. The sources below are real fixture bytes behind
// a `{ size, read(offset, length) }` reader that records every request.
// ---------------------------------------------------------------------------

const concat = (chunks) => {
  const out = new Uint8Array(chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0));
  let at = 0;
  for (const chunk of chunks) { out.set(chunk, at); at += chunk.byteLength; }
  return out;
};
async function drain(stream) {
  const chunks = [];
  for await (const chunk of stream) chunks.push(chunk);
  return chunks;
}
// Records every request; `map` may corrupt the bytes returned for a range.
function randomSource(bytes, map) {
  const reads = [];
  return {
    reads,
    size: BigInt(bytes.length),
    async read(offset, length) {
      assert.equal(typeof offset, 'bigint');
      assert.equal(typeof length, 'bigint');
      reads.push([offset, length]);
      const out = bytes.slice(Number(offset), Number(offset + length));
      return map ? map(out, offset, length) : out;
    },
  };
}
const sinkOf = () => {
  const sink = { chunks: [], closed: false, aborted: undefined,
    write(chunk) { sink.chunks.push(chunk); }, close() { sink.closed = true; }, abort(error) { sink.aborted = error; } };
  return sink;
};
const chunkFor = async (budget) => BigInt((await loadArchiveV2Native()).n4a_chunk_bytes(budget));
const BUDGET = 65536n; // an explicit budget; Core derives the streaming chunk from it

// A larger member (300 kB) so a small budget forces many chunks.
async function largeArchive(fixture) {
  // The high-bit term makes successive chunks (4 KiB and up, power-of-two sizes) differ, so a chunk delivered from the wrong buffer position cannot compare equal.
  const big = Uint8Array.from({ length: 300_000 }, (_, i) => (i * 31 + 7 + (i >>> 12)) & 255);
  const base = await writeBase(fixture);
  const written = await withContainer(base.archiveBytes, (container) =>
    writeN4aContainer(fixture.update.manifestText, [{ path: 'host/model.bin', bytes: big }], { base: container }));
  return { big, archiveBytes: written.archiveBytes, receipt: written.receipt };
}

test('budget and source arguments are validated before any bridge work', async () => {
  const bytes = new Uint8Array([1]);
  for (const bad of [1.5, -1, '1.0', '-1', 'abc', 2n ** 64n, {}, true]) {
    await assert.rejects(openN4aContainer(bytes, { memoryBudgetBytes: bad }), /memoryBudgetBytes/, String(bad));
  }
  const reader = (size) => ({ size, read: async () => bytes });
  await refuses(openN4aContainer(reader(1.5)), /size/);
  await refuses(openN4aContainer(reader(-1n)), /size/);
  await refuses(openN4aContainer(reader(2n ** 64n)), /size/);
  await refuses(openN4aContainer({ read: async () => bytes }), /exact bytes/);
  await refuses(writeN4aStream('{}', [], null), /sink/);
  await refuses(writeN4aStream('{}', {}, { write() {} }), /members must be an array/);
  await refuses(writeN4aStream('{}', [{ path: 'a', source: 'text' }], { write() {} }), /exact bytes/);
  await refuses(writeN4aStream('{}', [], { write() {} }, { base: {} }), /N4aContainer/);
});

test('an external source is scanned by Core in ranged reads with exact BigInt offsets', async () => {
  const fixture = loadFixture();
  const { archiveBytes, receipt } = await writeBase(fixture);
  const source = randomSource(archiveBytes);
  const container = await openN4aContainer(source, { memoryBudgetBytes: '00065536' });
  try {
    assert.equal(container.kind, 'external');
    assert.equal(container.size, BigInt(archiveBytes.length));
    const [firstOffset, firstLength] = source.reads[0];
    assert.equal(firstOffset + firstLength, container.size, 'the first request is the archive tail');
    assert.ok(source.reads.some(([offset]) => offset === 0n), 'a contiguous pass starts at offset 0');
    const limit = await chunkFor(BUDGET);
    for (const [, length] of source.reads) assert.ok(length <= (limit > 98n ? limit : 98n), 'no request exceeds the budget chunk');
    const view = container.view();
    assert.equal(view.readability.storage, 'verified');
    assert.equal(view.readability.container_sha256, receipt.container_sha256);
    assert.equal(view.executability, 'not_evaluated');
    // No member bytes were read, so no semantic or training result is claimed.
    assert.match(JSON.stringify(view.readability.semantics), /not_evaluated/);
    assert.match(JSON.stringify(view.readability.training_view), /not_evaluated/);
    same(container.manifestBytes(), (await withContainer(archiveBytes, (c) => c.manifestBytes())));
    assert.deepEqual(container.artifactMemberPaths('model'), ['host/model.bin']);
    assert.deepEqual(container.artifactMemberPaths('dataset-ref'), []);
    assert.equal(container.artifactMemberPaths('unknown'), undefined);
    // The synchronous byte methods need bytes in WASM memory.
    assert.throws(() => container.member('host/model.bin'), /external source/);
    assert.throws(() => container.archiveBytes(), /external source/);
  } finally { container.close(); }
  // Blob/File sources are accepted directly.
  await withContainer(new Blob([archiveBytes]), (blob) => assert.equal(blob.view().readability.container_sha256, receipt.container_sha256));
});

test('streams verify actual bytes: members and archive match, tampered source refuses before the last chunk', async () => {
  const fixture = loadFixture();
  const { big, archiveBytes, receipt } = await largeArchive(fixture);
  const limit = await chunkFor(BUDGET);
  const source = randomSource(archiveBytes);
  const container = await openN4aContainer(source, { memoryBudgetBytes: BUDGET });
  try {
    const opened = source.reads.length;
    const stream = container.memberStream('host/model.bin');
    assert.equal(stream.byteLength, BigInt(big.length));
    const chunks = await drain(stream);
    same(concat(chunks), big);
    assert.ok(chunks.length > 10 && chunks.every((c) => BigInt(c.byteLength) <= limit), 'streamed in budget-sized chunks');
    same(await container.readMember('host/recipe.json'), bytesOf(fixture, 'host/recipe.json'));
    assert.equal(container.memberStream('host/absent.bin'), undefined);
    assert.equal(await container.readMember('host/absent.bin'), undefined);
    same(concat(await drain(container.archiveStream())), archiveBytes);
    assert.equal(sha256(concat(await drain(container.archiveStream()))), receipt.container_sha256);
    assert.ok(source.reads.length > opened, 'streaming re-reads the source');

    // A source that now returns different bytes is refused, and the last chunk is withheld.
    const at = Buffer.from(archiveBytes).indexOf(Buffer.from(big.subarray(1000, 1064)));
    assert.ok(at > 0);
    let tampered = false;
    const mutable = randomSource(archiveBytes, (out, offset) => {
      if (tampered && offset <= BigInt(at) && BigInt(at) < offset + BigInt(out.length)) out[Number(BigInt(at) - offset)] ^= 1;
      return out;
    });
    const second = await openN4aContainer(mutable, { memoryBudgetBytes: BUDGET });
    try {
      tampered = true;
      const delivered = [];
      await assert.rejects(async () => { for await (const chunk of second.memberStream('host/model.bin')) delivered.push(chunk); }, (error) => {
        assert.match(error.message, /n4a integrity refusal/);
        return true;
      });
      assert.equal(delivered.length, chunks.length - 1, 'every chunk but the held-back last one was delivered');
      assert.ok(concat(delivered).length < big.length);
    } finally { second.close(); }
  } finally { container.close(); }
});

test('a bytes-held container streams the same chunks from WASM memory', async () => {
  const fixture = loadFixture();
  const { big, archiveBytes } = await largeArchive(fixture);
  await withContainer(archiveBytes, async (container) => {
    assert.equal(container.kind, 'bytes');
    const chunks = await drain(container.memberStream('host/model.bin'));
    same(concat(chunks), big);
    same(concat(await drain(container.archiveStream())), archiveBytes);
    assert.equal(container.size, BigInt(archiveBytes.length));
    assert.equal(container.memberStream('host/absent.bin'), undefined);
  });
});

test('a reader that reuses one buffer still yields exact, independent chunks', async () => {
  const fixture = loadFixture();
  const { big, archiveBytes } = await largeArchive(fixture);
  // 1 MiB budget: Core's streaming chunk is 256 KiB, so the 300 kB member spans two chunks.
  const budget = 1048576n;
  const limit = await chunkFor(budget);
  assert.ok(BigInt(big.length) > limit, 'the member must span at least two chunks');
  const shared = new Uint8Array(Number(limit > 98n ? limit : 98n));
  const reusing = {
    size: BigInt(archiveBytes.length),
    async read(offset, length) {
      // Every call overwrites and returns a view of the same buffer.
      const view = shared.subarray(0, Number(length));
      view.set(archiveBytes.subarray(Number(offset), Number(offset + length)));
      return view;
    },
  };
  const container = await openN4aContainer(reusing, { memoryBudgetBytes: budget });
  try {
    same(await container.readMember('host/model.bin'), big);
    const chunks = await drain(container.memberStream('host/model.bin'));
    assert.ok(chunks.length >= 2);
    same(concat(chunks), big);
    assert.equal(new Set(chunks.map((chunk) => chunk.buffer)).size, chunks.length, 'chunks do not share a buffer');
    same(concat(await drain(container.archiveStream())), archiveBytes);
  } finally { container.close(); }
});

test('Core admits against the budget before copies and reads', async () => {
  const fixture = loadFixture();
  const { archiveBytes } = await largeArchive(fixture);
  // Bytes: admission happens before the wasm-bindgen copy.
  await refuses(openN4aContainer(archiveBytes, { memoryBudgetBytes: 10 }), MEMORY);
  // A budget below one streaming chunk refuses the external scan.
  await refuses(openN4aContainer(randomSource(archiveBytes), { memoryBudgetBytes: 100 }), MEMORY);
  // Materializing a member larger than the budget is refused before any read; streaming still works.
  const bounded = randomSource(archiveBytes);
  const container = await openN4aContainer(bounded, { memoryBudgetBytes: BUDGET });
  try {
    const before = bounded.reads.length;
    await refuses(container.readMember('host/model.bin'), MEMORY);
    assert.equal(bounded.reads.length, before, 'admission failed before reading the source');
    assert.equal((await drain(container.memberStream('host/model.bin'))).length > 10, true);
  } finally { container.close(); }

  // Aggregate admission: the in-memory writer retains every member, so the running total counts,
  // not each array alone. Each member (400 bytes) is far below the budget, the cumulative total is not.
  const manifestSize = new TextEncoder().encode(fixture.manifestText).length;
  const synthetic = fixture.members.map(({ path }, i) => ({ path, bytes: new Uint8Array(400).fill(i + 1) }));
  const native = await loadArchiveV2Native();
  const original = native.N4aWriter.prototype.add_member;
  let added = 0;
  native.N4aWriter.prototype.add_member = function counted(...args) { added += 1; return original.apply(this, args); };
  try {
    await refuses(writeN4aContainer(fixture.manifestText, synthetic, { memoryBudgetBytes: manifestSize + 1000 }), MEMORY);
    // manifest + 400 and manifest + 800 fit, manifest + 1200 does not: refused before the third copy.
    assert.equal(added, 2, 'the cumulative total was admitted before each member copy');
  } finally { native.N4aWriter.prototype.add_member = original; }
});

test('BigInt offsets stay exact beyond 2^53 and a failing source frees the scan', async () => {
  const size = 2n ** 60n;
  const requests = [];
  const source = { size, read: async (offset, length) => { requests.push([offset, length]); throw new Error('source stopped'); } };
  await assert.rejects(openN4aContainer(source), /source stopped/);
  assert.equal(requests.length, 1);
  assert.equal(requests[0][0] + requests[0][1], size, 'the tail request ends exactly at 2^60');
  assert.ok(requests[0][0] > BigInt(Number.MAX_SAFE_INTEGER), 'the offset is above the JavaScript safe-integer range');
});

test('writeN4aStream reproduces the in-memory writer byte for byte and leaves the base untouched', async () => {
  const fixture = loadFixture();
  const base = await writeBase(fixture);
  const sources = fixture.members.map(({ path, bytes }, i) => ({ path, source: i % 2 === 0 ? randomSource(bytes) : new Blob([bytes]) }));
  const sink = sinkOf();
  const { receipt } = await writeN4aStream(fixture.manifestText, sources, sink, { memoryBudgetBytes: BUDGET });
  assert.equal(sink.closed, true);
  assert.equal(sink.aborted, undefined);
  same(concat(sink.chunks), base.archiveBytes);
  assert.equal(receipt.container_sha256, base.receipt.container_sha256);
  assert.deepEqual(receipt.members, base.receipt.members);

  const expected = await withContainer(base.archiveBytes, (container) =>
    writeN4aContainer(fixture.update.manifestText, fixture.update.members, { base: container }));
  // Update from a bytes-held base and from an external base: retained members are copied verified.
  for (const open of [() => openN4aContainer(base.archiveBytes), () => openN4aContainer(randomSource(base.archiveBytes), { memoryBudgetBytes: BUDGET })]) {
    const container = await open();
    try {
      const out = sinkOf();
      const updated = await writeN4aStream(fixture.update.manifestText,
        fixture.update.members.map(({ path, bytes }) => ({ path, source: bytes })), out, { base: container, memoryBudgetBytes: BUDGET });
      same(concat(out.chunks), expected.archiveBytes);
      assert.equal(updated.receipt.container_sha256, expected.receipt.container_sha256);
      same(await container.readArchive(), base.archiveBytes);
    } finally { container.close(); }
  }
});

test('writeN4aStream refuses a source that changed between passes and aborts the sink', async () => {
  const fixture = loadFixture();
  const target = fixture.members.find((member) => member.path === 'host/model.bin');
  let passes = 0;
  const changing = { size: BigInt(target.bytes.length), async read(offset, length) {
    const out = target.bytes.slice(Number(offset), Number(offset + length));
    if (offset === 0n) passes += 1;
    if (passes > 1) out[0] ^= 1; // the second pass sees different bytes
    return out;
  } };
  const sink = sinkOf();
  const sources = fixture.members.map(({ path, bytes }) => ({ path, source: path === target.path ? changing : bytes }));
  await refuses(writeN4aStream(fixture.manifestText, sources, sink), /n4a integrity refusal/);
  assert.equal(passes, 2);
  assert.equal(sink.closed, false);
  assert.match(sink.aborted.message, /n4a integrity refusal/);
  // Admission and sink failures abort the sink too.
  const refusedSink = sinkOf();
  await refuses(writeN4aStream(fixture.manifestText, fixture.members.map(({ path, bytes }) => ({ path, source: bytes })), refusedSink,
    { memoryBudgetBytes: 100 }), MEMORY);
  assert.ok(refusedSink.aborted && !refusedSink.closed);
  const broken = { write() { throw new Error('disk full'); }, abort(error) { broken.error = error; } };
  await assert.rejects(writeN4aStream(fixture.manifestText, fixture.members.map(({ path, bytes }) => ({ path, source: bytes })), broken), /disk full/);
  assert.match(broken.error.message, /disk full/);
});

test('writeN4aStream with a small budget streams a large member in budget-sized reads', async () => {
  const fixture = loadFixture();
  const big = Uint8Array.from({ length: 300_000 }, (_, i) => (i * 7 + 3) & 255);
  const limit = await chunkFor(BUDGET);
  const source = randomSource(big);
  const sink = sinkOf();
  const sources = fixture.members.map(({ path, bytes }) => ({ path, source: path === 'host/model.bin' ? source : bytes }));
  const { receipt } = await writeN4aStream(fixture.manifestText, sources, sink, { memoryBudgetBytes: BUDGET });
  assert.ok(source.reads.length >= 2 * Math.floor(big.length / Number(limit)), 'two passes over the large source');
  assert.ok(source.reads.every(([, length]) => length <= limit));
  const archive = concat(sink.chunks);
  assert.equal(sha256(archive), receipt.container_sha256);
  await withContainer(archive, async (container) => same(container.member('host/model.bin'), big));
});

// Shared Core ZIP64 fixture (a small archive with at least 65535 members, generated by
// Core at test time): NIRS4ALL_N4A_ZIP64_FIXTURE names the file, NIRS4ALL_N4A_ZIP64_BUDGET
// the explicit budget (default 256 MiB). It is read through a positional reader, never
// loaded whole. NIRS4ALL_REQUIRE_CORE_N4A=1 turns a missing fixture into a failure.
test('the shared ZIP64 fixture opens and streams through a file reader with a small budget', async (t) => {
  const path = process.env.NIRS4ALL_N4A_ZIP64_FIXTURE;
  if (!path || !fs.existsSync(path)) {
    assert.notEqual(process.env.NIRS4ALL_REQUIRE_CORE_N4A, '1', 'NIRS4ALL_N4A_ZIP64_FIXTURE is required but missing');
    t.skip('NOT RUN: NIRS4ALL_N4A_ZIP64_FIXTURE is not set; no ZIP64 assertion ran');
    return;
  }
  const budget = BigInt(process.env.NIRS4ALL_N4A_ZIP64_BUDGET ?? 256 * 1024 * 1024);
  const handle = await fs.promises.open(path, 'r');
  let largest = 0n;
  const source = {
    size: BigInt((await handle.stat()).size),
    async read(offset, length) {
      if (length > largest) largest = length;
      const buffer = new Uint8Array(Number(length));
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, Number(offset));
      return buffer.subarray(0, bytesRead);
    },
  };
  try {
    const container = await openN4aContainer(source, { memoryBudgetBytes: budget });
    try {
      const view = container.view();
      const members = view.readability.manifest.members;
      assert.ok(members.length >= 65535, `expected a ZIP64 inventory, got ${members.length}`);
      assert.equal(view.readability.storage, 'verified');
      for (const member of [members[0], members[members.length - 1]]) {
        const hash = createHash('sha256');
        let length = 0n;
        for await (const chunk of container.memberStream(member.path)) { hash.update(chunk); length += BigInt(chunk.byteLength); }
        assert.equal(length, BigInt(member.byte_length), member.path);
        assert.equal(hash.digest('hex'), member.sha256, member.path);
      }
      const hash = createHash('sha256');
      for await (const chunk of container.archiveStream()) hash.update(chunk);
      assert.equal(hash.digest('hex'), view.readability.container_sha256);
      const chunk = await chunkFor(budget);
      assert.ok(largest <= (chunk > 98n ? chunk : 98n), 'no read exceeded the budget chunk');
    } finally { container.close(); }
  } finally { await handle.close(); }
});
