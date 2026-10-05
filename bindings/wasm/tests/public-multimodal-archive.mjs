import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import * as io from '../../../../nirs4all-io/bindings/wasm/public-dataset.mjs';
import * as methods from '../../../../nirs4all-methods/bindings/js/dist/index.js';
import * as dagMl from '../node_modules/dag-ml-wasm/dag_ml_wasm.js';
import {predictMultimodalArchive} from '../src/multimodal-archive.js';
const directory=process.argv[2],helperPath=process.argv[3];
if(!directory||!helperPath)throw new Error('canonical fixture directory and installed DAG replay helper required');
const {replayMultimodalDatasetArchive:replayHelper}=await import(helperPath);
await methods.loadModule();
await dagMl.default({module_or_path:fs.readFileSync(new URL('../node_modules/dag-ml-wasm/dag_ml_wasm_bg.wasm',import.meta.url))});
const digest=bytes=>createHash('sha256').update(bytes).digest('hex');
const archiveBytes=Uint8Array.from(fs.readFileSync(path.join(directory,'u07-native.n4a')));
const predict=JSON.parse(fs.readFileSync(path.join(directory,'predict.json'))),expected=JSON.parse(fs.readFileSync(path.join(directory,'expected.json')));
const reversed=structuredClone(predict);for(const source of reversed.dataset.sources){source.sample_ids.reverse();source.array.values.reverse();source.presence_mask.values.reverse();}
const originalFit=methods.MultimodalPipeline.prototype.fit;
methods.MultimodalPipeline.prototype.fit=()=>{throw new Error('FIT forbidden during archive replay');};
try {
  const result=await predictMultimodalArchive(archiveBytes,reversed,{io,dagMl,methods,replayHelper,digest});
  assert.deepEqual(result.sample_ids,expected.sample_ids);assert.deepEqual(result.target_names,expected.target_names);
  result.values.forEach((row,i)=>assert.ok(Math.abs(row[0]-expected.values[i][0])<1e-8*(1+Math.abs(expected.values[i][0]))));
  assert.deepEqual(result.audit.map(event=>event.operation),['hydrate','PREDICT','dispose','release']);
  assert.equal(result.training_performed,false);
  const wrong=structuredClone(predict);wrong.dataset.sources[0].axis_units.wavelength='cm-1';
  await assert.rejects(predictMultimodalArchive(archiveBytes,wrong,{io,dagMl,methods,replayHelper,digest}),/schema differs/);
  const broken=archiveBytes.slice();broken[500]^=1;
  await assert.rejects(predictMultimodalArchive(broken,predict,{io,dagMl,methods,replayHelper,digest}));
  fs.writeFileSync(path.join(directory,'wasm-core-archive-evidence.json'),JSON.stringify(result,null,2));
} finally {methods.MultimodalPipeline.prototype.fit=originalFit;}
console.log('PASS JS/WASM unchanged native .n4a archive, target-free independent cohort, no FIT, schema/storage failures');
