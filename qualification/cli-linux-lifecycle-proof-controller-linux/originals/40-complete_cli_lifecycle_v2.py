import datetime,hashlib,json,pathlib,subprocess
C=pathlib.Path('/mnt/c/Temp/n4a-core045-20261008');S=C/'audit/cli-snapshot-lifecycle-linux-v2';P=C/'audit/cli-snapshot-lifecycle-proposal-v2';W=C/'worktree'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
x=json.loads((S/'qualification.json').read_text());assert x['all_commands_expected_exit'] and len(x['steps'])==9
assert all(not r['temporary_roots_remaining'] for r in x['steps'])
for row in x['steps']:
    for log in row['logs'].values():assert sha(pathlib.Path(log['path']))==log['sha256']
pre=json.loads((S/'PRE.json').read_text())
for p,expected in pre['inputs'].items():assert sha(pathlib.Path(p))==expected['sha256']
web='/home/delete/nirs4all/nirs4all-web';ref='ff18fdc4199156b5a2dead90345b0d8521f6765d';name='web-app/src/engine/fixtures/archive-v2/README.md'
readme=subprocess.check_output(['git','-C',web,'show',ref+':'+name]);(S/'original-witness-README.md').write_bytes(readme)
import re
text=readme.decode();flat=json.loads(re.search(r'Expected row-major output:\s*`(\[[^`]+\])`',text).group(1));expected=[flat[:2],flat[2:]]
expected_x=json.loads(re.search(r'Replay input: `(\[[^`]+\])`',text).group(1))
assert json.loads((S/'input.json').read_text())['x']==expected_x
pred=x['real_prediction_observation'];assert len(pred['outputs'])==1
output=pred['outputs'][0];assert output['binding']['target_names']==['protein','moisture'];assert len(output['predictions'])==1
values=output['predictions'][0];assert values['target_names']==['protein','moisture'];assert values['sample_ids']==['predict.0','predict.1'];assert values['values']==expected
for gate in ['unit-success_closes_owned_snapshot_tree','unit-refusal_closes_owned_snapshot_tree_and_preserves_exit']:
    log=(S/'logs'/(gate+'.stdout.log')).read_text();assert '1 passed; 0 failed; 0 ignored' in log
assert not list((S/'runtime-tmp').iterdir())
assert subprocess.check_output(['git','-C',str(W),'-c','core.filemode=false','rev-parse','HEAD'],text=True).strip()=='adb71815926f05b1ab43a96179f91ead5243ebbd'
x['numeric_assertion_status']='PASS exact equality to original committed witness README; no tolerance'
x['numeric_witness']={'values':values['values'],'target_names':values['target_names'],'sample_ids':values['sample_ids'],'original_web_commit':ref,'original_readme_blob':name,'readme_sha256':sha(S/'original-witness-README.md'),'reference_observation_time':'read original immutable Git blob after executions; not claimed in PRE','archive_sha256':pre['inputs'][str(C/'fixtures/historical-web-archive-v2/multitarget-pls.n4a')]['sha256']}
x['completed_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();x['directbindings_and_Windows_qualified']=False
(S/'qualification-completed.json').write_text(json.dumps(x,indent=2)+'\n')
proofpaths=[p for p in S.rglob('*') if p.is_file() and 'output' not in p.relative_to(S).parts]
proofpaths += [C/'audit/validate_cli_lifecycle_v2.py',C/'audit/complete_cli_lifecycle_v2.py',P/'nirs4all-core-archive.rs',P/'proposal.json']
proofs=[{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size} for p in proofpaths]
freeze={'schema':'nirs4all.core.cli-lifecycle-prototype-linux.v1','source_proposal':str(P/'nirs4all-core-archive.rs'),'source_sha256':sha(P/'nirs4all-core-archive.rs'),'source_worktree_informational_head':'adb71815926f05b1ab43a96179f91ead5243ebbd','compiled_native_core_reused_unchanged':True,'compiled_native_core_not_rebuilt':True,'all_commands_and_numeric_checks_pass':True,'own_roots_after_terminal':0,'unit_tests_passed':2,'actual_native_prediction_cells_exact':4,'proofs':proofs,'artifacts':x['artifacts'],'product_source_modified':False,'requires_Sol_review_before_integration':True,'Windows_and_direct_host_binding_scope':'not qualified/fixed by this Linux CLI prototype','forced_supervisor_kill':'can leave owned temp directory, same limitation as ordinary TempDir; no global sweeper proposed'}
(S/'freeze.json').write_text(json.dumps(freeze,indent=2)+'\n')
print(json.dumps({'qualification_sha256':sha(S/'qualification-completed.json'),'freeze_sha256':sha(S/'freeze.json'),'source_sha256':freeze['source_sha256'],'proofs':len(proofs),'input_artifacts_verified_unchanged':len(pre['inputs']),'cli_sha256':sha(S/'output/nirs4all-core-archive'),'test_exe_sha256':sha(S/'output/cli-lifecycle-tests')}))
