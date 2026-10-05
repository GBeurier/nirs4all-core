import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import * as io from '../../../../nirs4all-io/bindings/wasm/public-dataset.mjs';
import * as methods from '../../../../nirs4all-methods/bindings/js/dist/index.js';
import {MultimodalPredictor,dataset} from '../src/multimodal.js';
const directory=process.argv[2];if(!directory)throw new Error('canonical U07 qualification fixture directory required');
const read=name=>JSON.parse(fs.readFileSync(path.join(directory,`${name}.json`),'utf8'));
await methods.loadModule();
const train=read('train'),predict=read('predict'),recipe=read('recipe'),expected=read('expected');
const check=result=>{assert.deepEqual(result.sample_ids,expected.sample_ids);assert.deepEqual(result.target_names,expected.target_names);result.values.forEach((row,i)=>assert.ok(Math.abs(row[0]-expected.values[i][0])<1e-8*(1+Math.abs(expected.values[i][0]))));};
const ds=await dataset(train,{io});assert.deepEqual(ds.sampleIds,train.dataset.sample_ids);
const duplicate=structuredClone(train);duplicate.dataset.sample_ids[1]=duplicate.dataset.sample_ids[0];assert.throws(()=>io.dataset(duplicate),/identit/);
const leak=structuredClone(train);leak.origin_ids[0]=leak.origin_ids[5];leak.fold_ids[0]='different';assert.throws(()=>io.dataset(leak),/origin/);
const malformed=structuredClone(train);malformed.dataset.sources[1].axes=['sample','feature'];assert.throws(()=>io.dataset(malformed),/axes/);
const unsafeCoordinates=structuredClone(train);unsafeCoordinates.dataset.sources[0].axis_coordinates.wavelength[0]=2**53;assert.throws(()=>io.dataset(unsafeCoordinates),/coordinates.*JavaScript/);
const reversed=structuredClone(predict);for(const s of reversed.dataset.sources){s.sample_ids.reverse();s.array.values.reverse();s.presence_mask.values.reverse();}
const loaded=await MultimodalPredictor.load(read('predictor'),{io,methods});
const fit=methods.MultimodalPipeline.prototype.fit;methods.MultimodalPipeline.prototype.fit=function(){throw new Error('FIT forbidden during cold replay');};
try{check(loaded.predict(reversed));const bad=structuredClone(predict);bad.dataset.sources[0].axis_units.wavelength='cm-1';assert.throws(()=>loaded.predict(bad),/schema differs/);const broken=read('predictor');broken.state[60]^=1;await assert.rejects(MultimodalPredictor.load(broken,{io,methods}));}finally{methods.MultimodalPipeline.prototype.fit=fit;loaded.close();}
const fresh=await MultimodalPredictor.fit(recipe,train,{io,methods});try{check(fresh.predict(predict));fs.writeFileSync(path.join(directory,'predictor-wasm.json'),JSON.stringify(fresh.toJSON()));}finally{fresh.close();}
console.log('PASS JS/WASM canonical U07 raw alignment, fit, cross-host cold N4MF replay, no FIT, schema/state failures');
