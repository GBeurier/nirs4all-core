// Generic n4m role recipes and trained envelopes (nirs4all.n4m.trained_pipeline.v8).
//
// A recipe step is the language-neutral token "n4m:<catalog method id>" (or
// {class: "n4m:<id>", params}). Every step resolves through the
// @nirs4all/methods manifest (methodClass / manifest); parameters, fitting,
// numerics and the portable N4ME state stay in Methods. This module only
// orders the steps, moves rows, and reads/writes the JSON envelope shared
// with the Python, R and Rust bindings.

import { coerceFeatures } from './execution.js';
import { loadMethodsWasm } from './index.js';

export const N4M_ROLE_PREFIX = 'n4m:';
export const N4M_TRAINED_PIPELINE_SCHEMA = 'nirs4all.n4m.trained_pipeline.v8';

const RECIPE_ROLES = new Set(['sample_filter', 'transformer', 'selector', 'regressor', 'classifier']);
const manifestCache = new WeakMap();

/** Catalog method id of an "n4m:<id>" class name, else null. */
export function n4mRoleMethodId(name) {
  return typeof name === 'string' && name.startsWith(N4M_ROLE_PREFIX) ? name.slice(N4M_ROLE_PREFIX.length) : null;
}

/** True when an "n4m:<id>" class name resolves to a Methods role class. */
export function resolvesN4mRole(name, methods) {
  const methodId = n4mRoleMethodId(name);
  if (methodId === null || typeof methods?.methodClass !== 'function') return false;
  try {
    methods.methodClass(methodId);
    return true;
  } catch {
    return false;
  }
}

/** Manifest-derived recipe steps: every Methods estimator usable in a role recipe. */
export async function n4mRoleCapabilities(options = {}) {
  const methods = await readyMethods(options);
  return methods.manifest().methods
    .filter((item) => item.kind === 'estimator' && item.roles.some((role) => RECIPE_ROLES.has(role)))
    .map((item) => ({
      token: N4M_ROLE_PREFIX + item.method_id,
      methodId: item.method_id,
      roles: [...item.roles],
      nodeKinds: [...item.node_kinds],
      parameters: item.params.map((param) => param.name),
    }));
}

/** A fitted recipe of n4m role steps, portable as N4ME states (envelope v8). */
export class N4mRolePipeline {
  constructor(recipe, nFeatures, steps) {
    this.recipe = recipe;
    this.nFeatures = nFeatures;
    this.steps = steps;
  }

  /** Fits every step of `recipe` natively on `dataset` ({X, y, rows, cols}). */
  static async fit(recipe, dataset, options = {}) {
    const methods = await readyMethods(options);
    const steps = recipeSteps(recipe, methods);
    const last = steps.at(-1);
    if (!last || !(last.roles.has('regressor') || last.roles.has('classifier'))) {
      throw new Error('An n4m role recipe ends with one regressor or classifier.');
    }
    const classification = last.roles.has('classifier');
    let X = coerceFeatures(dataset);
    const nFeatures = X.cols;
    let target = classification ? encodeLabels(dataset.y, X.rows) : regressionTarget(dataset.y, X.rows);
    const fitted = [];
    try {
      for (const step of steps.slice(0, -1)) {
        const stepY = step.needsY ? target.values : undefined;
        if (step.roles.has('sample_filter')) {
          const filter = step.create();
          try {
            const keep = filter.fit(X, stepY).getMask(X, stepY);
            const rows = keep.flatMap((flag, index) => (flag ? [index] : []));
            X = selectRows(X, rows);
            target = selectTarget(target, rows);
          } finally {
            filter.dispose();
          }
        } else if (step.roles.has('transformer') || step.roles.has('selector')) {
          const estimator = step.create();
          fitted.push({ estimator, methodId: step.methodId });
          X = estimator.fit(X, stepY).transform(X);
        } else {
          throw new Error(`n4m:${step.methodId} is not a portable pipeline step.`);
        }
      }
      const model = last.create();
      fitted.push({ estimator: model, methodId: last.methodId, classNames: target.classNames });
      model.fit(X, classification ? target.ids : target.matrix);
      const states = await Promise.all(fitted.map(exportState));
      return new N4mRolePipeline(clone(recipe), nFeatures, fitted.map((item, i) => ({ ...item, state: states[i] })));
    } catch (error) {
      fitted.forEach((item) => item.estimator.dispose());
      throw error;
    }
  }

  /** Rebuilds the estimators of a v8 envelope (object or JSON text) from their N4ME states. */
  static async fromJSON(source, options = {}) {
    const methods = await readyMethods(options);
    const envelope = typeof source === 'string' ? JSON.parse(source) : source;
    if (!envelope || typeof envelope !== 'object' || envelope.schema !== N4M_TRAINED_PIPELINE_SCHEMA) {
      throw new Error('Unsupported trained n4m pipeline envelope.');
    }
    const stateful = recipeSteps(envelope.recipe, methods).filter((step) => !step.roles.has('sample_filter'));
    if (!Array.isArray(envelope.states) || envelope.states.length !== stateful.length) {
      throw new Error('Envelope states do not match the recipe steps.');
    }
    const steps = [];
    try {
      for (let i = 0; i < stateful.length; i += 1) {
        const state = envelope.states[i];
        const payload = base64ToBytes(state.n4me_base64);
        if (await sha256Hex(payload) !== state.sha256) {
          throw new Error(`N4ME state of ${state.method_id} fails its checksum.`);
        }
        const estimator = methods.NativeEstimator.fromN4me(payload);
        steps.push({ estimator, methodId: state.method_id, classNames: state.class_names, state: { ...state } });
        if (estimator.methodId !== state.method_id || state.method_id !== stateful[i].methodId) {
          throw new Error(`N4ME state ${state.method_id} does not match its recipe step.`);
        }
      }
    } catch (error) {
      steps.forEach((item) => item.estimator.dispose());
      throw error;
    }
    return new N4mRolePipeline(clone(envelope.recipe), Number(envelope.n_features), steps);
  }

  /** The v8 envelope (JSON.stringify(pipeline) writes it). */
  toJSON() {
    return {
      schema: N4M_TRAINED_PIPELINE_SCHEMA,
      recipe: clone(this.recipe),
      n_features: this.nFeatures,
      states: this.steps.map((step) => ({ ...step.state })),
    };
  }

  /**
   * Predictions of the final model: {data, rows, cols} for a regressor,
   * {labels, rows} (class names, or ids when trained on integer labels) for a classifier.
   */
  predict(dataset) {
    let X = coerceFeatures(dataset);
    if (X.cols !== this.nFeatures) {
      throw new RangeError(`Expected ${this.nFeatures} input columns, got ${X.cols}.`);
    }
    for (const step of this.steps.slice(0, -1)) {
      X = step.estimator.transform(X);
    }
    const last = this.steps.at(-1);
    if (typeof last.estimator.predictLabels === 'function') {
      const ids = last.estimator.predictLabels(X);
      return { labels: last.classNames ? ids.map((id) => last.classNames[id]) : ids, rows: X.rows };
    }
    const predicted = last.estimator.predict(X);
    return { data: Array.from(predicted.data), rows: predicted.rows, cols: predicted.cols };
  }

  /** Fits the same recipe afresh on new training rows. */
  retrain(dataset, options = {}) {
    return N4mRolePipeline.fit(this.recipe, dataset, options);
  }

  /** Releases the native estimators. */
  dispose() {
    this.steps.forEach((step) => step.estimator.dispose());
  }
}

async function readyMethods(options) {
  const methods = options.methods ?? await loadMethodsWasm();
  if (typeof methods.loadModule === 'function') {
    await methods.loadModule();
  }
  return methods;
}

function manifestIndex(methods) {
  let index = manifestCache.get(methods);
  if (!index) {
    index = new Map(methods.manifest().methods.map((item) => [item.method_id, item]));
    manifestCache.set(methods, index);
  }
  return index;
}

function recipeSteps(recipe, methods) {
  const pipeline = recipe?.pipeline;
  if (!Array.isArray(pipeline) || pipeline.length === 0) {
    throw new TypeError('An n4m role recipe is {pipeline: [n4m:<method id> steps]}.');
  }
  const index = manifestIndex(methods);
  return pipeline.map((token) => {
    const name = token && typeof token === 'object' ? token.class : token;
    const methodId = n4mRoleMethodId(name);
    const info = methodId === null ? undefined : index.get(methodId);
    if (!info) {
      throw new Error(`n4m role recipes contain n4m:<method id> steps only, got ${JSON.stringify(token)}.`);
    }
    const params = token && typeof token === 'object' ? token.params ?? {} : {};
    const cls = methods.methodClass(methodId);
    return {
      methodId,
      roles: new Set(info.roles),
      needsY: info.inputs.y === 'required',
      create: () => new cls(clone(params)),
    };
  });
}

function regressionTarget(y, rows) {
  const nested = Array.isArray(y) && Array.isArray(y[0]);
  const cols = nested ? y[0].length : 1;
  const data = nested ? Float64Array.from(y.flat()) : Float64Array.from(y ?? []);
  if (data.length !== rows * cols) {
    throw new RangeError(`Target length ${data.length} does not match ${rows} rows.`);
  }
  const matrix = { data, rows, cols };
  return { matrix, values: cols === 1 ? data : undefined };
}

// Integer labels are class ids; other labels map to ids in sorted order and
// travel as class_names (the same encoding as the Python and R bindings).
function encodeLabels(y, rows) {
  const labels = Array.from(y ?? []);
  if (labels.length !== rows) {
    throw new RangeError(`Label count ${labels.length} does not match ${rows} rows.`);
  }
  if (labels.every((label) => Number.isInteger(label))) {
    return { ids: labels, values: Float64Array.from(labels), classNames: undefined };
  }
  const numeric = labels.every((label) => typeof label === 'number');
  const names = [...new Set(numeric ? labels : labels.map(String))]
    .sort(numeric ? (a, b) => a - b : (a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const position = new Map(names.map((name, id) => [name, id]));
  const ids = labels.map((label) => position.get(numeric ? label : String(label)));
  return { ids, values: Float64Array.from(ids), classNames: names };
}

function selectTarget(target, rows) {
  if (target.ids) {
    const ids = rows.map((row) => target.ids[row]);
    return { ...target, ids, values: Float64Array.from(ids) };
  }
  const matrix = selectRows(target.matrix, rows);
  return { matrix, values: matrix.cols === 1 ? matrix.data : undefined };
}

function selectRows(matrix, rows) {
  const data = new Float64Array(rows.length * matrix.cols);
  rows.forEach((row, i) => data.set(matrix.data.subarray(row * matrix.cols, (row + 1) * matrix.cols), i * matrix.cols));
  return { data, rows: rows.length, cols: matrix.cols };
}

async function exportState({ estimator, methodId, classNames }) {
  const payload = estimator.toN4me();
  const state = { method_id: methodId, n4me_base64: bytesToBase64(payload), sha256: await sha256Hex(payload) };
  if (classNames) state.class_names = [...classNames];
  return state;
}

async function sha256Hex(bytes) {
  const digest = new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', bytes));
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function bytesToBase64(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

function base64ToBytes(text) {
  if (typeof text !== 'string') throw new TypeError('n4me_base64 must be a base64 string.');
  return Uint8Array.from(atob(text), (char) => char.charCodeAt(0));
}

function clone(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
}
