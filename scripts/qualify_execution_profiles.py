#!/usr/bin/env python3
"""Qualify the native execution-profile metadata contract and write a receipt.

Receipt schema: ``nirs4all.execution-profile-qualification.v1`` (separate from
``compat/local-qualification.json`` and its verifier, which this never touches).

The receipt reports the METADATA SOURCE contract: the TOML owner registry,
the data generated from it, the host APIs that return it and the entrypoint
definitions it names. Native SCIENTIFIC replay (libn4m, the archive CLI,
installed packages, live Methods WASM) is reported as separate steps and is
``unavailable`` when its dependencies are missing. Nothing here is numerical
parity or release certification.

Every step has a ``kind`` (``metadata_source`` or ``scientific_replay``) and
ends in exactly one status:

* ``executed``    - the command ran, passed, and a non-zero number of test bodies
                    ran. Skipped, TODO and expected-failure bodies are counted
                    separately and never as passed.
* ``unavailable`` - a required executable/dependency is missing, nothing ran, or
                    every body was skipped. An environment-gated script that
                    exits 0 without running its body is unavailable.
* ``unsupported`` - the registry declares the surface out of scope AND the host
                    API actually refuses it (probed, not self-declared).
* ``failed``      - the command failed or an executed assertion failed.

Any failed step makes the runner exit non-zero. ``--require-all`` also fails on
any unavailable step or skipped/TODO body.

Example::

    python3.11 scripts/qualify_execution_profiles.py \\
        --node ~/.nvm/versions/node/v24.16.0/bin/node \\
        --rscript ~/miniconda3/bin/Rscript --output /tmp/receipt.json
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = "nirs4all.execution-profile-qualification.v1"
SCOPE = (
    "execution-profile metadata contract (source assertions) plus separately reported native "
    "scientific replay; not numerical parity, not release or deployment certification"
)
STATUSES = ("executed", "unavailable", "unsupported", "failed")
KINDS = ("metadata_source", "scientific_replay")
TAIL_CHARS = 3000
MATLAB_SENTINEL = "nirs4all MATLAB/Octave smoke passed"
# The ONLY startup error that is a known, positively identified environment mismatch.
WEB_ASSET_MISMATCH = "Vitest DAG npm asset and staged browser asset must match authenticated DAG 0.3.44 bytes."
SCOPE_QUALI = (
    "consumer scientific regression of Quali against the Methods stage resolved by methods-stage.mjs; "
    "scientific-contract.test.mjs replaces external Core with a boundary stub, so this is not live Core integration"
)
R_SENTINEL = re.compile(r"^EXECUTED:execution-profiles cells=(\d+) entrypoints=(\d+)$", re.MULTILINE)
REMOVED_PYTHON = ("runtime_contracts", "artifact_contracts", "controller_capabilities", "required_keyword_registry_entries", "PORTABLE_OPERATOR_CLASSES")
REMOVED_JS = ("runtimeContracts", "artifactContracts", "controllerCapabilities", "requiredKeywordRegistryEntries", "portableOperatorClasses")

PY_TEST_FILES = [
    "bindings/python/tests/test_capability_matrix.py",
    "bindings/python/tests/test_cross_language_surface.py",
    "bindings/python/tests/test_facade.py",
]
# Core files whose content defines the metadata contract: registry, generator,
# generated data, API implementations, entrypoint sources and tests.
CORE_INPUTS = [
    "compat/capabilities.toml",
    "scripts/generate_capability_profiles.py",
    "scripts/qualify_execution_profiles.py",
    "scripts/test_qualify_execution_profiles.py",
    "tests/parity/expected/capability_profiles.json",
    "bindings/python/src/nirs4all_core/_capability_profiles.py",
    "bindings/python/src/nirs4all_core/_capabilities.py",
    "bindings/python/src/nirs4all_core/__init__.py",
    "bindings/python/src/nirs4all_core/_topology.py",
    "bindings/python/src/nirs4all_core/_native_pipeline.py",
    "bindings/python/src/nirs4all_core/_conformal.py",
    "bindings/wasm/src/capability-profiles.js",
    "bindings/wasm/src/index.js",
    "bindings/wasm/src/index.d.ts",
    "bindings/wasm/src/browser-native-pipeline.js",
    "bindings/wasm/src/conformal.js",
    "bindings/rust/nirs4all/src/capability_profiles_data.rs",
    "bindings/rust/nirs4all/src/lib.rs",
    "bindings/rust/nirs4all/src/pipeline_workflow.rs",
    "bindings/rust/nirs4all/src/native_pipeline_contract.rs",
    "bindings/rust/nirs4all/src/conformal.rs",
    "bindings/rust/nirs4all/src/conformal_json.rs",
    "bindings/matlab/+nirs4all/executionProfiles.m",
    "bindings/matlab/+nirs4all/executionProfilesJson.m",
    "bindings/matlab/+nirs4all/capabilityManifest.m",
    "bindings/matlab/+nirs4all/runtimeSurfaces.m",
    "bindings/matlab/+nirs4all/NativePipeline.m",
    "bindings/matlab/tests/smoke.m",
    "bindings/wasm/tests/index.test.js",
    "bindings/wasm/tests/types/consumer.ts",
    *PY_TEST_FILES,
    "docs/EXECUTION_PROFILES.md",
]
R_INPUTS = [
    "R/execution_profiles.R",
    "R/execution_profiles_data.R",
    "tests/execution-profiles.R",
    "NAMESPACE",
    "man/nirs4all_execution_profiles.Rd",
]
WEB_INPUTS = [
    "web-app/src/engine/nirs4all-core.ts",
    "web-app/src/engine/core-execution-profiles.generated.ts",
    "web-app/src/engine/nirs4all-core.test.ts",
    "web-app/src/app/custom-app-host.contract.test.ts",
]


def sha256_file(path: Path) -> str | None:
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None, timeout: int = 1800) -> dict[str, Any]:
    started = time.monotonic()
    merged = {**os.environ, **(env or {})}
    try:
        completed = subprocess.run(command, cwd=cwd, env=merged, capture_output=True, text=True, timeout=timeout, check=False)
        code, output = completed.returncode, completed.stdout + completed.stderr
    except subprocess.TimeoutExpired as error:
        code, output = 124, f"timeout after {timeout}s\n{error.stdout or ''}{error.stderr or ''}"
    except OSError as error:
        code, output = 127, f"cannot execute: {error}"
    return {"exit_code": code, "output": output, "duration_s": round(time.monotonic() - started, 3)}


def tail(text: str) -> str:
    return text[-TAIL_CHARS:]


def resolve_executable(explicit: str | None, names: tuple[str, ...]) -> str | None:
    """Return an executable path from an explicit option or PATH, else None."""

    if explicit:
        path = Path(explicit).expanduser()
        return str(path) if path.is_file() and os.access(path, os.X_OK) else None
    for name in names:
        found = shutil.which(name)
        if found:
            return found
    return None


def version_of(executable: str | None, flag: str = "--version") -> str | None:
    if executable is None:
        return None
    result = run([executable, flag], cwd=ROOT, timeout=60)
    first = result["output"].strip().splitlines()
    return first[0] if first and result["exit_code"] == 0 else None


def empty_tests() -> dict[str, list[str]]:
    return {"passed": [], "failed": [], "skipped": [], "todo": [], "expected_failures": []}


def parse_unittest(output: str) -> dict[str, list[str]]:
    """Per-test outcomes from ``unittest -v`` output."""

    pattern = re.compile(
        r"^(\S+) \(([\w.]+)\)(?:\n[^\n]*?)? \.\.\. (ok|FAIL|ERROR|skipped '.*?'|expected failure|unexpected success)$",
        re.MULTILINE,
    )
    tests = empty_tests()
    for _name, owner, outcome in pattern.findall(output):
        if outcome == "ok":
            tests["passed"].append(owner)
        elif outcome == "expected failure":
            tests["expected_failures"].append(owner)
        elif outcome.startswith("skipped"):
            tests["skipped"].append(f"{owner}: {outcome[8:-1]}")
        else:
            tests["failed"].append(owner)
    return tests


def parse_tap(output: str) -> dict[str, list[str]]:
    """Top-level test outcomes from ``node --test --test-reporter=tap`` output."""

    tests = empty_tests()
    for line in output.splitlines():
        match = re.match(r"^(not )?ok \d+ - (.*?)(?: # (SKIP|TODO)\b(.*))?$", line)
        if not match:
            continue
        negative, name, directive, reason = match.groups()
        if directive == "SKIP":
            tests["skipped"].append(f"{name}:{reason or ''}".rstrip(":"))
        elif directive == "TODO":
            tests["todo"].append(name)
        elif negative:
            tests["failed"].append(name)
        else:
            tests["passed"].append(name)
    return tests


def new_step(step_id: str, kind: str, surface: str, command: list[str] | None, executable: str | None) -> dict[str, Any]:
    return {"id": step_id, "kind": kind, "surface": surface, "status": "unavailable", "reason": None,
            "command": command, "executable": executable, "exit_code": None, "duration_s": 0.0,
            "evidence": "test_bodies", "counts": None, "skipped_tests": [], "todo_tests": [], "output_tail": ""}


def step_from_tests(step: dict[str, Any], result: dict[str, Any], tests: dict[str, list[str]]) -> dict[str, Any]:
    """Classify a finished test command; only passed bodies count as success."""

    counts = {key: len(values) for key, values in tests.items()}
    step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], counts=counts,
                skipped_tests=tests["skipped"], todo_tests=tests["todo"], output_tail=tail(result["output"]))
    if result["exit_code"] != 0 or tests["failed"]:
        step.update(status="failed", reason=f"exit code {result['exit_code']}; failed tests: {tests['failed'][:5]}")
    elif counts["passed"] == 0:
        step.update(status="unavailable", reason="no test body passed (all skipped/TODO/expected-failure, or none collected)")
    else:
        step.update(status="executed", reason=None)
    return step


def unavailable(step: dict[str, Any], reason: str) -> dict[str, Any]:
    step.update(status="unavailable", reason=reason)
    return step


def classify_web_vitest(step: dict[str, Any], result: dict[str, Any], report: Path) -> dict[str, Any]:
    """Classify a finished Web vitest run. Only the exact authenticated-asset mismatch is unavailable."""

    step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]))
    if result["exit_code"] != 0:
        if WEB_ASSET_MISMATCH in result["output"] and not report.is_file():
            return unavailable(step, "staged/installed DAG asset mismatch reported by the unmodified Web vitest config; run npm ci in web-app")
        step.update(status="failed", reason=f"vitest exited {result['exit_code']}: startup, config, source or assertion failure")
        return step
    if not report.is_file():
        step.update(status="failed", reason="vitest exited 0 but wrote no JSON report")
        return step
    try:
        data = json.loads(report.read_text(encoding="utf-8"))
        suites = data["testResults"]
        tests = empty_tests()
        for suite in suites:
            for case in suite["assertionResults"]:
                bucket = {"passed": "passed", "failed": "failed", "pending": "skipped", "skipped": "skipped", "todo": "todo"}.get(case["status"], "failed")
                tests[bucket].append(case["fullName"])
    except (OSError, ValueError, KeyError, TypeError) as error:
        step.update(status="failed", reason=f"malformed vitest report: {error!r}")
        return step
    return step_from_tests(step, result, tests)


def finalize_evidence(step: dict[str, Any]) -> dict[str, Any]:
    """Safety net: an executed step needs exit code 0 and a non-zero passed count of its evidence kind."""

    if step["status"] != "executed":
        return step
    counts = step.get("counts") or {}
    passed = counts.get("passed", counts.get("checks_passed", counts.get("scripts_passed", 0)))
    if step.get("exit_code") != 0 or not passed:
        step.update(status="failed", reason=f"executed step lacks exit code 0 or a non-zero passed count ({step.get('exit_code')}, {passed})")
    return step


def git_state(repo: Path) -> dict[str, Any]:
    """Source revision plus dirty-content identity (not just the status paths)."""

    def git(*args: str, binary: bool = False) -> Any:
        proc = subprocess.run(["git", *args], cwd=repo, capture_output=True, check=False)
        if proc.returncode != 0:
            return None
        return proc.stdout if binary else proc.stdout.decode("utf-8", "replace").strip()

    status = git("status", "--porcelain")
    digest = hashlib.sha256()
    tracked = git("diff", "HEAD", "--binary", binary=True)
    digest.update(tracked or b"")
    untracked = git("ls-files", "--others", "--exclude-standard") or ""
    hashed_untracked = 0
    for name in sorted(filter(None, untracked.splitlines())):
        path = repo / name
        if path.is_file() and path.stat().st_size <= 64 * 1024 * 1024:
            digest.update(name.encode() + b"\0" + hashlib.sha256(path.read_bytes()).digest())
            hashed_untracked += 1
    return {
        "path": str(repo),
        "revision": git("rev-parse", "HEAD"),
        "branch": git("rev-parse", "--abbrev-ref", "HEAD"),
        "dirty": bool(status),
        "changed_paths": len(status.splitlines()) if status else 0,
        "status_sha256": hashlib.sha256((status or "").encode()).hexdigest() if status is not None else None,
        "working_tree_content_sha256": digest.hexdigest() if tracked is not None else None,
        "untracked_files_hashed": hashed_untracked,
    }


def generator_command(python: str, r_root: Path, web_root: Path, r_present: bool, web_present: bool,
                      r_required: bool, web_required: bool) -> list[str]:
    """Drift-check command; a REQUIRED root is always passed with --require-* so absence fails (exit 2)."""

    command = [python, "scripts/generate_capability_profiles.py", "--check"]
    if r_present or r_required:
        command += ["--r-root", str(r_root)]
    if web_present or web_required:
        command += ["--web-root", str(web_root)]
    if r_required:
        command.append("--require-r")
    if web_required:
        command.append("--require-web")
    return command


def load_generator():
    spec = importlib.util.spec_from_file_location("generate_capability_profiles", ROOT / "scripts/generate_capability_profiles.py")
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def python_env(extra_pythonpath: str = "") -> dict[str, str]:
    """The exact PYTHONPATH used by both the native import probe and the test command."""

    parts = [str(ROOT / "bindings/python/src"), os.environ.get("NIRS4ALL_METHODS_PYTHONPATH", ""), extra_pythonpath]
    return {"PYTHONPATH": os.pathsep.join(part for part in parts if part), "PYTHONDONTWRITEBYTECODE": "1"}


def js_refusal_probe(node: str, node_env: dict[str, str]) -> dict[str, Any]:
    script = (
        "import * as core from './src/index.js';"
        f"const removed = {json.dumps(list(REMOVED_JS))};"
        "const present = removed.filter((name) => name in core);"
        "const keys = Object.keys(core.capabilityManifest()).sort();"
        "const ok = present.length === 0 && JSON.stringify(keys) === JSON.stringify(['aggregate','executionProfiles','runtimeSurfaces','schema']);"
        "console.log(JSON.stringify({ok, present, keys}));"
        "process.exit(ok ? 0 : 3);"
    )
    return run([node, "--input-type=module", "-e", script], cwd=ROOT / "bindings/wasm", env=node_env, timeout=120)


def qualify(args: argparse.Namespace) -> dict[str, Any]:
    python = args.python or sys.executable
    node = resolve_executable(args.node, ("node",))
    rscript = resolve_executable(args.rscript, ("Rscript",))
    octave = resolve_executable(args.octave, ("octave", "octave-cli"))
    matlab = resolve_executable(args.matlab, ("matlab",))
    cargo = resolve_executable(args.cargo, ("cargo",)) if args.run_cargo else None
    r_root = Path(args.r_root).expanduser() if args.r_root else ROOT.parent / "nirs4all-r"
    web_root = Path(args.web_root).expanduser() if args.web_root else ROOT.parent / "nirs4all-web"
    quali_root = Path(args.quali_root).expanduser() if args.quali_root else None
    r_present = (r_root / "DESCRIPTION").is_file()
    web_present = (web_root / "web-app/package.json").is_file()
    wasm = ROOT / "bindings/wasm"
    node_env = {"PATH": f"{Path(node).parent}{os.pathsep}{os.environ.get('PATH', '')}"} if node else {}
    steps: list[dict[str, Any]] = []
    meta, sci = "metadata_source", "scientific_replay"

    # 1. Generated host data equals the TOML owner (the drift gate, optional targets included).
    r_required = bool(args.r_root) or args.require_all
    web_required = bool(args.web_root) or args.require_all
    command = generator_command(python, r_root, web_root, r_present, web_present, r_required, web_required)
    step = new_step("generator_check", meta, "all-hosts", command, python)
    result = run(command, cwd=ROOT)
    optional_unavailable = [line for line in result["output"].splitlines() if line.startswith("unavailable ")]
    ok_targets = [line for line in result["output"].splitlines() if line.startswith("ok ")]
    needed = 5 + (1 if r_required else 0) + (1 if web_required else 0)
    if result["exit_code"] == 0 and len(ok_targets) < needed:
        result = {**result, "exit_code": 1}  # fewer targets than required can never count as executed
    step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]),
                evidence="validation_check", optional_targets_unavailable=optional_unavailable,
                counts={"checks_passed": len(ok_targets)},
                status="executed" if result["exit_code"] == 0 and ok_targets else "failed",
                reason=None if result["exit_code"] == 0 and ok_targets else "generated host data drifted from compat/capabilities.toml (or no target checked)")
    steps.append(step)

    # 2. Exact registry set and retired-scope reverse grep, evaluated in process.
    generator = load_generator()
    payload = generator.build_payload(generator.load_registry())
    step = new_step("registry_exact_set", meta, "all-hosts", None, python)
    problems = []
    expected_profiles = ["native-workflow", "native-conformal", "native-methods-package", "native-n4a-canonical", "n4a-container"]
    if [p["id"] for p in payload["profiles"]] != expected_profiles:
        problems.append("profile set is not exactly " + ", ".join(expected_profiles))
    if [p["version"] for p in payload["profiles"]] != [1] * len(expected_profiles) or payload["version"] != 1:
        problems.append("versions are not 1")
    retired = ("legacy", "wasm-methods-extension", "portable_pipeline", "runtime_contract", "parity-validated", "conformal_store")
    toml_text = (ROOT / "compat/capabilities.toml").read_text(encoding="utf-8")
    payload_text = json.dumps(payload)
    for term in retired:
        if term in toml_text:
            problems.append(f"retired term in registry: {term}")
        if term in payload_text:
            problems.append(f"retired term in payload: {term}")
    checks = 2 + 2 * len(retired)
    step.update(evidence="in_process_assertions", exit_code=0 if not problems else 1,
                counts={"checks_passed": checks - len(problems), "checks_failed": len(problems)},
                status="failed" if problems else "executed", reason="; ".join(problems) or None)
    steps.append(step)

    # 3. Python structural tests (TOML owner vs host data vs live manifests vs entrypoint sources).
    command = [python, "-m", "unittest", "-v", *PY_TEST_FILES]
    step = new_step("python_metadata_tests", meta, "python", command, python)
    result = run(command, cwd=ROOT, env=python_env())
    steps.append(step_from_tests(step, result, parse_unittest(result["output"])))

    # 4. Python removed-API refusal: probe the real facades.
    probe_src = (
        "import json, nirs4all_core as c, n4a;"
        f"names = {list(REMOVED_PYTHON)!r};"
        "present = [n for n in names if hasattr(c, n) or hasattr(n4a, n)];"
        "keys = sorted(c.capability_manifest());"
        "ok = not present and keys == ['aggregate','execution_profiles','runtime_surfaces','schema'];"
        "print(json.dumps({'ok': ok, 'present': present, 'keys': keys}));"
        "raise SystemExit(0 if ok else 3)"
    )
    step = new_step("python_removed_api_refusal", meta, "python", [python, "-c", "<probe>"], python)
    result = run([python, "-c", probe_src], cwd=ROOT, env=python_env())
    step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]))
    if result["exit_code"] == 0:
        step.update(status="unsupported", reason="the Python facade refuses the retired capability APIs (probed absent)")
    else:
        step.update(status="failed", reason="a retired capability API is still exposed by the Python facade")
    steps.append(step)

    # 5. Python native Methods replay (needs numpy + n4m + pls4all + libn4m).
    env = python_env()
    command = [python, "-m", "unittest", "-v", "bindings/python/tests/test_execution_parity.py"]
    step = new_step("python_native_replay", sci, "python", command, python)
    probe = run([python, "-c", "import numpy, n4m, pls4all"], cwd=ROOT, env=env, timeout=120)
    if probe["exit_code"] != 0:
        step["output_tail"] = tail(probe["output"])
        steps.append(unavailable(step, "native Methods Python binding (numpy, n4m, pls4all) is not importable with the test PYTHONPATH"))
    else:
        result = run(command, cwd=ROOT, env={**env, "NIRS4ALL_CORE_REQUIRE_METHODS_PARITY": "1"})
        steps.append(step_from_tests(step, result, parse_unittest(result["output"])))

    # 6. JavaScript/WASM.
    if node is None:
        for step_id, kind in (("js_metadata_tests", meta), ("js_removed_api_refusal", meta), ("js_runner_regression", sci), ("js_typecheck", meta)):
            steps.append(unavailable(new_step(step_id, kind, "javascript_wasm", None, None), "node executable not found (use --node)"))
    else:
        command = [node, "--test", "--test-reporter=tap", "tests/index.test.js", "tests/browser-native-pipeline.test.js"]
        step = new_step("js_metadata_tests", meta, "javascript_wasm", command, node)
        result = run(command, cwd=wasm, env=node_env)
        steps.append(step_from_tests(step, result, parse_tap(result["output"])))

        step = new_step("js_removed_api_refusal", meta, "javascript_wasm", [node, "--input-type=module", "-e", "<probe>"], node)
        result = js_refusal_probe(node, node_env)
        step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]))
        if result["exit_code"] == 0:
            step.update(status="unsupported", reason="the JavaScript/WASM entry refuses the retired capability APIs (probed absent)")
        else:
            step.update(status="failed", reason="a retired capability API is still exposed by the JavaScript/WASM entry")
        steps.append(step)

        # The temporary portable runner/compiler regression: proves live callers still run. It is
        # reported as scientific replay and never as a supported profile.
        command = [node, "--test", "--test-reporter=tap", "tests/execution.test.js", "tests/parity.test.js"]
        step = new_step("js_runner_regression", sci, "javascript_wasm", command, node)
        result = run(command, cwd=wasm, env=node_env)
        steps.append(step_from_tests(step, result, parse_tap(result["output"])))

        tsc = wasm / "node_modules/typescript/bin/tsc"
        command = [node, str(tsc), "--project", "tsconfig.typecheck.json"]
        step = new_step("js_typecheck", meta, "javascript_wasm", command, node)
        if not tsc.is_file():
            steps.append(unavailable(step, "typescript is not installed in bindings/wasm/node_modules"))
        else:
            result = run(command, cwd=wasm, env=node_env)
            step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]),
                        evidence="validation_check", counts={"checks_passed": 1 if result["exit_code"] == 0 else 0},
                        status="executed" if result["exit_code"] == 0 else "failed",
                        reason=None if result["exit_code"] == 0 else "tsc reported errors")
            steps.append(step)

    # 7. Rust targeted profile tests (opt-in: they compile the whole crate).
    filters = ["tests::execution_profiles", "tests::exactly_the_native", "tests::rust_entrypoints", "tests::r_is_federated"]
    command = ["cargo", "test", "--offline", "-p", "nirs4all", "--lib", "--", *filters]
    step = new_step("rust_profile_tests", meta, "rust", command, cargo)
    if cargo is None:
        steps.append(unavailable(step, "not requested (pass --run-cargo [--cargo PATH] [--cargo-target-dir DIR]) or cargo not found; "
                                       "targeted command: cargo test -p nirs4all --lib -- " + " ".join(filters)))
    else:
        command[0] = cargo
        step["command"] = command
        env = {"CARGO_TARGET_DIR": args.cargo_target_dir} if args.cargo_target_dir else {}
        result = run(command, cwd=ROOT, env=env, timeout=3600)
        ran = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored", result["output"])
        tests = empty_tests()
        if ran:
            tests["passed"] = ["rust"] * int(ran.group(1))
            tests["failed"] = ["rust"] * int(ran.group(2))
            tests["skipped"] = ["rust"] * int(ran.group(3))
        steps.append(step_from_tests(step, result, tests))

    # 8. R source metadata assertions (real R, no package install needed).
    command = [rscript or "Rscript", "tests/execution-profiles.R"]
    step = new_step("r_source_metadata", meta, "r (external)", command, rscript)
    if rscript is None:
        steps.append(unavailable(step, "Rscript not found (use --rscript)"))
    elif not r_present:
        steps.append(unavailable(step, f"no nirs4all-r checkout at {r_root} (use --r-root)"))
    else:
        result = run(command, cwd=r_root, env={"NIRS4ALL_R_SOURCE": str(r_root)})
        step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]))
        match = R_SENTINEL.search(result["output"])
        if result["exit_code"] != 0:
            step.update(status="failed", reason="R metadata assertions failed")
        elif match is None or int(match.group(1)) == 0:
            step.update(status="failed", reason="exit 0 without the EXECUTED sentinel carrying a non-zero cell count: assertions did not all run")
        else:
            step.update(status="executed", reason=None, evidence="in_process_assertions",
                        counts={"checks_passed": int(match.group(1)) + int(match.group(2))})
        steps.append(step)

    # 9. R native replay needs the installed package + n4m. A script that exits 0 without printing
    #    EXECUTED: was gated off and counts as unavailable, never executed.
    step = new_step("r_native_replay", sci, "r (external)", None, rscript)
    if rscript is None or not r_present:
        steps.append(unavailable(step, "Rscript or nirs4all-r checkout not available"))
    else:
        probe_cmd = [rscript, "-e", "quit(status = if (requireNamespace('nirs4all', quietly = TRUE) && requireNamespace('n4m', quietly = TRUE)) 0L else 3L)"]
        probe = run(probe_cmd, cwd=r_root, timeout=300)
        step["command"] = probe_cmd
        if probe["exit_code"] != 0:
            step["output_tail"] = tail(probe["output"])
            steps.append(unavailable(step, "installed R packages nirs4all and n4m are required for native replay (default library lacks them)"))
        else:
            script = "tests/conformal-native.R"
            command = [rscript, script]
            step["command"] = command
            result = run(command, cwd=r_root, env={"NIRS4ALL_R_SOURCE": str(r_root)})
            step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]))
            if result["exit_code"] != 0:
                step.update(status="failed", reason=f"{script} failed")
            elif "EXECUTED:" not in result["output"]:
                step.update(status="unavailable", reason=f"{script} exited 0 without an EXECUTED: sentinel; its body is environment-gated and did not run")
            else:
                step.update(status="executed", reason=None)
            steps.append(step)

    # 10. Web metadata snapshot wrapper tests (the official vitest config authenticates staged assets).
    step = new_step("web_metadata_tests", meta, "web (consumer)", None, node)
    vitest = web_root / "web-app/node_modules/vitest/vitest.mjs"
    if node is None or not web_present:
        steps.append(unavailable(step, "node executable or nirs4all-web checkout not available"))
    elif not vitest.is_file():
        steps.append(unavailable(step, "vitest is not installed in nirs4all-web/web-app/node_modules (run npm ci)"))
    else:
        report = Path(args.output).expanduser().with_suffix(".web-vitest.json")
        report.unlink(missing_ok=True)  # a stale report must never be reused
        command = [node, str(vitest), "run", "--config", "vitest.config.ts", "--reporter=json",
                   f"--outputFile={report}", "src/engine/nirs4all-core.test.ts", "src/app/custom-app-host.contract.test.ts"]
        step["command"] = command
        result = run(command, cwd=web_root / "web-app", env=node_env)
        steps.append(classify_web_vitest(step, result, report))
        report.unlink(missing_ok=True)

    # 11. Quali scientific regressions against the Methods stage resolved by the production helper.
    step = new_step("quali_scientific_regressions", sci, "quali (consumer)", None, node)
    step["scope_note"] = SCOPE_QUALI
    if node is None or quali_root is None:
        steps.append(unavailable(step, "pass --quali-root to run the Quali scientific regressions"))
    else:
        quali_env = dict(node_env)
        if args.quali_methods_wasm_root:  # explicit local override only when supplied
            quali_env["NIRS4ALL_METHODS_WASM_ROOT"] = args.quali_methods_wasm_root
        stage_cmd = [node, "--input-type=module", "-e",
                     "import('./scripts/methods-stage.mjs').then(m => m.qualifiedMethodsStage())"
                     ".then(s => console.log(JSON.stringify(s))).catch(e => { console.error(e); process.exit(3) })"]
        stage = run(stage_cmd, cwd=quali_root / "app", env=quali_env, timeout=300)
        command = [node, "--test", "--test-reporter=tap", "scripts/scientific-contract.test.mjs"]
        step["command"] = command
        if stage["exit_code"] != 0:
            step.update(exit_code=stage["exit_code"], output_tail=tail(stage["output"]), status="failed",
                        reason="the Methods stage resolved by methods-stage.mjs is not qualified (stale or unloadable)")
            steps.append(step)
        else:
            try:
                step["methods_stage"] = json.loads(stage["output"].strip().splitlines()[-1])
            except (ValueError, IndexError):
                step["methods_stage"] = None
            result = run(command, cwd=quali_root / "app", env=quali_env)
            steps.append(step_from_tests(step, result, parse_tap(result["output"])))

    # 12. MATLAB and Octave are reported separately; Octave never stands in for MATLAB.
    for step_id, executable, argv in (
        ("octave_smoke", octave, lambda exe: [exe, "--quiet", "--eval", "run('bindings/matlab/tests/smoke.m')"]),
        ("matlab_smoke", matlab, lambda exe: [exe, "-batch", "run('bindings/matlab/tests/smoke.m')"]),
    ):
        step = new_step(step_id, meta, "matlab_octave", argv(executable) if executable else None, executable)
        if executable is None:
            steps.append(unavailable(step, f"{step_id.split('_')[0]} executable not found (use --{step_id.split('_')[0]})"))
            continue
        result = run(argv(executable), cwd=ROOT)
        step.update(exit_code=result["exit_code"], duration_s=result["duration_s"], output_tail=tail(result["output"]))
        if result["exit_code"] != 0:
            step.update(status="failed", reason="smoke script failed")
        elif MATLAB_SENTINEL not in result["output"]:
            step.update(status="failed", reason="exit 0 without the smoke sentinel: assertions did not all run")
        else:
            step.update(status="executed", reason=None, evidence="script_sentinel", counts={"scripts_passed": 1})
        steps.append(step)

    steps = [finalize_evidence(item) for item in steps]
    counts = {status: sum(1 for item in steps if item["status"] == status) for status in STATUSES}
    by_kind = {kind: {status: sum(1 for item in steps if item["kind"] == kind and item["status"] == status) for status in STATUSES} for kind in KINDS}
    skipped = sum(len(item["skipped_tests"]) + len(item["todo_tests"]) for item in steps)
    profile_operations = {
        profile["id"]: {
            "supported": sum(1 for o in profile["operations"].values() for c in o.values() if c["status"] == "supported"),
            "unsupported": sum(1 for o in profile["operations"].values() for c in o.values() if c["status"] == "unsupported"),
        }
        for profile in payload["profiles"]
    }
    strict_failure = args.require_all and (counts["unavailable"] > 0 or skipped > 0)
    inputs = {"core": {path: sha256_file(ROOT / path) for path in CORE_INPUTS},
              "nirs4all_r": {path: sha256_file(r_root / path) for path in R_INPUTS} if r_present else None,
              "nirs4all_web": {path: sha256_file(web_root / path) for path in WEB_INPUTS} if web_present else None}
    return {
        "schema": SCHEMA,
        "created_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "scope": SCOPE,
        "require_all": bool(args.require_all),
        "source": {"core": git_state(ROOT), "nirs4all_r": git_state(r_root) if r_present else None,
                   "nirs4all_web": git_state(web_root) if web_present else None},
        "inputs_sha256": inputs,
        "environment": {
            "platform": platform.platform(),
            "python": {"path": python, "version": platform.python_version() if python == sys.executable else version_of(python)},
            "node": {"path": node, "version": version_of(node)},
            "rscript": {"path": rscript, "version": version_of(rscript)},
            "octave": {"path": octave, "version": version_of(octave)},
            "matlab": {"path": matlab, "version": None},
            "cargo": {"path": cargo, "version": version_of(cargo)},
            "NIRS4ALL_METHODS_PYTHONPATH": os.environ.get("NIRS4ALL_METHODS_PYTHONPATH"),
        },
        "profile_operations": profile_operations,
        "steps": steps,
        "summary": {"status_counts": counts, "status_counts_by_kind": by_kind, "skipped_or_todo_test_bodies": skipped},
        "ok": counts["failed"] == 0 and not strict_failure,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--output", required=True, help="receipt JSON path")
    parser.add_argument("--python", help="Python interpreter for the Python steps (default: this one)")
    parser.add_argument("--node", help="node executable (default: PATH)")
    parser.add_argument("--rscript", help="Rscript executable (default: PATH)")
    parser.add_argument("--octave", help="octave executable (default: PATH)")
    parser.add_argument("--matlab", help="matlab executable (default: PATH)")
    parser.add_argument("--cargo", help="cargo executable (used with --run-cargo)")
    parser.add_argument("--cargo-target-dir", help="CARGO_TARGET_DIR for --run-cargo (avoid contending with other builds)")
    parser.add_argument("--run-cargo", action="store_true", help="also run the targeted Rust profile tests")
    parser.add_argument("--r-root", help="nirs4all-r checkout (default: sibling directory)")
    parser.add_argument("--web-root", help="nirs4all-web checkout (default: sibling directory)")
    parser.add_argument("--quali-root", help="nirs4all-quality checkout (optional scientific regressions)")
    parser.add_argument("--quali-methods-wasm-root", help="optional explicit local Methods stage override (default: the staged artifact)")
    parser.add_argument("--require-all", action="store_true", help="fail on any unavailable step or skipped/TODO body")
    args = parser.parse_args(argv)

    receipt = qualify(args)
    output = Path(args.output).expanduser()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    for item in receipt["steps"]:
        note = f" - {item['reason']}" if item["reason"] else ""
        print(f"{item['status']:<11} {item['kind']:<16} {item['id']}{note}")
    summary = receipt["summary"]
    print(f"receipt: {output}  counts={summary['status_counts']} skipped_or_todo={summary['skipped_or_todo_test_bodies']}")
    return 0 if receipt["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
