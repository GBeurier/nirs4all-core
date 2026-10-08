"""Real Windows CLI success/refusal and owned-root cleanup observation."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import sys
import threading
import time

BASE = Path('C:/Temp/n4a-core045-20261008')
AUDIT = BASE / 'audit/cli-snapshot-lifecycle-windows-v1'
CLI = Path('C:/Temp/n4a-downstream041-20261008/core045-candidate79dc-artifacts-v1/core-windows-cli/nirs4all-core-archive.exe')
RUNTIME = Path('D:/nirs4all-release-qualification-20261007/strict-b345054ef9584759a3c1121e3658056b/application/resources/backend/python-runtime/python')
DLL = RUNTIME / 'Lib/site-packages/n4m/lib/n4m.dll'
DEPS = RUNTIME / 'Lib/site-packages/nirs4all_methods.libs'
ARCHIVE = BASE / 'fixtures/historical-web-archive-v2/multitarget-pls.n4a'
FREEZE = BASE / 'audit/core045-cli-lifecycle-source-v6/freeze.json'
WT = BASE / 'worktree'

def stamp():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()

def digest(p):
    with open(p, 'rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()

def write(p, obj):
    with p.open('x', encoding='utf-8') as handle:
        json.dump(obj, handle, indent=2)

def sources():
    expected = json.loads(FREEZE.read_text())['all_tracked_plus_new_source_files']
    actual = {name: {'bytes': (WT/name).stat().st_size, 'sha256': digest(WT/name)} for name in expected}
    assert all(actual[name]['sha256'] == proof['sha256'] and actual[name]['bytes'] == proof['bytes'] for name, proof in expected.items())
    return actual

def main():
    assert os.name == 'nt'
    AUDIT.mkdir(exist_ok=False)
    (AUDIT/'tmp').mkdir()
    (AUDIT/'output').mkdir()
    input_path = AUDIT/'input.json'
    input_path.write_bytes((BASE/'audit/cli-snapshot-lifecycle-linux-v2/input.json').read_bytes())
    inputs = [CLI, DLL, ARCHIVE, FREEZE, Path(__file__), input_path, Path(sys.executable)]
    inputs.extend(sorted(DEPS.glob('*.dll')))
    before = {str(p): {'bytes': p.stat().st_size, 'sha256': digest(p)} for p in inputs}
    assert digest(CLI) == '5df1198f4c3b830674391f60f478515143a4c8aa74e869941fb27551cb1730f7'
    assert digest(DLL) == 'daf98de3fe85b8c7ab9ce432f7e74a6a61f74f8ac3045ec0d8c5a2060ca912c9'
    assert digest(ARCHIVE) == '994252030ff80129d0431995bae53eb473082f05825b65714379262b72af13fa'
    source_before = sources()
    write(AUDIT/'PRE.json', {'created_at': stamp(), 'platform': platform.platform(), 'python': sys.version, 'executable': sys.executable, 'source_commit': '79dc40796a7ea8efba451806035d6336fee9b168', 'native_library_build_source': '923ffb2aa2d20f40d6bbe7d7fd72b85b65cb90cb', 'inputs': before, 'source_files': source_before})
    env = dict(os.environ)
    env.pop('NIRS4ALL_CORE_ARCHIVE_WORKER_ROOT', None)
    env['PATH'] = str(DEPS) + os.pathsep + env.get('PATH', '')
    for key in ('TEMP', 'TMP', 'TMPDIR'):
        env[key] = str(AUDIT/'tmp')
    cookie = os.add_dll_directory(str(DEPS))
    receipts = []

    def run(name, args, expected_exit, custom_env=None):
        observations = {}
        stopping = threading.Event()
        def observe():
            while not stopping.is_set():
                for directory, _, files in os.walk(AUDIT/'tmp', followlinks=False):
                    for filename in files:
                        p = Path(directory)/filename
                        if p.suffix.lower() == '.dll':
                            try:
                                metadata = p.lstat()
                                observations[str(p)] = {'bytes': metadata.st_size, 'file_attributes': metadata.st_file_attributes, 'readonly': bool(metadata.st_file_attributes & stat.FILE_ATTRIBUTE_READONLY), 'observed_at': stamp()}
                            except FileNotFoundError:
                                pass
                stopping.wait(.002)
        command = [str(CLI), *map(str, args)]
        started_at = stamp()
        started = time.perf_counter()
        thread = threading.Thread(target=observe, daemon=True)
        thread.start()
        try:
            with (AUDIT/(name+'.stdout.log')).open('xb') as stdout, (AUDIT/(name+'.stderr.log')).open('xb') as stderr:
                completed = subprocess.run(command, env=custom_env or env, stdout=stdout, stderr=stderr, timeout=60, check=False)
        finally:
            stopping.set()
            thread.join()
        ended_at = stamp()
        remaining = sorted(str(p.relative_to(AUDIT/'tmp')) for p in (AUDIT/'tmp').iterdir())
        receipt = {'command': command, 'started_at': started_at, 'ended_at': ended_at, 'elapsed_seconds': time.perf_counter()-started, 'direct_exit_code': completed.returncode, 'expected_exit_code': expected_exit, 'observed_dll_snapshots': observations, 'remaining_owned_tmp_entries': remaining, 'stdout_sha256': digest(AUDIT/(name+'.stdout.log')), 'stderr_sha256': digest(AUDIT/(name+'.stderr.log'))}
        write(AUDIT/(name+'.terminal.json'), receipt)
        receipts.append({'name': name, **receipt})
        assert completed.returncode == expected_exit, name
        assert not remaining, name
        return receipt

    prediction = ['workflow-predict', '--input', input_path, '--archive', ARCHIVE, '--methods-library', DLL, '--methods-library-sha256', digest(DLL), '--run-id', 'run:cli:lifecycle', '--output']
    run('archive-inspect', ['inspect', '--archive', ARCHIVE, '--output', AUDIT/'output/inspect.json'], 0)
    success = run('native-prediction', [*prediction, AUDIT/'output/predict.json'], 0)
    block = json.loads((AUDIT/'output/predict.json').read_text())['outputs'][0]['predictions'][0]
    expected = [[1.6363636363636365, 13.272727272727273], [2.4999999999999996, 15.0]]
    assert block['values'] == expected and block['sample_ids'] == ['predict.0', 'predict.1'] and block['target_names'] == ['protein', 'moisture']
    assert any(row['readonly'] and row['bytes'] == DLL.stat().st_size for row in success['observed_dll_snapshots'].values())
    wrong_hash = list(prediction)
    wrong_hash[wrong_hash.index('--methods-library-sha256')+1] = '0'*64
    run('wrong-hash-refusal', [*wrong_hash, AUDIT/'output/refused.json'], 1)
    assert not (AUDIT/'output/refused.json').exists()
    output_directory = AUDIT/'output/refuse-directory'
    output_directory.mkdir()
    after_load = run('after-load-output-refusal', [*prediction, output_directory], 1)
    assert any(row['readonly'] and row['bytes'] == DLL.stat().st_size for row in after_load['observed_dll_snapshots'].values())

    outside = AUDIT/'outside-owned-target'
    outside.mkdir()
    sentinel = outside/'readonly.fixture'
    sentinel.write_bytes(b'owned outside sentinel; must remain unchanged')
    os.chmod(sentinel, stat.S_IREAD)
    sentinel_before = {'sha256': digest(sentinel), 'attributes': sentinel.stat().st_file_attributes}
    junction = AUDIT/'nirs4all-core-archive-junction'
    junction_command = ['cmd.exe', '/d', '/c', 'mklink', '/J', str(junction), str(outside)]
    started_at = stamp()
    junction_result = subprocess.run(junction_command, capture_output=True, check=False, timeout=30)
    (AUDIT/'junction.stdout.log').write_bytes(junction_result.stdout)
    (AUDIT/'junction.stderr.log').write_bytes(junction_result.stderr)
    write(AUDIT/'junction.terminal.json', {'command': junction_command, 'started_at': started_at, 'ended_at': stamp(), 'direct_exit_code': junction_result.returncode})
    assert junction_result.returncode == 0
    assert junction.lstat().st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT
    junction_env = dict(env)
    for key in ('TEMP', 'TMP', 'TMPDIR', 'NIRS4ALL_CORE_ARCHIVE_WORKER_ROOT'):
        junction_env[key] = str(junction)
    run('junction-root-refusal', ['inspect', '--archive', ARCHIVE, '--output', AUDIT/'output/junction-refused.json'], 1, junction_env)
    assert 'invalid private CLI worker directory' in (AUDIT/'junction-root-refusal.stderr.log').read_text()
    assert sentinel_before == {'sha256': digest(sentinel), 'attributes': sentinel.stat().st_file_attributes}
    assert not (AUDIT/'output/junction-refused.json').exists()
    source_after = sources()
    after = {str(p): {'bytes': p.stat().st_size, 'sha256': digest(p)} for p in inputs}
    assert before == after and source_before == source_after
    cookie.close()
    write(AUDIT/'qualification-completed.json', {'created_at': stamp(), 'status': 'PASS', 'scope': 'Actual Windows CLI targeted lifecycle; no full scientific rerun. Reparse root refusal proves root guard, not adversarial traversal mutation. Direct in-process binding snapshots remain a separate limitation.', 'runs': receipts, 'numeric_witness': block, 'outside_sentinel_unchanged': sentinel_before, 'inputs_before': before, 'inputs_after': after, 'source_files_before': source_before, 'source_files_after': source_after})
    print(json.dumps({'status': 'PASS', 'runs': len(receipts), 'proof': str(AUDIT/'qualification-completed.json')}))

if __name__ == '__main__':
    main()
