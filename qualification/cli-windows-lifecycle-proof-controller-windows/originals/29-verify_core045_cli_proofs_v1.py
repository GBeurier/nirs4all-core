#!/usr/bin/env python3
"""Verify already executed CLI lifecycle commands, including their authentic refusal exits."""
import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import re
from pathlib import Path

CLI_SOURCE_SHA='f9a807ea5d2856a45aa18eb52816e119737513eb146db6637c97ea67daf8a074'
GOLDEN=[[1.6363636363636365,13.272727272727273],[2.4999999999999996,15.0]]
ARCHIVE_SHA='994252030ff80129d0431995bae53eb473082f05825b65714379262b72af13fa'
LIFECYCLE_REL='bindings/rust/nirs4all/src/bin/nirs4all-core-archive.rs'
SOURCES={'linux':'cc96bff31e8855cf60be2279a30f887cefd9880cdf88cee0e3dcbf12dca9c20f','windows':'5df1198f4c3b830674391f60f478515143a4c8aa74e869941fb27551cb1730f7'}
METHODS={'linux':'cd1b370691c599b6fed66d3e389cd99a9ded565de3ab58ac2a6dd744d1cf0385','windows':'daf98de3fe85b8c7ab9ce432f7e74a6a61f74f8ac3045ec0d8c5a2060ca912c9'}
EXPECTED={'linux':{'rustc-cli':0,'rustc-tests':0,'unit-success_closes_owned_snapshot_tree':0,'unit-refusal_closes_owned_snapshot_tree_and_preserves_exit':0,'original-help-refusal':1,'original-noargs-refusal':1,'actual-archive-inspect':0,'actual-native-archive-prediction':0,'actual-native-wrong-hash-refusal':1},'windows':{'archive-inspect':0,'native-prediction':0,'wrong-hash-refusal':1,'after-load-output-refusal':1,'junction-root-refusal':1}}


def require(value,message):
    if not value:raise ValueError(message)


def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


def load(path):return json.loads(Path(path).read_text())


def descriptor(path):
    p=Path(path);return {'path':str(p),'sha256':digest(p),'bytes':p.stat().st_size}


def verify(evidence_root, source_root, os_family):
    require(('windows' if os.name=='nt' else 'linux')==os_family,'Verifier must execute on the actual requested host OS')
    pre=load(evidence_root/'PRE.json');raw=load(evidence_root/'qualification-completed.json')
    windows=os_family=='windows';steps=raw['runs'] if windows else raw['steps']
    require(len(steps)==len(EXPECTED[os_family]),'Actual command inventory differs')
    require(windows and raw['status']=='PASS' or not windows and raw['all_commands_expected_exit'] is True,'Original controller did not pass')
    observed=raw['inputs_before'] if windows else raw['input_fingerprints_before_after']
    require(observed==pre['inputs'],'Original PRE/artifact map differs')
    if windows:
        require(observed==raw['inputs_after'],'Windows artifacts changed during original commands')
        require(raw['source_files_before']==raw['source_files_after']==pre['source_files'],'Original Windows source changed')
    for path,row in observed.items():
        require(digest(path)==row['sha256'] and Path(path).stat().st_size==row['bytes'],'Retained original input differs: '+path)
    require(digest(source_root/LIFECYCLE_REL)==CLI_SOURCE_SHA,'Current CLI source is not the tested prototype')
    ids=[];retained=[descriptor(evidence_root/'PRE.json'),descriptor(evidence_root/'qualification-completed.json')]
    for row in steps:
        name=row['name'] if windows else row['id'];ids.append(name)
        require(name in EXPECTED[os_family],'Unexpected command')
        start=row['started_at'] if windows else row['start_utc'];finish=row['ended_at'] if windows else row['end_utc']
        require(dt.datetime.fromisoformat(finish)>=dt.datetime.fromisoformat(start),'Original command chronology reversed')
        direct=row['direct_exit_code'] if windows else row['direct_exit'];expected=row['expected_exit_code'] if windows else row['expected_exit']
        require(type(direct) is int and direct==expected==EXPECTED[os_family][name],'Original command exit differs')
        terminal=evidence_root/(name+'.terminal.json'); terminal_row={k:v for k,v in row.items() if not (windows and k=='name')}; require(load(terminal)==terminal_row,'Retained independent terminal differs')
        retained.append(descriptor(terminal))
        remaining=row['remaining_owned_tmp_entries'] if windows else row['temporary_roots_remaining']
        require(remaining==[],'Owned temporary roots remained after original command')
        for stream in ('stdout','stderr'):
            path=evidence_root/(name+'.'+stream+'.log') if windows else Path(row['logs'][stream]['path'])
            expected_hash=row[stream+'_sha256'] if windows else row['logs'][stream]['sha256']
            require(digest(path)==expected_hash,'Original command log changed')
            retained.append(descriptor(path))
    require(len(set(ids))==len(ids) and set(ids)==set(EXPECTED[os_family]),'Missing or duplicate original command')
    if not windows:
        for path,row in raw['artifacts'].items():
            require(digest(path)==row['sha256'] and Path(path).stat().st_size==row['bytes'],'Retained produced artifact differs')
    input_json=load(evidence_root/'input.json')
    require(input_json['x']==[[1.5,.5],[3.5,1.5]] and input_json['sample_ids']==['predict.0','predict.1'],'Actual input differs from original witness')
    actual=load(evidence_root/'output/predict.json')
    block=actual['outputs'][0]['predictions'][0]
    require(len(actual['outputs'])==1 and len(actual['outputs'][0]['predictions'])==1,'Prediction inventory differs')
    require(block['values']==GOLDEN and block['target_names']==['protein','moisture'] and block['sample_ids']==['predict.0','predict.1'],'Authentic golden prediction differs')
    retained.append(descriptor(evidence_root/'output/predict.json'))
    require(any(v['sha256']==ARCHIVE_SHA for v in observed.values()),'Authentic archive missing')
    require(any(v['sha256']==SOURCES[os_family] for v in observed.values()) if windows else any(v['sha256']==SOURCES[os_family] for v in raw['artifacts'].values()),'Actual tested CLI identity differs')
    require(any(v['sha256']==METHODS[os_family] for v in observed.values()),'Actual Methods134 identity differs')
    if windows:
        after_load=next(r for r in steps if r['name']=='after-load-output-refusal')
        require(after_load['observed_dll_snapshots'] and all(r['readonly'] is True for r in after_load['observed_dll_snapshots'].values()),'Missing genuine loaded readonly DLL refusal observation')
        require('invalid private CLI worker directory' in (evidence_root/'junction-root-refusal.stderr.log').read_text(),'Reparse root refusal evidence absent')
        require(raw['outside_sentinel_unchanged']['attributes'] & 1,'Readonly outside sentinel observation absent')
    else:
        for name in ('unit-success_closes_owned_snapshot_tree','unit-refusal_closes_owned_snapshot_tree_and_preserves_exit'):
            require('1 passed; 0 failed; 0 ignored' in (evidence_root/'logs'/(name+'.stdout.log')).read_text(),'Actual lifecycle unit did not pass')
        require('unknown archive command: --help' in (evidence_root/'logs/original-help-refusal.stderr.log').read_text(),'Original help refusal absent')
    return {'platform':platform.platform(),'os_family':os_family,'measured_original_commands':len(steps),'expected_refusals':sum(v==1 for v in EXPECTED[os_family].values()),'owned_roots_after_each_terminal':0,'prediction_cells_exact':4,'predictions':block['values'],'source_sha256':CLI_SOURCE_SHA,'actual_cli_sha256':SOURCES[os_family],'actual_methods_sha256':METHODS[os_family],'retained_originals':retained,'scope':'Validation of retained genuine local command captures; no compilation or scientific execution replay. Original negative direct exits remain1.'}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--os-family',choices=('linux','windows'),required=True);parser.add_argument('--evidence-root',type=Path,required=True);parser.add_argument('--source-root',type=Path,required=True)
    args=parser.parse_args();print(json.dumps(verify(args.evidence_root,args.source_root,args.os_family),sort_keys=True))


if __name__=='__main__':main()
