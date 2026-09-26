"""Static cross-language public-surface parity gate for the aggregate.

The nirs4all-core aggregate public surface is declared once per language
binding. Three invariants must hold for the aggregate to be truthful, and none
of them is otherwise checked when Node, Octave, or a Rust toolchain are
unavailable (those runtime gates simply skip):

1. The original portable operator subset is identical across Python,
   MATLAB/Octave, and Rust. WASM contains that subset plus its separately
   qualified local Methods controllers; their extra names are not presented as
   cross-language support by the other facades.
2. The upstream registry (keys and role strings) is identical across all four
   bindings and the machine-readable ``compat`` registry.
3. Every binding exposes a thin facade or re-export for the upstream DAG-ML
   process-local loss/metric registry.

These checks run in pure Python by reading the binding source files, so surface
drift in *any* binding is caught in the required Python gate even on machines
where Node, Octave, or ``cargo`` are unavailable. They complement (do not
duplicate) the WASM/MATLAB/Rust gates in ``tests/index.test.js``, ``tests/smoke.m``, and the ``cargo test`` unit tests,
which additionally require the respective runtimes.
"""

from __future__ import annotations

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
PYTHON_INIT = ROOT / "bindings/python/src/nirs4all_core/__init__.py"
PYTHON_UPSTREAMS = ROOT / "bindings/python/src/nirs4all_core/_upstreams.py"
WASM_INDEX = ROOT / "bindings/wasm/src/index.js"
WASM_TYPES = ROOT / "bindings/wasm/src/index.d.ts"
WASM_PACKAGE = ROOT / "bindings/wasm/package.json"
WASM_PACKAGE_LOCK = ROOT / "bindings/wasm/package-lock.json"
WASM_NATIVE_CARGO = ROOT / "bindings/wasm-native/Cargo.toml"
PYPROJECT = ROOT / "bindings/python/pyproject.toml"
MATLAB_OPERATORS = ROOT / "bindings/matlab/+nirs4all/portableOperatorClasses.m"
MATLAB_UPSTREAMS = ROOT / "bindings/matlab/+nirs4all/upstreams.m"
MATLAB_LOCAL_REGISTRY = ROOT / "bindings/matlab/+nirs4all/localImplementationRegistry.m"
MATLAB_README = ROOT / "bindings/matlab/README.md"
MATLAB_BUILDER = ROOT / "scripts/build-matlab-package.sh"
RUST_CARGO = ROOT / "bindings/rust/nirs4all/Cargo.toml"
RUST_LIB = ROOT / "bindings/rust/nirs4all/src/lib.rs"
COMPAT = ROOT / "compat/upstreams.toml"

EXPECTED_OPERATOR_COUNT = 14
EXPECTED_WASM_OPERATOR_COUNT = 37
EXPECTED_UPSTREAM_COUNT = 6


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _bracketed(text: str, pattern: str) -> str:
    match = re.search(pattern, text, re.DOTALL)
    if match is None:
        raise AssertionError(f"could not locate block for pattern: {pattern!r}")
    return match.group(1)


def _wasm_operator_classes() -> list[str]:
    block = _bracketed(
        _read(WASM_INDEX),
        r"portableOperatorClasses\s*=\s*Object\.freeze\(\[(.*?)\]\)",
    )
    return re.findall(r"'([^']+)'", block)


def _wasm_upstreams() -> tuple[list[str], dict[str, str]]:
    block = _bracketed(
        _read(WASM_INDEX),
        r"export const upstreams\s*=\s*Object\.freeze\(\[(.*?)\]\);",
    )
    keys = re.findall(r"key:\s*'([^']+)'", block)
    roles = re.findall(r"role:\s*'([^']+)'", block)
    return keys, dict(zip(keys, roles))


def _matlab_operator_classes() -> list[str]:
    block = _bracketed(_read(MATLAB_OPERATORS), r"classes\s*=\s*\{(.*?)\};")
    return re.findall(r"'([^']+)'", block)


def _matlab_upstreams() -> tuple[list[str], dict[str, str]]:
    text = _read(MATLAB_UPSTREAMS)
    keys = re.findall(r"'([^']+)'", _bracketed(text, r"'key'\s*,\s*\{(.*?)\}"))
    roles = re.findall(r"'([^']+)'", _bracketed(text, r"'role'\s*,\s*\{(.*?)\}"))
    return keys, dict(zip(keys, roles))


def _matlab_upstream_packages() -> dict[str, str]:
    text = _read(MATLAB_UPSTREAMS)
    keys = re.findall(r"'([^']+)'", _bracketed(text, r"'key'\s*,\s*\{(.*?)\}"))
    packages = re.findall(
        r"'([^']*)'",
        _bracketed(text, r"'package'\s*,\s*\{(.*?)\}"),
    )
    return dict(zip(keys, packages))


def _rust_operator_classes() -> list[str]:
    block = _bracketed(
        _read(RUST_LIB),
        r"PORTABLE_OPERATOR_CLASSES:\s*&\[&str\]\s*=\s*&\[(.*?)\];",
    )
    return re.findall(r'"([^"]+)"', block)


def _rust_upstreams() -> tuple[list[str], dict[str, str]]:
    block = _bracketed(
        _read(RUST_LIB),
        r"UPSTREAMS:\s*&\[Upstream\]\s*=\s*&\[(.*?)\];",
    )
    keys = re.findall(r'key:\s*"([^"]+)"', block)
    roles = re.findall(r'role:\s*"([^"]+)"', block)
    return keys, dict(zip(keys, roles))


def _compat_upstreams() -> tuple[list[str], dict[str, str]]:
    data = tomllib.loads(_read(COMPAT))["upstream"]
    keys = [item["key"] for item in data]
    return keys, {item["key"]: item["role"] for item in data}


def _cargo_to_pep440(version: str) -> str:
    base, sep, prerelease = version.partition("-")
    if not sep:
        return base
    kind, _, number = prerelease.partition(".")
    suffix = {"alpha": "a", "beta": "b", "rc": "rc"}[kind]
    return f"{base}{suffix}{number or '0'}"


class VersionMetadataParityTests(unittest.TestCase):
    def test_binding_manifest_versions_match_the_rust_source_of_truth(self) -> None:
        cargo_version = str(tomllib.loads(_read(RUST_CARGO))["package"]["version"])
        pyproject = tomllib.loads(_read(PYPROJECT))
        wasm_package = json.loads(_read(WASM_PACKAGE))
        wasm_lock = json.loads(_read(WASM_PACKAGE_LOCK))
        wasm_native = tomllib.loads(_read(WASM_NATIVE_CARGO))

        self.assertEqual(
            pyproject["project"]["version"],
            _cargo_to_pep440(cargo_version),
        )
        self.assertEqual(n4core.__version__, pyproject["project"]["version"])
        self.assertEqual(wasm_package["version"], cargo_version)
        self.assertEqual(wasm_lock["version"], cargo_version)
        self.assertEqual(wasm_lock["packages"][""]["version"], cargo_version)
        self.assertEqual(wasm_native["package"]["version"], cargo_version)

    def test_release_surface_version_metadata_covers_all_bindings(self) -> None:
        manifest = n4core.release_topology_manifest()
        surfaces = {item["ecosystem"]: item for item in manifest["v1_release_surfaces"]}
        expected = {
            "python": ("bindings/python/pyproject.toml:project.version", "pep440"),
            "javascript_wasm": ("bindings/wasm/package.json:version", "cargo-semver"),
            "rust": (
                "bindings/rust/nirs4all/Cargo.toml:package.version",
                "cargo-semver",
            ),
            "matlab_octave": (
                "bindings/rust/nirs4all/Cargo.toml:package.version",
                "derived-cargo-semver",
            ),
        }

        self.assertEqual(set(surfaces), set(expected))
        for ecosystem, (source, spelling) in expected.items():
            with self.subTest(ecosystem=ecosystem):
                surface = surfaces[ecosystem]
                self.assertEqual(
                    (surface["version_source"], surface["version_spelling"]),
                    (source, spelling),
                )
                self.assertTrue((ROOT / surface["package_manifest"]).exists())

        self.assertTrue(surfaces["rust"]["version_source_of_truth"])
        self.assertEqual(
            surfaces["matlab_octave"]["package_manifest"],
            "bindings/matlab/README.md",
        )
        self.assertTrue(MATLAB_README.exists())
        self.assertIn("bindings/rust/nirs4all/Cargo.toml", _read(MATLAB_BUILDER))


class PortableOperatorSubsetParityTests(unittest.TestCase):
    def test_shared_operator_subset_and_wasm_extension(self) -> None:
        python = set(n4core.PORTABLE_OPERATOR_CLASSES)
        self.assertEqual(len(python), EXPECTED_OPERATOR_COUNT)

        extractors = {
            "matlab": _matlab_operator_classes,
            "rust": _rust_operator_classes,
        }
        for label, extractor in extractors.items():
            with self.subTest(binding=label):
                classes = extractor()
                # Guard against a silently-empty extraction masking real drift.
                self.assertEqual(len(classes), EXPECTED_OPERATOR_COUNT, classes)
                self.assertEqual(
                    len(set(classes)),
                    EXPECTED_OPERATOR_COUNT,
                    f"duplicate {label} operator class",
                )
                self.assertEqual(python, set(classes))

        wasm = _wasm_operator_classes()
        self.assertEqual(len(wasm), EXPECTED_WASM_OPERATOR_COUNT, wasm)
        self.assertEqual(len(set(wasm)), len(wasm), "duplicate wasm operator class")
        self.assertTrue(python <= set(wasm), "WASM lost a shared portable operator")
        self.assertEqual(
            set(wasm) - python,
            {
                "n4m.MSC",
                "nirs4all.operators.transforms.MSC",
                "nirs4all.operators.transforms.MultiplicativeScatterCorrection",
                "nirs4all.operators.transforms.nirs.MultiplicativeScatterCorrection",
                "n4m.SPA",
                "n4m.SPASelector",
                "pls4all.sklearn.SPASelector",
                "n4m.Selector",
                "n4m.Ridge",
                "n4m.RidgePLS",
                "n4m.RobustPLS",
                "n4m.CPPLS",
                "n4m.SparseSIMPLS",
                "n4m.ECR",
                "n4m.ContinuumRegression",
                "n4m.MIRPLS",
                "n4m.FusedSparsePLS",
                "n4m.BaggingPLS",
                "n4m.BoostingPLS",
                "n4m.RandomSubspacePLS",
                "n4m.NPLS",
                "n4m.MBPLS",
                "n4m.GroupSparsePLS",
            },
        )


class UpstreamRegistryParityTests(unittest.TestCase):
    def test_upstream_keys_are_identical_and_ordered_across_bindings(self) -> None:
        python = list(n4core.upstreams)
        self.assertEqual(len(python), EXPECTED_UPSTREAM_COUNT)

        wasm_keys, _ = _wasm_upstreams()
        matlab_keys, _ = _matlab_upstreams()
        rust_keys, _ = _rust_upstreams()
        compat_keys, _ = _compat_upstreams()

        bindings = {
            "wasm": wasm_keys,
            "matlab": matlab_keys,
            "rust": rust_keys,
            "compat": compat_keys,
        }
        for label, keys in bindings.items():
            with self.subTest(binding=label):
                self.assertEqual(keys, python)

    def test_upstream_role_strings_match_across_bindings_and_compat(self) -> None:
        python = {name: item.role for name, item in n4core.upstreams.items()}
        self.assertEqual(len(python), EXPECTED_UPSTREAM_COUNT)

        _, wasm_roles = _wasm_upstreams()
        _, matlab_roles = _matlab_upstreams()
        _, rust_roles = _rust_upstreams()
        _, compat_roles = _compat_upstreams()

        bindings = {
            "wasm": wasm_roles,
            "matlab": matlab_roles,
            "rust": rust_roles,
            "compat": compat_roles,
        }
        for label, roles in bindings.items():
            with self.subTest(binding=label):
                self.assertEqual(roles, python)

    def test_matlab_upstream_packages_are_matlab_candidates_or_metadata_only(
        self,
    ) -> None:
        packages = _matlab_upstream_packages()

        self.assertEqual(len(packages), EXPECTED_UPSTREAM_COUNT, packages)
        self.assertEqual(packages["methods"], "+n4m")
        self.assertEqual(packages["dag_ml"], "+dagml")
        metadata_only = {"dag_ml_data", "formats", "io", "datasets"}
        self.assertEqual(
            {key for key, package in packages.items() if package == ""},
            metadata_only,
        )
        for key, package in packages.items():
            with self.subTest(upstream=key):
                self.assertNotIn("wasm", package.lower())
                self.assertFalse(package.startswith("@"))
                self.assertNotIn("/", package)


class LocalImplementationRegistryFacadeParityTests(unittest.TestCase):
    def test_all_bindings_expose_the_upstream_dag_ml_registry(self) -> None:
        markers = {
            "python-export": (PYTHON_INIT, r"\blocal_implementation_registry\b"),
            "python-delegation": (
                PYTHON_UPSTREAMS,
                r"(?s)def local_implementation_registry\(\).*require_upstream\(\"dag_ml\"\)",
            ),
            "python-task-bound-loss": (PYTHON_UPSTREAMS, r"\bbind_training_loss\b"),
            "wasm-export": (
                WASM_INDEX,
                r"export async function localImplementationRegistry\(",
            ),
            "wasm-type": (
                WASM_TYPES,
                r"localImplementationRegistry(?:<[^>]+>)?\(",
            ),
            "wasm-task-bound-loss": (WASM_TYPES, r"\bbind_training_loss\b"),
            "matlab-delegation": (
                MATLAB_LOCAL_REGISTRY,
                r"function registry = localImplementationRegistry\(\)",
            ),
            "matlab-task-bound-loss": (
                MATLAB_LOCAL_REGISTRY,
                r"\binvokeTrainingLoss\b",
            ),
            "rust-reexport": (RUST_LIB, r"pub use dag_ml_crate::\*;"),
            "rust-facade": (
                RUST_LIB,
                r"pub fn local_implementation_registry<T>\(\) -> LocalImplementationRegistry<T>",
            ),
        }
        for label, (path, pattern) in markers.items():
            with self.subTest(binding=label):
                self.assertRegex(_read(path), pattern)


if __name__ == "__main__":
    unittest.main()
