"""High-level raw archive replay; Core, IO and DAG own their existing seams."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from ._archive import read_portable_predictor_package_v2


def predict_multimodal_archive(path: str | Path, value: Any) -> dict[str, Any]:
    """Replay a Core U07 archive on a current IO dataset, without a workspace.

    Requires DAG's public multimodal replay helper. Archive/container validation
    happens in native Core; current raw data assembly stays in IO; trust, request
    signing, artifact hydration and PREDICT scheduling stay in DAG-ML.
    """
    try:
        from dag_ml.multimodal_replay import replay_multimodal_predictor_package
    except ImportError as error:
        raise ImportError("Raw archive replay requires DAG-ML's multimodal replay helper") from error
    from nirs4all_io.multimodal_runtime import multimodal_runtime_input
    from nirs4all_io.public_content import compatible_source_schemas

    package = json.loads(read_portable_predictor_package_v2(path))
    current = multimodal_runtime_input(value)
    saved = package["effective_plan"]["graph_plan"]["graph"]["nodes"][0]["operator"]["source_schemas"]
    current["source_schemas"] = compatible_source_schemas(current["source_schemas"], saved)
    evidence = replay_multimodal_predictor_package(package, current)
    outputs = evidence["outcome"]["outputs"]
    if len(outputs) != 1 or len(outputs[0]["predictions"]) != 1:
        raise ValueError("Raw archive replay requires one complete sample prediction block")
    block = outputs[0]["predictions"][0]
    import dag_ml

    alignment = dag_ml.align_named_source_rows({
        "sample_ids": current["sample_ids"], "required_source_ids": ["prediction"],
        "sources": [{"source_id": "prediction", "sample_ids": block["sample_ids"]}],
    })
    positions = alignment["sources"][0]["row_indices"]
    return {"sample_ids": current["sample_ids"], "target_names": block["target_names"],
            "values": [block["values"][i] for i in positions], "training_performed": False,
            "outcome": evidence["outcome"], "audit": evidence["audit"]}
