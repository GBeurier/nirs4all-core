"""Cold CPU transport of browser-native Methods tuning predictors."""
from __future__ import annotations

import hashlib
import json
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ._dataset import dataset


def _hash(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False).encode()).hexdigest()


@dataclass(frozen=True)
class BrowserTuningResult:
    """Exact browser study export; CPU replays its selected N4ME predictor.

    Study checkpoints remain in their browser-host optimizer contract. CPU
    prediction does not resume or reinterpret that optimizer history.
    """
    record: dict[str, Any]

    def export(self, path: str | Path) -> Path:
        target = Path(path)
        with target.open("x", encoding="utf-8") as stream:
            json.dump(self.record, stream, ensure_ascii=False, allow_nan=False)
        return target

    def trials(self) -> list[dict[str, Any]]:
        return deepcopy(self.record["search"]["trials"])

    def predict(self, data: Any, *, run_id: str = "run:browser-tuning:cpu:predict") -> dict[str, Any]:
        from dag_ml import attach_predict_cohort_to_envelope
        from dag_ml.role_pipeline_replay import replay_browser_role_pipeline

        value = dataset(data).to_dict()
        raw = value["dataset"]
        source_id = self.record["config"]["sourceId"]
        if raw["y"] is not None or any(role != "predict" for role in raw["partitions"]["values"]):
            raise ValueError("Browser package prediction requires target-free predict rows")
        if (raw["groups"] is not None or "independent_unit_ids" in raw or "repetition_ids" in raw
                or value["origin_ids"] != raw["sample_ids"]):
            raise ValueError("Browser package prediction requires independent physical samples")
        source = next((source for source in raw["sources"] if source["name"] == source_id), None)
        if source is None or len(source["array"]["shape"]) != 2 or not all(source["presence_mask"]["values"]):
            raise ValueError("Browser package prediction requires its complete numeric source")
        schema = {"source_id": source["name"], "representation_id": source["representation_id"],
                  "axes": source["axes"], "dtype": source["array"]["dtype"],
                  "shape": [None, *source["array"]["shape"][1:]], "feature_names": source["feature_names"],
                  "axis_units": source["axis_units"], "axis_coordinates": source["axis_coordinates"]}
        package = json.loads(self.record["packageJson"])
        if schema != self.record["contracts"]["objective"]["source_schema"] or _hash(schema) != package["training_envelope"]["schema_fingerprint"]:
            raise ValueError("Browser predictor source schema differs from training")
        ids = raw["sample_ids"]
        relations = {"records": [{"observation_id": "obs:" + sample, "sample_id": sample,
                                  "target_id": None, "source_id": source_id, "group_id": None,
                                  "origin_sample_id": None, "is_augmented": False} for sample in ids]}
        envelope = attach_predict_cohort_to_envelope(package["training_envelope"], {
            "role": "inference", "target_names": package["effective_plan"]["graph_plan"]["graph"]["metadata"]["target_names"],
            "data_content_fingerprint": _hash(source["array"]["values"]), "target_content_fingerprint": None,
            "relations": relations})
        return replay_browser_role_pipeline(self.record["packageJson"], envelope, x=source["array"]["values"],
                                            sample_ids=ids, source_id=source_id, run_id=run_id)


def load_browser_tuning(record: dict[str, Any] | str | Path) -> BrowserTuningResult:
    """Validate exact browser package bytes before exposing fit-free CPU replay."""
    from dag_ml import InitialFullRefitPackage

    value = json.loads(Path(record).read_text()) if isinstance(record, (str, Path)) else deepcopy(record)
    if value.get("schema") != "nirs4all.browser-tuning.v1" or value.get("packageKind") != "dagml.initial-full-refit.v1":
        raise ValueError("Unsupported browser tuning export")
    payload = value["packageJson"]
    if hashlib.sha256(payload.encode()).hexdigest() != value["packageSha256"]:
        raise ValueError("Browser tuning package hash mismatch")
    package = InitialFullRefitPackage(payload).to_dict()
    config, request, search = value["config"], value["contracts"]["request"], value["search"]
    node = package["effective_plan"]["graph_plan"]["graph"]["nodes"][0]
    if (config["seed"] != package["execution_root_seed"] or request["trial_budget"] != config["trials"]
            or config["metric"] != request["metric"] or config["sampler"] != request["optimizer_descriptor"]["sampler"]
            or request["optimizer_descriptor"]["seed"] != config["seed"]
            or len(search["trials"]) != config["trials"]):
        raise ValueError("Browser study options differ from native package")
    for key, selected in search["selected_params"].items():
        if node["params"].get(key) != selected:
            raise ValueError("Browser selected predictor differs from search selection")
    return BrowserTuningResult(value)
