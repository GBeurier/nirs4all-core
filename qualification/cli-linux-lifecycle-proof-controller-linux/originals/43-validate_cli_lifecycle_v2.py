import datetime,hashlib,json,os,pathlib,shutil,subprocess,time
C=pathlib.Path('/mnt/c/Temp/n4a-core045-20261008');P=C/'audit/cli-snapshot-lifecycle-proposal-v2';S=C/'audit/cli-snapshot-lifecycle-linux-v2';W=C/'worktree';assert not S.exists();S.mkdir()
for name in ['output','runtime-tmp','logs']:(S/name).mkdir()
SOURCE=P/'nirs4all-core-archive.rs';RUST=pathlib.Path('/home/delete/.cargo/bin/rustc');D=C/'target/release/deps';CORE=D/'libnirs4all-eb25aba5ce731694.rlib';TEMP=D/'libtempfile-c17a4fb8bb52058e.rlib';NATIVE=C/'target/release/build/zstd-sys-e5d23fd6f3a2046f/out'
ARCHIVE=C/'fixtures/historical-web-archive-v2/multitarget-pls.n4a';METHODS=pathlib.Path('/home/delete/nirs4all/_audits/2026-10-06-debt-extension/workflow/public-methods-134-full/n4m/lib/libn4m.so.2.17.0')
CAP=512*1024**2;MINFREE=16*1024**3
sha=lambda p:hashlib.file_digest(open(p,'rb'),'sha256').hexdigest()
write=lambda p,x:p.write_text(json.dumps(x,indent=2)+'\n')
def guard():
    size=sum(p.stat().st_size for p in S.rglob('*') if p.is_file());assert size<CAP;assert shutil.disk_usage(S).free>MINFREE;return size
assert sha(SOURCE)=='f9a807ea5d2856a45aa18eb52816e119737513eb146db6637c97ea67daf8a074'
assert sha(ARCHIVE)=='994252030ff80129d0431995bae53eb473082f05825b65714379262b72af13fa'
assert sha(METHODS)=='cd1b370691c599b6fed66d3e389cd99a9ded565de3ab58ac2a6dd744d1cf0385'
inputs=[SOURCE,CORE,TEMP,ARCHIVE,METHODS,C/'target/release/.fingerprint/nirs4all-eb25aba5ce731694/lib-nirs4all.json',C/'target/release/.fingerprint/tempfile-c17a4fb8bb52058e/lib-tempfile.json']
inputs+=sorted(D.glob('*.rlib'))+sorted(D.glob('*.so'))+sorted(NATIVE.glob('*.a'))
inputs=list(dict.fromkeys(inputs));before={str(p):{'sha256':sha(p),'bytes':p.stat().st_size} for p in inputs}
source_state=subprocess.check_output(['git','-C',str(W),'-c','core.filemode=false','rev-parse','HEAD'],text=True).strip()
pre={'observed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'inputs':before,'worktree_informational_head':source_state,'rlibs_origin':'owner sdk_final_inventory_publication identifies these exact release cache rlibs as Source923; this runner does not rebuild or relabel native library sources','rustc':subprocess.check_output([str(RUST),'--version','--verbose'],text=True),'runtime_provenance':'actual unchanged public Methods134 DLL and original stored Web ArchiveV2; new CLI prototype source only','source_product_modified':False}
write(S/'PRE.json',pre)
env=os.environ.copy();env.update(TMPDIR=str(S/'runtime-tmp'),TMP=str(S/'runtime-tmp'),TEMP=str(S/'runtime-tmp'),CARGO_INCREMENTAL='0')
steps=[]
def run(gate,cmd,expected=0):
    guard();start=datetime.datetime.now(datetime.timezone.utc).isoformat();t=time.monotonic();out=S/'logs'/(gate+'.stdout.log');err=S/'logs'/(gate+'.stderr.log')
    with out.open('xb') as o,err.open('xb') as e:r=subprocess.run([str(x) for x in cmd],cwd=S,env=env,stdout=o,stderr=e)
    row={'id':gate,'command':[str(x) for x in cmd],'cwd':str(S),'start_utc':start,'end_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'duration_seconds':time.monotonic()-t,'direct_exit':r.returncode,'expected_exit':expected,'logs':{stream:{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size} for stream,p in [('stdout',out),('stderr',err)]},'temporary_roots_remaining':sorted(p.name for p in (S/'runtime-tmp').iterdir())}
    write(S/(gate+'.terminal.json'),row);steps.append(row);assert r.returncode==expected,row;assert not row['temporary_roots_remaining'],row;return row
base=[RUST,'--edition=2021','-C','debuginfo=0','-C','strip=debuginfo','--extern','nirs4all='+str(CORE),'--extern','tempfile='+str(TEMP),'-L','dependency='+str(D),'-L','native='+str(NATIVE)]
cli=S/'output/nirs4all-core-archive';tests=S/'output/cli-lifecycle-tests'
run('rustc-cli',base+['--crate-name','nirs4all_core_archive',SOURCE,'-o',cli])
run('rustc-tests',base+['--test','--crate-name','nirs4all_core_archive',SOURCE,'-o',tests])
for name in ['success_closes_owned_snapshot_tree','refusal_closes_owned_snapshot_tree_and_preserves_exit']:
    run('unit-'+name,[tests,'--exact','tests::'+name,'--nocapture'])
run('original-help-refusal',[cli,'--help'],1)
assert 'unknown archive command: --help' in (S/'logs/original-help-refusal.stderr.log').read_text()
run('original-noargs-refusal',[cli],1)
inspect=S/'output/archive-inspect.json'
run('actual-archive-inspect',[cli,'inspect','--archive',ARCHIVE,'--output',inspect])
input_json=S/'input.json';write(input_json,{'x':[[1.5,0.5],[3.5,1.5]],'sample_ids':['predict.0','predict.1']})
pred=S/'output/predict.json';args=[cli,'workflow-predict','--input',input_json,'--archive',ARCHIVE,'--methods-library',METHODS,'--methods-library-sha256',before[str(METHODS)]['sha256'],'--run-id','run:cli:lifecycle','--output',pred]
run('actual-native-archive-prediction',args)
parsed=json.loads(pred.read_text());write(S/'actual-prediction-observation.json',parsed)
refusal=S/'output/refused.json';bad=args.copy();bad[bad.index('--methods-library-sha256')+1]='0'*64;bad[bad.index('--output')+1]=refusal
run('actual-native-wrong-hash-refusal',bad,1);assert not refusal.exists()
after={str(p):{'sha256':sha(p),'bytes':p.stat().st_size} for p in inputs};assert before==after
write(S/'qualification.json',{'all_commands_expected_exit':True,'input_fingerprints_before_after':before,'steps':steps,'artifacts':{str(p):{'sha256':sha(p),'bytes':p.stat().st_size} for p in [cli,tests,pred,inspect,input_json]},'source_product_modified':False,'native_core_or_wasm_rebuilt':False,'real_prediction_observation':parsed,'numeric_assertion_status':'pending explicit shape extraction against original golden; no success claim until checked','scope':'Linux prototypeCLI lifecycle/transport checks; direct host bindings unchanged, no full scientific replay','storage_bytes':guard()})
print(json.dumps({'commands':len(steps),'all_expected_exit':True,'storage_bytes':guard(),'prediction_keys':list(parsed),'source_state':source_state}))
