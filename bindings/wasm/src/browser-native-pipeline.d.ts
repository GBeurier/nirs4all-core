import type { DatasetRecord } from '@nirs4all/io-wasm/public-dataset';
export interface BrowserNativeStep { method_id: string; role: 'transformer' | 'selector' | 'regressor' | 'classifier'; params?: Record<string, unknown>; }
export interface BrowserNativeRecipe { steps: BrowserNativeStep[]; candidates: Record<string, unknown>[]; }
export interface BrowserNativePipelineOptions { pipeline: BrowserNativeRecipe; sourceId?: string; seed?: number; folds?: number; runId?: string; dagMl?: unknown; methods?: unknown; io?: unknown; estimatorAdapter?: unknown; }
export declare class BrowserNativePipeline {
  readonly config: { source_id: string; pipeline: BrowserNativeRecipe };
  readonly outcome: Record<string, unknown>;
  export(): string;
  predict(value: DatasetRecord, options?: Partial<BrowserNativePipelineOptions>): Promise<Record<string, unknown>>;
  retrain(value: DatasetRecord, options?: Partial<BrowserNativePipelineOptions>): Promise<BrowserNativePipeline>;
  compare(query?: Record<string, unknown>): unknown;
}
export declare function runBrowserPipeline(value: DatasetRecord, options: BrowserNativePipelineOptions): Promise<BrowserNativePipeline>;
export declare function loadBrowserPipeline(text: string, options?: Partial<BrowserNativePipelineOptions>): Promise<BrowserNativePipeline>;
export declare function predictBrowserPipeline(model: BrowserNativePipeline, value: DatasetRecord, options?: Partial<BrowserNativePipelineOptions>): Promise<Record<string, unknown>>;
