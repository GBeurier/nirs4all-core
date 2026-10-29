"""Verify retained actual Core046 lifecycle captures on their original native host.

This measures retained evidence and does not repeat the nine/five CLI operations.
"""
from __future__ import annotations
import argparse,datetime,hashlib,json,os,platform,stat,zlib
from pathlib import Path
SOURCE='a857710eab4692be905f043102d2c41a561ee4b9'
CLI_SOURCE_SHA='9fce5ffdfeac850445622e58845f539a0d7a0b2bf51fdb62dfd5b2b68925f59d'
ARCHIVE_SHA='994252030ff80129d0431995bae53eb473082f05825b65714379262b72af13fa'
CLI={'linux':'7f3a940fff385501d7fe4f3e31731e5726c2104d003411c5bc08c69000edf11b','windows':'bee90f9c155cce23cdaac741a69f6c6fe1922560e7280d313ea7899c36146a4f'}
METHODS={'linux':'b54bc8e9ee781eb12560445f092d9258b8c8cd2385786892b1e07940ec0ac7cd','windows':'a821dd2bc4d3d9f9a4640966542c31a60a54f7434c9d66b0d579e01acf61a7fb'}
GOLDEN=[[1.6363636363636365,13.272727272727273],[2.4999999999999996,15.0]]
EXPECTED={'linux':{'cargo-build-cli':0,'cargo-build-lifecycle-tests':0,'unit-success_closes_owned_snapshot_tree':0,'unit-refusal_closes_owned_snapshot_tree_and_preserves_exit':0,'help-refusal':1,'noargs-refusal':1,'archive-inspect':0,'native-prediction':0,'wrong-hash-refusal':1},'windows':{'archive-inspect':0,'native-prediction':0,'wrong-hash-refusal':1,'after-load-output-refusal':1,'junction-root-refusal':1}}
def digest(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def load(p):return json.loads(Path(p).read_text())
def descriptor(p):
 p=Path(p);data=p.read_bytes();return {'path':str(p),'sha256':hashlib.sha256(data).hexdigest(),'crc32':f'{zlib.crc32(data):08x}','bytes':len(data)}
def require(value,message):
 if not value:raise ValueError(message)
def verify(args):
 family='windows' if os.name=='nt' else 'linux';require(args.os_family==family,'Verifier must run on actual requested host')
 root=args.evidence_root;pre=load(root/'PRE.json');raw=load(root/'qualification-completed.json');freeze=load(args.source_freeze)
 require(pre['source_commit']==raw['source_commit']==freeze['source_commit']==SOURCE,'Source binding differs from actual a857 capture')
 require(raw['status']=='PASS','Original controller did not complete')
 require(raw['inputs_before']==raw['inputs_after']==pre['inputs'],'Original artifacts changed')
 require(raw['source_files_before']==raw['source_files_after']==pre['source_files'],'Original source changed')
 require(set(pre['source_files'])==set(freeze['files']),'Frozen runtime source inventory differs')
 for name,expected in freeze['files'].items():
  p=args.source_root/name;observed=descriptor(p);row=pre['source_files'][name]
  require(observed['sha256']==expected['sha256']==row['sha256'] and observed['bytes']==expected['bytes']==row['bytes'],'Retained source differs: '+name)
 for name,row in pre['inputs'].items():
  p=Path(name);require(digest(p)==row['sha256'] and p.stat().st_size==row['bytes'],'Retained original input differs: '+name)
 cli=Path(raw['actual_cli']['path']);require(digest(cli)==CLI[family]==raw['actual_cli']['sha256'],'Actual CLI identity differs')
 require(digest(args.source_root/'bindings/rust/nirs4all/src/bin/nirs4all-core-archive.rs')==CLI_SOURCE_SHA,'Actual CLI source differs')
 require(any(row['sha256']==METHODS[family] for row in pre['inputs'].values()),'Expected Methods139 artifact absent')
 require(any(row['sha256']==ARCHIVE_SHA for row in pre['inputs'].values()),'Original archive994 absent')
 require(len(raw['steps'])==len(EXPECTED[family]),'Original command count differs');seen=[];retained=[descriptor(root/'PRE.json'),descriptor(root/'qualification-completed.json'),descriptor(args.source_freeze)]
 for row in raw['steps']:
  name=row['id'];seen.append(name);require(name in EXPECTED[family],'Unexpected command '+name)
  require(type(row['direct_exit']) is int and row['direct_exit']==row['expected_exit']==EXPECTED[family][name],'Original exit differs '+name)
  require(datetime.datetime.fromisoformat(row['end_utc'])>=datetime.datetime.fromisoformat(row['start_utc']),'Original chronology reversed')
  require(row['remaining_owned_roots']==[],'CLI temporary roots remained')
  terminal=root/(name+'.terminal.json');require(load(terminal)==row,'Retained terminal differs '+name);retained.append(descriptor(terminal))
  for stream in ['stdout','stderr']:
   r=row['logs'][stream];p=Path(r['path']);require(digest(p)==r['sha256'] and p.stat().st_size==r['bytes'],'Retained log differs '+name);retained.append(descriptor(p))
 require(len(set(seen))==len(seen) and set(seen)==set(EXPECTED[family]),'Original inventory duplicated or incomplete')
 require(not list((root/'tmp').iterdir()),'Owned temporary root currently remains')
 inp=load(root/'input.json');require(inp=={'x':[[1.5,.5],[3.5,1.5]],'sample_ids':['predict.0','predict.1']},'Actual witness input differs')
 prediction=load(root/'output/predict.json');require(len(prediction['outputs'])==1 and len(prediction['outputs'][0]['predictions'])==1,'Prediction inventory differs');block=prediction['outputs'][0]['predictions'][0]
 require(block['values']==GOLDEN and block['sample_ids']==['predict.0','predict.1'] and block['target_names']==['protein','moisture'],'Actual four-cell historical witness differs')
 require(raw['numeric_witness']==block and raw['exact_prediction_cells']==4 and raw['refusals']==3 and raw['owned_roots_after_each_terminal']==0,'Controller final facts differ');retained += [descriptor(root/'input.json'),descriptor(root/'output/predict.json'),descriptor(root/'output/inspect.json')]
 windows_proof=None
 if family=='windows':
  history={}
  for name in ['native-prediction','after-load-output-refusal']:
   row=next(r for r in raw['steps'] if r['id']==name);states=row['observed_dll_snapshots'];require(any(r['readonly'] and r['attributes'] & stat.FILE_ATTRIBUTE_READONLY and r['bytes']==4174848 for r in states.values()),'Actual readonly DLL state absent '+name);history[name]=states
  require('invalid private CLI worker directory' in (root/'logs/junction-root-refusal.stderr.log').read_text(),'Actual junction root refusal absent')
  sentinel=root/'outside-owned-target/readonly.fixture';expected_sentinel=b'owned outside sentinel; must remain unchanged';require(sentinel.read_bytes()==expected_sentinel and sentinel.stat().st_file_attributes & stat.FILE_ATTRIBUTE_READONLY,'Outside sentinel content/readonly differs')
  junction=root/'nirs4all-core-archive-junction';require(junction.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT,'Retained junction missing');require(junction.resolve()==sentinel.parent.resolve(),'Retained junction points elsewhere')
  require((root/'logs/after-load-output-refusal.stderr.log').stat().st_size>0,'After-load refusal has no actual diagnostic');require(not (root/'output/refused.json').exists() and not (root/'output/junction-refused.json').exists(),'Refusal produced forbidden output')
  retained += [descriptor(root/'junction.stdout.log'),descriptor(root/'junction.stderr.log'),descriptor(sentinel)]
  windows_proof={'readonly_DLL_observed_in_success_and_after_load_refusal':True,'attribute_state_history':history,'outside_sentinel':descriptor(sentinel),'outside_sentinel_readonly':True,'sentinel_provenance':'Retained controller source creates this exact byte literal and asserts unchanged hash+attributes before/after the junction refusal; no separate original PRE sentinel receipt was emitted. Current verification confirms the exact literal and READONLY flag.','junction_reparse_point_retained':True,'actual_junction_refusal':True}
 else:
  for name in ['unit-success_closes_owned_snapshot_tree','unit-refusal_closes_owned_snapshot_tree_and_preserves_exit']:
   require('1 passed; 0 failed; 0 ignored' in (root/'logs'/f'{name}.stdout.log').read_text(),'Actual lifecycle unit failed')
  require('unknown archive command: --help' in (root/'logs/help-refusal.stderr.log').read_text(),'Original help refusal differs')
 return {'status':'PASS','platform':platform.platform(),'os_family':family,'source_commit':SOURCE,'source_sha256':CLI_SOURCE_SHA,'frozen_runtime_files':len(freeze['files']),'actual_cli_sha256':CLI[family],'actual_methods_sha256':METHODS[family],'archive_sha256':ARCHIVE_SHA,'measured_original_commands':len(seen),'expected_refusals':3,'owned_roots_after_each_terminal':0,'prediction_cells_exact':4,'predictions':block['values'],'Windows_observations':windows_proof,'retained_originals':retained,'scope':'Verification of retained original native lifecycle executions; no compilation or scientific execution replay. Original expected refusal exits remain1.'}
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--os-family',choices=['linux','windows'],required=True);p.add_argument('--evidence-root',type=Path,required=True);p.add_argument('--source-root',type=Path,required=True);p.add_argument('--source-freeze',type=Path,required=True);args=p.parse_args();print(json.dumps(verify(args),sort_keys=True))
if __name__=='__main__':main()
