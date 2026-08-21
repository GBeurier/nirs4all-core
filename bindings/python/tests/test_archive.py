"""The Python Archive V2 facade only delegates to the native aggregate."""

from __future__ import annotations

import sys
import types
import unittest
from unittest.mock import patch

from nirs4all_core import NativeArchiveUnavailableError, read_portable_predictor_package_v2


class ArchiveFacadeTests(unittest.TestCase):
    def test_returns_exact_native_package_bytes(self) -> None:
        observed: list[str] = []

        def read(path: str) -> bytes:
            observed.append(path)
            return b'{"schema_version":2}'

        module = types.SimpleNamespace(read_portable_predictor_package_v2=read)
        with patch.dict(sys.modules, {"nirs4all_core._native": module}):
            result = read_portable_predictor_package_v2("/tmp/model.n4a")

        self.assertEqual(result, b'{"schema_version":2}')
        self.assertEqual(observed, ["/tmp/model.n4a"])

    def test_missing_native_bridge_fails_closed(self) -> None:
        with patch.dict(sys.modules, {"nirs4all_core._native": None}):
            with self.assertRaises(NativeArchiveUnavailableError):
                read_portable_predictor_package_v2("/tmp/model.n4a")
