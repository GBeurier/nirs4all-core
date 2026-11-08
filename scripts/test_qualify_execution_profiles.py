"""Regression tests for the qualifier's evidence accounting (no scientific execution)."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("qualify", Path(__file__).with_name("qualify_execution_profiles.py"))
assert SPEC is not None and SPEC.loader is not None
Q = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(Q)


def result(code: int, output: str = "") -> dict:
    return {"exit_code": code, "output": output, "duration_s": 0.0}


def report_with(statuses: list[str]) -> str:
    cases = [{"status": status, "fullName": f"case {i}"} for i, status in enumerate(statuses)]
    return json.dumps({"testResults": [{"assertionResults": cases}]})


class WebVitestClassification(unittest.TestCase):
    def classify(self, res: dict, report_text: str | None) -> dict:
        with tempfile.TemporaryDirectory() as tmp:
            report = Path(tmp) / "report.json"
            if report_text is not None:
                report.write_text(report_text, encoding="utf-8")
            step = Q.new_step("web", "metadata_source", "web", None, "node")
            return Q.finalize_evidence(Q.classify_web_vitest(step, res, report))

    def test_real_parsed_pass_is_executed_with_counts(self) -> None:
        step = self.classify(result(0), report_with(["passed"] * 25))
        self.assertEqual((step["status"], step["counts"]["passed"], step["counts"]["skipped"]), ("executed", 25, 0))

    def test_exact_authenticated_asset_mismatch_is_the_only_unavailable_startup_error(self) -> None:
        step = self.classify(result(1, f"Startup Error\nError: {Q.WEB_ASSET_MISMATCH}\n"), None)
        self.assertEqual(step["status"], "unavailable")

    def test_other_startup_config_or_source_errors_fail(self) -> None:
        for output in (
            "Startup Error\nSyntaxError: Unexpected token '}' in vitest.config.ts",
            "Startup Error\nError: Cannot find module './missing'",
            "Startup Error\nError: Vitest DAG npm asset mismatch (different message)",
            "FAIL src/engine/nirs4all-core.test.ts\nSyntaxError: Unexpected end of input",
        ):
            with self.subTest(output=output):
                self.assertEqual(self.classify(result(1, output), None)["status"], "failed")

    def test_nonzero_exit_is_never_success_even_with_a_passing_report(self) -> None:
        self.assertEqual(self.classify(result(1, "boom"), report_with(["passed"] * 3))["status"], "failed")
        # A stale/leftover report cannot turn the known mismatch into anything but a hidden failure.
        mismatch = f"Error: {Q.WEB_ASSET_MISMATCH}"
        self.assertEqual(self.classify(result(1, mismatch), report_with(["passed"]))["status"], "failed")

    def test_missing_or_malformed_report_after_exit_zero_fails(self) -> None:
        self.assertEqual(self.classify(result(0), None)["status"], "failed")
        for text in ("not json", "{}", json.dumps({"testResults": [{"assertionResults": [{"status": "passed"}]}]})):
            with self.subTest(text=text):
                self.assertEqual(self.classify(result(0), text)["status"], "failed")

    def test_failed_bodies_fail_and_zero_or_skipped_bodies_are_unavailable(self) -> None:
        self.assertEqual(self.classify(result(0), report_with(["passed", "failed"]))["status"], "failed")
        self.assertEqual(self.classify(result(0), report_with([]))["status"], "unavailable")
        step = self.classify(result(0), report_with(["pending", "skipped", "todo"]))
        self.assertEqual((step["status"], step["counts"]["passed"]), ("unavailable", 0))
        mixed = self.classify(result(0), report_with(["passed", "pending"]))
        self.assertEqual((mixed["status"], mixed["counts"]["skipped"]), ("executed", 1))


class GeneratorRequirements(unittest.TestCase):
    def test_required_roots_are_always_passed_with_require_flags(self) -> None:
        root = Path("/nonexistent")
        required = Q.generator_command("py", root, root, False, False, True, True)
        self.assertIn("--require-r", required)
        self.assertIn("--require-web", required)
        self.assertEqual(required.count("--r-root"), 1)  # an absent required root is still named, so the generator exits 2
        optional = Q.generator_command("py", root, root, False, False, False, False)
        self.assertEqual(optional, ["py", "scripts/generate_capability_profiles.py", "--check"])

    def test_missing_required_target_fails_in_the_real_generator(self) -> None:
        import subprocess
        import sys
        with tempfile.TemporaryDirectory() as empty:
            command = Q.generator_command(sys.executable, Path(empty), Path(empty), False, False, True, True)
            done = subprocess.run(command, cwd=Q.ROOT, capture_output=True, text=True)
            self.assertEqual(done.returncode, 2)
            self.assertNotIn("\nok ", "\n" + done.stdout)


class EvidenceAccounting(unittest.TestCase):
    def test_skipped_todo_and_expected_failure_bodies_never_count_as_passed(self) -> None:
        tap = "ok 1 - a\nok 2 - b # SKIP no lib\nok 3 - c # TODO later\nnot ok 4 - d\n"
        tests = Q.parse_tap(tap)
        self.assertEqual({k: len(v) for k, v in tests.items()},
                         {"passed": 1, "failed": 1, "skipped": 1, "todo": 1, "expected_failures": 0})
        unit = "t1 (m.C.t1) ... expected failure\nt2 (m.C.t2) ... skipped 'x'\nt3 (m.C.t3) ... ok\n"
        parsed = Q.parse_unittest(unit)
        self.assertEqual((len(parsed["passed"]), len(parsed["expected_failures"]), len(parsed["skipped"])), (1, 1, 1))

    def test_zero_collection_and_exit_zero_gated_bodies_are_unavailable_not_executed(self) -> None:
        step = Q.new_step("x", "scientific_replay", "s", None, None)
        out = Q.step_from_tests(step, result(0), Q.parse_tap(""))
        self.assertEqual(out["status"], "unavailable")
        step = Q.new_step("x", "scientific_replay", "s", None, None)
        out = Q.step_from_tests(step, result(0), Q.parse_tap("ok 1 - a # SKIP gated\n"))
        self.assertEqual(out["status"], "unavailable")

    def test_nonzero_exit_fails_even_when_bodies_passed(self) -> None:
        step = Q.new_step("x", "scientific_replay", "s", None, None)
        self.assertEqual(Q.step_from_tests(step, result(2), Q.parse_tap("ok 1 - a\n"))["status"], "failed")

    def test_finalize_demotes_executed_steps_without_exit_zero_or_passed_count(self) -> None:
        for exit_code, counts in ((1, {"passed": 3}), (0, {"passed": 0}), (0, None), (0, {"checks_passed": 0})):
            with self.subTest(exit_code=exit_code, counts=counts):
                step = Q.new_step("x", "metadata_source", "s", None, None)
                step.update(status="executed", exit_code=exit_code, counts=counts)
                self.assertEqual(Q.finalize_evidence(step)["status"], "failed")
        for counts in ({"passed": 2}, {"checks_passed": 2}, {"scripts_passed": 1}):
            step = Q.new_step("x", "metadata_source", "s", None, None)
            step.update(status="executed", exit_code=0, counts=counts)
            self.assertEqual(Q.finalize_evidence(step)["status"], "executed")

    def test_working_tree_identity_is_content_based(self) -> None:
        state = Q.git_state(Q.ROOT)
        self.assertEqual(len(state["working_tree_content_sha256"]), 64)
        self.assertIn("scripts/test_qualify_execution_profiles.py", Q.CORE_INPUTS)


if __name__ == "__main__":
    unittest.main()
