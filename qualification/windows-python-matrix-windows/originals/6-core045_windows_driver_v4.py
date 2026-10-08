"""Run existing Core tests on actual Windows candidate bytes, with an owned site."""
import argparse
import ctypes
import hashlib
import importlib.metadata
import importlib.util
import json
import os
from pathlib import Path
import platform
import runpy
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--config", type=Path, required=True)
parser.add_argument("--gate", choices=("identity", "generate-fixtures", "python-oracle", "python-complete", "python-matrix"), required=True)
args = parser.parse_args()
config = json.loads(args.config.read_text())
root = Path(config["source_root"]).resolve()
site = Path(config["core_site"]).resolve()
sys.path[:0] = [str(site), config["dag_site"], *config.get("extra_sites", [])]
os.environ.update(config["environment"])
assert sys.platform == "win32"
assert platform.python_version() == "3.11.13"
import nirs4all_core
if args.gate != "python-complete":
    import nirs4all_core._native as core_native
assert nirs4all_core.__version__ == importlib.metadata.version("nirs4all-core") == "0.4.5"
assert importlib.metadata.version("dag-ml") == "0.3.41"
assert importlib.metadata.version("nirs4all-methods") == "1.3.4"
assert importlib.metadata.version("nirs4all-io") == "0.2.6"
core_path = Path(importlib.util.find_spec("nirs4all_core._native").origin).resolve() if args.gate == "python-complete" else Path(core_native.__file__).resolve()
dag_path = (Path(config["dag_site"]) / "dag_ml/_dag_ml.pyd").resolve()
assert core_path.is_relative_to(site)
assert dag_path.is_relative_to(Path(config["dag_site"]).resolve())
assert core_path.read_bytes()[:2] == dag_path.read_bytes()[:2] == b"MZ"
methods = Path(os.environ["N4M_LIBRARY_PATH"]).resolve()
lib = ctypes.CDLL(str(methods))
lib.n4m_get_version_string.restype = ctypes.c_char_p
engine = lib.n4m_get_version_string().decode()
assert engine == "1.3.4+abi.2.17.0"
observations = []
for row in config["artifacts"]:
    path = Path(row["artifact_path"]).resolve()
    with path.open("rb") as stream:
        observed = hashlib.file_digest(stream, "sha256").hexdigest()
    assert path.is_file() and not path.is_symlink()
    assert observed == row["sha256"] and path.stat().st_size == row["bytes"]
    observations.append(dict(row, observed_sha256=observed))
assert str(core_path) in [row["artifact_path"] for row in config["artifacts"]]
assert str(dag_path) in [row["artifact_path"] for row in config["artifacts"]]
assert str(methods) in [row["artifact_path"] for row in config["artifacts"]]
print("N4A_CORE_NATIVE_OBSERVATION=" + json.dumps(dict(platform=sys.platform, host=platform.node(),
    python=platform.python_version(), core_version=nirs4all_core.__version__, dag_version=importlib.metadata.version("dag-ml"),
    methods_engine=engine, native_module_preloaded=args.gate != "python-complete", io_version=importlib.metadata.version("nirs4all-io"), artifacts=observations), sort_keys=True), flush=True)
if args.gate == "identity":
    import dag_ml
    import dag_ml._dag_ml as dag_native
    assert dag_ml.version() == dag_native.version() == "0.3.41"
    assert Path(dag_native.__file__).resolve() == dag_path
elif args.gate == "generate-fixtures":
    target = root / "scripts/parity/generate_native_workflow_fixtures.py"
    sys.argv = [str(target), "--output", config["fixture_root"], "--cli", config["environment"]["NIRS4ALL_CORE_CLI"], "--methods-library", str(methods)]
    runpy.run_path(str(target), run_name="__main__")
elif args.gate == "python-oracle":
    target = root / "bindings/python/tests/test_execution_parity.py"
    sys.argv = [str(target), "-v"]
    runpy.run_path(str(target), run_name="__main__")
elif args.gate == "python-complete":
    sys.argv = ["pytest", "--capture=sys", "-q", str(root / "bindings/python/tests"),
        "--basetemp", config["pytest_tmp"], "--junitxml", config["junit_report"]]
    runpy.run_module("pytest", run_name="__main__")

elif args.gate == "python-matrix":
    target = root / "bindings/python/tests/test_archive_matrix_live.py"
    sys.argv = [str(target), "-v"]
    runpy.run_path(str(target), run_name="__main__")
