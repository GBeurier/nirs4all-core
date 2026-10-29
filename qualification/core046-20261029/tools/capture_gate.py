"""Capture a direct Core qualification command on the actual requested host."""
from __future__ import annotations
import ctypes, hashlib, importlib.metadata, json, os, platform, socket, subprocess, sys, time
from datetime import UTC, datetime
from pathlib import Path

def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def write(path, data):
    Path(path).write_text(json.dumps(data, indent=2, allow_nan=False)+'\n', encoding='utf-8')

def observe(plan):
    rows=[]
    for spec in plan['artifacts']:
        p=Path(spec['artifact_path'])
        if not p.is_file() or p.is_symlink(): raise RuntimeError('Artifact is not a regular file: '+str(p))
        row={**spec,'sha256':sha(p),'bytes':p.stat().st_size}
        if spec.get('expected_sha256') and row['sha256']!=spec['expected_sha256']:raise RuntimeError('Artifact changed before run: '+str(p))
        rows.append(row)
    versions={name:importlib.metadata.version(name) for name in plan.get('expected_packages',{})}
    if versions!=plan.get('expected_packages',{}):raise RuntimeError('Wrong installed package cohort: '+repr(versions))
    cookies=[os.add_dll_directory(p) for p in plan.get('dll_dependency_directories',[])] if os.name=='nt' else []
    methods={}
    if plan.get('methods_library'):
        p=Path(plan['methods_library']); lib=ctypes.CDLL(str(p));lib.n4m_get_version_string.restype=ctypes.c_char_p
        abi=[]
        for name in ['n4m_get_abi_version_major','n4m_get_abi_version_minor','n4m_get_abi_version_patch']:
            f=getattr(lib,name);f.restype=ctypes.c_uint32;abi.append(f())
        methods={'path':str(p),'sha256':sha(p),'version':lib.n4m_get_version_string().decode(),'abi':abi}
        if methods['version']!='1.3.9+abi.2.20.0' or abi!=[2,20,0]:raise RuntimeError('Wrong actual native Methods runtime: '+repr(methods))
    tools={'python':platform.python_version()}
    for name,args in plan.get('tool_commands',{}).items():
        r=subprocess.run(args,capture_output=True,text=True,check=True);tools[name]=(r.stdout+r.stderr).strip()
    if os.name=='nt':
        class MemoryStatus(ctypes.Structure):
            _fields_=[('length',ctypes.c_ulong),('load',ctypes.c_ulong)]+[(n,ctypes.c_ulonglong)for n in ['total','available','page_total','page_available','virtual_total','virtual_available','extended']]
        status=MemoryStatus();status.length=ctypes.sizeof(status)
        if not ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):raise RuntimeError('Cannot observe actual Windows memory')
        ram=status.total
    else:ram=os.sysconf('SC_PAGE_SIZE')*os.sysconf('SC_PHYS_PAGES')
    return {'artifacts':rows,'versions':versions,'methods':methods,'host':{'id':plan['host'],'os_family':'windows'if os.name=='nt'else'linux','os':platform.platform(),'arch':platform.machine(),'hostname':socket.gethostname(),'cpu_count':os.cpu_count(),'ram_bytes':ram,'tools':tools}}

def main():
    plan=json.loads(Path(sys.argv[1]).read_text(encoding='utf-8')); root=Path(plan['source_root']);output=Path(plan['output']);output.mkdir(parents=True,exist_ok=False)
    if ('windows'if os.name=='nt'else'linux')!=plan['os_family']:raise RuntimeError('Must execute on the actual required host')
    os.environ.update(plan['environment'])
    expected=plan['runtime_inputs'];before={name:sha(root/name)for name in expected}
    if before!=expected:raise RuntimeError('Frozen source differs before execution')
    observation=observe(plan)
    identity={k:plan[k]for k in ['id','host','command','cwd','environment','input_fingerprints']};identity['started_at']=datetime.now(UTC).isoformat()
    write(output/'before.json',{'runtime_inputs':before,'observation':observation,'run_identity_before':identity,'runner_sha256':sha(__file__)})
    t=time.monotonic()
    with (output/'stdout.log').open('wb')as stdout,(output/'stderr.log').open('wb')as stderr:
        child=subprocess.run(plan['command'],cwd=plan['cwd'],env=os.environ.copy(),stdout=stdout,stderr=stderr,check=False)
    identity.update(finished_at=datetime.now(UTC).isoformat(),exit_code=child.returncode)
    after={name:sha(root/name)for name in expected};observed_after=observe(plan)
    if before!=after or observation!=observed_after:raise RuntimeError('Source or installed artifacts changed during execution')
    write(output/'terminal.json',{'run_identity':identity,'duration_seconds':time.monotonic()-t,'source_sha':plan['source_sha'],'runtime_inputs_before':before,'runtime_inputs_after':after,'observation_before':observation,'observation_after':observed_after,'runner_sha256':sha(__file__),'logs':{n:{'sha256':sha(output/n),'bytes':(output/n).stat().st_size}for n in ['stdout.log','stderr.log']}})
    print(json.dumps({'gate':plan['id'],'exit_code':child.returncode,'platform':sys.platform}),flush=True)
    return child.returncode

if __name__=='__main__':raise SystemExit(main())
