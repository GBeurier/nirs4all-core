"""Owner-neutral ``.n4a`` exchange through the Python native handles.

Same five steps as the Rust, WASM, CLI, R and Octave tests over the shared
fixture ``bindings/rust/nirs4all/tests/parity/fixtures/n4a_host_opaque_v1``:
write, open once, read every member byte-exact, immutable update, refusals.
The storage cases use no Methods runtime and never deserialize host payloads.

Both routes are covered: the in-memory ``N4aContainer`` over caller bytes and
the file-backed ``N4aFile`` (verified streaming reads, ``write_n4a_file``,
change-after-open refusals at the read that would deliver the last byte, and an
explicit ``memory_budget_bytes``). Core has no content-size cap, so no test
depends on a numeric default; budget tests declare their own budget.

The tests need the ``nirs4all_core._native`` wheel. Without it they are skipped
with an explicit reason, unless ``NIRS4ALL_REQUIRE_CORE_N4A=1`` turns the skip
into a failure. The create and update goldens in ``expected.json`` are mandatory:
a ``null`` (pending) value fails until the real CLI output has been recorded.

Native action parity reuses the real retained ``n4a_wasm_exchange`` fixture
directory (``native.n4a``, ``train.json``, ``natural.json``), selected by
``NIRS4ALL_CORE_N4A_NATIVE_FIXTURE``. It never produces another source model.
Supply ``N4M_LIBRARY_PATH`` and ``N4M_EXPECTED_LIBRARY_SHA256`` for prediction
and the single retrain. ``NIRS4ALL_REQUIRE_CORE_N4A_ACTIONS=1`` makes missing
fixture/runtime inputs fail instead of skip. These cases require a rebuilt
matching native extension; an older installed wheel does not qualify them.
"""

from __future__ import annotations

import hashlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import nirs4all_core as core
from nirs4all_core import _n4a

FIXTURE = Path(__file__).resolve().parents[3] / "bindings/rust/nirs4all/tests/parity/fixtures/n4a_host_opaque_v1"

try:
    from nirs4all_core import _native  # noqa: F401

    NATIVE = True
except ImportError:  # pragma: no cover - depends on the wheel build
    NATIVE = False

if not NATIVE and os.environ.get("NIRS4ALL_REQUIRE_CORE_N4A") == "1":  # pragma: no cover
    raise ImportError("NIRS4ALL_REQUIRE_CORE_N4A=1 but nirs4all_core._native is not installed")


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _members(listing: Path) -> list[tuple[str, bytes]]:
    entries = json.loads(listing.read_text(encoding="utf-8"))
    return [(entry["path"], (listing.parent / entry["file"]).read_bytes()) for entry in entries]


def _member_paths(listing: Path) -> list[tuple[str, Path]]:
    entries = json.loads(listing.read_text(encoding="utf-8"))
    return [(entry["path"], listing.parent / entry["file"]) for entry in entries]


def _assert_golden(case: unittest.TestCase, label: str, section: dict, archive: bytes) -> None:
    for key in ("container_sha256", "manifest_sha256", "byte_length"):
        case.assertIsNotNone(
            section[key],
            f"expected.json {label}.{key} is pending: record the real CLI output first",
        )
    handle = core.open_n4a_container(archive)
    readability = handle.view()["readability"]
    case.assertEqual(_sha(archive), section["container_sha256"], label)
    case.assertEqual(readability["container_sha256"], section["container_sha256"], label)
    case.assertEqual(readability["manifest_sha256"], section["manifest_sha256"], label)
    case.assertEqual(len(archive), section["byte_length"], label)
    case.assertEqual(readability["byte_length"], section["byte_length"], label)


@unittest.skipUnless(NATIVE, "nirs4all_core._native is not installed; build the Core native wheel to run the exchange tests")
class N4aExchangeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.manifest = (FIXTURE / "manifest.json").read_bytes()
        cls.files = _members(FIXTURE / "members.json")
        cls.expected = json.loads((FIXTURE / "expected.json").read_text(encoding="utf-8"))
        cls.update_manifest = (FIXTURE / "update/manifest.json").read_bytes()
        cls.replacement = _members(FIXTURE / "update/members.json")

    def assert_golden(self, label: str, section: dict, archive: bytes) -> None:
        _assert_golden(self, label, section, archive)

    def written(self) -> bytes:
        return core.write_n4a_container(self.manifest, self.files)

    def test_write_open_and_read_every_member_byte_exact(self) -> None:
        archive = self.written()
        self.assertIsInstance(archive, bytes)
        self.assertEqual(archive, core.write_n4a_container(self.manifest.decode("utf-8"), self.files))
        handle = core.open_n4a_container(archive)
        self.assertEqual(handle.archive_bytes(), archive)
        self.assert_golden("container", self.expected["container"], archive)
        for path, original in self.files:
            self.assertEqual(handle.member(path), original, path)
        for entry in self.expected["members"]:
            data = handle.member(entry["path"])
            self.assertEqual(len(data), entry["byte_length"])
            self.assertEqual(_sha(data), entry["sha256"])
        model = handle.member("host/model.bin")
        self.assertEqual(model[:2], b"\x80\x04", "inert pickle prefix is preserved as bytes")
        self.assertTrue(b"\x00" in model and b"\xff" in model and b"\r\n" in model)
        self.assertEqual(handle.artifact_members("model"), {"host/model.bin": model})
        self.assertEqual(handle.artifact_members("dataset-ref"), {})
        with self.assertRaises(KeyError):
            handle.member("host/absent.bin")
        with self.assertRaises(KeyError):
            handle.artifact_members("absent")
        # The stored manifest is re-serialized by Core from its typed form.
        stored = json.loads(handle.manifest_bytes())
        self.assertEqual(stored["container_id"], "container:host.opaque.v1")
        self.assertEqual(stored["members"][0]["sha256"], _sha(self.files[0][1]))

    def test_view_is_storage_only_and_unknown_owners_stay_opaque(self) -> None:
        archive = self.written()
        view = core.open_n4a_container(archive).view()
        self.assertEqual(view["executability"], "not_evaluated")
        self.assertEqual(view["readability"]["storage"], "verified")
        self.assertEqual(view["readability"]["container_sha256"], _sha(archive))
        self.assertEqual(view["readability"]["training_view"]["status"], "not_available")
        semantics = view["readability"]["semantics"]
        self.assertEqual(semantics["status"], "report_only")
        self.assertTrue(all(item["content"] == "opaque" for item in semantics["artifacts"]))

    def test_the_native_model_inspector_still_refuses_a_host_container(self) -> None:
        with self.assertRaises(ValueError):
            core.inspect_n4a_bytes(self.written())

    def test_handle_is_immutable_and_not_constructible(self) -> None:
        handle = core.open_n4a_container(self.written())
        with self.assertRaises(TypeError):
            type(handle)()
        with self.assertRaises(AttributeError):
            handle.archive_bytes = lambda: b""  # type: ignore[misc]

    def test_update_creates_a_new_container_and_never_touches_the_base(self) -> None:
        archive = self.written()
        base = core.open_n4a_container(archive)
        updated = core.write_n4a_container(self.update_manifest, self.replacement, base=base)
        self.assertEqual(base.archive_bytes(), archive)
        self.assertNotEqual(updated, archive)
        self.assert_golden("update.container", self.expected["update"]["container"], updated)
        after = core.open_n4a_container(updated)
        self.assertEqual(after.member("host/model.bin"), self.replacement[0][1])
        originals = dict(self.files)
        for path in ("host/recipe.json", "host/data.csv", "host/scores.json", "host/results.json"):
            self.assertEqual(after.member(path), originals[path], path)
        with self.assertRaises(KeyError):
            after.member("host/foreign-model.bin")
        with self.assertRaises(KeyError):
            after.artifact_members("foreign-model")

    def test_refusals_are_typed_and_never_collapsed(self) -> None:
        base = core.open_n4a_container(self.written())
        extra = [("host/undeclared.bin", b"\x01")]
        with self.assertRaises(ValueError):
            core.write_n4a_container(self.update_manifest, self.replacement + extra, base=base)
        with self.assertRaises(ValueError):
            core.write_n4a_container(self.update_manifest, self.replacement * 2, base=base)
        with self.assertRaises(ValueError):
            core.write_n4a_container(self.manifest, self.files + self.files[:1])
        with self.assertRaises(ValueError):
            core.write_n4a_container(self.manifest, self.files[:-1])
        future = self.manifest.replace(b'"schema_version": 1,', b'"schema_version": 2,')
        with self.assertRaisesRegex(ValueError, "unsupported format"):
            core.write_n4a_container(future, self.files)
        with self.assertRaises(TypeError):
            core.write_n4a_container(123, self.files)  # type: ignore[arg-type]

    def test_tampered_and_foreign_bytes_are_refused(self) -> None:
        archive = bytearray(self.written())
        offset = bytes(archive).index(b"HOST-OPAQUE-MODEL")
        archive[offset] ^= 0xFF
        with self.assertRaisesRegex(ValueError, "n4a (integrity|physical) refusal"):
            core.open_n4a_container(bytes(archive))
        with self.assertRaises(ValueError):
            core.open_n4a_container(b"not a container")

    def test_python_layer_never_imports_a_deserializer(self) -> None:
        source = Path(_n4a.__file__).read_text(encoding="utf-8").lower()
        for name in ("pickle", "joblib", "dill", "cloudpickle"):
            self.assertNotIn(name, source)


class FacadeContractTests(unittest.TestCase):
    """Facade-only checks: they need no native wheel and run everywhere."""

    def test_view_has_exactly_the_three_core_keys(self) -> None:
        self.assertEqual(_n4a._VIEW_KEYS, frozenset({"manifest", "receipt", "training"}))
        three = json.dumps({"manifest": {}, "receipt": {}, "training": {}})
        self.assertEqual(set(_n4a._decode(three, "inspection", _n4a._VIEW_KEYS)), set(_n4a._VIEW_KEYS))
        with self.assertRaises(RuntimeError):
            _n4a._decode(json.dumps({"manifest": {}, "receipt": {}, "training": {}, "limits": {}}), "inspection", _n4a._VIEW_KEYS)
        with self.assertRaises(RuntimeError):
            _n4a._decode(json.dumps({"manifest": {}, "receipt": {}}), "inspection", _n4a._VIEW_KEYS)

    def test_memory_budget_is_optional_and_validated_at_the_boundary(self) -> None:
        self.assertIsNone(_n4a._budget(None))
        self.assertEqual(_n4a._budget(0), 0)
        self.assertEqual(_n4a._budget(1 << 40), 1 << 40)
        with self.assertRaises(ValueError):
            _n4a._budget(-1)
        for bad in (True, 1.5, "10"):
            with self.subTest(bad=bad), self.assertRaises(TypeError):
                _n4a._budget(bad)  # type: ignore[arg-type]

    def test_budget_is_forwarded_unchanged_and_defaults_to_none(self) -> None:
        view = json.dumps({"manifest": {}, "receipt": {}, "training": {}})
        native = mock.Mock(return_value=view)
        with mock.patch.object(_n4a, "_native", return_value=native) as lookup:
            _n4a.inspect_n4a_bytes(b"x")
            native.assert_called_with(b"x", memory_budget_bytes=None)
            _n4a.inspect_n4a_bytes(b"x", memory_budget_bytes=123)
            native.assert_called_with(b"x", memory_budget_bytes=123)
            lookup.assert_called_with("inspect_n4a_bytes")
        produced = mock.Mock(return_value=(b"archive", view))
        with mock.patch.object(_n4a, "_native", return_value=produced):
            result = _n4a.produce_n4a_native(
                b"record", "spectra", "{}",
                methods_library_path="/lib", methods_library_sha256="0" * 64,
                run_id="r", container_id="c", memory_budget_bytes=7,
            )
        self.assertEqual(produced.call_args.kwargs["memory_budget_bytes"], 7)
        self.assertEqual(set(result["view"]), {"manifest", "receipt", "training"})
        with mock.patch.object(_n4a, "_native", return_value=mock.Mock()) as lookup:
            with self.assertRaises(ValueError):
                _n4a.inspect_n4a_bytes(b"x", memory_budget_bytes=-5)
            lookup.return_value.assert_not_called()

    def test_new_names_are_exported_once_from_the_facade(self) -> None:
        for name in ("N4aFile", "N4aMemberStream", "open_n4a_file", "write_n4a_file"):
            with self.subTest(name=name):
                self.assertIn(name, core.__all__)
                self.assertIs(getattr(core, name), getattr(_n4a, name))
        source = Path(_n4a.__file__).read_text(encoding="utf-8")
        self.assertNotIn("N4aLimits", source)
        self.assertNotIn('"limits"', source, "the owner view no longer carries a limits key")


@unittest.skipUnless(NATIVE, "nirs4all_core._native is not installed; build the Core native wheel to run the exchange tests")
class N4aFileTests(unittest.TestCase):
    """File-backed handle, streams, mutation, budget and the streaming writer."""

    MODEL = "host/model.bin"

    @classmethod
    def setUpClass(cls) -> None:
        cls.manifest = (FIXTURE / "manifest.json").read_bytes()
        cls.files = _members(FIXTURE / "members.json")
        cls.paths = _member_paths(FIXTURE / "members.json")
        cls.originals = dict(cls.files)
        cls.expected = json.loads((FIXTURE / "expected.json").read_text(encoding="utf-8"))
        cls.update_manifest = (FIXTURE / "update/manifest.json").read_bytes()
        cls.replacement = _members(FIXTURE / "update/members.json")
        cls.replacement_paths = _member_paths(FIXTURE / "update/members.json")

    def setUp(self) -> None:
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.dir = Path(scratch.name)
        self.path = self.dir / "host.n4a"
        self.receipt = core.write_n4a_file(self.path, self.manifest, self.files)

    def open(self, path: Path | None = None, **kwargs: object):
        handle = core.open_n4a_file(path or self.path, **kwargs)  # type: ignore[arg-type]
        self.addCleanup(handle.close)
        return handle

    def native_calls(self, archive: object, **kwargs: object):
        """Invalid science inputs must never be reached on an archive refusal."""
        runtime = {"methods_library_path": "/not-loaded", "methods_library_sha256": "0" * 64}
        return {
            "inspect": lambda: core.inspect_n4a_bytes(archive, **kwargs),
            "predict": lambda: core.predict_n4a_native(
                archive, record=b"unreached", entrypoint="predict-external",
                source_id="spectra", partition="predict", request_id="request:refused",
                run_id="run:refused", **runtime, **kwargs,
            ),
            "retrain": lambda: core.retrain_n4a_native(
                archive, record=b"unreached", run_id="run:refused",
                container_id="container:refused", **runtime, **kwargs,
            ),
        }

    def test_native_actions_refuse_closed_changed_and_incorrect_archive_types(self) -> None:
        closed = self.open()
        closed.close()
        for name, call in self.native_calls(closed).items():
            with self.subTest(action=name, source="closed"), self.assertRaisesRegex(ValueError, "closed"):
                call()
        for wrong in (object(), str(self.path), core.open_n4a_container(self.path.read_bytes())):
            for name, call in self.native_calls(wrong).items():
                with self.subTest(action=name, source=type(wrong)), self.assertRaises(TypeError):
                    call()
        handle = self.open()
        data = self.path.read_bytes()
        offset = data.index(b"HOST-OPAQUE-MODEL")
        with self.path.open("r+b") as changed:
            changed.seek(offset)
            changed.write(bytes([data[offset] ^ 1]))
        for name, call in self.native_calls(handle).items():
            with self.subTest(action=name, source="changed"), self.assertRaisesRegex(ValueError, "integrity"):
                call()
        self.assertFalse(handle.closed)

    def test_native_actions_inherit_and_override_the_file_materialization_budget(self) -> None:
        big = b"x" * (128 * 1024)
        manifest = json.loads(self.manifest)
        next(a for a in manifest["artifacts"] if a["id"] == "dataset")["identity"]["fingerprint"] = _sha(big)
        members = [(path, big if path == "host/data.csv" else data) for path, data in self.files]
        path = self.dir / "budget.n4a"
        core.write_n4a_file(path, json.dumps(manifest), members)
        handle = self.open(path, memory_budget_bytes=65_536)
        for name, call in self.native_calls(handle).items():
            with self.subTest(action=name, budget="inherited"), self.assertRaisesRegex(ValueError, "authorized"):
                call()
        generous = self.open(path)
        for name, call in self.native_calls(generous, memory_budget_bytes=1).items():
            with self.subTest(action=name, budget="explicit"), self.assertRaisesRegex(ValueError, "authorized"):
                call()
        # A larger override reaches the genuine owner-profile refusal, rather
        # than retaining the small opening budget. No Methods runtime is loaded.
        with self.assertRaises(ValueError) as refusal:
            core.inspect_n4a_bytes(handle, memory_budget_bytes=1 << 30)
        self.assertNotIn("memory refusal", str(refusal.exception))
        self.assertFalse(handle.closed)

    # -- writer -----------------------------------------------------------------

    def test_file_writer_matches_the_byte_writer_and_reports_what_it_wrote(self) -> None:
        expected = core.write_n4a_container(self.manifest, self.files)
        written = self.path.read_bytes()
        self.assertEqual(written, expected)
        readability = core.open_n4a_container(expected).view()["readability"]
        self.assertEqual(
            self.receipt,
            {
                "container_sha256": _sha(expected),
                "manifest_sha256": readability["manifest_sha256"],
                "byte_length": len(expected),
                "members": self.receipt["members"],
            },
        )
        # The receipt members are Core's own final inventory: no recomputation by the facade.
        self.assertEqual(
            {entry["path"]: (entry["byte_length"], entry["sha256"]) for entry in self.receipt["members"]},
            {path: (len(data), _sha(data)) for path, data in self.files},
        )
        self.assertEqual(len(self.receipt["members"]), len(self.files))
        self.assertTrue(all(set(entry) == {"path", "byte_length", "sha256"} for entry in self.receipt["members"]))
        _assert_golden(self, "container (file writer)", self.expected["container"], written)

    def test_path_and_mixed_members_are_streamed_by_core(self) -> None:
        expected = self.path.read_bytes()
        by_path = self.dir / "by-path.n4a"
        core.write_n4a_file(by_path, self.manifest, [(path, source) for path, source in self.paths])
        self.assertEqual(by_path.read_bytes(), expected)
        mixed = self.dir / "mixed.n4a"
        core.write_n4a_file(
            mixed,
            self.manifest.decode("utf-8"),
            [(path, source if index % 2 else self.originals[path]) for index, (path, source) in enumerate(self.paths)],
        )
        self.assertEqual(mixed.read_bytes(), expected)

    def test_update_from_a_file_base_or_a_byte_base_never_touches_the_base(self) -> None:
        before = self.path.read_bytes()
        reference = core.write_n4a_container(
            self.update_manifest, self.replacement, base=core.open_n4a_container(before)
        )
        _assert_golden(self, "update.container", self.expected["update"]["container"], reference)
        from_file = self.dir / "from-file.n4a"
        with self.open() as base_file:
            core.write_n4a_file(from_file, self.update_manifest, self.replacement_paths, base=base_file)
        from_bytes = self.dir / "from-bytes.n4a"
        core.write_n4a_file(
            from_bytes, self.update_manifest, self.replacement, base=core.open_n4a_container(before)
        )
        self.assertEqual(from_file.read_bytes(), reference)
        self.assertEqual(from_bytes.read_bytes(), reference)
        self.assertEqual(self.path.read_bytes(), before)

    def test_writer_never_replaces_and_refuses_before_publishing(self) -> None:
        before = self.path.read_bytes()
        with self.assertRaises(ValueError):
            core.write_n4a_file(self.path, self.manifest, self.files)
        self.assertEqual(self.path.read_bytes(), before)
        listing = sorted(os.listdir(self.dir))
        base = core.open_n4a_container(before)
        refused = self.dir / "refused.n4a"
        bad_cases = {
            "undeclared": (self.update_manifest, self.replacement + [("host/undeclared.bin", b"\x01")], base),
            "duplicate": (self.update_manifest, self.replacement * 2, base),
            "uncovered": (self.manifest, self.files[:-1], None),
            "missing source": (self.update_manifest, [(self.MODEL, self.dir / "absent.bin")], base),
        }
        for label, (manifest, members, base_handle) in bad_cases.items():
            with self.subTest(label=label), self.assertRaises((ValueError, OSError)):
                core.write_n4a_file(refused, manifest, members, base=base_handle)
        self.assertFalse(refused.exists())
        self.assertEqual(sorted(os.listdir(self.dir)), listing, "no staged file may remain")
        with self.assertRaises(TypeError):
            core.write_n4a_file(refused, self.manifest, [("host/recipe.json", "a str is ambiguous")])  # type: ignore[list-item]
        with self.assertRaises(TypeError):
            core.write_n4a_file(refused, self.manifest, self.files, base=object())  # type: ignore[arg-type]

    # -- handle -----------------------------------------------------------------

    def test_file_handle_agrees_with_the_in_memory_handle(self) -> None:
        archive = self.path.read_bytes()
        memory = core.open_n4a_container(archive)
        handle = self.open()
        self.assertEqual(handle.manifest_bytes(), memory.manifest_bytes())
        view = handle.view()
        self.assertEqual(view["executability"], "not_evaluated")
        for key in ("storage", "container_sha256", "manifest_sha256", "byte_length"):
            self.assertEqual(view["readability"][key], memory.view()["readability"][key], key)
        for path, original in self.files:
            self.assertEqual(handle.member(path), original, path)
        self.assertEqual(handle.artifact_member_paths("model"), [self.MODEL])
        self.assertEqual(handle.artifact_member_paths("dataset-ref"), [])
        with self.assertRaises(KeyError):
            handle.member("host/absent.bin")
        with self.assertRaises(KeyError):
            handle.open_member("host/absent.bin")
        with self.assertRaises(KeyError):
            handle.artifact_member_paths("absent")
        sink = io.BytesIO()
        self.assertEqual(handle.copy_archive_to(sink), len(archive))
        self.assertEqual(sink.getvalue(), archive)
        sink = io.BytesIO()
        self.assertEqual(handle.copy_member_to(self.MODEL, sink), len(self.originals[self.MODEL]))
        self.assertEqual(sink.getvalue(), self.originals[self.MODEL])
        with self.assertRaises(KeyError):
            handle.copy_member_to("host/absent.bin", io.BytesIO())

    def test_copy_destinations_keep_their_own_errors_and_partial_writes(self) -> None:
        handle = self.open()

        class Failing:
            def write(self, data: bytes) -> int:
                raise OSError("disk full")

        class Short:
            def __init__(self) -> None:
                self.chunks: list[bytes] = []

            def write(self, data: bytes) -> int:
                taken = max(1, len(data) // 2)
                self.chunks.append(bytes(data[:taken]))
                return taken

        with self.assertRaisesRegex(OSError, "disk full"):
            handle.copy_archive_to(Failing())  # type: ignore[arg-type]
        with self.assertRaises(TypeError):
            handle.copy_archive_to(object())  # type: ignore[arg-type]
        short = Short()
        handle.copy_archive_to(short)  # type: ignore[arg-type]
        self.assertEqual(b"".join(short.chunks), self.path.read_bytes())

    def test_handle_lifecycle(self) -> None:
        handle = self.open()
        stream = handle.open_member(self.MODEL)
        self.assertFalse(handle.closed)
        handle.close()
        self.assertTrue(handle.closed)
        for call in (handle.view, handle.manifest_bytes, lambda: handle.member(self.MODEL), lambda: handle.open_member(self.MODEL)):
            with self.assertRaises(ValueError):
                call()
        # A stream opened earlier owns its reference and still verifies.
        self.assertEqual(stream.read(), self.originals[self.MODEL])
        self.assertTrue(stream.verified)
        with core.open_n4a_file(self.path) as scoped:
            self.assertFalse(scoped.closed)
        self.assertTrue(scoped.closed)

    # -- streams ------------------------------------------------------------------

    def test_stream_is_unverified_until_its_last_byte_and_works_with_io(self) -> None:
        model = self.originals[self.MODEL]
        handle = self.open()
        with handle.open_member(self.MODEL) as stream:
            self.assertEqual((stream.byte_length, stream.remaining, stream.verified), (len(model), len(model), False))
            self.assertTrue(stream.readable() and not stream.seekable())
            head = stream.read(5)
            self.assertEqual(head, model[:5])
            self.assertEqual((stream.tell(), stream.remaining, stream.verified), (5, len(model) - 5, False))
            rest = stream.read()
            self.assertEqual(head + rest, model)
            self.assertTrue(stream.verified)
            self.assertEqual(stream.read(), b"")
            self.assertEqual(stream.readinto(bytearray(8)), 0)
        self.assertTrue(stream.closed)
        with self.assertRaises(ValueError):
            stream.read()
        collected = bytearray()
        reader = handle.open_member(self.MODEL)
        buffer = bytearray(7)
        while count := reader.readinto(buffer):
            collected += buffer[:count]
        self.assertEqual(bytes(collected), model)
        self.assertTrue(reader.verified)
        raw = handle.open_member(self.MODEL)
        with io.BufferedReader(raw) as buffered:
            self.assertEqual(buffered.read(), model)
            self.assertTrue(raw.verified)
            self.assertIsNone(raw.flush())
        self.assertTrue(buffered.closed)
        self.assertTrue(raw.closed)
        with self.assertRaises(ValueError):
            raw.flush()
        with self.assertRaises(TypeError):
            handle.open_member(self.MODEL).readinto(b"\x00" * 8)  # type: ignore[arg-type]

    def test_a_file_changed_after_open_is_refused_at_the_last_byte(self) -> None:
        handle = self.open()
        data = self.path.read_bytes()
        offset = data.index(b"HOST-OPAQUE-MODEL")
        with self.path.open("r+b") as changed:
            changed.seek(offset)
            changed.write(bytes([data[offset] ^ 0xFF]))
        # Per-member verification: members that were not touched still read.
        self.assertEqual(handle.member("host/recipe.json"), self.originals["host/recipe.json"])
        with self.assertRaises(ValueError):
            handle.member(self.MODEL)
        stream = handle.open_member(self.MODEL)
        stream.read(10)  # unverified bytes may be delivered before the end
        self.assertFalse(stream.verified)
        with self.assertRaises(ValueError):
            stream.read()  # the final bytes are withheld
        self.assertFalse(stream.verified)
        buffer = bytearray(64)
        with self.assertRaises(ValueError):
            handle.open_member(self.MODEL).readinto(buffer)
        self.assertEqual(bytes(buffer), bytes(64), "withheld bytes must not reach the caller's buffer")
        with self.assertRaises(ValueError):
            handle.copy_member_to(self.MODEL, io.BytesIO())
        with self.assertRaises(ValueError):
            handle.copy_archive_to(io.BytesIO())
        with self.assertRaises(ValueError):
            core.open_n4a_file(self.path)

    def test_a_truncated_file_is_refused(self) -> None:
        handle = self.open()
        os.truncate(self.path, 10)
        with self.assertRaises(ValueError):
            handle.member("host/recipe.json")
        with self.assertRaises(ValueError):
            handle.copy_archive_to(io.BytesIO())
        with self.assertRaises(ValueError):
            core.open_n4a_file(self.path)

    # -- memory budget ------------------------------------------------------------

    def large_archive(self) -> tuple[Path, bytes]:
        big = bytes((index * 7) % 256 for index in range(40_000))
        manifest = json.loads(self.manifest)
        next(a for a in manifest["artifacts"] if a["id"] == "dataset")["identity"]["fingerprint"] = _sha(big)
        members = [(path, big if path == "host/data.csv" else data) for path, data in self.files]
        destination = self.dir / "large.n4a"
        core.write_n4a_file(destination, json.dumps(manifest), members)
        return destination, big

    def test_explicit_budget_refuses_materialization_but_not_streaming(self) -> None:
        destination, big = self.large_archive()
        handle = self.open(destination, memory_budget_bytes=65_536)
        with self.assertRaises(ValueError):
            handle.member("host/data.csv", memory_budget_bytes=4096)
        self.assertEqual(handle.member("host/data.csv", memory_budget_bytes=1 << 30), big)
        self.assertEqual(handle.member("host/data.csv"), big)
        # The same member streams in bounded chunks whatever the content size.
        stream = handle.open_member("host/data.csv")
        buffer = bytearray(100_000)
        first = stream.readinto(buffer)
        self.assertTrue(0 < first < len(big), "one readinto is bounded by the streaming chunk")
        collected = bytes(buffer[:first])
        while piece := stream.read(1000):
            collected += piece
        self.assertEqual(collected, big)
        self.assertTrue(stream.verified)
        sink = io.BytesIO()
        self.assertEqual(handle.copy_member_to("host/data.csv", sink), len(big))
        self.assertEqual(sink.getvalue(), big)
        self.assertEqual(self.open(destination, memory_budget_bytes=None).member("host/data.csv"), big)

    def test_a_budget_below_the_streaming_chunk_is_refused_and_bad_values_are_typed(self) -> None:
        with self.assertRaises(ValueError):
            core.open_n4a_file(self.path, memory_budget_bytes=1)
        with self.assertRaises(ValueError):
            core.open_n4a_file(self.path, memory_budget_bytes=-1)
        with self.assertRaises(TypeError):
            core.open_n4a_file(self.path, memory_budget_bytes=True)  # type: ignore[arg-type]
        with self.assertRaises(ValueError):
            core.open_n4a_file(self.dir / "absent.n4a")
        with self.assertRaises(ValueError):
            core.open_n4a_file(self.dir)

    def test_a_generous_budget_changes_no_byte_of_the_in_memory_routes(self) -> None:
        archive = self.path.read_bytes()
        budget = 1 << 30
        self.assertEqual(core.write_n4a_container(self.manifest, self.files, memory_budget_bytes=budget), archive)
        handle = core.open_n4a_container(archive, memory_budget_bytes=budget)
        self.assertEqual(handle.archive_bytes(), archive)
        self.assertEqual(handle.member(self.MODEL), self.originals[self.MODEL])
        updated = core.write_n4a_container(
            self.update_manifest, self.replacement, base=handle, memory_budget_bytes=budget
        )
        self.assertEqual(updated, core.write_n4a_container(self.update_manifest, self.replacement, base=handle))

    def test_in_memory_copies_admit_the_resident_archive_and_destination(self) -> None:
        destination, big = self.large_archive()
        archive = destination.read_bytes()
        # One archive fits; the owned Rust archive plus each requested copy
        # exceeds this budget. File reads fill Python memory directly instead.
        budget = 65_536
        self.assertLess(len(archive), budget)
        self.assertGreater(len(archive) + len(big), budget)
        handle = core.open_n4a_container(archive, memory_budget_bytes=budget)
        for copy in (handle.archive_bytes, lambda: handle.member("host/data.csv"), lambda: handle.artifact_members("dataset")):
            with self.subTest(copy=copy), self.assertRaisesRegex(ValueError, "authorized"):
                copy()
        self.assertEqual(core.open_n4a_container(archive).archive_bytes(), archive)


class N4aNativeFileTests(unittest.TestCase):
    """Real frozen standard-scale/ridge model; no preparatory or duplicate fit."""

    @classmethod
    def setUpClass(cls) -> None:
        required = ("NIRS4ALL_CORE_N4A_NATIVE_FIXTURE", "N4M_LIBRARY_PATH", "N4M_EXPECTED_LIBRARY_SHA256")
        missing = [name for name in required if not os.environ.get(name)]
        if not NATIVE:
            missing.append("nirs4all_core._native")
        if missing:
            message = "native file-action parity requires " + ", ".join(missing)
            if os.environ.get("NIRS4ALL_REQUIRE_CORE_N4A_ACTIONS") == "1":
                raise RuntimeError(message)
            raise unittest.SkipTest(message)
        fixture = Path(os.environ[required[0]])
        cls.archive = (fixture / "native.n4a").read_bytes()
        cls.training_record = (fixture / "train.json").read_bytes()
        cls.prediction_record = (fixture / "natural.json").read_bytes()
        cls.runtime = {
            "methods_library_path": os.environ[required[1]],
            "methods_library_sha256": os.environ[required[2]],
        }

    def setUp(self) -> None:
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.path = Path(scratch.name) / "native.n4a"
        self.path.write_bytes(self.archive)
        self.handle = core.open_n4a_file(self.path)
        self.addCleanup(self.handle.close)

    def prediction(self, archive: bytes | core.N4aFile) -> dict:
        return core.predict_n4a_native(
            archive, record=self.prediction_record, entrypoint="predict-external",
            source_id="spectra", partition="predict", request_id="request:file-parity",
            run_id="run:file-parity", **self.runtime,
        )

    def assert_handle_usable(self) -> None:
        self.assertFalse(self.handle.closed)
        output = io.BytesIO()
        self.assertEqual(self.handle.copy_archive_to(output), len(self.archive))
        self.assertEqual(output.getvalue(), self.archive)

    def test_real_native_file_inspection_and_prediction_match_bytes(self) -> None:
        expected = core.inspect_n4a_bytes(self.archive)
        self.assertEqual(core.inspect_n4a_bytes(self.handle), expected)
        self.assertEqual(core.inspect_n4a_bytes(self.handle, memory_budget_bytes=1 << 30), expected)
        with self.assertRaisesRegex(ValueError, "authorized"):
            core.inspect_n4a_bytes(self.handle, memory_budget_bytes=1)
        self.assertEqual(self.prediction(self.handle), self.prediction(self.archive))
        self.assert_handle_usable()

    def test_real_native_file_retrain_seals_one_fit_and_preserves_source(self) -> None:
        produced = core.retrain_n4a_native(
            self.handle, record=self.training_record, run_id="run:file-retrain",
            container_id="container:file-retrain", **self.runtime,
        )
        # Inspect and replay this same single fit via file and bytes, without
        # retraining a second time merely to obtain a second view.
        self.assertEqual(core.inspect_n4a_bytes(produced["archive_bytes"]), produced["view"])
        path = self.path.with_name("retrained.n4a")
        path.write_bytes(produced["archive_bytes"])
        with core.open_n4a_file(path) as retrained:
            self.assertEqual(core.inspect_n4a_bytes(retrained), produced["view"])
            self.assertEqual(self.prediction(retrained), self.prediction(produced["archive_bytes"]))
        self.assert_handle_usable()


if __name__ == "__main__":
    unittest.main()
