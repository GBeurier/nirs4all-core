"""Generic n4m role recipes and trained envelopes (``nirs4all.n4m.trained_pipeline.v8``).

A recipe step is the language-neutral token ``"n4m:<catalog method id>"`` (or
``{"class": "n4m:<id>", "params": {...}}``), resolved through the
``nirs4all-methods`` manifest. The recipe (sample filters, transformers and
selectors, then one regressor or classifier) runs in the native role pipeline
of Methods (``n4m.roles.RolePipeline``, ABI 2.14), which validates the recipe,
routes every target column to the steps that need it, keeps filters on the
training rows, checks the input column names and refuses states that
contradict the recipe. This module only reads and writes the envelope shared
with the full Python ``nirs4all``, the R package and the Rust and JS/WASM
bindings.

The envelope holds ``schema``, ``recipe``, ``n_features``, ``feature_names``
(the fitted column names, when the fit had names) and one entry per stateful
step: ``method_id``, ``n4me_base64``, ``sha256``, ``contains_training_rows``
and, for a classifier trained on label names, ``class_names``. Envelopes
written before ``feature_names`` and ``contains_training_rows`` still load.

Every envelope field this module reads is checked here before it reaches the
native pipeline: ``n_features`` is a positive JSON integer equal to the native
width, column names hold no NUL character, and ``class_names`` is a non-empty
table of unique strings or finite numbers that labels every fitted class id
(index = id; it may keep labels a sample filter removed). The recipe is
snapshotted at fit and import, so the exported recipe is the one the states
attest.
"""

from __future__ import annotations

import base64
import copy
import hashlib
import json
import math
from pathlib import Path
from typing import Any

N4M_ROLE_PREFIX = "n4m:"
N4M_TRAINED_PIPELINE_SCHEMA = "nirs4all.n4m.trained_pipeline.v8"

_RECIPE_ROLES = frozenset({"sample_filter", "transformer", "selector", "regressor", "classifier"})


def _roles():
    try:
        from n4m import roles
    except ImportError as exc:  # pragma: no cover - exercised by optional installs
        raise ImportError(
            "n4m role recipes require the nirs4all-methods Python binding with ABI 2.14 "
            "role pipelines. Install `nirs4all-core[methods]`."
        ) from exc
    return roles


def n4m_role_method_id(name: Any) -> str | None:
    """Catalog method id of an ``"n4m:<id>"`` class name, else None."""

    if isinstance(name, str) and name.startswith(N4M_ROLE_PREFIX):
        return name[len(N4M_ROLE_PREFIX):]
    return None


def resolves_n4m_role(name: Any) -> bool:
    """True when an ``"n4m:<id>"`` class name resolves in the Methods manifest."""

    method_id = n4m_role_method_id(name)
    if method_id is None:
        return False
    try:
        _roles().method_class(method_id)
    except (ImportError, ValueError):
        return False
    return True


def n4m_role_capabilities() -> tuple[dict[str, Any], ...]:
    """Methods estimators usable as role recipe steps, read from the Methods manifest."""

    return tuple(
        {
            "token": N4M_ROLE_PREFIX + item["method_id"],
            "method_id": item["method_id"],
            "roles": tuple(item["roles"]),
            "node_kinds": tuple(item["node_kinds"]),
            "parameters": tuple(param["name"] for param in item["params"]),
        }
        for item in _roles().manifest()["methods"]
        if item["kind"] == "estimator" and _RECIPE_ROLES.intersection(item["roles"])
    )


def _steps(recipe: Any) -> list[Any]:
    """The recipe tokens, each an ``n4m:<method id>`` step."""

    pipeline = recipe.get("pipeline") if isinstance(recipe, dict) else None
    if not isinstance(pipeline, list):
        raise ValueError('an n4m role recipe is {"pipeline": [n4m:<method id> steps]}')
    for token in pipeline:
        if n4m_role_method_id(token["class"] if isinstance(token, dict) else token) is None:
            raise ValueError(f"n4m role recipes contain n4m:<method id> steps only, got {token!r}")
    return pipeline


def _check_column_names(names: Any) -> None:
    """Column names reach the native pipeline as C strings: refuse NUL first."""

    for name in names:
        if "\0" in str(name):
            raise ValueError(f"column name {name!r} contains a NUL character")


def _check_label_table(names: Any) -> None:
    """A label table (index = class id): non-empty, unique, all strings or all finite numbers."""

    if not isinstance(names, list) or not names:
        raise ValueError("class_names must be a non-empty list of labels")
    for name in names:
        if isinstance(name, bool) or not isinstance(name, (str, int, float)) or (isinstance(name, float) and not math.isfinite(name)):
            raise ValueError(f"class label {name!r} is not a string or a finite number")
    if len({isinstance(name, str) for name in names}) > 1:
        raise ValueError("class_names mixes strings and numbers")
    for name in names:
        # JSON, JS and R numbers are doubles: an integral label beyond 2^53 would round into another.
        if not isinstance(name, str) and float(name).is_integer() and abs(name) > 2**53:
            raise ValueError(f"class label {name!r} is not exactly representable as float64 (beyond ±2^53)")
    seen: set[Any] = set()
    for name in names:
        if name in seen:
            raise ValueError(f"class label {name!r} is duplicated")
        seen.add(name)


class N4mRolePipeline:
    """A fitted recipe of n4m role steps, portable as N4ME states.

    Args:
        recipe: ``{"pipeline": [step tokens]}``; a deep copy is kept as the
            recipe the fitted states attest.
        pipeline: The fitted native ``n4m.roles.RolePipeline`` of that recipe
            (also used for ``transform``, ``decision_function``,
            ``predict_proba`` and ``steps_info_``).
    """

    def __init__(self, recipe: dict[str, Any], pipeline: Any) -> None:
        self._recipe = copy.deepcopy(recipe)
        self.pipeline = pipeline

    @property
    def recipe(self) -> dict[str, Any]:
        """The recipe of the fitted states (a copy: editing it changes neither the model nor its export)."""

        return copy.deepcopy(self._recipe)

    @property
    def n_features(self) -> int:
        """Input width."""

        return int(self.pipeline.n_features_in_)

    @property
    def feature_names(self) -> list[str] | None:
        """Fitted input column names, in order (None: positional input)."""

        names = getattr(self.pipeline, "feature_names_in_", None)
        return None if names is None else [str(name) for name in names]

    @classmethod
    def fit_recipe(cls, recipe: dict[str, Any], X: Any, y: Any) -> N4mRolePipeline:
        """Fit ``recipe`` natively on ``X`` (DataFrame column names are kept) and ``y``.

        ``y`` holds the responses of a final regressor (one or several
        columns) or the labels of a final classifier. Missing or non-finite
        labels are refused.
        """

        recipe = copy.deepcopy(recipe)
        if hasattr(X, "columns"):
            _check_column_names(X.columns)
        return cls(recipe, _roles().RolePipeline(_steps(recipe)).fit(X, y))

    @classmethod
    def from_json(cls, source: str | Path) -> N4mRolePipeline:
        """Read a v8 envelope (JSON text or path) and rebuild its fitted pipeline."""

        if isinstance(source, Path) or not source.lstrip().startswith("{"):
            source = Path(source).read_text(encoding="utf-8")
        document = json.loads(source)
        if not isinstance(document, dict) or document.get("schema") != N4M_TRAINED_PIPELINE_SCHEMA:
            raise ValueError("unsupported trained n4m pipeline envelope")
        recipe, states = document["recipe"], document["states"]
        n_features = document.get("n_features")
        if isinstance(n_features, bool) or not isinstance(n_features, int) or n_features <= 0:
            raise ValueError(f"n_features must be a positive JSON integer, got {n_features!r}")
        payloads = []
        for state in states:
            payload = base64.b64decode(state["n4me_base64"], validate=True)
            if hashlib.sha256(payload).hexdigest() != state["sha256"]:
                raise ValueError(f"N4ME state of {state['method_id']} fails its checksum")
            payloads.append(payload)
        feature_names = document.get("feature_names")
        if feature_names is not None:
            if not (isinstance(feature_names, list) and all(isinstance(name, str) for name in feature_names)):
                raise ValueError("feature_names must be a list of strings")
            _check_column_names(feature_names)
        labelled = bool(states) and "class_names" in states[-1]
        if labelled:
            _check_label_table(states[-1]["class_names"])
        steps = _steps(recipe)
        pipeline = _roles().RolePipeline.from_states(steps, payloads, feature_names)
        fitted = [step for step in pipeline.steps_info_ if step["state_index"] >= 0]
        for step, state in zip(fitted, states, strict=True):
            if state["method_id"] != step["method_id"]:
                raise ValueError(f"N4ME state {state['method_id']} does not match its recipe step")
            if state.get("contains_training_rows", step["contains_training_rows"]) is not step["contains_training_rows"]:
                raise ValueError(f"contains_training_rows of {state['method_id']} contradicts its N4ME state")
        if n_features != pipeline.n_features_in_:
            raise ValueError(f"n_features is {n_features} but the states take {pipeline.n_features_in_} columns")
        if labelled:
            class_names = states[-1]["class_names"]
            if pipeline.steps_info_[-1]["role"] != "classifier":
                raise ValueError("class_names label the classes of a final classifier")
            for class_id in pipeline.classes_.tolist():
                if not 0 <= class_id < len(class_names):
                    raise ValueError(f"class id {class_id} has no entry in class_names ({len(class_names)} labels)")
            pipeline = _roles().RolePipeline.from_states(steps, payloads, feature_names, class_names)
        return cls(recipe, pipeline)

    def to_json(self, file: str | Path | None = None, *, allow_training_rows: bool = False) -> str:
        """The v8 envelope; also written to ``file`` when given.

        A state that embeds training rows (kernel PLS, LW-PLS, ...) is
        refused unless ``allow_training_rows`` is set.
        """

        states = [
            {
                "method_id": method_id,
                "n4me_base64": base64.b64encode(payload).decode("ascii"),
                "sha256": hashlib.sha256(payload).hexdigest(),
                "contains_training_rows": contains_training_rows,
            }
            for method_id, payload, contains_training_rows in self.pipeline.export_states(allow_training_rows=allow_training_rows)
        ]
        # Classifier label names (N4ME holds integer class ids only).
        class_names = self.pipeline.label_names_
        if class_names is not None:
            states[-1]["class_names"] = class_names.tolist()
        document: dict[str, Any] = {
            "schema": N4M_TRAINED_PIPELINE_SCHEMA,
            "recipe": self._recipe,
            "n_features": self.n_features,
        }
        if self.feature_names is not None:
            document["feature_names"] = self.feature_names
        document["states"] = states
        text = json.dumps(document, indent=1)
        if file is not None:
            Path(file).write_text(text + "\n", encoding="utf-8")
        return text

    def predict(self, X: Any) -> Any:
        """Predictions (regressor) or class labels (classifier) of the final model.

        A DataFrame is checked against the fitted column names and order.
        """

        if hasattr(X, "columns"):
            _check_column_names(X.columns)
        return self.pipeline.predict(X)

    def retrain(self, X: Any, y: Any) -> N4mRolePipeline:
        """Fit the same recipe afresh on new training rows."""

        return type(self).fit_recipe(self._recipe, X, y)
