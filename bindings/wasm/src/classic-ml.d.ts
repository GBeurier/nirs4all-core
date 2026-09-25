import type { JsEstimatorController, JsEstimatorControllerOptions } from './index.js';

export type MlJsModelName = 'RandomForestRegressor' | 'RandomForestClassifier' |
  'DecisionTreeRegressor' | 'DecisionTreeClassifier' | 'KNeighborsClassifier';
export type ScikitJsSyncModelName = 'DecisionTreeRegressor' | 'DecisionTreeClassifier';
export interface HostEstimator {
  getParams(): Record<string, unknown>;
  setParams(params: Record<string, unknown>): HostEstimator;
  clone(): HostEstimator;
  fit(X: number[][], y: number[]): HostEstimator | Promise<HostEstimator>;
  predict(X: number[][]): number[] | number[][] | Promise<number[] | number[][]>;
  toJSON(): unknown | Promise<unknown>;
}
export interface HostTransformer {
  getParams(): Record<string, unknown>;
  setParams(params: Record<string, unknown>): HostTransformer;
  clone(): HostTransformer;
  fit(X: number[][], y?: number[]): HostTransformer | Promise<HostTransformer>;
  transform(X: number[][]): number[][] | Promise<number[][]>;
  fitTransform(X: number[][], y?: number[]): number[][] | Promise<number[][]>;
  toJSON(): unknown | Promise<unknown>;
}
export function loadMlJs(): Promise<Record<string, unknown>>;
export function loadScikitJs(tensorflow?: unknown): Promise<Record<string, unknown>>;
export function createMlJsEstimator(options: {
  ml: Record<string, unknown>;
  estimatorName: MlJsModelName;
  params?: Record<string, unknown>;
}): HostEstimator & { load(payload: unknown): HostEstimator };
export function createMlJsPca(options: {
  ml: Record<string, unknown>;
  params?: Record<string, unknown>;
}): HostTransformer & {
  inverseTransform(X: number[][]): number[][];
  load(payload: unknown): HostTransformer;
};
export function createMlJsController(options:
  Omit<JsEstimatorControllerOptions, 'controllerId' | 'createEstimator' | 'restoreEstimator'> & {
    ml: Record<string, unknown>;
    estimatorName: MlJsModelName;
    controllerId?: string;
  }): JsEstimatorController;
export function createScikitJsEstimator(options: {
  scikitJs: Record<string, unknown>;
  estimatorName: string;
  params?: Record<string, unknown>;
}): HostEstimator & HostTransformer & { load(payload: unknown): Promise<HostEstimator & HostTransformer> };
export function createScikitJsController(options:
  Omit<JsEstimatorControllerOptions, 'controllerId' | 'createEstimator' | 'restoreEstimator'> & {
    scikitJs: Record<string, unknown>;
    estimatorName: ScikitJsSyncModelName;
    controllerId?: string;
  }): JsEstimatorController;
