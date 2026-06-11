export const upstreams = Object.freeze([
  {
    key: 'dag_ml',
    candidates: ['dag-ml'],
    role: 'Leakage-safe DAG/ML execution coordinator',
  },
  {
    key: 'dag_ml_data',
    candidates: ['dag-ml-data'],
    role: 'Sample-aligned data contracts for DAG/ML runtimes',
  },
  {
    key: 'formats',
    candidates: ['nirs4all-formats'],
    role: 'Spectroscopy/NIRS vendor file readers',
  },
  {
    key: 'io',
    candidates: ['nirs4all-io'],
    role: 'Dataset assembly bridge',
  },
  {
    key: 'datasets',
    candidates: ['nirs4all-datasets'],
    role: 'DOI-pinned NIRS dataset catalog',
  },
  {
    key: 'methods',
    candidates: ['nirs4all-methods'],
    role: 'Portable C ABI PLS/NIRS numerical engine',
  },
]);

const upstreamByKey = new Map(upstreams.map((item) => [item.key, item]));

export function upstream(name) {
  return upstreamByKey.get(name) ?? null;
}

export async function importUpstream(name) {
  const item = upstream(name);
  if (!item) {
    throw new Error(`Unknown nirs4all upstream: ${name}`);
  }

  for (const candidate of item.candidates) {
    try {
      return await import(candidate);
    } catch (error) {
      if (isMissingModuleError(error)) {
        continue;
      }
      throw error;
    }
  }

  throw new Error(
    `nirs4all upstream '${name}' is not installed. Tried ${item.candidates.join(', ')}.`,
  );
}

export const formats = Object.freeze({ key: 'formats', import: () => importUpstream('formats') });
export const io = Object.freeze({ key: 'io', import: () => importUpstream('io') });
export const datasets = Object.freeze({ key: 'datasets', import: () => importUpstream('datasets') });
export const methods = Object.freeze({ key: 'methods', import: () => importUpstream('methods') });
export const dagMl = Object.freeze({ key: 'dag_ml', import: () => importUpstream('dag_ml') });
export const dagMlData = Object.freeze({
  key: 'dag_ml_data',
  import: () => importUpstream('dag_ml_data'),
});

function isMissingModuleError(error) {
  return error && (error.code === 'ERR_MODULE_NOT_FOUND' || error.code === 'ERR_PACKAGE_PATH_NOT_EXPORTED');
}
