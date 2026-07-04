#!/usr/bin/env python3
"""Consume a repository-served pipeline through nirs4all-core bindings."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


PIPELINE_ARTIFACT = "repository-pipeline.n4a.json"
RESOLUTION_ARTIFACT = "provider-resolution.json"
OUTPUT_ARTIFACT = "cross-language-consumption.json"


def _core_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _workspace_root() -> Path:
    return _core_root().parent


def _read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def _write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=True, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _load_python(pipeline_path: Path) -> dict[str, Any]:
    src = _core_root() / "bindings" / "python" / "src"
    src_text = str(src)
    if src_text not in sys.path:
        sys.path.insert(0, src_text)

    import nirs4all_lite as n4a

    definition = n4a.load_pipeline_definition(pipeline_path)
    return {
        "surface": "bindings/python",
        "status": "passed",
        "name": definition.name,
        "random_state": definition.random_state,
        "classes": n4a.portable_class_names(definition),
        "pipeline": definition.as_dict(),
    }


def _find_node() -> Path | None:
    candidates = [
        shutil.which("node"),
        "/mnt/c/Program Files/nodejs/node.exe",
    ]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    return None


def _node_arg(path: Path, node: Path) -> str:
    if node.suffix.lower() != ".exe":
        return str(path)
    try:
        proc = subprocess.run(
            ["wslpath", "-w", str(path)],
            text=True,
            capture_output=True,
            check=True,
        )
    except (FileNotFoundError, subprocess.CalledProcessError):
        return str(path)
    return proc.stdout.strip()


def _load_javascript_wasm(pipeline_path: Path) -> dict[str, Any]:
    node = _find_node()
    if node is None:
        raise RuntimeError("Node.js runtime not found; cannot validate the JavaScript/WASM binding surface")

    module_path = _core_root() / "bindings" / "wasm" / "src" / "index.js"
    code = """
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const modulePath = process.argv[1];
const pipelinePath = process.argv[2];
const nirs4all = await import(pathToFileURL(modulePath).href);
const source = readFileSync(pipelinePath, 'utf8');
const definition = nirs4all.loadPipelineDefinition(source);
console.log(JSON.stringify({
  surface: 'bindings/wasm',
  status: 'passed',
  node: process.version,
  name: definition.name,
  random_state: definition.random_state ?? null,
  classes: nirs4all.portableClassNames(definition),
  pipeline: definition,
}));
"""
    proc = subprocess.run(
        [
            str(node),
            "--input-type=module",
            "-e",
            code,
            _node_arg(module_path, node),
            _node_arg(pipeline_path, node),
        ],
        text=True,
        capture_output=True,
        check=False,
        timeout=30,
    )
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or f"node exited with {proc.returncode}")
    return _read_json_from_text(proc.stdout)


def _read_json_from_text(text: str) -> dict[str, Any]:
    stripped = text.strip()
    if not stripped:
        raise RuntimeError("JavaScript/WASM loader emitted no JSON")
    return json.loads(stripped.splitlines()[-1])


def consume(artifacts_dir: Path) -> dict[str, Any]:
    pipeline_path = artifacts_dir / PIPELINE_ARTIFACT
    resolution_path = artifacts_dir / RESOLUTION_ARTIFACT
    if not pipeline_path.is_file():
        raise FileNotFoundError(f"missing repository pipeline artifact: {pipeline_path}")
    if not resolution_path.is_file():
        raise FileNotFoundError(f"missing provider resolution artifact: {resolution_path}")

    resolution = _read_json(resolution_path)
    python = _load_python(pipeline_path)
    javascript_wasm = _load_javascript_wasm(pipeline_path)

    parity = {
        "classes_match": python["classes"] == javascript_wasm["classes"],
        "random_state_match": python["random_state"] == javascript_wasm["random_state"],
        "name_match": python["name"] == javascript_wasm["name"],
    }
    if not all(parity.values()):
        raise AssertionError(f"Python and JavaScript/WASM repository consumption diverged: {parity}")

    return {
        "schema_version": "n4a.e2e.repository-consumption/v1",
        "pipeline_id": resolution["repository"]["pipeline_id"],
        "repository_index_count": resolution["repository"]["catalog_count"],
        "source_artifacts": {
            "provider_resolution": str(resolution_path),
            "repository_pipeline": str(pipeline_path),
        },
        "python": python,
        "javascript_wasm": javascript_wasm,
        "parity": parity,
        "known_followups": [
            {
                "surface": "r",
                "status": "not_executed_in_this_gate",
                "reason": (
                    "R remains covered by separate core/methods gates; the current n4m R binding ABI "
                    "does not expose the preprocessing/splitter functions needed for this repository pipeline."
                ),
            }
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args(argv)

    artifacts_dir = args.artifacts_dir.expanduser().resolve()
    result = consume(artifacts_dir)
    _write_json(artifacts_dir / OUTPUT_ARTIFACT, result)
    print(json.dumps(result["parity"], ensure_ascii=True, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
