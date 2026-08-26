"""The Python Archive V2 facade only delegates to the native aggregate."""

from __future__ import annotations

import sys
import types
import unittest
from unittest.mock import patch

from nirs4all_core import (
    NativeArchiveUnavailableError,
    read_portable_predictor_package_v2,
    read_portable_refit_package_v3,
    write_archive_v2_from_native_payloads,
    write_archive_v3_from_native_payloads,
)


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

    def test_returns_exact_native_refit_package_v3_bytes(self) -> None:
        observed: list[str] = []

        def read(path: str) -> bytes:
            observed.append(path)
            return b'{"schema_version":3}'

        module = types.SimpleNamespace(read_portable_refit_package_v3=read)
        with patch.dict(sys.modules, {"nirs4all_core._native": module}):
            result = read_portable_refit_package_v3("/tmp/model.n4a")

        self.assertEqual(result, b'{"schema_version":3}')
        self.assertEqual(observed, ["/tmp/model.n4a"])

    def test_writer_forwards_opaque_dagml_members_without_zip_logic(self) -> None:
        observed: list[object] = []

        def write(path: str, manifest: dict[str, object], members: list[tuple[str, bytes]]) -> tuple[str, str]:
            observed.extend([path, manifest, members])
            return ("archive:v2", "a" * 64)

        module = types.SimpleNamespace(write_archive_v2_from_native_payloads=write)
        with patch.dict(sys.modules, {"nirs4all_core._native": module}):
            reference = write_archive_v2_from_native_payloads(
                "/tmp/model.n4a",
                {"schema_version": 2},
                {"dagml/package.json": bytearray([1, 2]), "methods/model.n4mm": b"raw"},
            )

        self.assertEqual(reference, {"archive_id": "archive:v2", "archive_sha256": "a" * 64})
        self.assertEqual(observed[0], "/tmp/model.n4a")
        self.assertEqual(observed[1], {"schema_version": 2})
        self.assertEqual(
            observed[2],
            [("dagml/package.json", b"\x01\x02"), ("methods/model.n4mm", b"raw")],
        )

    def test_writer_rejects_non_bytes_before_native_call(self) -> None:
        with self.assertRaises(TypeError, msg="non-byte payload must be refused"):
            write_archive_v2_from_native_payloads(
                "/tmp/model.n4a", {"schema_version": 2}, {"member": "not-bytes"}
            )

    def test_v3_writer_forwards_opaque_dagml_members_without_zip_logic(self) -> None:
        observed: list[object] = []

        def write(path: str, manifest: dict[str, object], members: list[tuple[str, bytes]]) -> tuple[str, str]:
            observed.extend([path, manifest, members])
            return ("archive:v3", "b" * 64)

        module = types.SimpleNamespace(write_archive_v3_from_native_payloads=write)
        with patch.dict(sys.modules, {"nirs4all_core._native": module}):
            reference = write_archive_v3_from_native_payloads(
                "/tmp/refit.n4a",
                {"schema_version": 3},
                {"dagml/refit.json": memoryview(b"refit"), "methods/model.n4mm": b"raw"},
            )

        self.assertEqual(reference, {"archive_id": "archive:v3", "archive_sha256": "b" * 64})
        self.assertEqual(observed[0], "/tmp/refit.n4a")
        self.assertEqual(observed[1], {"schema_version": 3})
        self.assertEqual(
            observed[2],
            [("dagml/refit.json", b"refit"), ("methods/model.n4mm", b"raw")],
        )

    def test_v3_writer_rejects_non_bytes_before_native_call(self) -> None:
        with self.assertRaises(TypeError, msg="non-byte payload must be refused"):
            write_archive_v3_from_native_payloads(
                "/tmp/refit.n4a", {"schema_version": 3}, {"member": "not-bytes"}
            )
