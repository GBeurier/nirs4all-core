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
  const fitted = await runBrowserPipeline(record, options);
  const loaded = await loadBrowserPipeline(fitted.export(), options);
  await loaded.predict(record);
  await loaded.retrain(record);
  return predictBrowserPipeline(loaded, record);
}

// @ts-expect-error Transport requires the public dataset envelope and identities.
void runBrowserPipeline({ dataset: {} }, { pipeline: { steps: [], candidates: [] } });
