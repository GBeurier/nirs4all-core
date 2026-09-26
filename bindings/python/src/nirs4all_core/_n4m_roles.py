"""Generic n4m role recipes and trained envelopes (``nirs4all.n4m.trained_pipeline.v8``).

A recipe step is the language-neutral token ``"n4m:<catalog method id>"`` (or
``{"class": "n4m:<id>", "params": {...}}``), resolved through the
``nirs4all-methods`` manifest (``n4m.roles.method_class``). Parameters, fitting,
numerics and the portable N4ME state stay in Methods; this module orders the
steps and reads/writes the envelope shared with the full Python ``nirs4all``,
the R package and the Rust and JS/WASM bindings.

Recipe steps, in order: sample filters (training rows only, no state),
transformers and selectors, then one regressor or classifier.
"""

from __future__ import annotations

import base64
import hashlib
import json
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
            "n4m role recipes require the nirs4all-methods Python binding with ABI 2.13 "
            "estimator roles. Install `nirs4all-core[methods]`."
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


def _step(token: Any) -> Any:
    name = token["class"] if isinstance(token, dict) else token
    method_id = n4m_role_method_id(name)
    if method_id is None:
        raise ValueError(f"n4m role recipes contain n4m:<method id> steps only, got {token!r}")
    params = (token.get("params") or {}) if isinstance(token, dict) else {}
    return _roles().method_class(method_id)(**params)


class N4mRolePipeline:
    """A fitted recipe of n4m role steps, portable as N4ME states.

    Args:
        recipe: ``{"pipeline": [step tokens]}``.
        estimators: The fitted transformers / selectors and the final model,
            in recipe order (filters excluded).
        n_features: Input width.
    """

    def __init__(self, recipe: dict[str, Any], estimators: list[Any], n_features: int) -> None:
        self.recipe = recipe
        self.estimators = estimators
        self.n_features = n_features

    @classmethod
    def fit_recipe(cls, recipe: dict[str, Any], X: Any, y: Any) -> N4mRolePipeline:
        """Fit every step of ``recipe`` natively on ``X``, ``y``."""

        import numpy as np

        roles = _roles()
        steps = [_step(token) for token in recipe["pipeline"]]
        if not steps or not isinstance(steps[-1], (roles.NativeRegressor, roles.NativeClassifier)):
            raise ValueError("an n4m role recipe ends with one regressor or classifier")
        values = np.asarray(X, dtype=np.float64)
        targets = np.asarray(y)
        n_features = values.shape[1]
        fitted: list[Any] = []
        for step in steps[:-1]:
            # Intermediate steps see y only when their method requires it.
            step_y = targets if step.input_requirements()["y"] == "required" else None
            if isinstance(step, roles.NativeSampleFilter):
                keep = step.fit(values, step_y).get_mask(values, step_y)
                values, targets = values[keep], targets[keep]
            elif isinstance(step, (roles.NativeTransformer, roles.NativeSelector)):
                values = step.fit(values, step_y).transform(values)
                fitted.append(step)
            else:
                raise ValueError(f"{type(step).__name__} is not a portable pipeline step")
        fitted.append(steps[-1].fit(values, targets))
        return cls(recipe, fitted, n_features)

    @classmethod
    def from_json(cls, source: str | Path) -> N4mRolePipeline:
        """Read a v8 envelope (JSON text or path) and rebuild its estimators."""

        import numpy as np

        roles = _roles()
        if isinstance(source, Path) or not source.lstrip().startswith("{"):
            source = Path(source).read_text(encoding="utf-8")
        document = json.loads(source)
        if not isinstance(document, dict) or document.get("schema") != N4M_TRAINED_PIPELINE_SCHEMA:
            raise ValueError("unsupported trained n4m pipeline envelope")
        recipe, states = document["recipe"], document["states"]
        stateful = [step for step in map(_step, recipe["pipeline"]) if not isinstance(step, roles.NativeSampleFilter)]
        if len(states) != len(stateful):
            raise ValueError("envelope states do not match the recipe steps")
        estimators = []
        for step, state in zip(stateful, states, strict=True):
            payload = base64.b64decode(state["n4me_base64"], validate=True)
            if hashlib.sha256(payload).hexdigest() != state["sha256"]:
                raise ValueError(f"N4ME state of {state['method_id']} fails its checksum")
            estimator = roles.NativeEstimator.from_n4me(payload)
            if not state["method_id"] == estimator._method_id == step._method_id:
                raise ValueError(f"N4ME state {state['method_id']} does not match its recipe step")
            if "class_names" in state:
                estimator._label_names_ = np.asarray(state["class_names"])
            estimators.append(estimator)
        return cls(recipe, estimators, int(document["n_features"]))

    def to_json(self, file: str | Path | None = None) -> str:
        """The v8 envelope; also written to ``file`` when given."""

        states = []
        for estimator in self.estimators:
            payload = estimator.to_n4me(allow_training_rows=True)
            state = {
                "method_id": estimator._method_id,
                "n4me_base64": base64.b64encode(payload).decode("ascii"),
                "sha256": hashlib.sha256(payload).hexdigest(),
            }
            # Classifier label names (N4ME holds integer class ids only).
            names = getattr(estimator, "_label_names_", None)
            if names is not None:
                state["class_names"] = names.tolist()
            states.append(state)
        document = {
            "schema": N4M_TRAINED_PIPELINE_SCHEMA,
            "recipe": self.recipe,
            "n_features": self.n_features,
            "states": states,
        }
        text = json.dumps(document, indent=1)
        if file is not None:
            Path(file).write_text(text + "\n", encoding="utf-8")
        return text

    def predict(self, X: Any) -> Any:
        """Predictions (regressor) or class labels (classifier) of the final model."""

        import numpy as np

        values = np.asarray(X, dtype=np.float64)
        if values.ndim != 2 or values.shape[1] != self.n_features:
            raise ValueError(f"expected {self.n_features} input columns")
        for step in self.estimators[:-1]:
            values = step.transform(values)
        return self.estimators[-1].predict(values)

    def retrain(self, X: Any, y: Any) -> N4mRolePipeline:
        """Fit the same recipe afresh on new training rows."""

        return type(self).fit_recipe(self.recipe, X, y)
