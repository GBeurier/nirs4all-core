"""Static cross-language public-surface parity gate for the aggregate.

The nirs4all-lite public surface is declared once per language binding. Three
invariants must hold for the aggregate to be truthful, and none of them is
otherwise checked when R and Node are not installed (the local R gate simply
skips):

1. The portable operator subset is identical across the Python, WASM, and R
   bindings.
2. The upstream registry (keys and role strings) is identical across the
   Python, WASM, and R bindings and the machine-readable ``compat`` registry.
3. The R package export surface is self-consistent: ``NAMESPACE`` exports equal
   the ``surface.R`` expected exports, every export has an ``\\alias`` in
   ``man/`` (what ``R CMD check`` enforces), and every export has a top-level
   definition in ``R/``.

These checks run in pure Python by reading the binding source files, so R- and
WASM-surface drift is caught in the required Python gate even on machines where
R or Node are unavailable. They complement (do not duplicate) the R/WASM gates
in ``surface.R`` and ``tests/index.test.js``, which additionally require the
respective runtimes.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

import nirs4all_lite as n4lite

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - local Python < 3.11 fallback
    import tomli as tomllib


ROOT = Path(__file__).resolve().parents[3]
WASM_INDEX = ROOT / "bindings/wasm/src/index.js"
R_PIPELINE = ROOT / "bindings/r/R/pipeline.R"
R_UPSTREAMS = ROOT / "bindings/r/R/upstreams.R"
R_NAMESPACE = ROOT / "bindings/r/NAMESPACE"
R_SURFACE = ROOT / "bindings/r/tests/surface.R"
R_MAN_DIR = ROOT / "bindings/r/man"
R_SRC_DIR = ROOT / "bindings/r/R"
COMPAT = ROOT / "compat/upstreams.toml"

EXPECTED_OPERATOR_COUNT = 9
EXPECTED_UPSTREAM_COUNT = 6
EXPECTED_R_EXPORT_COUNT = 12


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


def _r_operator_classes() -> list[str]:
    block = _bracketed(
        _read(R_PIPELINE),
        r"NIRS4ALL_PORTABLE_OPERATOR_CLASSES\s*<-\s*c\((.*?)\)",
    )
    return re.findall(r'"([^"]+)"', block)


def _r_upstreams() -> tuple[list[str], dict[str, str]]:
    block = _bracketed(_read(R_UPSTREAMS), r"NIRS4ALL_UPSTREAMS\s*<-\s*list\((.*?)\n\)")
    keys = re.findall(r"(?m)^\s*(\w+)\s*=\s*list\(", block)
    roles = re.findall(r'role\s*=\s*"([^"]+)"', block)
    return keys, dict(zip(keys, roles))


def _r_namespace_exports() -> list[str]:
    return re.findall(r"export\((\w+)\)", _read(R_NAMESPACE))


def _r_surface_expected_exports() -> list[str]:
    block = _bracketed(_read(R_SURFACE), r"expected_exports\s*<-\s*c\((.*?)\)")
    return re.findall(r'"([^"]+)"', block)


def _r_man_aliases() -> set[str]:
    aliases: set[str] = set()
    for path in sorted(R_MAN_DIR.glob("*.Rd")):
        aliases.update(re.findall(r"\\alias\{([^}]+)\}", _read(path)))
    return aliases


def _r_top_level_functions() -> set[str]:
    functions: set[str] = set()
    for path in sorted(R_SRC_DIR.glob("*.R")):
        functions.update(
            re.findall(r"(?m)^([A-Za-z_.][A-Za-z0-9_.]*)\s*<-\s*function", _read(path))
        )
    return functions


def _compat_upstreams() -> tuple[list[str], dict[str, str]]:
    data = tomllib.loads(_read(COMPAT))["upstream"]
    keys = [item["key"] for item in data]
    return keys, {item["key"]: item["role"] for item in data}


class PortableOperatorSubsetParityTests(unittest.TestCase):
    def test_operator_subset_is_identical_across_python_wasm_and_r(self) -> None:
        python = set(n4lite.PORTABLE_OPERATOR_CLASSES)
        wasm = _wasm_operator_classes()
        r = _r_operator_classes()

        # Guard against a silently-empty extraction masking real drift.
        self.assertEqual(len(python), EXPECTED_OPERATOR_COUNT)
        self.assertEqual(len(wasm), EXPECTED_OPERATOR_COUNT, wasm)
        self.assertEqual(len(r), EXPECTED_OPERATOR_COUNT, r)
        self.assertEqual(len(set(wasm)), EXPECTED_OPERATOR_COUNT, "duplicate WASM operator class")
        self.assertEqual(len(set(r)), EXPECTED_OPERATOR_COUNT, "duplicate R operator class")

        self.assertEqual(python, set(wasm))
        self.assertEqual(python, set(r))


class UpstreamRegistryParityTests(unittest.TestCase):
    def test_upstream_keys_are_identical_and_ordered_across_bindings(self) -> None:
        python = list(n4lite.upstreams)
        wasm_keys, _ = _wasm_upstreams()
        r_keys, _ = _r_upstreams()
        compat_keys, _ = _compat_upstreams()

        self.assertEqual(len(python), EXPECTED_UPSTREAM_COUNT)
        for label, keys in (("wasm", wasm_keys), ("r", r_keys), ("compat", compat_keys)):
            with self.subTest(binding=label):
                self.assertEqual(keys, python)

    def test_upstream_role_strings_match_across_bindings_and_compat(self) -> None:
        python = {name: item.role for name, item in n4lite.upstreams.items()}
        _, wasm_roles = _wasm_upstreams()
        _, r_roles = _r_upstreams()
        _, compat_roles = _compat_upstreams()

        self.assertEqual(len(python), EXPECTED_UPSTREAM_COUNT)
        for label, roles in (("wasm", wasm_roles), ("r", r_roles), ("compat", compat_roles)):
            with self.subTest(binding=label):
                self.assertEqual(roles, python)


class RPublicSurfaceConsistencyTests(unittest.TestCase):
    def test_namespace_exports_match_surface_test_expected_exports(self) -> None:
        exports = _r_namespace_exports()
        expected = _r_surface_expected_exports()

        self.assertEqual(len(exports), EXPECTED_R_EXPORT_COUNT, exports)
        self.assertEqual(len(expected), EXPECTED_R_EXPORT_COUNT, expected)
        self.assertEqual(set(exports), set(expected))

    def test_every_r_export_is_documented_in_man(self) -> None:
        exports = set(_r_namespace_exports())
        aliases = _r_man_aliases()

        self.assertEqual(len(exports), EXPECTED_R_EXPORT_COUNT)
        undocumented = sorted(exports - aliases)
        self.assertEqual(undocumented, [], f"exports without a man \\alias: {undocumented}")

    def test_every_r_export_has_a_top_level_definition(self) -> None:
        exports = set(_r_namespace_exports())
        defined = _r_top_level_functions()

        self.assertEqual(len(exports), EXPECTED_R_EXPORT_COUNT)
        undefined = sorted(exports - defined)
        self.assertEqual(undefined, [], f"exports without an R/*.R definition: {undefined}")


if __name__ == "__main__":
    unittest.main()
