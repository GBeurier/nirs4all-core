import {
  dagMl,
  dagMlData,
  datasets,
  formats,
  importUpstream,
  inspectMethodsArchiveV2Predictors,
  inspectN4aBytes,
  io,
  loadDagMlDataWasm,
  loadDagMlWasm,
  loadDataIoWasm,
  loadDatasets,
  loadDatasetsWasm,
  loadFormats,
  loadIo,
  localImplementationRegistry,
  loadMethods,
  loadMethodsWasm,
  loadPipelineDefinition,
  loadPortableStack,
  methods,
  methodsWasm,
  parseExecutionPlan,
  portableClassNames,
  predictN4aNative,
  produceN4aNative,
  retrainN4aNative,
  openN4aContainer,
  writeN4aStream,
  type N4aChunkStream,
  type N4aContainer,
  type N4aPrediction,
  type N4aRandomAccessSource,
  type N4aSink,
  type N4aProduction,
  type N4aView,
  executionProfiles,
  type ExecutionProfiles,
  predictPortablePipeline,
  runPortablePipeline,
  upstream,
  upstreams,
  N4mRolePipeline,
  n4mRoleCapabilities,
  type N4mRoleCapability,
  type N4mRolePrediction,
  type N4mTrainedPipelineEnvelope,
  type PipelineDefinition,
  type NativePredictorDescriptorV1,
  type PortableExecutionResult,
  type PortableMatrixDataset,
  type PortablePredictionResult,
  type PortablePlsModel,
  type PortableSplitResult,
  type PortableVariantResult,
  type Upstream,
  type UpstreamProxy,
} from 'nirs4all';

const definition: PipelineDefinition = loadPipelineDefinition({
  name: 'typed-consumer',
  pipeline: [
    { class: 'nirs4all.operators.transforms.StandardNormalVariate' },
    {
      model: {
        class: 'sklearn.cross_decomposition.PLSRegression',
        params: { n_components: 2 },
      },
    },
  ],
});

const classes: string[] = portableClassNames(definition);
const plan = parseExecutionPlan(definition);
const upstreamList: readonly Upstream[] = upstreams;
const firstUpstream: Upstream | null = upstream('methods');
const localRegistry: Promise<unknown> = localImplementationRegistry();
const proxy: UpstreamProxy = methods;
const allProxyKeys: string[] = [formats.key, io.key, datasets.key, dagMl.key, dagMlData.key, proxy.key];
const profiles: ExecutionProfiles = executionProfiles();
const profileIds: readonly ('native-workflow' | 'native-conformal' | 'native-n4a-canonical' | 'n4a-container')[] = profiles.profiles.map((item) => item.id);

const split: PortableSplitResult = {
  kind: 'all',
  trainIndices: [0, 1],
  testIndices: [0, 1],
};
const variant: PortableVariantResult = {
  n_components: 2,
  rmse: 0,
  predictions: [1, 2],
};
const model: PortablePlsModel = {
  type: 'PLSRegression',
  n_components: 2,
  coefficients: [1, 0],
  xMean: [0, 0],
  yMean: [0],
  intercept: null,
  n_features: 2,
  n_targets: 1,
};
const fitted: PortableExecutionResult = {
  name: definition.name,
  rows: 2,
  cols: 2,
  split,
  preprocessing: [{ type: 'StandardNormalVariate', params: [] }],
  variants: [variant],
  selected: variant,
  model,
  targets: [1, 2],
};
const dataset: PortableMatrixDataset = {
  X: [
    [1, 2],
    [3, 4],
  ],
  y: [1, 2],
  rows: 2,
  cols: 2,
};

const fittedPromise: Promise<PortableExecutionResult> = runPortablePipeline(definition, dataset, {
  methods: {},
});
const predictionPromise: Promise<PortablePredictionResult> = predictPortablePipeline(fitted, dataset, {
  methods: {},
});
const loaders: Promise<unknown>[] = [
  importUpstream('methods'),
  loadFormats(),
  loadIo(),
  loadDatasets(),
  loadMethods(),
  loadDagMlWasm(),
  loadDagMlDataWasm(),
  loadDatasetsWasm(),
  loadMethodsWasm(),
];
const stackPromise: Promise<Record<string, unknown>> = loadPortableStack(['methods', 'formats']);
const dataIoPromise: Promise<{ formats: unknown; io: unknown }> = loadDataIoWasm();
const maybeLoadedMethods: unknown = methodsWasm();
const inspectedPredictors: Promise<readonly NativePredictorDescriptorV1[]> =
  inspectMethodsArchiveV2Predictors(new Uint8Array());

const n4aRecipe = {
  steps: [
    { method_id: 'preprocessing.scaling.standard_scale', role: 'transformer' as const },
    { method_id: 'models.regularized.ridge', role: 'regressor' as const, params: { scale_x: false } },
  ],
  candidates: [{ alpha: 0.1 }, { alpha: 1 }],
};
const n4aProduced: Promise<N4aProduction> = produceN4aNative(new Uint8Array(), { sourceId: 'spectra', recipe: n4aRecipe });
const n4aRetrained: Promise<N4aProduction> = retrainN4aNative(new Uint8Array(), new Uint8Array(), { embedTrainingRecord: false });
const n4aInspected: Promise<N4aView> = inspectN4aBytes(new Uint8Array());
const n4aPredicted: Promise<N4aPrediction> = predictN4aNative(new Uint8Array(), {
  entrypoint: 'predict-external', sourceId: 'spectra', record: new Uint8Array(),
});

void classes;
void plan;
void upstreamList;
void firstUpstream;
void localRegistry;
void allProxyKeys;
void profileIds;
void fittedPromise;
void predictionPromise;
void loaders;
void stackPromise;
void dataIoPromise;
void maybeLoadedMethods;
void inspectedPredictors;
void n4aProduced;
void n4aRetrained;
void n4aInspected;
void n4aPredicted;

async function roleRecipe(): Promise<void> {
  const fitted: N4mRolePipeline = await N4mRolePipeline.fit(
    { pipeline: ['n4m:preprocessing.scatter.snv', { class: 'n4m:models.pls.cppls', params: { n_components: 3 } }] },
    { X: [[1, 2], [3, 4]], rows: 2, cols: 2, y: [[1, 0], [2, 1]], featureNames: ['a', 'b'] },
  );
  const envelope: N4mTrainedPipelineEnvelope = fitted.toJSON({ allowTrainingRows: false });
  const names: string[] | undefined = envelope.feature_names ?? fitted.featureNames;
  const retainsRows: boolean | undefined = envelope.states[0]?.contains_training_rows;
  const replayed = await N4mRolePipeline.fromJSON(JSON.stringify(envelope));
  const prediction: N4mRolePrediction = replayed.predict({ X: [[1, 2]], rows: 1, cols: 2, featureNames: names });
  void retainsRows;
  const steps: N4mRoleCapability[] = await n4mRoleCapabilities();
  void prediction;
  void steps;
}
void roleRecipe;

// Streaming `.n4a` storage: external random-access source, verified chunks, sink and budget.
async function streamingStorage(): Promise<void> {
  const source: N4aRandomAccessSource = {
    size: 16n,
    read: async (offset: bigint, length: bigint) => new Uint8Array(Number(length)).fill(Number(offset)),
  };
  const container: N4aContainer = await openN4aContainer(source, { memoryBudgetBytes: '1048576' });
  const kind: 'bytes' | 'external' = container.kind;
  const size: bigint = container.size;
  const stream: N4aChunkStream | undefined = container.memberStream('host/model.bin');
  if (stream !== undefined) {
    const length: bigint = stream.byteLength;
    for await (const chunk of stream) { const bytes: Uint8Array = chunk; void bytes; }
    void length;
  }
  const whole: Uint8Array | undefined = await container.readMember('host/model.bin');
  const sink: N4aSink = { write: (chunk: Uint8Array) => { void chunk; }, abort: (error: unknown) => { void error; } };
  const { receipt } = await writeN4aStream('{}', [{ path: 'host/model.bin', source: new Blob([new Uint8Array(1)]) }], sink,
    { base: container, memoryBudgetBytes: 4096n });
  const total: number = receipt.byte_length;
  const budgeted = await inspectN4aBytes(new Uint8Array(1), { memoryBudgetBytes: 1024 });
  void kind; void size; void whole; void total; void budgeted;
  container.close();
}
void streamingStorage;
