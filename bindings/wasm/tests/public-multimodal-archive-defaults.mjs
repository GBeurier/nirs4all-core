import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {predictMultimodalArchive,loadMethodsWasm} from '../src/index.js';
import {Dataset} from '@nirs4all/io-wasm/public-dataset';

const directory=process.argv[2];
if(!directory)throw new Error('canonical fixture directory required');
const archive=Uint8Array.from(fs.readFileSync(path.join(directory,'u07-native.n4a')));
const input=JSON.parse(fs.readFileSync(path.join(directory,'predict.json')));
const expected=JSON.parse(fs.readFileSync(path.join(directory,'expected.json')));
const methods=await loadMethodsWasm();
const originalFit=methods.MultimodalPipeline.prototype.fit;
methods.MultimodalPipeline.prototype.fit=()=>{throw new Error('FIT forbidden during archive replay');};
try {
  // No runtime, helper, digest, or archive-reader injection: exercise installed defaults.
  const result=await predictMultimodalArchive(archive,input);
  assert.deepEqual(result.sample_ids,expected.sample_ids);
  assert.deepEqual(result.target_names,expected.target_names);
  result.values.forEach((row,i)=>assert.ok(Math.abs(row[0]-expected.values[i][0])<1e-8*(1+Math.abs(expected.values[i][0]))));
  assert.deepEqual(result.audit.map(event=>event.operation),['hydrate','PREDICT','dispose','release']);
  assert.equal(result.training_performed,false);
  const logicalFile=path.join(directory,'logical-host-dataset.json');
  if(fs.existsSync(logicalFile)){
    const logical=JSON.parse(fs.readFileSync(logicalFile)),raw=logical.dataset;
    const host=Dataset.fromSources(Object.fromEntries(raw.sources.map(source=>[source.name,source.array.values])),{
      name:raw.name,sampleIds:raw.sample_ids,partitions:raw.partitions.values,targetNames:raw.target_names,
      originIds:logical.origin_ids,foldIds:logical.fold_ids,groups:raw.groups?.values,
      independentUnitIds:raw.independent_unit_ids,repetitionIds:raw.repetition_ids,
      axisUnits:Object.fromEntries(raw.sources.map(source=>[source.name,source.axis_units])),
      axisCoordinates:Object.fromEntries(raw.sources.map(source=>[source.name,source.axis_coordinates])),
      featureNames:Object.fromEntries(raw.sources.map(source=>[source.name,source.feature_names]))});
    const prediction=await predictMultimodalArchive(archive,host);
    assert.deepEqual(prediction.sample_ids,expected.sample_ids);
    prediction.values.forEach((row,i)=>assert.ok(Math.abs(row[0]-expected.values[i][0])<1e-8));
    assert.deepEqual(prediction.audit.map(event=>event.operation),['hydrate','PREDICT','dispose','release']);
    fs.writeFileSync(path.join(directory,'wasm-logical-archive-evidence.json'),JSON.stringify(prediction,null,2));
  }
  const incompatible=structuredClone(input);
  incompatible.dataset.sources[0].axis_units.wavelength='cm-1';
  await assert.rejects(predictMultimodalArchive(archive,incompatible),/schema differs/);
  const corrupt=archive.slice();corrupt[500]^=1;
  await assert.rejects(predictMultimodalArchive(corrupt,input),/archive|zip|CRC|checksum|integrity|signature|invalid|member|compressed|corrupt|sha|fingerprint/i);
  fs.writeFileSync(path.join(directory,'wasm-defaults-core-archive-evidence.json'),JSON.stringify(result,null,2));
} finally {methods.MultimodalPipeline.prototype.fit=originalFit;}
console.log('PASS JS/WASM public archive replay with installed defaults and no FIT');
