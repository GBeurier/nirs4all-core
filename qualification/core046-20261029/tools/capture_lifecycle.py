"""Fresh native CLI lifecycle qualification; never rewrites historical captures.

Config is supplied by the owner only after immutable Core/DAG source freeze.
Linux inventory replaces historical rustc-cache builds with honest Cargo builds.
"""
from __future__ import annotations
import argparse,datetime,hashlib,json,os,platform,stat,subprocess,sys,threading,time
from pathlib import Path

ARCHIVE_SHA='994252030ff80129d0431995bae53eb473082f05825b65714379262b72af13fa'
GOLDEN=[[1.6363636363636365,13.272727272727273],[2.4999999999999996,15.0]]
def digest(p):
 with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def stamp():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def write(p,data):
 with Path(p).open('x',encoding='utf-8') as f:json.dump(data,f,indent=2);f.write('\n')
def describe(p):return {'path':str(p),'sha256':digest(p),'bytes':Path(p).stat().st_size}
def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--config',type=Path,required=True);args=parser.parse_args();cfg=json.loads(args.config.read_text());windows=os.name=='nt';assert cfg['os_family']==('windows' if windows else 'linux')
 root=Path(cfg['evidence_root']);root.mkdir(exist_ok=False,parents=True)
 for name in ('tmp','output','logs'):(root/name).mkdir()
 repo=Path(cfg['source_root']);freeze=json.loads(Path(cfg['source_freeze']).read_text());expected=freeze['files'];assert freeze['source_commit']==cfg['source_commit']
 if (repo/'.git').exists():assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==cfg['source_commit']
 def sources():
  rows={name:describe(repo/name) for name in expected}
  for name,item in rows.items():assert item['sha256']==expected[name]['sha256'] and item['bytes']==expected[name]['bytes'],name
  return rows
 source_before=sources();archive=Path(cfg['archive']);methods=Path(cfg['methods']);assert digest(archive)==ARCHIVE_SHA;assert methods.is_file() and not methods.is_symlink();assert digest(methods)==cfg['methods_sha256'];cli=Path(cfg['cli']);inputs=[args.config,Path(cfg['source_freeze']),Path(__file__),archive,methods,Path(sys.executable)]
 inputs.extend(Path(name) for name in cfg.get('additional_inputs',[]))
 deps=Path(cfg['dll_dependencies']) if windows else None
 if windows:inputs += [cli,*sorted(deps.glob('*.dll'))];assert digest(cli)==cfg['cli_sha256']
 before={str(p):describe(p) for p in inputs};env=os.environ.copy()
 for key in list(env):
  if key.startswith(('N4M_','PLS4ALL_')) or key in ('NIRS4ALL_CORE_ARCHIVE_WORKER_ROOT','PYTHONPATH','PYTHONHOME'):env.pop(key,None)
 for key in ('TMP','TEMP','TMPDIR'):env[key]=str(root/'tmp')
 env.update(cfg.get('build_env',{}));cookie=None
 if windows:env['PATH']=str(deps)+os.pathsep+env.get('PATH','');cookie=os.add_dll_directory(str(deps))
 write(root/'PRE.json',{'observed_utc':stamp(),'host':platform.platform(),'python':sys.version,'source_commit':cfg['source_commit'],'source_files':source_before,'inputs':before,'expected_commands':5 if windows else 9,'historical_archive_unchanged':True,'golden_cells_unchanged':GOLDEN,'build_environment_explicit':cfg.get('build_env',{}),'cargo_version':subprocess.check_output([cfg.get('cargo','cargo'),'--version'],env=env,text=True).strip() if not windows else 'prebuilt CLI from owner receipt','rustc_version':subprocess.check_output([cfg.get('rustc','rustc'),'--version','--verbose'],env=env,text=True).strip() if not windows else 'prebuilt CLI from owner receipt'})
 rows=[]
 def run(name,command,expected_exit=0,custom_env=None):
  observations={};stop=threading.Event()
  def observe():
   while not stop.is_set():
    for p in (root/'tmp').rglob('*.dll'):
     try:m=p.lstat();observations[str(p)]={'bytes':m.st_size,'readonly':bool(m.st_file_attributes & stat.FILE_ATTRIBUTE_READONLY),'attributes':m.st_file_attributes,'observed_at':stamp()}
     except FileNotFoundError:pass
    stop.wait(.002)
  thread=threading.Thread(target=observe,daemon=True) if windows else None
  if thread:thread.start()
  start=stamp();t=time.monotonic();out=root/'logs'/f'{name}.stdout.log';err=root/'logs'/f'{name}.stderr.log'
  try:
   with out.open('xb') as o,err.open('xb') as e:r=subprocess.run([str(x) for x in command],cwd=repo,env=custom_env or env,stdout=o,stderr=e,timeout=cfg.get('command_timeout_seconds',1200))
  finally:
   stop.set()
   if thread:thread.join()
  remaining=sorted(p.name for p in (root/'tmp').iterdir());row={'id':name,'command':[str(x) for x in command],'cwd':str(repo),'start_utc':start,'end_utc':stamp(),'duration_seconds':time.monotonic()-t,'direct_exit':r.returncode,'expected_exit':expected_exit,'remaining_owned_roots':remaining,'observed_dll_snapshots':observations,'logs':{'stdout':describe(out),'stderr':describe(err)}};write(root/f'{name}.terminal.json',row);rows.append(row);assert r.returncode==expected_exit,row;assert not remaining,row;return row
 if not windows:
  cargo=cfg.get('cargo','cargo');build=[cargo,'build','--locked','--release','-p','nirs4all','--bin','nirs4all-core-archive'];run('cargo-build-cli',build)
  run('cargo-build-lifecycle-tests',[cargo,'test','--locked','--release','-p','nirs4all','--bin','nirs4all-core-archive','--no-run','--message-format=json'])
  messages=[]
  for line in (root/'logs/cargo-build-lifecycle-tests.stdout.log').read_text().splitlines():
   try:messages.append(json.loads(line))
   except ValueError:pass
  executables=[m['executable'] for m in messages if m.get('reason')=='compiler-artifact' and m.get('profile',{}).get('test') and m.get('executable')];assert len(executables)==1,executables;tests=Path(executables[0])
  for name in ('success_closes_owned_snapshot_tree','refusal_closes_owned_snapshot_tree_and_preserves_exit'):
   run('unit-'+name,[tests,'--exact','tests::'+name,'--nocapture']);assert '1 passed; 0 failed; 0 ignored' in (root/'logs'/f'unit-{name}.stdout.log').read_text()
  run('help-refusal',[cli,'--help'],1);assert 'unknown archive command: --help' in (root/'logs/help-refusal.stderr.log').read_text();run('noargs-refusal',[cli],1)
 input_path=root/'input.json';write(input_path,{'x':[[1.5,.5],[3.5,1.5]],'sample_ids':['predict.0','predict.1']})
 run('archive-inspect',[cli,'inspect','--archive',archive,'--output',root/'output/inspect.json'])
 prediction=[cli,'workflow-predict','--input',input_path,'--archive',archive,'--methods-library',methods,'--methods-library-sha256',cfg['methods_sha256'],'--run-id','run:cli:lifecycle','--output']
 success=run('native-prediction',prediction+[root/'output/predict.json']);actual=json.loads((root/'output/predict.json').read_text());assert len(actual['outputs'])==1 and len(actual['outputs'][0]['predictions'])==1;block=actual['outputs'][0]['predictions'][0];assert block['values']==GOLDEN and block['sample_ids']==['predict.0','predict.1'] and block['target_names']==['protein','moisture'],block
 bad=list(prediction);bad[bad.index('--methods-library-sha256')+1]='0'*64;run('wrong-hash-refusal',bad+[root/'output/refused.json'],1);assert not (root/'output/refused.json').exists()
 if windows:
  assert any(x['readonly'] and x['bytes']==methods.stat().st_size for x in success['observed_dll_snapshots'].values())
  directory=root/'output/refuse-directory';directory.mkdir();after_load=run('after-load-output-refusal',prediction+[directory],1);assert any(x['readonly'] and x['bytes']==methods.stat().st_size for x in after_load['observed_dll_snapshots'].values())
  outside=root/'outside-owned-target';outside.mkdir();sentinel=outside/'readonly.fixture';sentinel.write_bytes(b'owned outside sentinel; must remain unchanged');os.chmod(sentinel,stat.S_IREAD);sentinel_before={'sha256':digest(sentinel),'attributes':sentinel.stat().st_file_attributes};junction=root/'nirs4all-core-archive-junction';jr=subprocess.run(['cmd.exe','/d','/c','mklink','/J',str(junction),str(outside)],capture_output=True,timeout=30);(root/'junction.stdout.log').write_bytes(jr.stdout);(root/'junction.stderr.log').write_bytes(jr.stderr);assert jr.returncode==0 and junction.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT
  junction_env=env.copy()
  for key in ('TEMP','TMP','TMPDIR','NIRS4ALL_CORE_ARCHIVE_WORKER_ROOT'):junction_env[key]=str(junction)
  run('junction-root-refusal',[cli,'inspect','--archive',archive,'--output',root/'output/junction-refused.json'],1,junction_env);assert 'invalid private CLI worker directory' in (root/'logs/junction-root-refusal.stderr.log').read_text();assert sentinel_before=={'sha256':digest(sentinel),'attributes':sentinel.stat().st_file_attributes};assert not (root/'output/junction-refused.json').exists()
 after={str(p):describe(p) for p in inputs};source_after=sources();assert before==after and source_before==source_after;assert len(rows)==(5 if windows else 9);assert sum(r['direct_exit']==1 for r in rows)==3
 if cookie:cookie.close()
 write(root/'qualification-completed.json',{'status':'PASS','source_commit':cfg['source_commit'],'inputs_before':before,'inputs_after':after,'source_files_before':source_before,'source_files_after':source_after,'steps':rows,'actual_cli':describe(cli),'numeric_witness':block,'exact_prediction_cells':4,'refusals':3,'owned_roots_after_each_terminal':0,'historical_oracles_unchanged':True,'scope':'Actual native CLI build/tests/lifecycle and one historical archive prediction, not broad numerical conformance.'});print(json.dumps({'status':'PASS','os_family':cfg['os_family'],'measured_original_commands':len(rows),'expected_refusals':3,'owned_roots_after_each_terminal':0,'prediction_cells_exact':4,'predictions':block['values'],'source_sha256':digest(repo/'bindings/rust/nirs4all/src/bin/nirs4all-core-archive.rs'),'actual_cli_sha256':digest(cli),'actual_methods_sha256':digest(methods),'evidence':str(root)}))
if __name__=='__main__':main()
