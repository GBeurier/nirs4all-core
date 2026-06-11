import unittest

import nirs4all_lite as n4lite


class UpstreamRegistryTests(unittest.TestCase):
    def test_expected_upstream_keys_are_registered(self) -> None:
        self.assertEqual(
            list(n4lite.upstreams),
            ["dag_ml", "dag_ml_data", "formats", "io", "datasets", "methods"],
        )

    def test_status_is_serializable(self) -> None:
        status = n4lite.upstream_status()
        self.assertEqual(len(status), 6)
        self.assertIn("available", status[0])
        self.assertIn("role", status[0])

    def test_lazy_proxy_points_to_registered_upstream(self) -> None:
        self.assertEqual(repr(n4lite.methods), "LazyUpstream(name='methods')")

    def test_unknown_upstream_is_rejected(self) -> None:
        with self.assertRaises(KeyError):
            n4lite.import_upstream("unknown")


if __name__ == "__main__":
    unittest.main()
