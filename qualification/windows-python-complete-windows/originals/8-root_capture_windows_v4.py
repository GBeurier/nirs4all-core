import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import subprocess
import time

OWNER = Path('/mnt/c/Temp/n4a-core045-20261008')
ROOT = OWNER / 'worktree'
FREEZE = OWNER / 'audit/core045-test-only-source-v5/freeze.json'
PYTHON = '/mnt/d/nirs4all-release-qualification-20261007/strict-b345054ef9584759a3c1121e3658056b/application/resources/backend/python-runtime/python/python.exe'

def stamp():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def wsl_path(value):
    p = PureWindowsPath(value)
    assert p.drive.lower() in ('c:', 'd:') and p.is_absolute()
    return Path('/mnt') / p.drive[0].lower() / Path(*p.parts[1:])

def source_capture():
    entries = json.loads(FREEZE.read_text())['all_tracked_plus_new_source_files']
    result = {name: digest(ROOT / name) for name in entries}
    assert all(result[name] == row['sha256'] for name, row in entries.items())
    return result

parser = argparse.ArgumentParser()
parser.add_argument('gate', choices=('identity', 'generate-fixtures', 'python-oracle', 'python-complete'))
parser.add_argument('--config', type=Path, required=True)
parser.add_argument('--driver', type=Path, required=True)
parser.add_argument('--series', required=True)
args = parser.parse_args()
assert args.config.is_relative_to(OWNER / 'audit') and args.driver.is_relative_to(OWNER / 'audit')
assert args.series.replace('-', '').isalnum()
config = json.loads(args.config.read_text())
output = OWNER / 'local-windows-raw' / args.series / args.gate
output.mkdir(parents=True, exist_ok=False)
(OWNER / 'windows-owned-tmp').mkdir(exist_ok=True)
inputs = source_capture()
artifacts = []
for row in config['artifacts']:
    path = wsl_path(row['artifact_path'])
    assert digest(path) == row['sha256'] and path.stat().st_size == row['bytes']
    artifacts.append(dict(row, actual_wsl_path=str(path)))
def to_windows(path):
    return str(PureWindowsPath(path.parts[2].upper() + ':/', *path.parts[3:]))
code = 'import sys,runpy,os;n4a_dll_dir=r"D:/nirs4all-release-qualification-20261007/strict-b345054ef9584759a3c1121e3658056b/application/resources/backend/python-runtime/python/Lib/site-packages/nirs4all_methods.libs";n4a_dll_cookie=os.add_dll_directory(n4a_dll_dir);os.environ["PATH"]=n4a_dll_dir+os.pathsep+os.environ.get("PATH","");sys.path.insert(0,' + repr('C:\\Temp\\n4a-core045-20261008\\windows-pytest-site') + ');sys.argv=' + repr([to_windows(args.driver), '--config', to_windows(args.config), '--gate', args.gate]) + ';runpy.run_path(sys.argv[0],run_name="__main__")'
command = [PYTHON, '-X', 'utf8', '-B', '-c', code]
pre = {'id': args.gate, 'host': 'windows', 'execution': {'location': 'local', 'github_actions': False}, 'captured_at': stamp(), 'source_origin': 'adb71815926f05b1ab43a96179f91ead5243ebbd', 'binary_build_source_commit': '923ffb2aa2d20f40d6bbe7d7fd72b85b65cb90cb', 'source_tree': json.loads(FREEZE.read_text())['virtual_full_git_tree'], 'command': command, 'cwd': str(OWNER), 'input_fingerprints': inputs, 'driver': {'path': str(args.driver), 'sha256': digest(args.driver)}, 'config': {'path': str(args.config), 'sha256': digest(args.config)}, 'supervisor': {'path': str(Path(__file__)), 'sha256': digest(Path(__file__))}, 'artifacts_before': artifacts}
(output / 'PRE.json').write_text(json.dumps(pre, indent=2) + '\n')
start = time.monotonic()
started = stamp()
with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
    child = subprocess.Popen(command, cwd=OWNER, stdout=stdout, stderr=stderr)
    (output / 'active.json').write_text(json.dumps({'pid': child.pid, 'started_at': started, 'command': command}, indent=2) + '\n')
    exit_code = child.wait()
finished = stamp()
terminal = {'command': command, 'direct_command_exit_code': exit_code, 'started_at': started, 'finished_at': finished, 'seconds': time.monotonic() - start, 'pid': child.pid}
(output / 'direct-terminal.json').write_text(json.dumps(terminal, indent=2) + '\n')
raw = dict(pre, direct_terminal=terminal, exit_code=exit_code, input_fingerprints_after=source_capture(), artifacts_after=[dict(row, observed_sha256=digest(wsl_path(row['artifact_path']))) for row in config['artifacts']], logs={name: {'path': str(output / name), 'bytes': (output / name).stat().st_size, 'sha256': digest(output / name)} for name in ('stdout.log', 'stderr.log')})
assert all(row['observed_sha256'] == row['sha256'] for row in raw['artifacts_after'])
(output / 'raw-receipt.json').write_text(json.dumps(raw, indent=2) + '\n')
print(json.dumps({'id': args.gate, 'exit_code': exit_code, 'seconds': terminal['seconds'], 'receipt': str(output / 'raw-receipt.json')}), flush=True)
if exit_code:
    print((output / 'stderr.log').read_text(errors='replace')[-3500:], flush=True)
raise SystemExit(exit_code)
