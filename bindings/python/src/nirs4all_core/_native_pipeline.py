"""Thin product façade over native DAG catalog pipelines and portable N4ME states."""

from __future__ import annotations

import json
import os
from collections.abc import Mapping
from copy import deepcopy
from pathlib import Path
from typing import Any
from uuid import uuid4

from ._dataset import dataset
from ._workflow import _invoke


def _library(value: str | Path | None) -> str:
    value = (
        value or os.environ.get("N4M_LIBRARY_PATH") or os.environ.get("N4M_LIB_PATH")
    )
    if value is None:
        raise ValueError("methods_library_path or N4M_LIBRARY_PATH is required")
    return str(Path(value).resolve())


class NativePipeline:
    """CV-selected, fully refitted native pipeline with a portable JSON package.

    Each transformer learns inside its training fold; native DAG owns OOF,
    selection and refit. The envelope is a Package V2 transport, not .n4a.
    """

    def __init__(
        self,
        record: Mapping[str, Any],
        cli: str | Path | None = None,
        methods_library_path: str | Path | None = None,
    ) -> None:
        self._record = deepcopy(dict(record))
        self._cli = cli
        self._methods_library_path = methods_library_path

    @property
    def outcome(self) -> dict[str, Any]:
        return deepcopy(self._record["training_outcome"])

    @property
    def config(self) -> dict[str, Any]:
        return deepcopy(self._record["config"])

    def to_dict(self) -> dict[str, Any]:
        return deepcopy(self._record)

    def predict(
        self,
        x: Any,
        *,
        sample_ids: Any = None,
        methods_library_path: str | Path | None = None,
        cli: str | Path | None = None,
    ) -> dict[str, Any]:
        rows = x.tolist() if hasattr(x, "tolist") else x
        ids = (
            list(sample_ids)
            if sample_ids is not None
            else [f"predict:{i}" for i in range(len(rows))]
        )
        return _invoke(
            cli or self._cli,
            "pipeline-predict",
            {"model": self.to_dict(), "x": rows, "sample_ids": ids},
            methods_library=_library(
                methods_library_path or self._methods_library_path
            ),
            run_id="run:predict:" + uuid4().hex,
        )

    def retrain(self, data: Any, **options: Any) -> NativePipeline:
        return run_pipeline(
            data,
            self.config["pipeline"],
            **(
                {
                    "source_id": self.config["source_id"],
                    "cli": self._cli,
                    "methods_library_path": self._methods_library_path,
                }
                | options
            ),
        )

    def export(self, path: str | Path) -> Path:
        path = Path(path)
        _invoke(
            self._cli, "pipeline-export", self.to_dict(), destination=path.resolve()
        )
        return path

    @classmethod
    def load(
        cls,
        value: Mapping[str, Any] | str | Path,
        *,
        cli: str | Path | None = None,
        methods_library_path: str | Path | None = None,
    ) -> NativePipeline:
        record = (
            dict(value)
            if isinstance(value, Mapping)
            else json.loads(Path(value).read_text(encoding="utf-8"))
        )
        return cls(_invoke(cli, "pipeline-load", record), cli, methods_library_path)


def run_pipeline(
    data: Any,
    pipeline: Mapping[str, Any],
    *,
    source_id: str = "spectra",
    methods_library_path: str | Path | None = None,
    run_id: str | None = None,
    cli: str | Path | None = None,
) -> NativePipeline:
    """Compile a live-catalog native recipe, execute CV/OOF selection and refit.

    Recipes declare ``steps`` (method_id, role, params) and ``candidates``
    (final regressor parameter overrides). Numerical algorithms stay in Methods.
    """
    library = _library(methods_library_path)
    record = _invoke(
        cli,
        "pipeline-run",
        {"dataset": dataset(data).to_dict(), "pipeline": dict(pipeline)},
        source_id=source_id,
        methods_library=library,
        run_id=run_id or "run:pipeline:" + uuid4().hex,
    )
    return NativePipeline(record, cli, library)
