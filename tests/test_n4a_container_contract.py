"""Tests for the n4a container JSON Schema instance gate (JSON shape only)."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/validate_n4a_container_contract.py"

_spec = importlib.util.spec_from_file_location("validate_n4a_container_contract", SCRIPT)
assert _spec is not None and _spec.loader is not None
gate = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = gate
_spec.loader.exec_module(gate)


def run_cli(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(SCRIPT), *args], capture_output=True, text=True, check=False)


class N4aContainerContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.schema = gate.load_json(gate.SCHEMA_PATH, "schema")
        cls.validator = gate.build_validator(cls.schema)
        cls.baseline = gate.load_json(gate.FIXTURES_ROOT / "combined/manifest.json", "manifest")

    def tmp_copy(self) -> Path:
        directory = Path(tempfile.mkdtemp(prefix="n4a-contract-"))
        self.addCleanup(shutil.rmtree, directory, True)
        root = directory / "n4a_v1"
        for name in gate.FIXTURE_NAMES:
            (root / name).mkdir(parents=True)
            shutil.copyfile(gate.FIXTURES_ROOT / name / "manifest.json", root / name / "manifest.json")
        return root

    def problems(self, doc: object) -> list[str]:
        return gate.validate_instance(self.validator, doc)

    def test_six_committed_manifests_pass_instance_validation(self) -> None:
        paths = gate.default_manifest_paths()
        self.assertEqual([p.parent.name for p in paths], list(gate.FIXTURE_NAMES))
        self.assertEqual(len(paths), 6)
        for path in paths:
            self.assertEqual(self.problems(gate.load_json(path, "manifest")), [], path)

    def test_combined_baseline_shape_and_reference_selected_by_kind(self) -> None:
        kinds = [a["storage"]["kind"] for a in self.baseline["artifacts"]]
        self.assertIn("embedded", kinds)
        self.assertIn("reference", kinds)
        index = gate._resolve_selector(self.baseline["artifacts"], "@reference")
        self.assertEqual(self.baseline["artifacts"][index]["storage"]["kind"], "reference")
        self.assertEqual(index, kinds.index("reference"))

    def test_mutation_bank_is_json_serializable_and_named_uniquely(self) -> None:
        bank = gate.build_mutation_bank()
        self.assertEqual(json.loads(json.dumps(bank)), bank)
        names = [case["name"] for case in bank]
        self.assertEqual(len(names), len(set(names)))
        self.assertTrue(any(c["expect"] == "accept" for c in bank))
        self.assertTrue(any(c["expect"] == "refuse" for c in bank))

    def test_every_bank_case_has_expected_outcome(self) -> None:
        counts = gate.run_mutation_bank(self.validator, self.baseline)
        self.assertEqual(counts["positive"] + counts["negative"], len(gate.build_mutation_bank()))
        self.assertGreaterEqual(counts["negative"], 34)

    def test_mutations_do_not_alter_baseline(self) -> None:
        before = copy.deepcopy(self.baseline)
        gate.run_mutation_bank(self.validator, self.baseline)
        self.assertEqual(self.baseline, before)

    def test_unknown_optional_owner_content_is_accepted(self) -> None:
        case = next(c for c in gate.build_mutation_bank() if c["name"] == "positive unknown optional owner content")
        doc = gate.apply_case(self.baseline, case)
        index = gate._resolve_selector(doc["artifacts"], "@embedded")
        self.assertEqual(doc["artifacts"][index]["owner"], "acme.unknown-owner")
        self.assertEqual(self.problems(doc), [])

    def test_nullable_requirements_absent_vs_null(self) -> None:
        nullable = copy.deepcopy(self.baseline)
        nullable["artifacts"][0]["requirements"] = None
        nullable["entrypoints"][0]["requirements"] = None
        self.assertEqual(self.problems(nullable), [])
        doc = copy.deepcopy(self.baseline)
        del doc["artifacts"][0]["requirements"]
        self.assertTrue(any("requirements" in p for p in self.problems(doc)))
        doc = copy.deepcopy(self.baseline)
        del doc["entrypoints"][0]["requirements"]
        self.assertTrue(any("requirements" in p for p in self.problems(doc)))

    def test_reference_root_sha_omitted_accepted_null_refused(self) -> None:
        index = gate._resolve_selector(self.baseline["artifacts"], "@reference")
        doc = copy.deepcopy(self.baseline)
        doc["artifacts"][index]["storage"].pop("root_sha256", None)
        self.assertEqual(self.problems(doc), [])
        omission = next(
            case for case in gate.build_mutation_bank()
            if case["name"] == "positive reference root_sha256 omitted"
        )
        self.assertEqual(self.problems(gate.apply_case(doc, omission)), [])
        doc["artifacts"][index]["storage"]["root_sha256"] = None
        self.assertTrue(self.problems(doc))

    def test_unfamiliar_link_relation_stays_accepted(self) -> None:
        doc = copy.deepcopy(self.baseline)
        doc["links"][0]["relation"] = "never.seen.relation"
        self.assertEqual(self.problems(doc), [])

    def test_unsupported_kind_and_future_versions_refused(self) -> None:
        doc = copy.deepcopy(self.baseline)
        doc["artifacts"][0]["kind"] = "calibration"
        self.assertTrue(self.problems(doc))
        for field in ("schema_version", "minimum_reader_version"):
            for bad in (2, True, "1", 0):
                doc = copy.deepcopy(self.baseline)
                doc[field] = bad
                self.assertTrue(self.problems(doc), (field, bad))

    def test_non_object_instances_refused(self) -> None:
        for bad in ([], "text", 1, None):
            self.assertTrue(self.problems(bad))

    def test_remote_reference_is_never_fetched(self) -> None:
        schema = {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.invalid/local.schema.json",
            "type": "object",
            "properties": {"a": {"$ref": "https://example.invalid/remote.schema.json"}},
        }
        validator = gate.build_validator(schema)
        with mock.patch("urllib.request.urlopen") as urlopen, mock.patch("socket.create_connection") as conn:
            with self.assertRaises(gate.N4aContainerContractError):
                gate.validate_instance(validator, {"a": 1})
            urlopen.assert_not_called()
            conn.assert_not_called()

    def test_internal_fragment_references_resolve_offline(self) -> None:
        with mock.patch("urllib.request.urlopen") as urlopen, mock.patch("socket.create_connection") as conn:
            self.assertEqual(self.problems(copy.deepcopy(self.baseline)), [])
            doc = copy.deepcopy(self.baseline)
            doc["members"][0]["sha256"] = "bad"
            self.assertTrue(self.problems(doc))
            urlopen.assert_not_called()
            conn.assert_not_called()

    def test_malformed_schema_is_an_error(self) -> None:
        with self.assertRaises(gate.N4aContainerContractError):
            gate.build_validator({"type": 12})

    def test_known_lexical_limitation_table_is_exact_and_serializable(self) -> None:
        table = gate.KNOWN_LEXICAL_LIMITATIONS
        self.assertEqual(json.loads(json.dumps(table)), table)
        self.assertEqual(len(table), 6)
        self.assertEqual(
            [(e["path"][-1], e["raw_token"]) for e in table],
            [("schema_version", "1.0"), ("schema_version", "1e0"), ("minimum_reader_version", "1.0"),
             ("byte_length", "26.0"), ("version", "9.0"), ("schema_version", "17.0")],
        )
        self.assertEqual({e["classification"] for e in table}, {"schema_accepts_rust_refuses"})
        names = [e["name"] for e in table]
        self.assertEqual(len(names), len(set(names)))

    def test_known_lexical_limitations_keep_raw_token_and_are_accepted(self) -> None:
        for entry in gate.KNOWN_LEXICAL_LIMITATIONS:
            raw_text, document = gate.build_limitation_document(self.baseline, entry)
            self.assertNotIn(gate.LIMITATION_PLACEHOLDER, raw_text)
            self.assertRegex(
                raw_text,
                rf'"{re.escape(entry["path"][-1])}": {re.escape(entry["raw_token"])}(?=[,}}])',
            )
            value = gate._walk(document, list(entry["path"]))
            self.assertIsInstance(value, float, entry["name"])
            self.assertTrue(value.is_integer(), entry["name"])
            self.assertEqual(self.problems(document), [], entry["name"])
        e0 = gate.KNOWN_LEXICAL_LIMITATIONS[1]
        raw_text, _ = gate.build_limitation_document(self.baseline, e0)
        self.assertIn('"schema_version": 1e0', raw_text)
        self.assertNotIn('"schema_version": 1.0', raw_text)

    def test_known_lexical_limitations_are_counted_apart_from_bank(self) -> None:
        known = gate.run_known_lexical_limitations(self.validator, self.baseline)
        self.assertEqual(known["count"], 6)
        self.assertEqual(known["classification"], "schema_accepts_rust_refuses")
        bank = gate.build_mutation_bank()
        bank_names = {case["name"] for case in bank}
        self.assertTrue(bank_names.isdisjoint(e["name"] for e in gate.KNOWN_LEXICAL_LIMITATIONS))
        counts = gate.run_mutation_bank(self.validator, self.baseline)
        self.assertEqual(counts["positive"] + counts["negative"], len(bank))

    def test_genuine_non_integers_bools_and_future_container_stay_refused(self) -> None:
        for field in ("schema_version", "minimum_reader_version"):
            for raw in ("1.5", "true", "2"):
                text = json.dumps({**self.baseline, field: gate.LIMITATION_PLACEHOLDER})
                doc = gate.parse_json_text(text.replace(json.dumps(gate.LIMITATION_PLACEHOLDER), raw))
                self.assertTrue(self.problems(doc), (field, raw))
        doc = copy.deepcopy(self.baseline)
        doc["schema"] = "nirs4all.n4a.container.v2"
        self.assertTrue(self.problems(doc))

    def test_limitation_no_longer_accepted_is_an_error(self) -> None:
        strict = copy.deepcopy(self.schema)
        strict["properties"]["schema_version"] = {"type": "string"}
        validator = gate.build_validator(strict)
        with self.assertRaises(gate.N4aContainerContractError):
            gate.run_known_lexical_limitations(validator, self.baseline)

    def test_cli_default_gate_passes_and_reports_counts(self) -> None:
        digest = hashlib.sha256((gate.FIXTURES_ROOT / "combined/manifest.json").read_bytes()).hexdigest()
        result = run_cli()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("6 manifests validated", result.stdout)
        self.assertIn("JSON shape only", result.stdout)
        self.assertIn("known lexical limitations (separate category, not a pass): 6", result.stdout)
        self.assertIn("schema_accepts_rust_refuses", result.stdout)
        self.assertIn("native qualification is separate", result.stdout)
        self.assertIn("no native parity execution claimed", result.stdout)
        self.assertEqual(hashlib.sha256((gate.FIXTURES_ROOT / "combined/manifest.json").read_bytes()).hexdigest(), digest)

    def test_cli_executes_instance_validation_on_mutated_manifest(self) -> None:
        for name in gate.FIXTURE_NAMES:
            root = self.tmp_copy()
            path = root / name / "manifest.json"
            doc = json.loads(path.read_text(encoding="utf-8"))
            doc["unexpected"] = 1
            path.write_text(json.dumps(doc), encoding="utf-8")
            result = run_cli("--fixtures-root", str(root))
            self.assertEqual(result.returncode, 1, name)
            self.assertIn(str(path), result.stderr)
            self.assertIn("$", result.stderr)

    def test_cli_mutated_corpus_value_is_located(self) -> None:
        root = self.tmp_copy()
        path = root / "model/manifest.json"
        doc = json.loads(path.read_text(encoding="utf-8"))
        doc["members"][0]["sha256"] = "XYZ"
        path.write_text(json.dumps(doc), encoding="utf-8")
        result = run_cli("--fixtures-root", str(root))
        self.assertEqual(result.returncode, 1)
        self.assertIn("$.members[0].sha256", result.stderr)

    def test_cli_missing_input_is_failure_not_skip(self) -> None:
        for name in gate.FIXTURE_NAMES:
            root = self.tmp_copy()
            (root / name / "manifest.json").unlink()
            result = run_cli("--fixtures-root", str(root))
            self.assertEqual(result.returncode, 1, name)
            self.assertIn("absent", result.stderr)

    def test_cli_empty_corpus_is_failure(self) -> None:
        empty = Path(tempfile.mkdtemp(prefix="n4a-contract-empty-"))
        self.addCleanup(shutil.rmtree, empty, True)
        result = run_cli("--fixtures-root", str(empty))
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        with self.assertRaises(gate.N4aContainerContractError):
            gate.validate_manifests(self.validator, [])

    def test_cli_non_json_and_malformed_instances_fail(self) -> None:
        for payload in ("not json", "{", '{"a": NaN}', '{"a": 1, "a": 2}', ""):
            root = self.tmp_copy()
            (root / "score/manifest.json").write_text(payload, encoding="utf-8")
            result = run_cli("--fixtures-root", str(root))
            self.assertEqual(result.returncode, 1, payload)
            self.assertIn("malformed JSON", result.stderr)
        root = self.tmp_copy()
        (root / "score/manifest.json").write_bytes(b"\xff\xfe\x00")
        result = run_cli("--fixtures-root", str(root))
        self.assertEqual(result.returncode, 1)
        self.assertIn("unreadable", result.stderr)

    def test_cli_schema_problems_fail(self) -> None:
        directory = Path(tempfile.mkdtemp(prefix="n4a-contract-schema-"))
        self.addCleanup(shutil.rmtree, directory, True)
        missing = directory / "missing.json"
        result = run_cli("--schema", str(missing))
        self.assertEqual(result.returncode, 1)
        self.assertIn("schema is absent", result.stderr)
        bad = directory / "bad.json"
        bad.write_text("{", encoding="utf-8")
        self.assertEqual(run_cli("--schema", str(bad)).returncode, 1)
        wrong = directory / "wrong.json"
        wrong.write_text(json.dumps({"type": 12}), encoding="utf-8")
        result = run_cli("--schema", str(wrong))
        self.assertEqual(result.returncode, 1)
        self.assertIn("not well-formed", result.stderr)

    def test_cli_combined_without_reference_fails_mutation_baseline(self) -> None:
        root = self.tmp_copy()
        path = root / "combined/manifest.json"
        doc = json.loads(path.read_text(encoding="utf-8"))
        doc["artifacts"] = [a for a in doc["artifacts"] if a["storage"]["kind"] != "reference"]
        path.write_text(json.dumps(doc), encoding="utf-8")
        result = run_cli("--fixtures-root", str(root))
        self.assertEqual(result.returncode, 1)
        self.assertIn("storage.kind", result.stderr)

    def test_missing_dependency_exits_nonzero_with_install_hint(self) -> None:
        code = (
            "import sys, runpy\n"
            "class Block:\n"
            "    def find_spec(self, name, path=None, target=None):\n"
            "        if name.split('.')[0] in ('jsonschema', 'referencing'):\n"
            "            raise ImportError('blocked ' + name)\n"
            "sys.meta_path.insert(0, Block())\n"
            "for m in [m for m in sys.modules if m.split('.')[0] in ('jsonschema', 'referencing')]:\n"
            "    del sys.modules[m]\n"
            f"runpy.run_path({str(SCRIPT)!r}, run_name='__main__')\n"
        )
        result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing dependency", result.stderr)
        self.assertIn("requirements-contracts.txt", result.stderr)


if __name__ == "__main__":
    unittest.main()
