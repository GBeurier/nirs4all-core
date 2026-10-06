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
    _record: dict[str, Any]

    @property
    def record(self) -> dict[str, Any]:
        return deepcopy(self._record)

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
    from dag_ml import validate_host_hpo_snapshot
    from n4m.model_selection import Direction, Metric, Optimizer, Sampler, SearchSpace

    contracts, snapshot = value["contracts"], value["snapshot"]
    options = {"sampler": config["sampler"], "pruner": "none", "direction": request["direction"],
               "metric": config["metric"], "seed": config["seed"], "startupTrials": 2}
    expected_contract = {"space": request["optimizer_descriptor"]["space"], "options": options,
                         "objective": contracts["objective"], "warmStart": None}
    if snapshot["schema"] != "dagml.n4m.wasm-hpo.v1" or snapshot["contract"] != expected_contract or snapshot["prepared"] is not None:
        raise ValueError("Browser optimizer snapshot contract mismatch")
    verified = validate_host_hpo_snapshot(contracts["plan"], contracts["envelope"], request, snapshot["committed"])
    if search != verified or search["selected_params"] != node["params"]:
        raise ValueError("Browser study history or selected predictor differs from native checkpoint")
    if any(package["training_envelope"][key] != contracts["envelope"][key]
           for key in ("schema_fingerprint", "data_content_fingerprint", "target_content_fingerprint")):
        raise ValueError("Browser predictor training provenance differs from study")
    expected_space = {"n_components": {"kind": "int", "low": 1, "high": 3},
                      "scale": {"kind": "categorical", "type": "boolean", "choices": [False, True]}}
    if request["optimizer_descriptor"]["space"] != expected_space:
        raise ValueError("Browser optimizer space differs from supported native profile")
    with SearchSpace() as space:
        space.add_int("n_components", 1, 3)
        space.add_categorical("scale", [False, True])
        with Optimizer(space, sampler=Sampler[config["sampler"].upper()], direction=Direction[request["direction"].upper()],
                       metric=Metric[config["metric"].upper()], seed=config["seed"], n_startup_trials=2) as expected_optimizer, Optimizer.load(bytes(snapshot["n4mopt"])) as optimizer:
            if not optimizer.configuration_matches(expected_optimizer):
                raise ValueError("Methods optimizer configuration differs from native study")
            trials = optimizer.get_trials()
    # Trial snapshots own their decoded data after their native owner closes.
    terminal = snapshot["committed"]["trials"]
    if len(trials) != len(terminal):
        raise ValueError("Methods optimizer history differs from native checkpoint")
    for index, (trial, recorded) in enumerate(zip(trials, terminal)):
        evidence = recorded.get("evidence", recorded)
        expected_status = {"complete": "COMPLETED", "failed": "FAILED", "pruned": "PRUNED"}[recorded["state"]]
        if (trial.id != index or trial.status.name != expected_status or trial.params != evidence["params"]
                or (recorded["state"] == "complete" and trial.score != evidence["score"])):
            raise ValueError("Methods optimizer terminal history differs from native checkpoint")
    return BrowserTuningResult(value)
