import {
  loadBrowserPipeline,
  runBrowserPipeline,
  predictBrowserPipeline,
  type BrowserNativeDatasetRecord,
  type BrowserNativePipelineOptions,
} from '../../src/index.js';

export async function consumeBrowserTransport(
  record: BrowserNativeDatasetRecord,
  options: BrowserNativePipelineOptions,
) {
  const metadataRecord: BrowserNativeDatasetRecord = {
    ...record,
    dataset: {
      ...record.dataset,
      target_names: ['class_a', 'class_b'],
      task_type: 'classification',
      source_alignment: 'left',
      target_mask: { dtype: 'bool', shape: [0, 2], values: [] },
      independent_unit_ids: [],
      repetition_ids: [],
    },
  };
  const fitted = await runBrowserPipeline(record, options);
  const loaded = await loadBrowserPipeline(fitted.export(), options);
  await loaded.predict(record);
  await loaded.retrain(record);
  await loaded.predict(metadataRecord);
  return predictBrowserPipeline(loaded, record);
}

// @ts-expect-error Transport requires the public dataset envelope and identities.
void runBrowserPipeline({ dataset: {} }, { pipeline: { steps: [], candidates: [] } });
