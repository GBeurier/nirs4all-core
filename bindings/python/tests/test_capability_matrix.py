"""Native execution-profile registry gate.

``compat/capabilities.toml`` is the single owner of the supported native
execution profiles (``native-workflow@1``, ``native-conformal@1``,
``native-methods-package@1``, the canonical V1 ``.n4a`` profile
``native-n4a-canonical@1`` and ``n4a-container@1``). These
tests parse that owner file, rebuild the payload with the generator, and compare
it structurally with the data embedded in every distributable host, with the
manifests the hosts actually return, and with the source files that define each
claimed entrypoint. No legacy, parity-labelled or extension profile exists.

Because the gate reads binding sources directly, it runs in the required Python
suite without R, Node, Octave, or a Rust toolchain; the JS, Rust, MATLAB and R
suites additionally compare their live return values with the same fixture.
"""

from __future__ import annotations

import ast
import copy
import importlib.util
import json
import re
import unittest
from pathlib import Path

import nirs4all_core as n4core

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - local Python < 3.11 fallback
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[3]
CAPABILITIES = ROOT / "compat/capabilities.toml"
OPERATORS_DOC = ROOT / "docs/OPERATORS.md"
R_SIBLING = ROOT.parent / "nirs4all-r"
WEB_SIBLING = ROOT.parent / "nirs4all-web"

EXPECTED_SURFACES = ("python", "javascript_wasm", "rust", "matlab_octave")
EXPECTED_OPERATIONS = ("compile", "fit", "predict", "import", "export", "produce", "inspect", "calibrate", "retrain", "read", "write", "update")
CANONICAL = "native-n4a-canonical"
STORAGE = "n4a-container"
NATIVE_PACKAGE = "native-methods-package"
EXPECTED_PROFILE_IDS = ("native-workflow", "native-conformal", NATIVE_PACKAGE, CANONICAL, STORAGE)
# MATLAB/Octave declares no entrypoint in the native package profile, so its surfaces are explicit.
EXPECTED_PROFILE_SURFACES = {NATIVE_PACKAGE: ("python", "javascript_wasm", "rust")}
CANONICAL_ACTIONS = ("produce", "inspect", "calibrate", "retrain")
STORAGE_ONLY = ("read", "write", "update")
S, U = "supported", "unsupported"

# The complete operation declaration: (status per surface in EXPECTED_SURFACES order).
# The envelope/V2 profiles declare the canonical V1 actions as explicit unsupported cells, and the
# canonical profile declares compile/fit/import/export unsupported: no operation is an alias.
EXPECTED_STATUS = {
    "native-workflow": {
        "compile": (U, U, S, U),
        "fit": (S, S, S, S),
        "predict": (S, S, S, S),
        "import": (S, S, U, S),
        "export": (S, S, U, S),
        **{action: (U, U, U, U) for action in CANONICAL_ACTIONS},
        **{action: (U, U, U, U) for action in STORAGE_ONLY},
    },
    "native-conformal": {
        "compile": (U, U, U, U),
        "fit": (S, S, S, S),
        "predict": (S, S, S, S),
        "import": (S, S, U, S),
        "export": (S, S, U, S),
        **{action: (U, U, U, U) for action in CANONICAL_ACTIONS},
        **{action: (U, U, U, U) for action in STORAGE_ONLY},
    },
    NATIVE_PACKAGE: {
        "compile": (U, U, U, U),
        "fit": (U, U, U, U),
        "predict": (S, S, S, U),
        "import": (S, S, S, U),
        "export": (S, S, S, U),
        **{action: (U, U, U, U) for action in CANONICAL_ACTIONS},
        **{action: (U, U, U, U) for action in STORAGE_ONLY},
    },
    CANONICAL: {
        "compile": (U, U, U, U),
        "fit": (U, U, U, U),
        "predict": (S, S, S, S),
        "import": (U, U, U, U),
        "export": (U, U, U, U),
        "produce": (S, S, S, S),
        "inspect": (S, S, S, S),
        "calibrate": (U, U, S, U),
        "retrain": (S, S, S, S),
        **{action: (U, U, U, U) for action in STORAGE_ONLY},
    },
    # Storage only: no compile, fit, predict, import, export, produce, calibrate or retrain.
    STORAGE: {
        **{action: (U, U, U, U) for action in ("compile", "fit", "predict", "import", "export", "produce", "calibrate", "retrain")},
        "inspect": (S, S, S, S),
        "read": (S, S, S, S),
        "write": (S, S, S, S),
        "update": (S, S, S, S),
    },
}
# Exact entrypoint symbols per surface (None = unsupported); a tuple lists several symbols.
EXPECTED_SYMBOLS = {
    ("native-workflow", "compile"): (None, None, "validate_pipeline", None),
    ("native-workflow", "fit"): ("run_pipeline", "runBrowserPipeline", "run_pipeline", "runPipeline"),
    ("native-workflow", "predict"): (
        "NativePipeline.predict",
        ("BrowserNativePipeline.predict", "predictBrowserPipeline"),
        "predict_pipeline",
        "NativePipeline.predict",
    ),
    ("native-workflow", "import"): ("NativePipeline.load", "loadBrowserPipeline", None, "NativePipeline.load"),
    ("native-workflow", "export"): ("NativePipeline.export", "BrowserNativePipeline.export", None, "NativePipeline.export"),
    ("native-conformal", "fit"): ("calibrate", "calibrate", "calibrate_archive", "calibrate"),
    ("native-conformal", "predict"): ("predict_calibrated", "predictCalibrated", "calibrated_prediction_json", "predictCalibrated"),
    ("native-conformal", "import"): ("load_calibrated", "loadCalibrated", None, "loadCalibrated"),
    ("native-conformal", "export"): ("export_calibrated", "exportCalibrated", None, "exportCalibrated"),
    (NATIVE_PACKAGE, "predict"): ("predict_methods_archive_v2_matrix", "replayMethodsArchiveV2", "predict_methods_archive_v2_matrix", None),
    (NATIVE_PACKAGE, "import"): (("open_native_package", "read_archive_v2_payloads"), "readPortableArchiveV2", "read_native_package", None),
    (NATIVE_PACKAGE, "export"): (
        ("write_native_predictor_package", "write_native_refit_package"),
        ("writeNativePredictorPackage", "writeNativeRefitPackage"),
        "write_native_package_file",
        None,
    ),
    (CANONICAL, "predict"): ("predict_n4a_native", "predictN4aNative", "predict_n4a_native", "n4aPredict"),
    (CANONICAL, "produce"): ("produce_n4a_native", "produceN4aNative", "produce_n4a_native", "n4aProduce"),
    (CANONICAL, "inspect"): ("inspect_n4a_bytes", "inspectN4aBytes", "inspect_n4a_view_json", "n4aInspect"),
    (CANONICAL, "calibrate"): (None, None, "calibrate_n4a_native", None),
    (CANONICAL, "retrain"): ("retrain_n4a_native", "retrainN4aNative", "retrain_n4a_native", "n4aRetrain"),
    (STORAGE, "inspect"): ("open_n4a_container", "openN4aContainer", "n4a_container_view", "n4aInspect"),
    (STORAGE, "read"): ("N4aContainer.member", "openN4aContainer", "open_owned_n4a_bytes", "n4aRead"),
    (STORAGE, "write"): ("write_n4a_container", "writeN4aContainer", "write_n4a_bytes", "n4aWrite"),
    (STORAGE, "update"): ("write_n4a_container", "writeN4aContainer", "write_n4a_update", "n4aWrite"),
}
# Per-surface runtime requirements, exact. JS/WASM runs in the browser: no native library path, no CLI.
_LIBRARY_CLI = (
    "Methods native library ({lib})",
    "nirs4all-core-archive CLI (cli argument or NIRS4ALL_CORE_CLI)",
)
EXPECTED_REQUIREMENTS = {
    profile_id: {
        "python": [_LIBRARY_CLI[0].format(lib="methods_library_path argument or N4M_LIBRARY_PATH"), _LIBRARY_CLI[1]],
        "javascript_wasm": [
            "Matching Core, DAG-ML, Methods and IO WASM builds loaded in the browser; no native library path and no CLI",
        ],
        "rust": [
            "Methods native library path passed as "
            + ("the library argument" if profile_id == "native-workflow" else "methods_library_path")
            + "; no CLI"
        ],
        "matlab_octave": [_LIBRARY_CLI[0].format(lib="methodsLibrary argument or N4M_LIBRARY_PATH"), _LIBRARY_CLI[1]],
    }
    for profile_id in ("native-workflow", "native-conformal")
}
# The canonical profile needs no CLI on Python or Rust (native extension / library API) but does on MATLAB/Octave.
EXPECTED_REQUIREMENTS[CANONICAL] = {
    "python": [
        "nirs4all-core native extension (nirs4all_core._native); no CLI",
        "Methods native library path and SHA-256 (methods_library_path, methods_library_sha256) for produce, predict and retrain; inspect needs neither",
    ],
    "javascript_wasm": [
        "Matching Core, DAG-ML, Methods and IO WASM builds with the Methods WASM manifest of the same cohort and the DAG-ML estimator controller; no native library path and no CLI",
        "inspect loads only the Core WASM bridge",
    ],
    "rust": [
        "Methods native library path and SHA-256 passed in NativeRuntime for produce, predict, calibrate and retrain; inspect_n4a_view_json needs no runtime; no CLI",
    ],
    "matlab_octave": [
        "Methods native library (methodsLibrary argument or N4M_LIBRARY_PATH) and its SHA-256 (methodsLibrarySha256 or N4M_EXPECTED_LIBRARY_SHA256) for produce, predict and retrain",
        "nirs4all-core-archive CLI with the n4a-* commands (cli argument or NIRS4ALL_CORE_CLI)",
    ],
}
EXPECTED_REQUIREMENTS[NATIVE_PACKAGE] = {
    "python": [
        "nirs4all-core native extension (nirs4all_core._native); no CLI",
        "Methods native library path and SHA-256 (methods_library_path, methods_library_sha256) for predict; reading and writing need neither",
    ],
    "javascript_wasm": ["Matching Core and Methods WASM builds loaded in the browser; no native library path and no CLI"],
    "rust": ["Methods native library path and SHA-256 passed with the replay request for predict; read and write need neither; no CLI"],
}
EXPECTED_REQUIREMENTS[STORAGE] = {
    "python": ["nirs4all-core native extension (nirs4all_core._native); no CLI and no Methods library"],
    "javascript_wasm": [
        "The Core WASM bridge only, with no Methods, DAG-ML or IO WASM builds, no native library path and no CLI",
    ],
    "rust": ["No runtime library and no CLI; the functions run in process over caller bytes"],
    "matlab_octave": [
        "nirs4all-core-archive CLI with the n4a-write, n4a-read and n4a-inspect commands (cli argument or NIRS4ALL_CORE_CLI); no Methods library",
    ],
}
EXPECTED_CONTENT_TYPES = {
    NATIVE_PACKAGE: [
        {
            "schema": "dagml.portable_predictor_package",
            "owner": "dag-ml",
            "carried_in": (
                "Package V2 root artifact `model` of a Core V1 container that requires the native-methods-package@1 "
                "feature; the exact package JSON bytes, with the TrainingOutcome as its evidence artifact"
            ),
        },
        {
            "schema": "dagml.portable_refit_package",
            "owner": "dag-ml",
            "carried_in": (
                "Package V3 root artifact `full_refit` of a Core V1 container that requires the native-methods-package@1 "
                "feature; a standalone child with no parent outcome, refit-of link or score"
            ),
        },
    ],
    STORAGE: [
        {
            "schema": "nirs4all.n4a.container.v1",
            "owner": "nirs4all-core",
            "carried_in": (
                "raw canonical n4a archive bytes (physical profile zip-stored.v1); every member payload is opaque "
                "owner or host bytes that are stored and returned unchanged"
            ),
        },
    ],
    "native-workflow": [
        {"schema": "nirs4all.native-pipeline.v1", "owner": "nirs4all-core", "carried_in": "native pipeline JSON envelope"},
    ],
    CANONICAL: [
        {
            "schema": "nirs4all.n4a.container.v1",
            "owner": "nirs4all-core",
            "carried_in": (
                "raw canonical n4a archive bytes (physical profile zip-stored.v1); the owner artifacts and metadata "
                "inside stay authoritative and hosts never re-encode them"
            ),
        },
        {
            "schema": "nirs4all.dataset.v1",
            "owner": "nirs4all-io",
            "carried_in": (
                "the exact IO dataset record JSON bytes passed unchanged to produce, retrain and predict; only this "
                "JSON record route is described here, not general ingestion"
            ),
        },
    ],
    "native-conformal": [
        {
            "schema": "nirs4all.calibrated.v1",
            "owner": "nirs4all-core",
            "carried_in": (
                "Core calibration wrapper JSON, only on the content_routes that name this schema: the Rust "
                "calibrate_archive return value, the CLI fit/load responses and the MATLAB struct. The JS/WASM host "
                "object and the Python host object carry no schema id; import/export transfer the native DAG Package V2 (scientific package schema2) "
                "carried in the owner-neutral Core V1 container (physical schema1) via native-methods-package@1"
            ),
        },
    ],
}
_S = "nirs4all.calibrated.v1"
_WRAPPER_FIELDS = ("calibration", "calibration_replay", "archive_sha256", "model_archive", "training_outcome")
# (operation, surface, route, carries, schema id or None, fields or None): the complete per-route content table.
EXPECTED_ROUTES = [
    ("fit", "rust", "public_return", "wrapper_json", _S, _WRAPPER_FIELDS),
    ("fit", "rust", "archive_transfer", "native_archive_v2", None, None),
    ("fit", "python", "cli_response", "wrapper_json", _S, _WRAPPER_FIELDS),
    ("fit", "python", "public_return", "host_object", None, ("archive", "calibration")),
    ("fit", "python", "archive_transfer", "native_archive_v2", None, None),
    ("fit", "matlab_octave", "cli_response", "wrapper_json", _S, _WRAPPER_FIELDS),
    ("fit", "matlab_octave", "public_return", "host_object", _S, ("archive", "calibration", "native_json", "cli")),
    ("fit", "matlab_octave", "archive_transfer", "native_archive_v2", None, None),
    ("fit", "javascript_wasm", "public_return", "host_object", None, ("archive", "calibration")),
    ("fit", "javascript_wasm", "archive_transfer", "native_archive_v2", None, None),
    ("import", "python", "cli_response", "wrapper_json", _S, ("calibration",)),
    ("import", "python", "public_return", "host_object", None, ("archive", "calibration")),
    ("import", "python", "archive_transfer", "native_archive_v2", None, None),
    ("import", "matlab_octave", "cli_response", "wrapper_json", _S, ("calibration",)),
    ("import", "matlab_octave", "public_return", "host_object", _S, ("archive", "calibration", "native_json", "cli")),
    ("import", "matlab_octave", "archive_transfer", "native_archive_v2", None, None),
    ("import", "javascript_wasm", "public_return", "host_object", None, ("archive", "calibration")),
    ("import", "javascript_wasm", "archive_transfer", "native_archive_v2", None, None),
    ("export", "python", "archive_transfer", "native_archive_v2", None, None),
    ("export", "matlab_octave", "archive_transfer", "native_archive_v2", None, None),
    ("export", "javascript_wasm", "archive_transfer", "native_archive_v2", None, None),
]
EXPECTED_MANIFEST_KEYS = {"schema", "aggregate", "runtime_surfaces", "execution_profiles"}
REMOVED_PUBLIC_NAMES = (
    "runtime_contracts",
    "artifact_contracts",
    "controller_capabilities",
    "required_keyword_registry_entries",
    "PORTABLE_OPERATOR_CLASSES",
)
# Words that must not appear anywhere in the registry, its generated payload or
# the profile embedded in any host: the removed legacy/extension scopes.
RETIRED_TERMS = (
    "legacy",
    "wasm-methods-extension",
    "portable_pipeline",
    "pipeline.portable_methods",
    "runtime_contract",
    "controller_capabilit",
    "conformal_store",
    "parity-validated",
    "portable_operator_subset",
    "extends",
)


def _claim(cell: dict, symbol: str, source: str) -> None:
    """Turn an unsupported cell into a well-formed supported one, so only the group rule can refuse it."""

    cell.pop("reason", None)
    cell.update(status="supported", entrypoints=[{"symbol": symbol, "source": source}])


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _load_capabilities() -> dict:
    return tomllib.loads(_read(CAPABILITIES))


def _load_generator():
    spec = importlib.util.spec_from_file_location(
        "generate_capability_profiles", ROOT / "scripts/generate_capability_profiles.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


GENERATOR = _load_generator()


def _ladder_from_operators_doc() -> set[str]:
    """The capability vocabulary, parsed from the OPERATORS.md ladder list."""

    section = re.search(r"## Capability Levels\n(.*?)\n## ", _read(OPERATORS_DOC), re.DOTALL)
    if section is None:
        raise AssertionError("could not locate the Capability Levels section")
    return set(re.findall(r"(?m)^-\s+`([a-z-]+)`:", section.group(1)))


def _owner_payload() -> dict:
    return GENERATOR.build_payload(_load_capabilities())


def _profile(payload: dict, profile_id: str) -> dict:
    return next(item for item in payload["profiles"] if item["id"] == profile_id)


def _host_payloads() -> dict[str, dict]:
    """Parse the data embedded in each distributable host back into payloads."""

    from nirs4all_core import _capability_profiles as python_data

    out = {"python": json.loads(python_data.EXECUTION_PROFILES_JSON)}
    wasm = _read(ROOT / GENERATOR.WASM_TARGET)
    out["wasm"] = json.loads(re.search(r"deepFreeze\((\{.*\})\);\s*$", wasm, re.DOTALL).group(1))
    rust = _read(ROOT / GENERATOR.RUST_TARGET)
    out["rust"] = json.loads(re.search(r'r######"\n(.*)"######;', rust, re.DOTALL).group(1))
    matlab_lines = re.findall(r"^lines\{end \+ 1\} = '(.*)';$", _read(ROOT / GENERATOR.MATLAB_TARGET), re.MULTILINE)
    out["matlab"] = json.loads("\n".join(line.replace("''", "'") for line in matlab_lines))
    out["canonical"] = json.loads(_read(ROOT / GENERATOR.CANONICAL_JSON))
    return out


def _defines(source: Path, symbol: str) -> bool:
    """True when ``source`` defines ``symbol`` (``Class.method`` for class members)."""

    text = _read(source)
    owner, _, member = symbol.partition(".")
    suffix = source.suffix
    if suffix == ".py":
        tree = ast.parse(text)
        for node in tree.body:
            if isinstance(node, ast.ClassDef) and node.name == owner and member:
                return any(isinstance(m, ast.FunctionDef) and m.name == member for m in node.body)
            if isinstance(node, ast.FunctionDef) and node.name == owner and not member:
                return True
        return False
    if suffix == ".js":
        if not member:
            return bool(re.search(rf"^export (async )?function {owner}\b|^export class {owner}\b", text, re.MULTILINE))
        cls = re.search(rf"^export class {owner}\b", text, re.MULTILINE)
        return bool(cls) and bool(re.search(rf"^  (async |static )?{member}\(", text[cls.end():], re.MULTILINE))
    if suffix == ".m":
        if not member:
            return source.stem == owner and bool(re.match(r"\s*function\b", text))
        return source.stem == owner and bool(re.search(rf"^\s*function [^\n]*\b{member}\(", text, re.MULTILINE))
    if suffix == ".rs":
        return not member and bool(re.search(rf"^\s*pub fn {owner}\b", text, re.MULTILINE))
    raise AssertionError(f"unsupported source type: {source}")


class RegistryOwnerTests(unittest.TestCase):
    def test_owner_file_holds_only_the_native_registry(self) -> None:
        caps = _load_capabilities()
        self.assertEqual(
            set(caps),
            {"runtime_surfaces", "upstream_domains", "execution_profiles", "execution_profile", "external_federation"},
        )
        self.assertEqual(tuple(caps["runtime_surfaces"]), EXPECTED_SURFACES)
        self.assertEqual(tuple(item["id"] for item in caps["execution_profile"]), EXPECTED_PROFILE_IDS)
        self.assertEqual(tuple(caps["execution_profiles"]["operations"]), EXPECTED_OPERATIONS)

    def test_retired_scopes_do_not_appear_in_registry_payload_or_host_data(self) -> None:
        texts = {"toml": _read(CAPABILITIES), "payload": json.dumps(_owner_payload())}
        texts.update({host: json.dumps(payload) for host, payload in _host_payloads().items()})
        for label, text in texts.items():
            for term in RETIRED_TERMS:
                with self.subTest(source=label, term=term):
                    self.assertNotIn(term, text)

    def test_ladder_vocabulary_covers_the_upstream_domain_level(self) -> None:
        ladder = _ladder_from_operators_doc()
        self.assertEqual(ladder, {"metadata", "plan", "execute-local", "execute-remote", "parity-validated"})
        self.assertIn(_load_capabilities()["upstream_domains"]["level"], ladder)

    def test_upstream_domains_only_claim_metadata(self) -> None:
        domains = _load_capabilities()["upstream_domains"]
        self.assertEqual(domains["level"], "metadata")
        keys = set(domains["keys"])
        self.assertTrue(keys <= set(n4core.upstreams))
        # `methods` is the executed upstream, not a metadata-only domain.
        self.assertNotIn("methods", keys)
        self.assertEqual(keys | {"methods"}, set(n4core.upstreams))

    def test_registry_validation_rejects_inconsistent_profiles(self) -> None:
        caps = _load_capabilities()

        def mutated(change) -> dict:
            clone = copy.deepcopy(caps)
            change(clone)
            return clone

        def add_profile(c: dict) -> None:
            extra = copy.deepcopy(c["execution_profile"][0])
            extra["id"] = "other-profile"
            c["execution_profile"].append(extra)

        cases = {
            "supported without entrypoints": lambda c: c["execution_profile"][0]["operations"]["fit"]["python"].pop("entrypoints"),
            "unsupported without reason": lambda c: c["execution_profile"][0]["operations"]["import"]["javascript_wasm"].pop("reason"),
            "supported outside profile surfaces": lambda c: c["execution_profile"][0]["surfaces"].remove("python"),
            "R promoted to a Core runtime surface": lambda c: c["runtime_surfaces"].append("r"),
            "missing operation": lambda c: c["execution_profile"][0]["operations"].pop("export"),
            "extra profile": add_profile,
            "requirements missing a surface": lambda c: c["execution_profile"][0]["runtime_requirements"].pop("rust"),
            "empty requirements": lambda c: c["execution_profile"][1]["runtime_requirements"].__setitem__("python", []),
            "dropped profile": lambda c: c["execution_profile"].pop(),
            "profile version drift": lambda c: c["execution_profile"][1].__setitem__("version", 2),
            "registry version drift": lambda c: c["execution_profiles"].__setitem__("version", 2),
            "empty content types": lambda c: c["execution_profile"][1].__setitem__("content_types", []),
            "federation of an unknown profile": lambda c: c["external_federation"][0]["profiles"].append("nope"),
            "canonical action claimed by the workflow profile": lambda c: _claim(
                c["execution_profile"][0]["operations"]["produce"]["rust"], "produce_n4a_native", "bindings/rust/nirs4all/src/n4a_owner_native.rs"),
            "canonical fit aliasing produce": lambda c: _claim(
                c["execution_profile"][3]["operations"]["fit"]["rust"], "produce_n4a_native", "bindings/rust/nirs4all/src/n4a_owner_native.rs"),
            "canonical import aliasing inspect": lambda c: _claim(
                c["execution_profile"][3]["operations"]["import"]["python"], "inspect_n4a_bytes", "bindings/python/src/nirs4all_core/_n4a.py"),
            "one symbol under two canonical actions": lambda c: c["execution_profile"][3]["operations"]["retrain"]["python"].update(
                entrypoints=[{"symbol": "produce_n4a_native", "source": "bindings/python/src/nirs4all_core/_n4a.py"}]),
            "canonical produce unsupported everywhere": lambda c: [
                c["execution_profile"][3]["operations"]["produce"][surface].update(status="unsupported", reason="no")
                or c["execution_profile"][3]["operations"]["produce"][surface].pop("entrypoints")
                for surface in ("python", "javascript_wasm", "rust", "matlab_octave")
            ],
            "R canonical fit alias": lambda c: c["external_federation"][0]["entrypoints"].append(
                {"profile": CANONICAL, "operation": "fit", "symbol": "nirs4all_n4a_produce"}),
            "R canonical action on the workflow profile": lambda c: c["external_federation"][0]["entrypoints"].append(
                {"profile": "native-workflow", "operation": "produce", "symbol": "nirs4all_n4a_produce"}),
        }
        for label, change in cases.items():
            with self.subTest(case=label):
                with self.assertRaises((GENERATOR.RegistryError, KeyError)):
                    GENERATOR.build_payload(mutated(change))


class ExecutionProfileContentTests(unittest.TestCase):
    def test_exact_profile_set_versions_and_content_types(self) -> None:
        payload = _owner_payload()
        self.assertEqual(payload["schema"], "nirs4all-core.execution-profiles.v1")
        self.assertEqual(payload["version"], 1)
        self.assertEqual(tuple(payload["runtime_surfaces"]), EXPECTED_SURFACES)
        self.assertEqual(tuple(payload["operations"]), EXPECTED_OPERATIONS)
        self.assertEqual(tuple(item["id"] for item in payload["profiles"]), EXPECTED_PROFILE_IDS)
        for profile in payload["profiles"]:
            with self.subTest(profile=profile["id"]):
                self.assertEqual(profile["version"], 1)
                self.assertEqual(tuple(profile["surfaces"]), EXPECTED_PROFILE_SURFACES.get(profile["id"], EXPECTED_SURFACES))
                self.assertEqual(profile["content_types"], EXPECTED_CONTENT_TYPES[profile["id"]])
                self.assertEqual(set(profile), {
                    "id", "version", "title", "surfaces", "dependencies", "runtime_requirements",
                    "content_types", "content_routes", "operations", "limits",
                })
                self.assertEqual(profile["runtime_requirements"], EXPECTED_REQUIREMENTS[profile["id"]])
                self.assertEqual(tuple(profile["runtime_requirements"]), EXPECTED_PROFILE_SURFACES.get(profile["id"], EXPECTED_SURFACES))
        conformal = _profile(payload, "native-conformal")
        self.assertIn("dag_ml", conformal["dependencies"])
        self.assertEqual(
            [item["schema"] for item in conformal["content_types"]], ["nirs4all.calibrated.v1"]
        )

    def test_exact_operation_declarations_and_entrypoints(self) -> None:
        payload = _owner_payload()
        for profile_id, table in EXPECTED_STATUS.items():
            profile = _profile(payload, profile_id)
            self.assertEqual(set(profile["operations"]), set(EXPECTED_OPERATIONS))
            for operation, statuses in table.items():
                cells = profile["operations"][operation]
                self.assertEqual(tuple(cells), EXPECTED_SURFACES)
                self.assertEqual(tuple(cells[surface]["status"] for surface in EXPECTED_SURFACES), statuses, f"{profile_id}.{operation}")
                symbols = EXPECTED_SYMBOLS.get((profile_id, operation), (None,) * 4)
                for surface, status, symbol in zip(EXPECTED_SURFACES, statuses, symbols):
                    cell = cells[surface]
                    with self.subTest(profile=profile_id, operation=operation, surface=surface):
                        if status == S:
                            wanted = list(symbol) if isinstance(symbol, tuple) else [symbol]
                            self.assertEqual([entry["symbol"] for entry in cell["entrypoints"]], wanted)
                            self.assertNotIn("reason", cell)
                        else:
                            self.assertIsNone(symbol)
                            self.assertTrue(cell["reason"])
                            self.assertNotIn("entrypoints", cell)

    def test_content_is_declared_per_operation_surface_and_route(self) -> None:
        payload = _owner_payload()
        self.assertEqual(_profile(payload, "native-workflow")["content_routes"], [])
        self.assertEqual(_profile(payload, NATIVE_PACKAGE)["content_routes"], [])
        self.assertEqual(_profile(payload, CANONICAL)["content_routes"], [])
        self.assertEqual(_profile(payload, STORAGE)["content_routes"], [])
        routes = _profile(payload, "native-conformal")["content_routes"]
        actual = [
            (r["operation"], r["surface"], r["route"], r["carries"], r.get("schema"), tuple(r["fields"]) if "fields" in r else None)
            for r in routes
        ]
        self.assertEqual(actual, EXPECTED_ROUTES)
        for route in routes:
            if route["route"] == "archive_transfer":
                self.assertEqual(route["archive_schema_version"], 2)
                self.assertEqual(route["transport"], "native-methods-package@1")
                self.assertEqual(route["calibration_in_archive"], "PortablePredictorPackage.conformal_calibration")
                self.assertNotIn("schema", route)
        # JS/WASM never carries the wrapper schema id; no route invents one.
        self.assertFalse([r for r in routes if r["surface"] == "javascript_wasm" and "schema" in r])
        self.assertEqual({r["schema"] for r in routes if "schema" in r}, {_S})

    def test_route_declarations_match_the_producer_sources(self) -> None:
        routes = _profile(_owner_payload(), "native-conformal")["content_routes"]

        def fields(surface: str, operation: str, route: str) -> set[str]:
            row = next(r for r in routes if (r["surface"], r["operation"], r["route"]) == (surface, operation, route))
            return set(row["fields"])

        # Rust calibrate_archive: the exact wrapper keys.
        conformal = _read(ROOT / "bindings/rust/nirs4all/src/conformal.rs")
        block = re.search(r'json!\(\{"schema": "nirs4all\.calibrated\.v1",(.*?)\}\),?\s*$', conformal, re.MULTILINE | re.DOTALL)
        self.assertIsNotNone(block)
        self.assertEqual(set(re.findall(r'"(\w+)"\s*:', block.group(1))), fields("rust", "fit", "public_return"))
        self.assertIn("calibrate_attached_training_replay_with_derived_context", conformal)

        # CLI: conformal-calibrate forwards that result; conformal-load emits only {schema, calibration}.
        command = _read(ROOT / "bindings/rust/nirs4all/src/archive_command.rs")
        calibrate = re.search(r"fn conformal_calibrate\b.*?\n}\n", command, re.DOTALL).group(0)
        self.assertIn("crate::conformal::calibrate_archive_selected(", calibrate)
        self.assertIn("emit_json(values, &result)", calibrate)
        load = re.search(r'if command == "conformal-load" \{(.*?)\n    \}\n', command, re.DOTALL).group(1)
        self.assertEqual(set(re.findall(r'"(\w+)"\s*:', re.search(r"json!\(\{(.*?)\}\)", load, re.DOTALL).group(1))), {"schema"} | fields("python", "import", "cli_response"))
        # Trust boundary: Core opens the selected native closure (Core V1 container) and requires a Package V2 root.
        self.assertIn("selected_native_archive(", load)
        self.assertIn("archive.predictor_package()?", load)
        selected = re.search(r"fn selected_native_archive\b.*?\n}\n", command, re.DOTALL).group(0)
        self.assertIn("open_native_package(", selected)
        self.assertIn("archive.predictor_package()?;", selected)
        self.assertIn(".conformal_calibration", load)
        self.assertEqual(fields("python", "import", "cli_response"), fields("matlab_octave", "import", "cli_response"))
        self.assertEqual(fields("python", "fit", "cli_response"), fields("rust", "fit", "public_return"))
        # Package schema 2 (DAG-ML predictor package) is independent of the Core V1 container schema:
        # the native package reports the root content version and refuses any root that is not Package2.
        package = _read(ROOT / "bindings/rust/nirs4all/src/n4a_native_package.rs")
        self.assertRegex(package, r"pub fn schema_version\(&self\) -> u32 \{\s*self\.package_version\s*\}")
        self.assertIn("let package_version = closure.root().content_type().version;", package)
        self.assertRegex(package, r"(?s)pub fn predictor_package\(&self\).*?\.predictor_package\(\)\s*\.ok_or_else\(\|\| refusal\(\"selected root is not Package2\"\)\)")

        # Python: host object has no schema id; export copies archive bytes; load/fit go through the CLI.
        source = _read(ROOT / "bindings/python/src/nirs4all_core/_conformal.py")
        self.assertNotIn("calibrated.v1", source)
        tree = ast.parse(source)
        workflow = next(n for n in tree.body if isinstance(n, ast.ClassDef) and n.name == "CalibratedWorkflow")
        self.assertEqual({n.target.id for n in workflow.body if isinstance(n, ast.AnnAssign)}, fields("python", "fit", "public_return"))
        self.assertIn('_invoke(cli, "conformal-calibrate"', source)
        self.assertIn('_invoke(cli, "conformal-load"', source)
        export = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "export_calibrated")
        self.assertIn("copyfileobj", ast.unparse(export))
        self.assertIn("model.archive.open", ast.unparse(export))

        # JS/WASM: never emits the wrapper schema; archive bytes out, conformal_calibration read from the DAG package.
        js = _read(ROOT / "bindings/wasm/src/conformal.js")
        self.assertNotIn("calibrated.v1", js)
        self.assertIn("this.archive = new Uint8Array(archive)", js)
        self.assertIn("this.calibration = structuredClone(calibration)", js)
        self.assertIn("return new Uint8Array(this.archive)", js)
        self.assertIn("JSON.parse(packageJson).conformal_calibration", js)
        # The immutable base is carried by the Core calibrateNativePackage route, which calls the
        # native calibrate_native_package_bytes; no fresh Archive V2 write remains in this file.
        self.assertIn("calibrateNativePackage(source", js)
        self.assertNotIn("writePortableArchiveV2(", js)

        # MATLAB: struct keys are exactly the declared host-object fields; export is a byte copy of model.archive.
        for name, operation in (("calibrate", "fit"), ("loadCalibrated", "import")):
            text = _read(ROOT / f"bindings/matlab/+nirs4all/{name}.m")
            start = text.index("struct('schema', 'nirs4all.calibrated.v1'")
            args, depth, current = [], 0, ""
            for char in text[start + len("struct("):].replace("...", ""):
                if char == "(":
                    depth += 1
                elif char == ")":
                    if depth == 0:
                        break
                    depth -= 1
                if char == "," and depth == 0:
                    args.append(current.strip())
                    current = ""
                else:
                    current += char
            args.append(current.strip())
            keys = [a.strip("'") for a in args[0::2]]
            self.assertEqual(args[1], "'nirs4all.calibrated.v1'")
            self.assertEqual(keys[0], "schema")
            self.assertEqual(set(keys[1:]), fields("matlab_octave", operation, "public_return"), name)
            self.assertEqual(len(args) % 2, 0, name)
        self.assertIn("copyfile(model.archive, path)", _read(ROOT / "bindings/matlab/+nirs4all/exportCalibrated.m"))

    def test_generator_required_targets_fail_instead_of_counting_fewer_targets(self) -> None:
        import subprocess
        import sys
        import tempfile

        script = ROOT / "scripts/generate_capability_profiles.py"
        with tempfile.TemporaryDirectory() as empty:
            def run(*args: str) -> subprocess.CompletedProcess:
                return subprocess.run([sys.executable, str(script), "--check", *args], capture_output=True, text=True)

            for flag, name in (("--require-r", "r"), ("--require-web", "web")):
                done = run("--r-root", empty, "--web-root", empty, flag)
                self.assertEqual(done.returncode, 2, done.stdout)
                self.assertIn(f"required target unavailable: {name}", done.stderr)
                self.assertNotIn("ok  ", done.stdout)  # fails before checking or writing anything
            optional = run("--r-root", empty, "--web-root", empty)
            self.assertEqual(optional.returncode, 0, optional.stderr)
            self.assertEqual(optional.stdout.count("\nok ") + optional.stdout.startswith("ok "), 5)
            self.assertIn("unavailable r:", optional.stdout)
            self.assertIn("unavailable web:", optional.stdout)

    def _generator_fixture(self, stack):
        import tempfile

        base = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        core, r_root, web_root = base / "nirs4all-core", base / "nirs4all-r", base / "nirs4all-web"
        for root, marker in ((r_root, "DESCRIPTION"), (web_root, "web-app/package.json")):
            (root / marker).parent.mkdir(parents=True)
            (root / marker).write_text("fake\n", encoding="utf-8")
        core.mkdir()
        sentinel = web_root / GENERATOR.WEB_TARGET
        sentinel.parent.mkdir(parents=True)
        sentinel.write_text("web sentinel\n", encoding="utf-8")
        return base, core, r_root, web_root, sentinel

    @staticmethod
    def _snapshot(base: Path) -> dict[str, bytes]:
        return {str(p.relative_to(base)): p.read_bytes() for p in sorted(base.rglob("*")) if p.is_file()}

    def _run_generator(self, core: Path, *args: str):
        import contextlib
        import io
        from unittest import mock

        out, err = io.StringIO(), io.StringIO()
        with mock.patch.object(GENERATOR, "ROOT", core), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = GENERATOR.main(list(args))
        return code, out.getvalue(), err.getvalue()

    def test_generator_default_write_skips_web_even_when_present(self) -> None:
        import contextlib

        for explicit_root in (False, True):
            with contextlib.ExitStack() as stack, self.subTest(explicit_web_root=explicit_root):
                base, core, r_root, web_root, sentinel = self._generator_fixture(stack)
                self.assertTrue(all(base in p.parents for p in (core, r_root, web_root, sentinel)))
                extra = ["--web-root", str(web_root)] if explicit_root else []
                code, out, err = self._run_generator(core, *extra)
                self.assertEqual(code, 0, err)
                self.assertEqual(sentinel.read_text(encoding="utf-8"), "web sentinel\n")
                self.assertEqual(out.count("written     "), 6, out)
                self.assertIn("skipped     web:", out)
                self.assertNotIn("unavailable web", out)
                with self._patched_root(core):
                    expected = GENERATOR.targets(_owner_payload(), r_root, web_root)
                for name, (path, content) in expected.items():
                    if name == "web":
                        continue
                    self.assertTrue(base in path.parents)
                    self.assertEqual(path.read_text(encoding="utf-8"), content, name)

    @staticmethod
    def _patched_root(core: Path):
        from unittest import mock

        return mock.patch.object(GENERATOR, "ROOT", core)

    def test_generator_include_web_writes_the_web_snapshot(self) -> None:
        import contextlib

        with contextlib.ExitStack() as stack:
            base, core, r_root, web_root, sentinel = self._generator_fixture(stack)
            code, out, err = self._run_generator(core, "--include-web", "--require-r", "--require-web")
            self.assertEqual(code, 0, err)
            self.assertEqual(out.count("written     "), 7, out)
            with self._patched_root(core):
                expected = GENERATOR.targets(_owner_payload(), r_root, web_root)
            for name, (path, content) in expected.items():
                self.assertTrue(base in path.parents)
                self.assertEqual(path.read_text(encoding="utf-8"), content, name)
            self.assertNotEqual(sentinel.read_text(encoding="utf-8"), "web sentinel\n")

    def test_generator_check_covers_seven_targets_read_only_without_include_web(self) -> None:
        import contextlib

        with contextlib.ExitStack() as stack:
            base, core, r_root, web_root, sentinel = self._generator_fixture(stack)
            self.assertEqual(self._run_generator(core, "--include-web")[0], 0)
            clean = self._snapshot(base)
            code, out, err = self._run_generator(core, "--check", "--require-r", "--require-web")
            self.assertEqual((code, out.count("ok          ")), (0, 7), err)
            self.assertEqual(self._snapshot(base), clean)
            sentinel.write_text("web sentinel\n", encoding="utf-8")
            drifted = self._snapshot(base)
            code, out, err = self._run_generator(core, "--check", "--require-r", "--require-web")
            self.assertEqual(code, 1)
            self.assertEqual((out.count("ok          "), out.count("DRIFT       ")), (6, 1), out)
            self.assertIn("DRIFT       web:", out)
            self.assertIn("1 generated target(s) drifted", err)
            self.assertIn("--include-web", err)
            self.assertEqual(self._snapshot(base), drifted)

    def test_generator_rejects_web_policy_conflicts_before_any_write(self) -> None:
        import contextlib

        for args, message in (
            (("--require-web",), "--require-web in write mode needs --include-web"),
            (("--include-web", "--no-web"), "contradictory"),
            (("--include-web", "--no-web", "--check"), "contradictory"),
        ):
            with contextlib.ExitStack() as stack, self.subTest(args=args):
                base, core, _r, _w, _s = self._generator_fixture(stack)
                before = self._snapshot(base)
                code, out, err = self._run_generator(core, *args)
                self.assertEqual(code, 2, out)
                self.assertIn(message, err)
                self.assertEqual(out, "")
                self.assertEqual(self._snapshot(base), before)

    def test_supported_entrypoints_are_defined_in_their_host_sources(self) -> None:
        checked = 0
        for profile in _owner_payload()["profiles"]:
            for operation in EXPECTED_OPERATIONS:
                for surface, cell in profile["operations"][operation].items():
                    if cell["status"] != S:
                        continue
                    host = {"javascript_wasm": "wasm", "matlab_octave": "matlab"}.get(surface, surface)
                    for entry in cell["entrypoints"]:
                        with self.subTest(profile=profile["id"], operation=operation, surface=surface, symbol=entry["symbol"]):
                            source = ROOT / entry["source"]
                            self.assertTrue(entry["source"].startswith(f"bindings/{host}/"), entry["source"])
                            self.assertTrue(source.exists(), source)
                            self.assertTrue(_defines(source, entry["symbol"]))
                            checked += 1
        # 75 supported entrypoints declared by the five-profile registry (generated fixture count).
        self.assertEqual(checked, 75)

    def test_r_is_externally_federated_and_never_a_core_runtime_surface(self) -> None:
        payload = _owner_payload()
        self.assertNotIn("r", n4core.runtime_surfaces())
        self.assertNotIn("r", payload["runtime_surfaces"])
        record, = payload["external_federation"]
        self.assertEqual(
            (record["surface"], record["repository"], record["package"], record["status"]),
            ("r", "nirs4all-r", "nirs4all", "externally-federated"),
        )
        self.assertIs(record["core_runtime_surface"], False)
        self.assertEqual(tuple(record["profiles"]), EXPECTED_PROFILE_IDS)
        self.assertEqual(
            {(entry["profile"], entry["operation"]) for entry in record["entrypoints"]},
            {(pid, op) for pid in EXPECTED_PROFILE_IDS[:2] for op in ("fit", "predict", "import", "export")}
            | {(CANONICAL, op) for op in ("produce", "inspect", "predict", "retrain")}
            | {(STORAGE, op) for op in ("inspect", "read", "write", "update")},
        )
        # R declares no canonical calibration.
        self.assertNotIn((CANONICAL, "calibrate"), {(e["profile"], e["operation"]) for e in record["entrypoints"]})

    def test_canonical_actions_match_the_real_cli_and_stay_alias_free(self) -> None:
        payload = _owner_payload()
        canonical = _profile(payload, CANONICAL)
        self.assertEqual(canonical["version"], 1)
        # Source support and qualification are stated separately, and inspect is not admission.
        limits = " ".join(canonical["limits"])
        self.assertIn("not parity", limits)
        self.assertIn("pending", limits)
        self.assertIn("Inspect is not executable admission", limits)
        self.assertIn("Raw canonical archive bytes and owner metadata remain authoritative", limits)
        for operation in ("compile", "fit", "import", "export"):
            for surface, cell in canonical["operations"][operation].items():
                self.assertEqual(cell["status"], U, f"{operation}.{surface}")
        # Per surface, no symbol serves two operations.
        for surface in EXPECTED_SURFACES:
            symbols = [
                entry["symbol"]
                for operation in EXPECTED_OPERATIONS
                for entry in canonical["operations"][operation][surface].get("entrypoints", [])
            ]
            self.assertEqual(len(symbols), len(set(symbols)), surface)
        # The CLI behind MATLAB/Octave and R really has exactly these n4a commands, and none for calibration.
        command = _read(ROOT / "bindings/rust/nirs4all/src/archive_command.rs")
        for name in ("n4a-inspect", "n4a-produce", "n4a-predict", "n4a-retrain"):
            self.assertIn(f'"{name}" =>', command)
        self.assertNotIn("n4a-calibrate", command)
        # Calibration is declared on Rust only, and the WASM facade really has no calibration.
        calibrate = canonical["operations"]["calibrate"]
        self.assertEqual([surface for surface, cell in calibrate.items() if cell["status"] == S], ["rust"])
        wasm_n4a = _read(ROOT / "bindings/wasm/src/n4a.js")
        self.assertNotIn("alibrate", wasm_n4a)
        self.assertNotIn("alibrate", _read(ROOT / "bindings/python/src/nirs4all_core/_n4a.py"))
        # R stays outside the Core surfaces; its adapters are the CLI wrappers of the sibling package.
        self.assertNotIn("r", canonical["surfaces"])

    def test_storage_profile_is_storage_only_and_matches_the_real_cli(self) -> None:
        storage = _profile(_owner_payload(), STORAGE)
        self.assertEqual(storage["version"], 1)
        self.assertEqual(storage["dependencies"], [])
        limits = " ".join(storage["limits"])
        self.assertIn("Readable never implies executable", limits)
        self.assertIn("not parity", limits)
        self.assertIn("pending", limits)
        for operation in ("compile", "fit", "predict", "import", "export", "produce", "calibrate", "retrain"):
            for surface, cell in storage["operations"][operation].items():
                self.assertEqual(cell["status"], U, f"{operation}.{surface}")
        command = _read(ROOT / "bindings/rust/nirs4all/src/archive_command.rs")
        for name in ("n4a-inspect", "n4a-write", "n4a-read"):
            self.assertIn(f'"{name}" =>', command)
        # Storage never names a model or a numerical method.
        self.assertNotIn("methods_library", json.dumps(storage))

    def test_web_snapshot_agrees_when_the_web_checkout_is_present(self) -> None:
        target = WEB_SIBLING / GENERATOR.WEB_TARGET
        if not target.exists():
            self.skipTest(f"unavailable: no Web snapshot at {target}")
        payload = _owner_payload()
        text = target.read_text(encoding="utf-8")
        lines = [json.loads(item) for item in re.findall(r'^  (".*"),$', text, re.MULTILINE)]
        embedded = "\n".join(lines)
        digest = re.search(r'CORE_EXECUTION_PROFILES_SHA256 = "([0-9a-f]{64})"', text).group(1)
        import hashlib

        self.assertEqual(hashlib.sha256(embedded.encode("utf-8")).hexdigest(), digest)
        self.assertEqual(json.loads(embedded), payload)
        self.assertEqual(text, GENERATOR.render_web(GENERATOR.canonical_text(payload)))

    def test_external_r_checkout_agrees_when_present(self) -> None:
        if not (R_SIBLING / "DESCRIPTION").exists():
            self.skipTest(f"unavailable: no sibling nirs4all-r checkout at {R_SIBLING}")
        payload = _owner_payload()
        record, = payload["external_federation"]
        for relative in record["contracts"] + record["evidence"]:
            self.assertTrue((R_SIBLING / relative).exists(), relative)
        namespace = (R_SIBLING / "NAMESPACE").read_text(encoding="utf-8")
        exports = set(re.findall(r"^export\((.+)\)$", namespace, re.MULTILINE))
        self.assertTrue({entry["symbol"] for entry in record["entrypoints"]} <= exports)
        self.assertIn("nirs4all_execution_profiles", exports)
        r_data = (R_SIBLING / GENERATOR.R_TARGET).read_text(encoding="utf-8")
        lines = [json.loads(item) for item in re.findall(r'^  (".*?"),?$', r_data, re.MULTILINE)]
        self.assertEqual(json.loads("\n".join(lines)), payload)
        self.assertEqual(r_data, GENERATOR.render_r(GENERATOR.canonical_text(payload)))


class HostDataAndApiTests(unittest.TestCase):
    def test_every_host_embeds_exactly_the_owner_payload(self) -> None:
        owner = _owner_payload()
        for host, payload in _host_payloads().items():
            with self.subTest(host=host):
                self.assertEqual(payload, owner)
        for name, (path, content) in GENERATOR.targets(owner, None).items():
            with self.subTest(target=name):
                self.assertEqual(path.read_text(encoding="utf-8"), content)

    def test_python_api_and_manifest_expose_only_native_profiles(self) -> None:
        owner = _owner_payload()
        self.assertEqual(n4core.execution_profiles(), owner)
        manifest = n4core.capability_manifest()
        self.assertEqual(set(manifest), EXPECTED_MANIFEST_KEYS)
        self.assertEqual(manifest["schema"], "nirs4all-core.capabilities.v1")
        self.assertEqual(manifest["aggregate"], "nirs4all-core")
        self.assertEqual(tuple(manifest["runtime_surfaces"]), EXPECTED_SURFACES)
        self.assertEqual(tuple(n4core.runtime_surfaces()), EXPECTED_SURFACES)
        self.assertEqual(manifest["execution_profiles"], owner)
        json.dumps(manifest)

    def test_removed_capability_apis_are_absent_everywhere_in_python(self) -> None:
        import n4a

        for name in REMOVED_PUBLIC_NAMES:
            with self.subTest(name=name):
                self.assertFalse(hasattr(n4core, name))
                self.assertFalse(hasattr(n4a, name))
                self.assertNotIn(name, n4core.__all__)
        self.assertEqual(
            [name for name in n4core.__all__ if "contract" in name or "controller_cap" in name], [],
        )

    def test_wasm_entrypoints_are_public_exports_of_index_js_and_its_types(self) -> None:
        index_js = _read(ROOT / "bindings/wasm/src/index.js")
        index_dts = _read(ROOT / "bindings/wasm/src/index.d.ts")

        def exported(text: str, name: str) -> bool:
            direct = re.search(rf"^export (?:async )?(?:const|function|class) {name}\b", text, re.MULTILINE)
            listed = any(
                re.search(rf"\b{name}\b", block)
                for block in re.findall(r"^export (?:type )?\{([^}]*)\}", text, re.MULTILINE | re.DOTALL)
            )
            return bool(direct) or listed

        checked = set()
        for profile in _owner_payload()["profiles"]:
            for operation in EXPECTED_OPERATIONS:
                cell = profile["operations"][operation]["javascript_wasm"]
                for entry in cell.get("entrypoints", []):
                    owner = entry["symbol"].split(".")[0]
                    self.assertTrue(exported(index_js, owner), f"index.js does not export {owner}")
                    self.assertTrue(exported(index_dts, owner), f"index.d.ts does not declare {owner}")
                    checked.add(owner)
        self.assertEqual(
            checked,
            {"runBrowserPipeline", "BrowserNativePipeline", "predictBrowserPipeline", "loadBrowserPipeline",
             "calibrate", "predictCalibrated", "loadCalibrated", "exportCalibrated",
             "produceN4aNative", "inspectN4aBytes", "predictN4aNative", "retrainN4aNative",
             "openN4aContainer", "writeN4aContainer",
             "readPortableArchiveV2", "writeNativePredictorPackage", "writeNativeRefitPackage",
             "replayMethodsArchiveV2"},
        )

    def test_python_returns_independent_deep_copies(self) -> None:
        owner = _owner_payload()
        first = n4core.execution_profiles()
        first["profiles"][0]["operations"]["fit"]["python"]["status"] = "unsupported"
        first["profiles"][1]["content_types"][0]["schema"] = "mutated"
        first["external_federation"].clear()
        manifest = n4core.capability_manifest()
        manifest["execution_profiles"]["profiles"].clear()
        manifest["runtime_surfaces"].append("r")
        self.assertEqual(n4core.execution_profiles(), owner)
        self.assertEqual(n4core.capability_manifest()["execution_profiles"], owner)
        self.assertEqual(tuple(n4core.capability_manifest()["runtime_surfaces"]), EXPECTED_SURFACES)

    def test_other_host_sources_no_longer_publish_the_removed_capability_apis(self) -> None:
        wasm_exports = set(re.findall(r"^export (?:async )?(?:const|function|class) (\w+)", _read(ROOT / "bindings/wasm/src/index.js"), re.MULTILINE))
        for name in ("runtimeContracts", "artifactContracts", "controllerCapabilities", "requiredKeywordRegistryEntries", "portableOperatorClasses"):
            self.assertNotIn(name, wasm_exports)
        for name in ("executionProfiles", "capabilityManifest", "runtimeSurfaces"):
            self.assertIn(name, wasm_exports)
        rust_public = set(re.findall(r"^pub (?:const|fn|struct) (\w+)", _read(ROOT / "bindings/rust/nirs4all/src/lib.rs"), re.MULTILINE))
        for name in ("RUNTIME_CONTRACTS", "RuntimeContract", "runtime_contracts", "ARTIFACT_CONTRACTS", "ArtifactContract",
                     "artifact_contracts", "CONTROLLER_CAPABILITIES", "ControllerCapability", "PORTABLE_OPERATOR_CLASSES",
                     "REQUIRED_KEYWORD_REGISTRY_ENTRIES"):
            self.assertNotIn(name, rust_public)
        for name in ("execution_profiles", "capability_manifest", "RUNTIME_SURFACES"):
            self.assertIn(name, rust_public)
        matlab = {path.stem for path in (ROOT / "bindings/matlab/+nirs4all").glob("*.m")}
        for name in ("runtimeContracts", "artifactContracts", "controllerCapabilities", "requiredKeywordRegistryEntries"):
            self.assertNotIn(name, matlab)
        for name in ("executionProfiles", "executionProfilesJson", "capabilityManifest", "runtimeSurfaces"):
            self.assertIn(name, matlab)


if __name__ == "__main__":
    unittest.main()
