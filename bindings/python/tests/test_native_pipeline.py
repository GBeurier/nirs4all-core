"""Real native CV and cold replay, compared with independent sklearn models."""

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from nirs4all_core import NativePipeline, run_pipeline
from sklearn.linear_model import Ridge
from sklearn.pipeline import make_pipeline
from sklearn.preprocessing import StandardScaler
from test_workflow_native import fixture


def recipe():
    return {
        "steps": [
            {
                "method_id": "preprocessing.scaling.standard_scale",
                "role": "transformer",
                "params": {},
            },
            {
                "method_id": "models.regularized.ridge",
                "role": "regressor",
                "params": {"scale_x": False},
            },
        ],
        "candidates": [{"alpha": 0.1}, {"alpha": 1.0}],
    }


@unittest.skipUnless(
    os.environ.get("NIRS4ALL_CORE_CLI") and os.environ.get("N4M_LIBRARY_PATH"),
    "real native runtime required",
)
class NativePipelineTest(unittest.TestCase):
    def test_oof_selection_refit_fresh_replay_and_metadata_refusals(self):
        data = fixture()
        model = run_pipeline(data, recipe())
        x = np.asarray(
            data["dataset"]["sources"][0]["array"]["values"], dtype=np.float32
        ).astype(np.float64)
        y = np.asarray(data["dataset"]["y"]["values"], dtype=np.float32).astype(
            np.float64
        )
        positions = {sample: i for i, sample in enumerate(data["origin_ids"])}
        errors = []
        for alpha in [0.1, 1.0]:
            predicted = np.empty(len(y))
            for fold in model.outcome["effective_plan"]["fold_set"]["folds"]:
                train = [positions[s] for s in fold["train_sample_ids"]]
                val = [positions[s] for s in fold["validation_sample_ids"]]
                oracle = make_pipeline(StandardScaler(), Ridge(alpha=alpha)).fit(
                    x[train], y[train]
                )
                predicted[val] = oracle.predict(x[val])
            errors.append(np.mean((predicted - y) ** 2))
        alpha = [0.1, 1.0][int(errors[1] < errors[0])]
        expected = (
            make_pipeline(StandardScaler(), Ridge(alpha=alpha)).fit(x, y).predict(x)
        )
        replay = model.predict(x, sample_ids=data["origin_ids"])
        values = replay["outputs"][0]["predictions"][0]["values"]
        np.testing.assert_allclose(
            np.asarray(values).reshape(-1), expected, rtol=2e-9, atol=2e-9
        )
        self.assertEqual({item["phase"] for item in replay["lineage"]}, {"PREDICT"})
        self.assertEqual(len(model.outcome["execution_bundle"]["refit_artifacts"]), 2)
        with tempfile.TemporaryDirectory() as directory:
            saved = model.export(Path(directory) / "model.json")
            loaded = NativePipeline.load(saved)
            np.testing.assert_equal(
                loaded.predict(x, sample_ids=data["origin_ids"])["outputs"],
                replay["outputs"],
            )
            code = "import json,sys; from nirs4all_core import NativePipeline; m=NativePipeline.load(sys.argv[1]); print(json.dumps(m.predict(json.loads(sys.argv[2]),sample_ids=json.loads(sys.argv[3]))['outputs'][0]['predictions'][0]['values']))"
            cold = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    code,
                    str(saved),
                    json.dumps(x.tolist()),
                    json.dumps(data["origin_ids"]),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(cold.returncode, 0, cold.stderr)
            np.testing.assert_equal(json.loads(cold.stdout), values)
        bad = model.to_dict()
        bad["config"]["pipeline"]["candidates"][0]["alpha"] = 100
        with self.assertRaisesRegex(RuntimeError, "signed training recipe"):
            NativePipeline.load(bad)
        bad = model.to_dict()
        artifact = next(
            iter(bad["package"]["execution_bundle"]["raw_artifact_payloads"])
        )
        bad["package"]["execution_bundle"]["raw_artifact_payloads"][artifact][0] ^= 1
        with self.assertRaises(RuntimeError):
            NativePipeline.load(bad)

    def test_declared_group_folds_and_multitarget(self):
        data = fixture()
        ids = data["origin_ids"]
        data["dataset"]["groups"] = {
            "dtype": "<U3",
            "shape": [12],
            "values": [f"g{i // 2}" for i in range(12)],
        }
        with self.assertRaisesRegex(RuntimeError, "require explicit"):
            run_pipeline(data, recipe())
        data["fold_ids"] = [f"f{(i // 2) % 3}" for i in range(12)]
        original = data["dataset"]["y"]["values"]
        data["dataset"]["y"] = {
            "dtype": "float64",
            "shape": [12, 2],
            "values": [[v, 2 * v + 1] for v in original],
        }
        data["dataset"]["target_names"] = ["protein", "moisture"]
        model = run_pipeline(data, recipe())
        self.assertEqual(len(model.outcome["effective_plan"]["fold_set"]["folds"]), 3)
        for fold in model.outcome["effective_plan"]["fold_set"]["folds"]:
            train = {
                data["dataset"]["groups"]["values"][ids.index(s)]
                for s in fold["train_sample_ids"]
            }
            val = {
                data["dataset"]["groups"]["values"][ids.index(s)]
                for s in fold["validation_sample_ids"]
            }
            self.assertFalse(train & val)
        x = data["dataset"]["sources"][0]["array"]["values"][:2]
        replay = model.predict(x, sample_ids=["fresh1", "fresh2"])
        block = replay["outputs"][0]["predictions"][0]
        self.assertEqual(block["target_names"], ["protein", "moisture"])
        self.assertEqual(np.asarray(block["values"]).shape, (2, 2))
        np.testing.assert_allclose(
            np.asarray(block["values"])[:, 1],
            2 * np.asarray(block["values"])[:, 0] + 1,
            rtol=2e-7,
            atol=2e-7,
        )
        full_x = np.asarray(
            data["dataset"]["sources"][0]["array"]["values"], dtype=np.float32
        ).astype(float)
        full_y = np.asarray(data["dataset"]["y"]["values"], dtype=np.float32).astype(
            float
        )
        positions = {sample: i for i, sample in enumerate(ids)}
        losses = []
        for alpha in [0.1, 1.0]:
            oof = np.zeros_like(full_y)
            for fold in model.outcome["effective_plan"]["fold_set"]["folds"]:
                train = [positions[s] for s in fold["train_sample_ids"]]
                val = [positions[s] for s in fold["validation_sample_ids"]]
                oof[val] = (
                    make_pipeline(StandardScaler(), Ridge(alpha=alpha))
                    .fit(full_x[train], full_y[train])
                    .predict(full_x[val])
                )
            losses.append(np.mean((oof - full_y) ** 2))
        chosen = [0.1, 1.0][int(losses[1] < losses[0])]
        for target in range(2):
            oracle = make_pipeline(StandardScaler(), Ridge(alpha=chosen)).fit(
                full_x, full_y[:, target]
            )
            np.testing.assert_allclose(
                np.asarray(block["values"])[:, target],
                oracle.predict(np.asarray(x)),
                atol=2e-7,
                rtol=2e-7,
            )
        bad = copy.deepcopy(data)
        bad["fold_ids"][1] = "other"
        with self.assertRaisesRegex((ValueError, RuntimeError), "group.*cross"):
            run_pipeline(bad, recipe())

    def test_binary_pls_lda_matches_independent_sklearn_latent_oracle(self):
        from sklearn.cross_decomposition import PLSRegression
        from sklearn.discriminant_analysis import LinearDiscriminantAnalysis

        data = fixture()
        labels = np.asarray([(i // 2) % 2 for i in range(12)])
        data["dataset"]["task_type"] = "classification"
        data["dataset"]["y"] = {
            "dtype": "int64",
            "shape": [12],
            "values": labels.tolist(),
        }
        pipeline = {
            "steps": [
                {
                    "method_id": "models.classification.pls_lda",
                    "role": "classifier",
                    "params": {"n_components": 1},
                }
            ],
            "candidates": [{"n_components": 1}],
        }
        model = run_pipeline(data, pipeline)
        x = np.asarray(
            data["dataset"]["sources"][0]["array"]["values"], dtype=np.float32
        ).astype(float)
        dummy = np.eye(2)[labels]
        latent = PLSRegression(n_components=1, scale=False).fit(x, dummy)
        lda = LinearDiscriminantAnalysis().fit(latent.x_scores_, labels)
        expected = lda.predict(latent.transform(x))
        replay = model.predict(x, sample_ids=data["origin_ids"])
        block = replay["outputs"][0]["predictions"][0]
        np.testing.assert_array_equal(np.asarray(block["values"]).ravel(), expected)
        self.assertEqual(
            replay["outputs"][0]["binding"]["prediction_kind"], "class_label"
        )
        self.assertEqual({item["phase"] for item in replay["lineage"]}, {"PREDICT"})
        bad = copy.deepcopy(data)
        bad["dataset"]["y"]["values"][0] = 16777217
        with self.assertRaisesRegex(RuntimeError, "exact float32"):
            run_pipeline(bad, pipeline)

    def test_partial_targets_fit_observed_rows_preserve_oof_and_cold_replay(self):
        data = fixture()
        data["schema"] = "nirs4all.dataset.v2"
        data["schema_version"] = 2
        x = np.asarray(
            data["dataset"]["sources"][0]["array"]["values"], dtype=np.float32
        ).astype(float)
        y = np.asarray(data["dataset"]["y"]["values"], dtype=np.float32).astype(float)
        observed = np.ones((12, 2), dtype=bool)
        observed[[2, 5, 8], 0] = False
        observed[[1, 6, 9], 1] = False
        truth = np.column_stack([y, 2 * y + 1]).astype(np.float32).astype(float)
        data["dataset"]["y"] = {
            "dtype": "float64",
            "shape": [12, 2],
            "values": [
                [float(truth[i, t]) if observed[i, t] else None for t in range(2)]
                for i in range(12)
            ],
        }
        data["dataset"]["target_names"] = ["protein", "moisture"]
        data["dataset"]["target_mask"] = {
            "dtype": "bool",
            "shape": [12, 2],
            "values": observed.tolist(),
        }
        positions = {sample: i for i, sample in enumerate(data["origin_ids"])}
        for target in range(2):
            selected = recipe()
            selected["steps"][-1]["params"]["native_target_index"] = target
            model = run_pipeline(data, selected)
            errors = []
            for alpha in [0.1, 1.0]:
                predicted = np.zeros(12)
                for fold in model.outcome["effective_plan"]["fold_set"]["folds"]:
                    train = np.array([positions[s] for s in fold["train_sample_ids"]])
                    val = np.array(
                        [positions[s] for s in fold["validation_sample_ids"]]
                    )
                    scaler = StandardScaler().fit(x[train])
                    rows = train[observed[train, target]]
                    oracle = Ridge(alpha=alpha).fit(
                        scaler.transform(x[rows]), truth[rows, target]
                    )
                    predicted[val] = oracle.predict(scaler.transform(x[val]))
                errors.append(
                    np.mean(
                        (
                            predicted[observed[:, target]]
                            - truth[observed[:, target], target]
                        )
                        ** 2
                    )
                )
            alpha = [0.1, 1.0][int(errors[1] < errors[0])]
            scaler = StandardScaler().fit(x)
            oracle = Ridge(alpha=alpha).fit(
                scaler.transform(x[observed[:, target]]),
                truth[observed[:, target], target],
            )
            replay = model.predict(x, sample_ids=data["origin_ids"])
            block = replay["outputs"][0]["predictions"][0]
            self.assertEqual(
                block["target_names"], [data["dataset"]["target_names"][target]]
            )
            np.testing.assert_allclose(
                np.asarray(block["values"]).ravel(),
                oracle.predict(scaler.transform(x)),
                atol=2e-8,
                rtol=2e-8,
            )
            self.assertEqual({item["phase"] for item in replay["lineage"]}, {"PREDICT"})
        bad = copy.deepcopy(data)
        bad["dataset"]["target_mask"]["values"] = [
            [False, row[1]] for row in data["dataset"]["target_mask"]["values"]
        ]
        selected = recipe()
        selected["steps"][-1]["params"]["native_target_index"] = 0
        with self.assertRaisesRegex(RuntimeError, "no observed training target"):
            run_pipeline(bad, selected)


if __name__ == "__main__":
    unittest.main()
