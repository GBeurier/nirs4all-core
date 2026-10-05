"""Emit one logical host-array cohort and content byte proof for every host."""
from __future__ import annotations

import argparse
import hashlib
import json
from copy import deepcopy
from pathlib import Path

from nirs4all_io.dataset_facade import to_u07_raw_sources
from nirs4all_io.public_content import (
    compatible_source_schemas,
    dataset_content_bytes,
    metadata_number,
)
from nirs4all_io.public_dataset import Dataset


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    directory = parser.parse_args().directory
    record = json.loads((directory / "predict.json").read_text())
    raw = record["dataset"]
    sources = {source["name"]: deepcopy(source["array"]["values"]) for source in raw["sources"]}
    for row in sources["metadata"]:
        row[0] = metadata_number(row[0])
    options = {field: {source["name"]: source[field] for source in raw["sources"]}
               for field in ["axis_units", "axis_coordinates", "feature_names"]}
    host = Dataset.from_sources(sources, sample_ids=raw["sample_ids"], name=raw["name"], partitions=raw["partitions"]["values"],
                                target_names=raw["target_names"], origin_ids=record["origin_ids"], fold_ids=record["fold_ids"],
                                groups=None if raw["groups"] is None else raw["groups"]["values"],
                                independent_unit_ids=raw.get("independent_unit_ids"), repetition_ids=raw.get("repetition_ids"), **options)
    current, original = to_u07_raw_sources(host.multimodal), to_u07_raw_sources(Dataset(record).multimodal)
    compatible_source_schemas(current["source_schemas"], original["source_schemas"])
    canonical = dataset_content_bytes(record)
    assert dataset_content_bytes(host.to_dict()) == canonical
    (directory / "logical-host-dataset.json").write_text(json.dumps(host.to_dict(), allow_nan=False, ensure_ascii=False))
    def reordered(value):
        if isinstance(value, dict):
            return {key: reordered(value[key]) for key in reversed(list(value))}
        if isinstance(value, list):
            return [reordered(cell) for cell in value]
        if isinstance(value, float) and value.is_integer():
            return int(value)
        return value
    alternate = reordered(deepcopy(record))
    for row in alternate["dataset"]["sources"][3]["array"]["values"]:
        row[0] = format(metadata_number(row[0]), ".17e")
    assert dataset_content_bytes(alternate) == canonical
    (directory / "reordered-host-dataset.json").write_text(json.dumps(alternate, allow_nan=False, ensure_ascii=False))
    evidence = {"canonical_content_utf8": canonical.decode("utf-8"), "fingerprint": hashlib.sha256(canonical).hexdigest()}
    (directory / "content-expected.json").write_text(json.dumps(evidence, ensure_ascii=False))
    print("PASS logical host arrays equal normalized file content", evidence["fingerprint"])


if __name__ == "__main__":
    main()
