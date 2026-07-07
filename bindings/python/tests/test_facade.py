"""Tests for the additive LOCK-GOV import facades (`n4a`, `nirs4all_core`).

These guard three properties of the slice:

1. legacy ``nirs4all_lite`` imports keep working unchanged;
2. ``n4a`` exposes the full aggregate surface without drift;
3. ``nirs4all_core`` advertises the no-engine core contract while keeping
   legacy explicit imports reachable through the compatibility passthrough;
4. the facades do not shadow the full Python ``nirs4all`` library.
"""

import importlib
import unittest
from pathlib import Path

import n4a
import nirs4all_core
import nirs4all_lite as n4lite


FIXTURE_DIR = Path(__file__).resolve().parents[3] / "tests" / "parity" / "fixtures"


class FacadeImportSurfaceTests(unittest.TestCase):
    def test_legacy_aggregate_import_is_unchanged(self) -> None:
        # The slice must not remove or rename the canonical import package.
        self.assertEqual(n4lite.__name__, "nirs4all_lite")
        self.assertTrue(hasattr(n4lite, "upstreams"))
        self.assertTrue(hasattr(n4lite, "load_pipeline_definition"))

    def test_aggregate_public_surface_is_explicit(self) -> None:
        expected_exports = [
            "LazyUpstream",
            "PORTABLE_OPERATOR_CLASSES",
            "PortableDataset",
            "PipelineDefinition",
            "CORE_FACADE_EXPORTS",
            "EXECUTION_ENGINE_EXPORTS",
            "TOPOLOGY_EXPORTS",
            "Upstream",
            "available_upstreams",
            "capability_manifest",
            "core_facade_exports",
            "controller_capabilities",
            "dag_ml",
            "dag_ml_data",
            "datasets",
            "execution_engine_exports",
            "formats",
            "import_upstream",
            "io",
            "load_pipeline_definition",
            "methods",
            "parse_execution_plan",
            "portable_class_names",
            "release_topology_manifest",
            "require_upstream",
            "run_portable_pipeline",
            "runtime_contracts",
            "runtime_surfaces",
            "upstream_status",
            "upstreams",
            "validate_core_facade",
        ]

        self.assertEqual(n4lite.__all__, expected_exports)
        for name in expected_exports:
            with self.subTest(name=name):
                self.assertTrue(hasattr(n4lite, name))

    def test_n4a_advertises_the_same_public_surface(self) -> None:
        self.assertEqual(set(n4a.__all__), set(n4lite.__all__))

    def test_n4a_re_exports_the_same_objects(self) -> None:
        for name in n4lite.__all__:
            with self.subTest(name=name):
                self.assertIs(getattr(n4a, name), getattr(n4lite, name))

    def test_core_advertises_only_the_core_contract(self) -> None:
        expected = set(n4lite.core_facade_exports()) | set(n4lite.TOPOLOGY_EXPORTS)
        self.assertEqual(set(nirs4all_core.__all__), expected)
        self.assertFalse(
            set(n4lite.execution_engine_exports()) & set(nirs4all_core.__all__)
        )
        self.assertEqual(
            n4lite.validate_core_facade(nirs4all_core),
            {"missing_core_exports": (), "unexpected_execution_exports": ()},
        )

    def test_core_re_exports_core_contract_objects(self) -> None:
        for name in n4lite.core_facade_exports():
            with self.subTest(name=name):
                self.assertIs(getattr(nirs4all_core, name), getattr(n4lite, name))

    def test_core_keeps_execution_imports_reachable_by_passthrough(self) -> None:
        from nirs4all_core import run_portable_pipeline

        self.assertIs(run_portable_pipeline, n4lite.run_portable_pipeline)
        for name in n4lite.execution_engine_exports():
            with self.subTest(name=name):
                self.assertIs(getattr(nirs4all_core, name), getattr(n4lite, name))

    def test_facades_point_at_the_shipped_aggregate(self) -> None:
        for facade in (n4a, nirs4all_core):
            with self.subTest(facade=facade.__name__):
                self.assertEqual(facade.__aggregate_import__, "nirs4all_lite")

    def test_getattr_passthrough_reaches_non_exported_attributes(self) -> None:
        # `_upstreams` is an internal submodule of the aggregate, not in __all__.
        for facade in (n4a, nirs4all_core):
            with self.subTest(facade=facade.__name__):
                self.assertIs(facade._upstreams, n4lite._upstreams)

    def test_facades_do_not_shadow_the_full_nirs4all_library(self) -> None:
        # The Python facade roots are `n4a` / `nirs4all_core`, never `nirs4all`.
        self.assertEqual(n4a.__name__, "n4a")
        self.assertEqual(nirs4all_core.__name__, "nirs4all_core")
        self.assertNotEqual(n4a.__name__, "nirs4all")
        self.assertNotEqual(nirs4all_core.__name__, "nirs4all")


class FacadeBehaviourParityTests(unittest.TestCase):
    def test_upstream_registry_is_shared(self) -> None:
        for facade in (n4a, nirs4all_core):
            with self.subTest(facade=facade.__name__):
                self.assertIs(facade.upstreams, n4lite.upstreams)
                self.assertEqual(facade.upstream_status(), n4lite.upstream_status())

    def test_from_import_binds_eagerly(self) -> None:
        # `from n4a import formats` must resolve without tripping __getattr__.
        from n4a import formats as n4a_formats
        from nirs4all_core import formats as core_formats

        self.assertIs(n4a_formats, n4lite.formats)
        self.assertIs(core_formats, n4lite.formats)

    def test_pipeline_loading_is_identical_through_the_facade(self) -> None:
        fixture = FIXTURE_DIR / "portable_methods_pipeline.json"

        reference = n4lite.load_pipeline_definition(fixture)
        for facade in (n4a, nirs4all_core):
            with self.subTest(facade=facade.__name__):
                self.assertEqual(
                    facade.load_pipeline_definition(fixture).as_dict(),
                    reference.as_dict(),
                )
                self.assertEqual(
                    facade.portable_class_names(reference),
                    n4lite.portable_class_names(reference),
                )

    def test_facades_are_importable_by_name(self) -> None:
        for module_name in ("n4a", "nirs4all_core"):
            with self.subTest(module=module_name):
                self.assertIsNotNone(importlib.import_module(module_name))


if __name__ == "__main__":
    unittest.main()
