#!/usr/bin/env python3
"""Fail closed on incomplete or contaminated nirs4all-core wheels."""

from __future__ import annotations

import sys
import zipfile
from pathlib import Path


def validate_wheel(path: Path) -> None:
    """Validate public package inventory without importing the wheel."""
    with zipfile.ZipFile(path) as archive:
        names = set(archive.namelist())

    required = {"nirs4all_core/__init__.py", "n4a/__init__.py"}
    missing = sorted(required - names)
    contaminated = sorted(
        name
        for name in names
        if "__pycache__/" in name or name.endswith((".pyc", ".pyo"))
    )
    if missing or contaminated:
        details = []
        if missing:
            details.append(f"missing public packages: {', '.join(missing)}")
        if contaminated:
            details.append(f"Python cache entries: {', '.join(contaminated)}")
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
