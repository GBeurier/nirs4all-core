"""Raw multimodal fit and native state transport; all numerics live in Methods."""
from __future__ import annotations

import json
from collections.abc import Mapping
from pathlib import Path
from typing import Any, Self

from ._dataset import dataset


def _project(value: Any) -> tuple[Any, dict[str, Any]]:
    from nirs4all_io.dataset_facade import to_u07_raw_sources
    cohort = dataset(value).multimodal
    # Projection for training uses the same target-free feature view as replay.
    from nirs4all_io.multimodal import MultimodalDataset
    features = MultimodalDataset(dict(cohort.sources), sample_ids=cohort.sample_ids,
                                target_names=["y"], partitions=["predict"] * len(cohort.sample_ids),
                                source_alignment=cohort.source_alignment, name=cohort.name)
    return cohort, to_u07_raw_sources(features)


def _compatible(current: Mapping[str, Any], expected: Mapping[str, Any]) -> dict[str, Any]:
    from nirs4all_io.public_content import compatible_source_schemas
    return compatible_source_schemas(current, expected)


class MultimodalPredictor:
    """One complete native N4MF predictor, independent of training row storage.

    The JSON envelope transports Methods state and IO input contracts. It does
    not claim the DAG provenance or signatures of a Core .n4a archive.
    """
    def __init__(self, native: Any, recipe: Mapping[str, Any], source_schemas: Mapping[str, Any], target_names: list[str]) -> None:
        self._native = native
        self._recipe = json.loads(json.dumps(recipe, allow_nan=False))
        self._source_schemas = json.loads(json.dumps(source_schemas, allow_nan=False))

        self._target_names = tuple(target_names)

    @property
    def recipe(self) -> dict[str, Any]:
        return json.loads(json.dumps(self._recipe))

    @property
    def source_schemas(self) -> dict[str, Any]:
        return json.loads(json.dumps(self._source_schemas))

    @classmethod
    def fit(cls, recipe: Mapping[str, Any], value: Any) -> MultimodalPredictor:
        import numpy as np
        from n4m import MultimodalPipeline
        cohort, raw = _project(value)
        if cohort.y is None or cohort.y.ndim != 1 or cohort.y.dtype.kind not in "iuf" or not np.all(cohort.target_mask) or not np.isfinite(cohort.y).all():
            raise ValueError("Multimodal fit requires one finite observed numeric target per sample")
        if any(p != "train" for p in cohort.partitions):
            raise ValueError("Multimodal full fit accepts training rows only")
        native = MultimodalPipeline(recipe, raw["source_schemas"])
        try:
            native.fit({name: source.values for name, source in cohort.sources.items()}, cohort.y)
            return cls(native, recipe, raw["source_schemas"], list(cohort.target_names))
        except Exception:
            native.close()
            raise

    def predict(self, value: Any) -> dict[str, Any]:
        cohort, raw = _project(value)
        if cohort.y is not None or any(p != "predict" for p in cohort.partitions):
            raise ValueError("Multimodal prediction requires a target-free predict cohort")
        schemas = _compatible(raw["source_schemas"], self.source_schemas)
        values = self._native.predict({name: source.values for name, source in cohort.sources.items()}, source_schemas=schemas)
        return {"sample_ids": list(cohort.sample_ids), "target_names": list(self._target_names), "values": values.reshape(-1, 1).tolist()}

    def to_dict(self) -> dict[str, Any]:
        return {"schema": "nirs4all.multimodal-predictor.v1", "schema_version": 1,
                "recipe": self.recipe, "source_schemas": self.source_schemas,
                "state": list(self._native.export_state()), "target_names": list(self._target_names)}

    def export(self, path: str | Path) -> Path:
        destination = Path(path)
        with destination.open("x", encoding="utf-8") as stream:
            json.dump(self.to_dict(), stream, ensure_ascii=False, allow_nan=False)
        return destination

    @classmethod
    def load(cls, value: Mapping[str, Any] | str | Path) -> MultimodalPredictor:
        from n4m import MultimodalPipeline
        record = dict(value) if isinstance(value, Mapping) else json.loads(Path(value).read_text(encoding="utf-8"))
        if set(record) != {"schema", "schema_version", "recipe", "source_schemas", "state", "target_names"}:
            raise ValueError("Invalid multimodal predictor fields")
        if record["schema"] != "nirs4all.multimodal-predictor.v1" or type(record["schema_version"]) is not int or record["schema_version"] != 1:
            raise ValueError("Unsupported multimodal predictor schema")
        names = record["target_names"]
        if not isinstance(names, list) or len(names) != 1 or not isinstance(names[0], str) or not names[0].strip():
            raise ValueError("One nonempty target name is required")
        state = record["state"]
        if not isinstance(state, list) or not state or len(state) > 67_108_864 or any(type(b) is not int or not 0 <= b <= 255 for b in state):
            raise ValueError("Native state must be a bounded exact byte array")
        native = MultimodalPipeline.from_state(bytes(state), recipe=record["recipe"], source_schemas=record["source_schemas"])
        return cls(native, record["recipe"], record["source_schemas"], names)

    def close(self) -> None:
        self._native.close()

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *_: object) -> None:
        self.close()
