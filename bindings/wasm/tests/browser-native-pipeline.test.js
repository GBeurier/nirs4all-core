import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import test from 'node:test';
import {runBrowserPipeline,loadBrowserPipeline} from '../src/browser-native-pipeline.js';
const paths = ['NIRS4ALL_METHODS_JS','NIRS4ALL_IO_JS','NIRS4ALL_DAG_JS','NIRS4ALL_ESTIMATOR_ADAPTER_JS'];
const qualified = paths.every(key => process.env[key]);
async function dependencies() {
  const [methods,io,dagMl,estimatorAdapter] = await Promise.all(paths.map(key=>import(pathToFileURL(process.env[key]))));
  await dagMl.default({module_or_path:await readFile(new URL('dag_ml_wasm_bg.wasm',pathToFileURL(process.env.NIRS4ALL_DAG_JS)))});
  await methods.loadModule();return {methods,io,dagMl,estimatorAdapter};
}
const X=Array.from({length:12},(_,i)=>Array.from({length:7},(_,j)=>Math.sin(i+j*.4)+i*.3+j*.2));
const pipeline={steps:[{method_id:'preprocessing.scaling.standard_scale',role:'transformer',params:{}},{method_id:'models.regularized.ridge',role:'regressor',params:{}}],candidates:[{alpha:.1},{alpha:1}]};
test('WASM role nodes retain one N4ME each; grouped multi-target replay cannot fit', {skip:!qualified}, async()=>{
  const options={...await dependencies(),pipeline};
  const y=X.map(row=>[row[0]*2-row[2]+.7,row[1]*-.5+row[4]*.3]);
  const input=options.io.Dataset.fromSources({spectra:X},{sampleIds:X.map((_,i)=>`id:${i}`),y,targetNames:['a','b'],groups:X.map((_,i)=>`group:${Math.floor(i/2)}`),foldIds:X.map((_,i)=>`fold:${Math.floor(i/2)%3}`)});
  const model=await runBrowserPipeline(input.toJSON(),options);
  assert.equal(model.outcome.execution_bundle.refit_artifacts.length,2);
  assert.equal(model.outcome.effective_plan.variants.length,2);
  assert.equal(model.outcome.effective_plan.fold_set.folds.length,3);
  for(const fold of model.outcome.effective_plan.fold_set.folds) {
    const group=id=>Math.floor(Number(id.split(':')[1])/2);
    assert.ok(fold.train_sample_ids.every(id=>!fold.validation_sample_ids.some(other=>group(id)===group(other))));
  }
  const inference=options.io.Dataset.fromSources({spectra:X.slice(0,3)},{sampleIds:['new:0','new:1','new:2'],targetNames:['a','b'],partitions:['predict','predict','predict']});
  const native=options.methods.getModule(), original=native.ccall;
  native.ccall=function(name,...args){if(name.includes('_fit'))throw Error('Native FIT forbidden in replay');return original.call(this,name,...args);};
  try {
    const restored=await loadBrowserPipeline(model.export(),options), out=await restored.predict(inference.toJSON(),options);
    assert.ok(out.lineage.every(row=>row.phase==='PREDICT'));
    assert.deepEqual(out.outputs[0].predictions[0].sample_ids,['new:0','new:1','new:2']);
    assert.ok(out.outputs[0].predictions[0].values.every(row=>row.length===2&&row.every(Number.isFinite)));
    const bad=inference.toJSON();bad.dataset.sources[0].axis_units.wavelength='changed';
    await assert.rejects(restored.predict(bad,options),/schema differs/);
    await assert.rejects(runBrowserPipeline(input.toJSON(),{...options,pipeline:{...pipeline,candidates:[{alpha:'1'}]}}),/parameter type/);
    const oversized=structuredClone(pipeline);oversized.steps[1].method_id='models.pls.pls_regression';oversized.steps[1].params.n_components=Number.MAX_SAFE_INTEGER+1;oversized.candidates=[{}];
    await assert.rejects(runBrowserPipeline(input.toJSON(),{...options,pipeline:oversized}),/parameter type|method\/role/);
    const unsafe=structuredClone(pipeline);unsafe.steps[0].params.with_mean='yes';
    await assert.rejects(runBrowserPipeline(input.toJSON(),{...options,pipeline:unsafe}),/parameter type/);
  }finally{native.ccall=original;}
});
test('PLS-LDA classification uses native labels through CV/refit/replay', {skip:!qualified}, async()=>{
  const options={...await dependencies(),pipeline:{steps:[{method_id:'models.classification.pls_lda',role:'classifier',params:{n_components:1}}],candidates:[{}]}};
  const input=options.io.Dataset.fromSources({spectra:X},{sampleIds:X.map((_,i)=>`id:${i}`),y:X.map((_,i)=>Math.floor(i/2)%2),taskType:'classification',targetNames:['class']});
  const model=await runBrowserPipeline(input.toJSON(),options);
  const input2=options.io.Dataset.fromSources({spectra:X.slice(0,3)},{sampleIds:['new:0','new:1','new:2'],partitions:['predict','predict','predict'],targetNames:['class']});
  const original=options.methods.NativeEstimator.prototype.fit;
  options.methods.NativeEstimator.prototype.fit=()=>{throw Error('Classifier replay FIT');};
  try {
    const out=await (await loadBrowserPipeline(model.export(),options)).predict(input2.toJSON(),options);
    assert.equal(out.outputs[0].binding.prediction_kind,'class_label');
    assert.ok(out.outputs[0].predictions[0].values.flat().every(label=>[0,1].includes(label)));
    assert.ok(out.lineage.every(row=>row.phase==='PREDICT'));
  }finally{options.methods.NativeEstimator.prototype.fit=original;}
});
