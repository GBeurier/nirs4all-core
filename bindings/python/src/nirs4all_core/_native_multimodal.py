"""Thin native ragged/missing-modality/partial-target workflow transport."""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from copy import deepcopy
from pathlib import Path
from typing import Any
from uuid import uuid4

from ._dataset import dataset
from ._native_pipeline import _library
from ._workflow import _invoke


class NativeMultimodal:
    """Source contracts plus independent captured native model states per target."""

    def __init__(
        self,
        record: Mapping[str, Any],
        cli: str | Path | None = None,
        methods_library_path: str | Path | None = None,
    ):
        self._record = deepcopy(dict(record))
        self._cli = cli
        self._methods_library_path = methods_library_path

    @property
    def config(self) -> dict[str, Any]:
        return deepcopy(self._record["config"])

    @property
    def outcomes(self) -> list[dict[str, Any]]:
        return [
            deepcopy(entry["model"]["training_outcome"])
            for entry in self._record["target_models"]
        ]

    def to_dict(self) -> dict[str, Any]:
        return deepcopy(self._record)

    def predict(
        self,
        data: Any,
        *,
        methods_library_path: str | Path | None = None,
        cli: str | Path | None = None,
    ) -> dict[str, Any]:
        return _invoke(
            cli or self._cli,
            "native-multimodal-predict",
            {"model": self.to_dict(), "dataset": dataset(data).to_dict()},
            methods_library=_library(
                methods_library_path or self._methods_library_path
            ),
            run_id="run:multimodal:predict:" + uuid4().hex,
        )

    def export(self, path: str | Path) -> Path:
        path = Path(path)
        _invoke(
            self._cli,
            "native-multimodal-export",
            self.to_dict(),
            destination=path.resolve(),
        )
        return path

    def retrain(self, data: Any, **options: Any) -> NativeMultimodal:
        return run_multimodal(
            data,
            self.config["pipeline"],
            self.config["source_policies"],
            **(
                {"cli": self._cli, "methods_library_path": self._methods_library_path}
                | options
            ),
        )

    @classmethod
    def load(
        cls,
        value: Mapping[str, Any] | str | Path,
        *,
        cli: str | Path | None = None,
        methods_library_path: str | Path | None = None,
    ) -> NativeMultimodal:
        record = (
            dict(value)
            if isinstance(value, Mapping)
            else json.loads(Path(value).read_text(encoding="utf-8"))
        )
        return cls(
            _invoke(cli, "native-multimodal-load", record), cli, methods_library_path
        )


def run_multimodal(
    data: Any,
    pipeline: Mapping[str, Any],
    source_policies: Sequence[Mapping[str, Any]],
    *,
    methods_library_path: str | Path | None = None,
    run_id: str | None = None,
    cli: str | Path | None = None,
) -> NativeMultimodal:
    library = _library(methods_library_path)
    record = _invoke(
        cli,
        "native-multimodal-run",
        {
            "dataset": dataset(data).to_dict(),
            "pipeline": dict(pipeline),
            "source_policies": list(source_policies),
        },
        methods_library=library,
        run_id=run_id or "run:multimodal:" + uuid4().hex,
    )
    return NativeMultimodal(record, cli, library)
