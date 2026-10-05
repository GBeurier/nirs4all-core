// Core transports; DAG-ML owns calibration, interval calculations and lineage.
import { Workflow, workflowDependencies, workflowReplay } from './workflow.js';
import { loadArchiveV2Native, readPortableArchiveV2, replayMethodsArchiveV2, writePortableArchiveV2 } from './archive-v2.js';
import { independentPhysicalDataset } from './uncertainty-cohort.js';

const json = JSON.stringify;
const text = bytes => new TextDecoder('utf-8', { fatal: true }).decode(bytes);
const ordered = value => Array.isArray(value) ? value.map(ordered) : value && typeof value === 'object'
  ? Object.fromEntries(Object.keys(value).sort().map(key => [key, ordered(value[key])])) : value;

export class CalibratedWorkflow {
  constructor(archive, calibration) {
    this.archive = new Uint8Array(archive);
    this.calibration = structuredClone(calibration);
  }
  predict(value, options = {}) { return predictCalibrated(this, value, options); }
  export() { return new Uint8Array(this.archive); }
}

export function exportCalibrated(model) {
  if (!(model instanceof CalibratedWorkflow)) throw new TypeError('A calibrated workflow is required');
  return model.export();
}

export async function calibrate(model, value, options = {}) {
  const source = model instanceof Workflow ? model.archive : model;
  const deps = await workflowDependencies(options);
  const record = independentPhysicalDataset(deps.io, value);
  const details = await workflowReplay(source, record, options, true);
  const original = details.archive;
  const native = await loadArchiveV2Native();
  const outcome = text(original.members[original.manifest.replay.training_artifacts.training_outcome.member_path]);
  const capture = JSON.parse(native.calibrate_workflow_replay_json(outcome, details.replayJson,
    json(details.relations), json(details.truth), json(options.coverages ?? [0.9]), json(options.smallSamplePolicy ?? 'error')));
  const { dag } = await workflowDependencies(options);
  const payloads = JSON.parse(dag.build_archive_v2_native_portable_payloads_json(
    'archive:public:calibrated', capture.training_outcome_json, capture.portable_predictor_package_json));
  const archive = await writePortableArchiveV2(payloads.manifest,
    Object.fromEntries(Object.entries(payloads.members).map(([key, bytes]) => [key, Uint8Array.from(bytes)])));
  return new CalibratedWorkflow(archive, capture.calibration);
}

export async function predictCalibrated(model, value, options = {}) {
  const deps = await workflowDependencies(options);
  const record = independentPhysicalDataset(deps.io, value);
  const bytes = model instanceof CalibratedWorkflow ? model.archive : model;
  const archive = await readPortableArchiveV2(bytes);
  const storedPackage = text(archive.members[archive.manifest.replay.portable_predictor_package.member_path]);
  const pkg = JSON.parse(storedPackage);
  if (Object.values(pkg.effective_plan.node_plans).some(node => node.controller_id === 'controller:methods.pls')) {
    const { io } = deps;
    const raw = record.dataset;
    if (raw.y !== null || raw.partitions.values.some(role => role !== 'predict')) throw new TypeError('Prediction requires target-free predict rows');
    const sourceId = Object.values(pkg.template.campaign.data_bindings).flat()[0].source_ids[0];
    const expected = pkg.template.campaign.metadata.raw_source_schema;
    if (!expected || json(ordered(io.publicSourceSchema(record, sourceId))) !== json(ordered(expected))) throw new TypeError('Prediction source schema differs from frozen predictor');
    const source = raw.sources.find(source => source.name === sourceId);
    if (source.array.shape.length !== 2 || source.presence_mask.values.some(present => !present)) throw new TypeError('Calibrated inference requires a complete numeric source');
    const replay = await replayMethodsArchiveV2(bytes, { X: source.array.values,
      rows: raw.sample_ids.length, cols: source.array.shape[1], sampleIds: raw.sample_ids }, { methods: deps.methods });
    const values = Array.from({ length: replay.rows }, (_, i) => replay.data.slice(i * replay.cols, (i + 1) * replay.cols));
    const native = await loadArchiveV2Native();
    const nativeJson = native.calibrated_methods_points_json(storedPackage, archive.archiveSha256,
      json(replay.sampleIds), json(values), json(replay.nativePredictorDescriptor));
    const result = JSON.parse(nativeJson);
    Object.defineProperty(result, 'native_json', { value: nativeJson });
    return result;
  }
  const details = await workflowReplay(bytes, record, options, false, true);
  const packageJson = details.packageJson;

  const native = await loadArchiveV2Native();
  return JSON.parse(native.calibrated_prediction_json(packageJson, details.request, details.replayJson));
}

export async function conformalMetrics(model, prediction, truth) {
  if (!(model instanceof CalibratedWorkflow)) throw new TypeError('A calibrated workflow is required');
  const native = await loadArchiveV2Native();
  const archive = await readPortableArchiveV2(model.archive);
  const packageJson = text(archive.members[archive.manifest.replay.portable_predictor_package.member_path]);
  return JSON.parse(native.conformal_metrics_json(packageJson, json(prediction.interval_block), json(truth)));
}

export async function loadCalibrated(bytes, options = {}) {
  const archive = await readPortableArchiveV2(bytes), { dag } = await workflowDependencies(options);
  const packageJson = text(archive.members[archive.manifest.replay.portable_predictor_package.member_path]);
  dag.validate_archive_v2_portable_payloads_json(json(archive.manifest), packageJson,
    json(Object.fromEntries(Object.entries(archive.members).map(([key, value]) => [key, [...value]]))));
  const calibration = JSON.parse(packageJson).conformal_calibration;
  if (!calibration) throw new TypeError('Native archive has no calibration');
  return new CalibratedWorkflow(bytes, calibration);
}
