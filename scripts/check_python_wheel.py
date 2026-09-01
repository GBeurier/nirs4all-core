#!/usr/bin/env python3
"""Fail closed on incomplete or contaminated nirs4all-core wheels."""

from __future__ import annotations

import re
import sys
import zipfile
from collections import Counter
from email.parser import BytesParser
from email.policy import default
from pathlib import Path
from pathlib import PurePosixPath


def validate_wheel(path: Path) -> None:
    """Validate public package inventory without importing the wheel."""
    with zipfile.ZipFile(path) as archive:
        raw_names = archive.namelist()
        names = set(raw_names)
        duplicates = sorted(
            name for name, count in Counter(raw_names).items() if count > 1
        )
        unsafe = sorted(
            name
            for name in raw_names
            if name.startswith("/")
            or "\\" in name
            or ".." in PurePosixPath(name).parts
        )
        metadata_names = sorted(
            name for name in names if name.endswith(".dist-info/METADATA")
        )
        if len(metadata_names) != 1:
            raise ValueError(f"{path}: expected exactly one dist-info/METADATA")
        metadata = BytesParser(policy=default).parsebytes(
            archive.read(metadata_names[0])
        )
        init_source = (
            archive.read("nirs4all_core/__init__.py").decode("utf-8")
            if "nirs4all_core/__init__.py" in names
            else ""
        )

    required = {"nirs4all_core/__init__.py", "n4a/__init__.py"}
    missing = sorted(required - names)
    contaminated = sorted(
        name
        for name in names
        if "__pycache__/" in name or name.endswith((".pyc", ".pyo"))
    )
    version = metadata.get("Version", "")
    source_version = re.search(r'^__version__ = "([^"]+)"$', init_source, re.MULTILINE)
    version_errors = []
    if not path.name.startswith(f"nirs4all_core-{version}-"):
        version_errors.append(f"filename does not encode metadata version {version!r}")
    if source_version is None or source_version.group(1) != version:
        observed = source_version.group(1) if source_version else None
        version_errors.append(
            f"nirs4all_core.__version__ is {observed!r}, expected {version!r}"
        )
    if missing or contaminated or duplicates or unsafe or version_errors:
        details = []
        if missing:
            details.append(f"missing public packages: {', '.join(missing)}")
        if contaminated:
            details.append(f"Python cache entries: {', '.join(contaminated)}")
        if duplicates:
            details.append(f"duplicate ZIP entries: {', '.join(duplicates)}")
        if unsafe:
            details.append(f"unsafe ZIP paths: {', '.join(unsafe)}")
        details.extend(version_errors)
        raise ValueError(f"{path}: {'; '.join(details)}")


def main(arguments: list[str]) -> int:
    """Validate every wheel named on the command line."""
    if not arguments:
        print("usage: check_python_wheel.py WHEEL [WHEEL ...]", file=sys.stderr)
        return 2
    try:
        for argument in arguments:
            validate_wheel(Path(argument))
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
