"""Stage an explicit unpublished source cohort without rewriting release pins.

The output is a disposable qualification checkout, never a publishable source.
Source manifests are copied verbatim; only explicit local Cargo overrides are added.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", required=True, type=Path)
    parser.add_argument("--dag-root", required=True, type=Path)
    parser.add_argument("--methods-root", required=True, type=Path)
    parser.add_argument("--formats-root", type=Path)
    parser.add_argument("--io-root", type=Path)
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1]
    target = args.destination.resolve()
    if target.exists():
        parser.error("destination must not exist; preserve previous qualification evidence")
    dag = args.dag_root.resolve()
    methods = args.methods_root.resolve()
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=source
    ).decode().split("\0")
    hashes = {}
    for name in sorted(set(filter(None, paths))):
        path = source / name
        if not path.is_file():
            continue
        destination = target / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, destination)
        hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    # Candidate versions are declared in the source manifests themselves.
    # No source or manifest rewrite is permitted in the qualification copy.
    staged_manifests = []
    config = target / ".cargo/config.toml"
    config.parent.mkdir(exist_ok=True)
    overrides = {
        name: str(dag / "crates" / name)
        for name in ("dag-ml", "dag-ml-core", "dag-ml-results")
    }
    overrides["n4m"] = str(methods / "bindings/rust/n4m")
    if args.formats_root:
        for name in ("nirs4all-formats", "nirs4all-formats-core"):
            overrides[name] = str(args.formats_root.resolve() / "crates" / name)
    if args.io_root:
        for name in ("nirs4all-io", "nirs4all-io-core", "nirs4all-io-dagml"):
            overrides[name] = str(args.io_root.resolve() / "crates" / name)
    config.write_text("[patch.crates-io]\n" + "".join(
        f'{name} = {{ path = {json.dumps(path)} }}\n' for name, path in overrides.items()
    ))
    receipt = {
        "schema_version": 1, "purpose": "unpublished local format3 qualification only",
        "publishable": False, "source_root": str(source),
        "source_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip(),
        "source_sha256": hashes, "staged_manifests": staged_manifests,
        "registry_overrides": overrides,
        "release_pins_modified_in_original": False,
    }
    (target / "LOCAL_FORMAT3_COHORT.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(target)


if __name__ == "__main__":
    main()
