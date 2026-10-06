"""Real native ragged encoder, observed-only target campaigns and cold replay."""

import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from nirs4all_core import NativeMultimodal, run_multimodal, run_pipeline
from sklearn.linear_model import Ridge
from sklearn.preprocessing import StandardScaler
from test_workflow_native import fixture


def ragged_fixture():
    data = fixture()
    data["schema"] = "nirs4all.dataset.v2"
    data["schema_version"] = 2
    ids = data["origin_ids"]
    packed = []
    times = []
    offsets = [0]
    present = []
    summary = []
    for i in range(12):
        n = 0 if i in [2, 7] else 1 + i % 4
        rows = np.array([[i * 0.2 + j * 0.3, np.sin(i + j)] for j in range(n)]).reshape(
            n, 2
        )
        clock = np.array([j * 0.25 for j in range(n)])
        packed.extend(rows.tolist())
        times.extend(clock.tolist())
        offsets.append(len(packed))
        present.append(n > 0)
        if n:
            summary.append(
                np.r_[
                    np.stack(
                        [rows.mean(0), rows.std(0), rows.min(0), rows.max(0)], axis=1
                    ).ravel(),
                    n,
                    clock[-1] - clock[0],
                    1,
                ]
            )
        else:
            summary.append(np.zeros(11))
    source = {
        "name": "kinetics",
        "source_kind": "ragged_series",
        "sample_ids": ids,
        "representation_id": "series_mv",
        "axes": ["sample", "time", "variable"],
        "array": {"dtype": "float64", "shape": [len(packed), 2], "values": packed},
        "offsets": {"dtype": "int64", "shape": [13], "values": offsets},
        "time_coordinates": {
            "dtype": "float64",
            "shape": [len(times)],
            "values": times,
        },
        "channel_names": ["a", "b"],
        "time_unit": "s",
        "presence_mask": {"dtype": "bool", "shape": [12], "values": present},
    }
    data["dataset"]["sources"].append(source)
    x = (
        np.column_stack(
            [
                np.array(data["dataset"]["sources"][0]["array"]["values"]),
                np.array(summary),
            ]
        )
        .astype(np.float32)
        .astype(float)
    )
    y = (
        np.column_stack(
            [
                1 + 0.5 * x[:, 0] + x[:, 7] + 0.2 * x[:, -1],
                2 + 0.4 * x[:, 2] - x[:, 11] + 0.3 * x[:, 10],
            ]
        )
        .astype(np.float32)
        .astype(float)
    )
    mask = np.ones((12, 2), bool)
    mask[[1, 5, 8], 0] = False
    mask[[3, 6, 9], 1] = False
    data["dataset"]["y"] = {
        "dtype": "float64",
        "shape": [12, 2],
        "values": [
            [float(y[i, t]) if mask[i, t] else None for t in range(2)]
            for i in range(12)
        ],
    }
    data["dataset"]["target_mask"] = {
        "dtype": "bool",
        "shape": [12, 2],
        "values": mask.tolist(),
    }
    data["dataset"]["target_names"] = ["protein", "moisture"]
    policies = [
        {"source_id": "spectra", "encoder": "identity", "missing_policy": "reject"},
        {
            "source_id": "kinetics",
            "encoder": "ragged_summary",
            "missing_policy": "zero_with_indicator",
        },
    ]
    pipeline = {
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
        "candidates": [{"alpha": 1.0}],
    }
    return data, policies, pipeline, x, y, mask


def predict_record(data):
    record = copy.deepcopy(data)
    record["dataset"]["y"] = None
    record["dataset"]["target_mask"] = None
    record["dataset"]["partitions"]["values"] = ["predict"] * 12
    record["dataset"]["partitions"]["dtype"] = "<U7"
    return record


@unittest.skipUnless(
    os.environ.get("NIRS4ALL_CORE_CLI") and os.environ.get("N4M_LIBRARY_PATH"),
    "real runtime required",
)
class NativeMultimodalTest(unittest.TestCase):
    def test_ragged_missing_modality_partial_multiy_and_independent_cold_oracles(self):
        data, policies, pipeline, x, y, mask = ragged_fixture()
        model = run_multimodal(data, pipeline, policies)
        fresh = predict_record(data)
        replay = model.predict(fresh)
        scaler = StandardScaler().fit(x)
        for target, entry in enumerate(replay["target_results"]):
            oracle = Ridge(alpha=1.0).fit(
                scaler.transform(x[mask[:, target]]), y[mask[:, target], target]
            )
            block = entry["replay"]["outputs"][0]["predictions"][0]
            self.assertEqual(block["sample_ids"], data["origin_ids"])
            self.assertEqual(
                block["target_names"], [data["dataset"]["target_names"][target]]
            )
            np.testing.assert_allclose(
                np.asarray(block["values"]).ravel(),
                oracle.predict(scaler.transform(x)),
                rtol=2e-8,
                atol=2e-8,
            )
            self.assertEqual(
                {item["phase"] for item in entry["replay"]["lineage"]}, {"PREDICT"}
            )
            refit = next(
                row
                for row in model.outcomes[target]["lineage"]
                if row["phase"] == "REFIT" and row["node_id"] == "model:pls"
            )
            self.assertEqual(refit["metrics"]["native_observed_target_rows"], 9)
        provenance = model.to_dict()["training_projection_provenance"]
        self.assertEqual(
            provenance["source_projections"][1]["input_presence_mask"]["values"],
            [i not in [2, 7] for i in range(12)],
        )
        with tempfile.TemporaryDirectory() as directory:
            path = model.export(Path(directory) / "model.json")
            record_path = Path(directory) / "predict.json"
            record_path.write_text(json.dumps(fresh))
            cold = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    "import json,sys; from nirs4all_core import NativeMultimodal; print(json.dumps(NativeMultimodal.load(sys.argv[1]).predict(json.load(open(sys.argv[2])))['target_results']))",
                    str(path),
                    str(record_path),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(cold.returncode, 0, cold.stderr)
            for actual, expected in zip(
                json.loads(cold.stdout), replay["target_results"]
            ):
                np.testing.assert_equal(
                    actual["replay"]["outputs"], expected["replay"]["outputs"]
                )
        altered = model.to_dict()
        altered["config"]["source_policies"][1]["missing_policy"] = "reject"
        with self.assertRaisesRegex(
            RuntimeError, "signed target campaign|projection provenance"
        ):
            NativeMultimodal.load(altered)
        wrong = predict_record(data)
        wrong["dataset"]["sources"][1]["time_unit"] = "ms"
        with self.assertRaisesRegex(RuntimeError, "source schemas differ"):
            model.predict(wrong)

    def test_absence_default_and_targets_without_observations_refuse(self):
        data, policies, pipeline, *_ = ragged_fixture()
        policies[1]["missing_policy"] = "reject"
        with self.assertRaisesRegex(RuntimeError, "explicit zero_with_indicator"):
            run_multimodal(data, pipeline, policies)
        policies[1]["missing_policy"] = "zero_with_indicator"
        data["dataset"]["target_mask"]["values"] = [
            [False, row[1]] for row in data["dataset"]["target_mask"]["values"]
        ]
        with self.assertRaisesRegex(RuntimeError, "no observed training target"):
            run_multimodal(data, pipeline, policies)

    def test_generic_matrix_profile_refuses_packed_ragged_even_when_all_present(self):
        data, _, pipeline, *_ = ragged_fixture()
        source = data["dataset"]["sources"][1]
        source["array"] = {
            "dtype": "float64",
            "shape": [13, 2],
            "values": [[i * 0.1, i * 0.2] for i in range(13)],
        }
        source["offsets"]["values"] = [0] + list(range(2, 14))
        source["time_coordinates"] = {
            "dtype": "float64",
            "shape": [13],
            "values": [0, 1] + [0] * 11,
        }
        source["presence_mask"]["values"] = [True] * 12
        data["dataset"]["sources"] = [source]
        with self.assertRaisesRegex(RuntimeError, "(?i)ragged|dense"):
            run_pipeline(data, pipeline, source_id="kinetics")

    def test_partial_multiple_classification_targets_match_independent_latent_oracles(
        self,
    ):
        from sklearn.cross_decomposition import PLSRegression

        data, policies, pipeline, x, _, mask = ragged_fixture()
        labels = np.array([(i // 2) % 2 for i in range(12)])
        truth = np.column_stack([labels, np.where(labels == 0, 3, 7)])
        data["dataset"]["target_names"] = ["class_a", "class_b"]
        data["dataset"]["task_type"] = "classification"
        data["dataset"]["y"] = {
            "dtype": "int64",
            "shape": [12, 2],
            "values": [
                [int(truth[i, t]) if mask[i, t] else 1e99 for t in range(2)]
                for i in range(12)
            ],
        }
        pipeline["steps"][-1] = {
            "method_id": "models.classification.pls_lda",
            "role": "classifier",
            "params": {"n_components": 1},
        }
        pipeline["candidates"] = [{}]
        model = run_multimodal(data, pipeline, policies)
        replay = model.predict(predict_record(data))
        scaler = StandardScaler().fit(x)
        for target, entry in enumerate(replay["target_results"]):
            train = mask[:, target]
            classes = np.unique(truth[train, target])
            dummy = (truth[train, target, None] == classes).astype(float)
            latent = PLSRegression(n_components=1, scale=False).fit(
                scaler.transform(x[train]), dummy
            )
            # Methods uses unbiased pooled within-class covariance and empirical
            # priors; sklearn's default SVD normalizes imbalanced classes differently.
            # Derive that contract from independent sklearn PLS scores and NumPy.
            scores = latent.x_scores_
            means = np.stack(
                [
                    scores[truth[train, target] == label].mean(axis=0)
                    for label in classes
                ]
            )
            covariance = sum(
                (scores[truth[train, target] == label] - means[index]).T
                @ (scores[truth[train, target] == label] - means[index])
                for index, label in enumerate(classes)
            ) / (train.sum() - len(classes))
            precision = np.linalg.inv(covariance)
            weights = means @ precision
            priors = np.array(
                [(truth[train, target] == label).mean() for label in classes]
            )
            intercepts = -0.5 * np.sum(weights * means, axis=1) + np.log(priors)
            discriminants = (
                latent.transform(scaler.transform(x)) @ weights.T + intercepts
            )
            expected = classes[discriminants.argmax(axis=1)]
            block = entry["replay"]["outputs"][0]["predictions"][0]
            np.testing.assert_array_equal(np.asarray(block["values"]).ravel(), expected)
            self.assertEqual(
                block["target_names"], [data["dataset"]["target_names"][target]]
            )

            self.assertEqual(
                {row["phase"] for row in entry["replay"]["lineage"]}, {"PREDICT"}
            )
        with tempfile.TemporaryDirectory() as directory:
            path = model.export(Path(directory) / "classifier.json")
            record_path = Path(directory) / "predict.json"
            record_path.write_text(json.dumps(predict_record(data)))
            cold = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    "import json,sys; from nirs4all_core import NativeMultimodal; print(json.dumps(NativeMultimodal.load(sys.argv[1]).predict(json.load(open(sys.argv[2])))))",
                    str(path),
                    str(record_path),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(cold.returncode, 0, cold.stderr)
            for actual, expected in zip(
                json.loads(cold.stdout)["target_results"], replay["target_results"]
            ):
                self.assertEqual(
                    actual["replay"]["outputs"], expected["replay"]["outputs"]
                )
                self.assertEqual(
                    {row["phase"] for row in actual["replay"]["lineage"]}, {"PREDICT"}
                )
