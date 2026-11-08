// Node driver for the native <-> WASM `.n4a` exchange gate.
//
// Driven only by bindings/rust/nirs4all/tests/n4a_wasm_exchange.rs, which owns
// the fixtures, the native side and every assertion. Usage:
//   node n4a-exchange-driver.mjs <job.json>
// The job names a command and file paths; results are written to `job.out`.
// A missing qualification input is a hard failure, never a skip.
import fs from 'node:fs';
import { pathToFileURL } from 'node:url';

import { loadArchiveV2Native } from '../src/archive-v2.js';
import { inspectN4aBytes, predictN4aNative, produceN4aNative, retrainN4aNative } from '../src/n4a.js';

const required = ['NIRS4ALL_METHODS_JS', 'NIRS4ALL_DAG_JS', 'NIRS4ALL_ESTIMATOR_ADAPTER_JS'];
const missing = required.filter((name) => !process.env[name]);
if (missing.length > 0) throw new Error(`Qualification environment incomplete: ${missing.join(', ')}`);

const [methods, dagMl, adapter] = await Promise.all(required.map((name) => import(pathToFileURL(process.env[name]))));
await dagMl.default({ module_or_path: fs.readFileSync(new URL('dag_ml_wasm_bg.wasm', pathToFileURL(process.env.NIRS4ALL_DAG_JS))) });
await methods.loadModule();

// Observes the real controller registration without replacing any behaviour.
const stats = { callbacks: 0, targetRequests: 0 };
let tamperManifests = false;
class Observed extends adapter.N4mWasmEstimatorControllers {
  constructor(options) {
    super({ ...options, resolveTargets: (task) => { stats.targetRequests += 1; return options.resolveTargets(task); } });
    const invoke = this.callback;
    this.callback = (owner, task) => { stats.callbacks += 1; return invoke(owner, task); };
    if (tamperManifests) this.manifests = this.manifests.map((item) => ({ ...item, controller_version: `${item.controller_version}+tampered` }));
  }
}
const runtime = { dagMl, methods, estimatorAdapter: { N4mWasmEstimatorControllers: Observed } };
const resetStats = () => { stats.callbacks = 0; stats.targetRequests = 0; };

const read = (path) => new Uint8Array(fs.readFileSync(path));
const write = (path, value) => fs.writeFileSync(path, typeof value === 'string' ? value : JSON.stringify(value));

// Replay must never fit: any native FIT entry point aborts the call.
async function noFit(action) {
  const native = methods.getModule(), ccall = native.ccall;
  native.ccall = function guarded(name, ...rest) {
    if (name.includes('_fit')) throw new Error('Native FIT forbidden in replay');
    return ccall.call(this, name, ...rest);
  };
  try { return await action(); } finally { native.ccall = ccall; }
}

async function refusal(action) {
  resetStats();
  try { await action(); } catch (error) {
    return { refused: true, message: String(error?.message ?? error), callbacks: stats.callbacks };
  }
  return { refused: false, message: '', callbacks: stats.callbacks };
}

const commands = {
  async produce(job) {
    const result = await produceN4aNative(read(job.record), { ...runtime, sourceId: job.sourceId,
      recipe: fs.readFileSync(job.recipe, 'utf8'), runId: job.runId, containerId: job.containerId });
    const inspected = await inspectN4aBytes(result.archiveBytes);
    fs.writeFileSync(job.archiveOut, result.archiveBytes);
    return { outcome: result.view.training.outcome, sourceId: result.view.training.source_id,
      containerSha256: result.view.receipt.container_sha256, inspectedContainerSha256: inspected.receipt.container_sha256,
      inspectedRequestFingerprint: inspected.training.outcome.training_request_fingerprint };
  },
  async retrain(job) {
    const result = await retrainN4aNative(read(job.archive), read(job.record), { ...runtime, runId: job.runId, containerId: job.containerId });
    fs.writeFileSync(job.archiveOut, result.archiveBytes);
    return { outcome: result.view.training.outcome, sourceId: result.view.training.source_id, containerSha256: result.view.receipt.container_sha256 };
  },
  async predict(job) {
    const archive = read(job.archive), cases = [];
    resetStats();
    for (const item of job.cases) {
      const prediction = await noFit(() => predictN4aNative(archive, { ...runtime, record: item.record ? read(item.record) : undefined,
        entrypoint: item.entrypoint, sourceId: job.sourceId, partition: item.partition }));
      cases.push({ name: item.name, ...prediction });
    }
    // Y is unreachable: no fitting-label request, and the native plan has no target accessor.
    const native = await loadArchiveV2Native();
    const planMembers = Object.getOwnPropertyNames(native.N4aPredictPlan.prototype);
    return { cases, targetRequests: stats.targetRequests, planMembers };
  },
  async boundaries(job) {
    const archive = read(job.archive), train = JSON.parse(fs.readFileSync(job.trainRecord, 'utf8'));
    const recipe = JSON.parse(fs.readFileSync(job.recipe, 'utf8'));
    const encode = (value) => new TextEncoder().encode(JSON.stringify(value));
    const produce = (record, changed = recipe) => () => produceN4aNative(encode(record), { ...runtime, sourceId: job.sourceId, recipe: changed });
    const out = {};

    // A record mixing train and test partitions is supported: the training
    // cohort is its train rows and the test rows are prediction-only. The Rust
    // gate covers it positively through the `produce` and `predict` commands.

    const classifier = structuredClone(recipe);
    classifier.steps = [{ method_id: 'models.classification.pls_lda', role: 'classifier', params: { n_components: 1 } }];
    classifier.candidates = [{}];
    out.classifier = await refusal(produce(train, classifier));

    const indexed = structuredClone(recipe);
    indexed.steps.at(-1).params.native_target_index = 1;
    out.nativeTargetIndex = await refusal(produce(train, indexed));

    const masked = structuredClone(train);
    const [rows, targets] = masked.dataset.y.shape;
    masked.schema = 'nirs4all.dataset.v2'; masked.schema_version = 2;
    masked.dataset.target_mask = { dtype: 'bool', shape: [rows, targets],
      values: Array.from({ length: rows }, (_, row) => Array.from({ length: targets }, (_, col) => !(row === 0 && col === 1))) };
    out.partialTargetMask = await refusal(produce(masked));

    const changed = JSON.parse(fs.readFileSync(job.externalRecord, 'utf8'));
    changed.dataset.sources[0].axis_units = { wavelength: 'changed' };
    out.schemaChange = await refusal(() => predictN4aNative(archive, { ...runtime, record: encode(changed),
      entrypoint: 'predict-external', sourceId: job.sourceId }));

    // Untrusted manifests are refused by DAG-ML before any estimator callback.
    tamperManifests = true;
    try {
      out.manifestMismatch = await refusal(() => predictN4aNative(archive, { ...runtime, record: read(job.externalRecord),
        entrypoint: 'predict-external', sourceId: job.sourceId }));
    } finally { tamperManifests = false; }
    return out;
  },
};

const job = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
if (typeof commands[job.command] !== 'function') throw new Error(`Unknown exchange command '${job.command}'`);
write(job.out, await commands[job.command](job));
