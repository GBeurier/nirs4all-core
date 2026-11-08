#!/usr/bin/env python3
"""Validate committed n4a V1 container manifests against the JSON Schema (shape only).

The gate validates every committed manifest of the six parity fixtures against
``docs/contracts/n4a-container.v1.schema.json`` with the Draft 2020-12 validator,
then replays a tracked bank of in-memory mutations of the combined manifest and
requires each positive mutation to be accepted and each negative one refused.

Scope is JSON shape only: no ZIP is opened and no Rust, native, numerical,
hydration or compiler code runs. ID/link closure, identity-owner equality, NFC,
raw-byte integrity, physical ZIP rules, scientific fingerprints and the meaning of
link relations stay with the Rust owner validators. No remote reference is ever
retrieved. Any absent dependency, input or schema fails with a nonzero exit.

Known lexical limitation (reported as a SEPARATE category, never as a pass): JSON
Schema treats every mathematically integral number (``1.0``, ``1e0``) as an
``integer``, so the schema accepts them. The authoritative Rust owner reader accepts
only unsigned-integer tokens and refuses them. Six exact cases are kept as a small
JSON-serializable table (``KNOWN_LEXICAL_LIMITATIONS``, classification
``schema_accepts_rust_refuses``). Each is built in memory by replacing one unique
quoted placeholder with the raw numeric lexeme (so ``1e0`` is never normalized by
``json.dumps``) and parsed with the strict decoder; the gate requires the schema to
accept it. This is a documented expectation of the Rust refusal, NOT an executed
native-parity check: native qualification is separate and owned by the Rust tests.
"""

from __future__ import annotations

import argparse
import copy
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SCHEMA_PATH = ROOT / "docs/contracts/n4a-container.v1.schema.json"
FIXTURES_ROOT = ROOT / "bindings/rust/nirs4all/tests/parity/fixtures/n4a_v1"
FIXTURE_NAMES = ("combined", "dataset", "recipe", "model", "result", "score")
BASELINE_NAME = "combined"
MAX_U32 = 4294967295


class N4aContainerContractError(Exception):
    """Raised when the gate cannot run, an instance is refused or a mutation misbehaves."""


try:
    from jsonschema import Draft202012Validator
    from jsonschema.exceptions import SchemaError
    from referencing import Registry
    from referencing.exceptions import NoSuchResource
except ImportError as error:  # explicit failure, never a skip
    raise SystemExit(
        f"n4a container gate: missing dependency ({error}); "
        "install with: python -m pip install -r scripts/requirements-contracts.txt"
    ) from error


def default_manifest_paths(fixtures_root: Path = FIXTURES_ROOT) -> list[Path]:
    """The six authoritative manifest paths; absence of any one is a failure."""
    return [fixtures_root / name / "manifest.json" for name in FIXTURE_NAMES]


def deny_retrieval(uri: str) -> Any:
    """Registry retrieval hook: remote or unknown resources are never fetched."""
    raise NoSuchResource(ref=uri)


def _reject_constant(token: str) -> Any:
    raise ValueError(f"non-JSON constant {token}")


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate object key {key!r}")
        result[key] = value
    return result


def parse_json_text(text: str) -> Any:
    """Strict decoder shared by every input: no NaN/Infinity, no duplicate keys."""
    return json.loads(text, parse_constant=_reject_constant, object_pairs_hook=_reject_duplicate_keys)


def load_json(path: Path, label: str) -> Any:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError as error:
        raise N4aContainerContractError(f"{label} is absent: {path}") from error
    except (OSError, UnicodeDecodeError) as error:
        raise N4aContainerContractError(f"{label} is unreadable: {path}: {error}") from error
    try:
        return parse_json_text(text)
    except ValueError as error:  # JSONDecodeError is a ValueError
        raise N4aContainerContractError(f"{label} is malformed JSON: {path}: {error}") from error


def format_path(path: Any) -> str:
    return "$" + "".join(f"[{p}]" if isinstance(p, int) else f".{p}" for p in list(path))


def build_validator(schema: Any) -> Draft202012Validator:
    """Check the schema itself and return a validator with network retrieval denied."""
    try:
        Draft202012Validator.check_schema(schema)
    except SchemaError as error:
        raise N4aContainerContractError(
            f"schema is not well-formed at schema path {format_path(error.absolute_path)}: {error.message}"
        ) from error
    return Draft202012Validator(schema, registry=Registry(retrieve=deny_retrieval))


def validate_instance(validator: Draft202012Validator, instance: Any) -> list[str]:
    """Return one message per shape violation; an empty list means accepted."""
    try:
        errors = sorted(validator.iter_errors(instance), key=lambda e: [str(p) for p in e.absolute_path])
    except Exception as error:  # unresolved or denied references must fail loudly
        raise N4aContainerContractError(f"schema reference could not be resolved offline: {error}") from error
    return [
        f"instance {format_path(e.absolute_path)} violates schema {format_path(e.absolute_schema_path)}: {e.message}"
        for e in errors
    ]


# --------------------------------------------------------------------------
# Mutation bank. Every entry is plain JSON data: {"name", "expect", "ops"}.
# Ops: {"op": "set"|"delete", "path": [...], "value": ...} and
#      optional "missing_ok": true for deleting an optional object field,
#      {"op": "repeat_first", "path": [...], "count": n} (grow a list to n items),
#      {"op": "keep_only", "path": [...], "selector": "@reference"} (keep one artifact).
# Path items are keys, list indices, or "@embedded"/"@reference" which select
# the first artifact whose storage.kind matches (never a hardcoded index).
# --------------------------------------------------------------------------

TOP_LEVEL_FIELDS = (
    "schema",
    "schema_version",
    "minimum_reader_version",
    "container_id",
    "required_features",
    "physical_profile",
    "members",
    "artifacts",
    "links",
    "entrypoints",
)
EMB = ["artifacts", "@embedded"]
REF = ["artifacts", "@reference"]
OBJECT_PATHS = {
    "manifest": [],
    "artifact": EMB,
    "entrypoint": ["entrypoints", 0],
    "member": ["members", 0],
    "link": ["links", 0],
    "identity": [*EMB, "identity"],
    "content_type": [*EMB, "content_type"],
    "embedded storage": [*EMB, "storage"],
    "reference storage": [*REF, "storage"],
}
LONG_ID = "x" * 4096


def _set(path: list[Any], value: Any) -> dict[str, Any]:
    return {"op": "set", "path": path, "value": value}


def _delete(path: list[Any], *, missing_ok: bool = False) -> dict[str, Any]:
    return {"op": "delete", "path": path, "missing_ok": missing_ok}


def _case(name: str, expect: str, *ops: dict[str, Any]) -> dict[str, Any]:
    return {"name": name, "expect": expect, "ops": list(ops)}


def build_mutation_bank() -> list[dict[str, Any]]:
    bank: list[dict[str, Any]] = []
    accept, refuse = "accept", "refuse"

    # Positive: opaque unknown owner/content/schema/version/fingerprint-profile strings.
    bank.append(
        _case(
            "positive unknown optional owner content",
            accept,
            _set([*EMB, "owner"], "acme.unknown-owner"),
            _set([*EMB, "content_type"], {"id": "acme.unknown/content+type", "version": 7}),
            _set(
                [*EMB, "identity"],
                {
                    "owner": "acme.unknown-owner",
                    "schema": "acme.unknown.schema",
                    "schema_version": 42,
                    "fingerprint_profile": "acme.fingerprint.profile.v9",
                    "fingerprint": "opaque-owner-token",
                },
            ),
        )
    )
    bank.append(_case("positive future content version within u32", accept, _set([*EMB, "content_type", "version"], MAX_U32)))
    bank.append(_case("positive future identity schema_version within u32", accept, _set([*EMB, "identity", "schema_version"], MAX_U32)))
    bank.append(_case("positive unfamiliar link relation", accept, _set(["links", 0, "relation"], "acme.never_seen_relation")))
    bank.append(_case("positive requirements object on artifact", accept, _set([*EMB, "requirements"], {"artifact_id": "recipe"})))
    bank.append(_case("positive requirements object on entrypoint", accept, _set(["entrypoints", 0, "requirements"], {"artifact_id": "recipe"})))
    bank.append(_case("positive requirements null on artifact", accept, _set([*EMB, "requirements"], None)))
    bank.append(_case("positive requirements null on entrypoint", accept, _set(["entrypoints", 0, "requirements"], None)))
    bank.append(_case("positive reference root_sha256 omitted", accept, _delete([*REF, "storage", "root_sha256"], missing_ok=True)))
    bank.append(_case("positive reference root_sha256 present", accept, _set([*REF, "storage", "root_sha256"], "ab" * 32)))
    bank.append(_case("positive empty links", accept, _set(["links"], [])))
    bank.append(_case("positive empty entrypoints", accept, _set(["entrypoints"], [])))
    bank.append(_case("positive empty entrypoint input_artifacts", accept, _set(["entrypoints", 0, "input_artifacts"], [])))
    bank.append(_case("positive member byte_length above the former 128 MiB cap", accept, _set(["members", 0, "byte_length"], 134217729)))
    bank.append(_case("positive member byte_length u64 bound", accept, _set(["members", 0, "byte_length"], 2**64 - 1)))
    bank.append(_case("positive 513-character path above the former path cap", accept, _set(["members", 0, "path"], "p" * 513)))
    bank.append(_case("positive 4096 members above the former entry cap", accept, {"op": "repeat_first", "path": ["members"], "count": 4096}))

    # Negative: top-level fields.
    for field in TOP_LEVEL_FIELDS:
        bank.append(_case(f"missing top-level {field}", refuse, _delete([field])))
    bank.append(_case("top-level schema future value", refuse, _set(["schema"], "nirs4all.n4a.container.v2")))
    bank.append(_case("top-level schema not a string", refuse, _set(["schema"], 1)))
    bank.append(_case("physical_profile other value", refuse, _set(["physical_profile"], "zip-deflate.v1")))
    for field in ("schema_version", "minimum_reader_version"):
        for label, value in (("future 2", 2), ("bool", True), ("string", "1"), ("zero", 0), ("null", None), ("float", 1.5), ("negative", -1)):
            bank.append(_case(f"{field} {label}", refuse, _set([field], value)))
    bank.append(_case("container_id empty", refuse, _set(["container_id"], "")))
    bank.append(_case("container_id whitespace only", refuse, _set(["container_id"], "   ")))
    bank.append(_case("container_id control character", refuse, _set(["container_id"], "bad\u0001id")))
    bank.append(_case("container_id not a string", refuse, _set(["container_id"], 5)))
    bank.append(_case("top-level required_features duplicates", refuse, _set(["required_features"], ["f", "f"])))
    bank.append(_case("top-level required_features empty id", refuse, _set(["required_features"], [""])))
    bank.append(_case("top-level required_features not an array", refuse, _set(["required_features"], "f")))

    # Negative: nullable requirements.
    for label, path in (("artifact", EMB), ("entrypoint", ["entrypoints", 0])):
        bank.append(_case(f"requirements absent on {label}", refuse, _delete([*path, "requirements"])))
        for bad_label, bad in (
            ("empty object", {}),
            ("empty artifact_id", {"artifact_id": ""}),
            ("artifact_id not a string", {"artifact_id": 3}),
            ("extra field", {"artifact_id": "x", "extra": 1}),
            ("string", "recipe"),
            ("array", []),
            ("null artifact_id", {"artifact_id": None}),
        ):
            bank.append(_case(f"requirements invalid on {label}: {bad_label}", refuse, _set([*path, "requirements"], bad)))

    # Negative: additional properties on every object kind.
    for label, path in OBJECT_PATHS.items():
        bank.append(_case(f"unknown field on {label}", refuse, _set([*path, "unknown"], True)))

    # Negative: reference root_sha256.
    for label, bad in (("null", None), ("integer", 7), ("uppercase", "AB" * 32), ("short", "ab" * 31), ("long", "ab" * 33), ("non-hex", "zz" * 32), ("empty", "")):
        bank.append(_case(f"reference root_sha256 {label}", refuse, _set([*REF, "storage", "root_sha256"], bad)))
    bank.append(_case("reference storage with embedded fields", refuse, _set([*REF, "storage", "root_member"], "a/b")))
    bank.append(_case("storage kind unsupported", refuse, _set([*EMB, "storage", "kind"], "remote")))
    bank.append(_case("embedded storage missing root_member", refuse, _delete([*EMB, "storage", "root_member"])))
    bank.append(_case("embedded storage missing member_paths", refuse, _delete([*EMB, "storage", "member_paths"])))

    # Negative: artifact kind and required fields.
    bank.append(_case("artifact kind calibration", refuse, _set([*EMB, "kind"], "calibration")))
    bank.append(_case("artifact kind not a string", refuse, _set([*EMB, "kind"], 3)))
    for field in ("id", "kind", "owner", "content_type", "identity", "storage"):
        bank.append(_case(f"artifact missing {field}", refuse, _delete([*EMB, field])))
    bank.append(_case("no embedded artifact remains", refuse, {"op": "keep_only", "path": ["artifacts"], "selector": "@reference"}))

    # Negative: wrong shapes and types.
    bank.append(_case("artifacts is an object", refuse, _set(["artifacts"], {})))
    bank.append(_case("artifacts empty", refuse, _set(["artifacts"], [])))
    bank.append(_case("artifacts item is a number", refuse, _set(["artifacts"], [1])))
    bank.append(_case("links is a string", refuse, _set(["links"], "none")))
    bank.append(_case("entrypoints is an object", refuse, _set(["entrypoints"], {})))
    bank.append(_case("members empty", refuse, _set(["members"], [])))
    bank.append(_case("members is an object", refuse, _set(["members"], {})))
    bank.append(_case("artifact storage is an array", refuse, _set([*EMB, "storage"], [])))
    bank.append(_case("artifact identity is a string", refuse, _set([*EMB, "identity"], "x")))
    bank.append(_case("artifact id is a number", refuse, _set([*EMB, "id"], 5)))
    bank.append(_case("artifact owner whitespace", refuse, _set([*EMB, "owner"], "   ")))
    bank.append(_case("artifact owner control after long prefix", refuse, _set([*EMB, "owner"], f"{LONG_ID}\u007f")))
    bank.append(_case("link relation empty", refuse, _set(["links", 0, "relation"], "")))
    bank.append(_case("link missing source_artifact", refuse, _delete(["links", 0, "source_artifact"])))
    bank.append(_case("link target not a string", refuse, _set(["links", 0, "target_artifact"], None)))
    bank.append(_case("entrypoint missing root_artifact", refuse, _delete(["entrypoints", 0, "root_artifact"])))
    bank.append(_case("entrypoint missing input_artifacts", refuse, _delete(["entrypoints", 0, "input_artifacts"])))
    bank.append(_case("entrypoint missing required_features", refuse, _delete(["entrypoints", 0, "required_features"])))
    bank.append(_case("entrypoint operation empty", refuse, _set(["entrypoints", 0, "operation"], "")))

    # Negative: versions (content_type.version and identity.schema_version).
    for holder, key in (("content_type", "version"), ("identity", "schema_version")):
        for label, value in (("bool", True), ("false", False), ("string", "1"), ("float", 1.5), ("zero", 0), ("negative", -1), ("above u32", MAX_U32 + 1), ("null", None)):
            bank.append(_case(f"{holder}.{key} {label}", refuse, _set([*EMB, holder, key], value)))

    # Negative: members and paths.
    bank.append(_case("member byte_length negative", refuse, _set(["members", 0, "byte_length"], -1)))
    bank.append(_case("member byte_length above u64", refuse, _set(["members", 0, "byte_length"], 2**64)))
    bank.append(_case("member byte_length string", refuse, _set(["members", 0, "byte_length"], "5")))
    bank.append(_case("member sha256 uppercase", refuse, _set(["members", 0, "sha256"], "AB" * 32)))
    bank.append(_case("member sha256 short", refuse, _set(["members", 0, "sha256"], "ab" * 31)))
    for label, bad in (
        ("empty", ""),
        ("absolute", "/abs/file.bin"),
        ("backslash", "dir\\file.bin"),
        ("dot segment", "a/./b"),
        ("dotdot segment", "a/../b"),
        ("leading dotdot", "../b"),
        ("empty segment", "a//b"),
        ("trailing slash", "a/"),
        ("drive prefix", "C:/b"),
        ("drive segment", "a/C:b"),
        ("control character", "a\u0001b"),
        ("reserved manifest.json", "manifest.json"),
        ("not a string", 5),
    ):
        bank.append(_case(f"member path {label}", refuse, _set(["members", 0, "path"], bad)))
        bank.append(_case(f"embedded root_member {label}", refuse, _set([*EMB, "storage", "root_member"], bad)))
    bank.append(_case("member_paths not an array", refuse, _set([*EMB, "storage", "member_paths"], "a")))
    bank.append(_case("member_paths empty", refuse, _set([*EMB, "storage", "member_paths"], [])))

    # Negative: declared uniqueItems.
    bank.append(_case("duplicate storage member_paths", refuse, _set([*EMB, "storage", "member_paths"], ["a/x", "a/x"])))
    bank.append(_case("duplicate entrypoint input_artifacts", refuse, _set(["entrypoints", 0, "input_artifacts"], ["dataset", "dataset"])))
    bank.append(_case("duplicate entrypoint required_features", refuse, _set(["entrypoints", 0, "required_features"], ["f", "f"])))
    return bank


def _resolve_selector(container: Any, item: Any) -> Any:
    if item in ("@embedded", "@reference"):
        kind = item[1:]
        for index, artifact in enumerate(container):
            storage = artifact.get("storage") if isinstance(artifact, dict) else None
            if isinstance(storage, dict) and storage.get("kind") == kind:
                return index
        raise N4aContainerContractError(f"baseline has no artifact with storage.kind == {kind!r}")
    return item


def _walk(document: Any, path: list[Any]) -> Any:
    node = document
    for item in path:
        node = node[_resolve_selector(node, item)]
    return node


def apply_op(document: Any, op: dict[str, Any]) -> Any:
    """Apply one bank operation in place and return the (possibly replaced) document."""
    path = list(op["path"])
    kind = op["op"]
    if kind == "keep_only":
        target = _walk(document, path)
        target[:] = [target[_resolve_selector(target, op["selector"])]]
        return document
    if kind == "repeat_first":
        target = _walk(document, path)
        target[:] = [copy.deepcopy(target[0]) for _ in range(op["count"])]
        return document
    if not path:
        raise N4aContainerContractError(f"cannot {kind} the document root: {op}")
    parent = _walk(document, path[:-1])
    leaf = _resolve_selector(parent, path[-1])
    if kind == "set":
        parent[leaf] = copy.deepcopy(op["value"])
    elif kind == "delete":
        if op.get("missing_ok") and isinstance(parent, dict):
            parent.pop(leaf, None)
        else:
            del parent[leaf]
    else:
        raise N4aContainerContractError(f"unknown mutation op {kind!r}")
    return document


def apply_case(baseline: Any, case: dict[str, Any]) -> Any:
    document = copy.deepcopy(baseline)  # owner bytes on disk are never touched
    for op in case["ops"]:
        document = apply_op(document, op)
    return document


def run_mutation_bank(validator: Draft202012Validator, baseline: Any) -> dict[str, int]:
    bank = build_mutation_bank()
    names = [case["name"] for case in bank]
    if len(set(names)) != len(names):
        raise N4aContainerContractError("mutation bank has duplicate case names")
    positive = negative = 0
    for case in bank:
        problems = validate_instance(validator, apply_case(baseline, case))
        if case["expect"] == "accept":
            if problems:
                raise N4aContainerContractError(
                    f"positive mutation {case['name']!r} was refused:\n  " + "\n  ".join(problems)
                )
            positive += 1
        else:
            if not problems:
                raise N4aContainerContractError(f"negative mutation {case['name']!r} was accepted")
            negative += 1
    return {"positive": positive, "negative": negative}


# --------------------------------------------------------------------------
# Known lexical limitations: the schema accepts, the Rust unsigned-integer-token
# reader refuses. Reported separately; never counted as positive or native parity.
# --------------------------------------------------------------------------

LIMITATION_CLASSIFICATION = "schema_accepts_rust_refuses"
LIMITATION_PLACEHOLDER = "__N4A_RAW_NUMERIC_TOKEN_PLACEHOLDER__"
LIMITATION_TEXT = (
    "JSON Schema accepts mathematically integral floats; the authoritative Rust unsigned-integer-token "
    "reader refuses them (documented expectation, no native parity execution claimed)"
)
KNOWN_LEXICAL_LIMITATIONS: list[dict[str, Any]] = [
    {"name": "schema_version raw 1.0", "path": ["schema_version"], "raw_token": "1.0", "classification": LIMITATION_CLASSIFICATION},
    {"name": "schema_version raw 1e0", "path": ["schema_version"], "raw_token": "1e0", "classification": LIMITATION_CLASSIFICATION},
    {"name": "minimum_reader_version raw 1.0", "path": ["minimum_reader_version"], "raw_token": "1.0", "classification": LIMITATION_CLASSIFICATION},
    {"name": "members[0].byte_length raw 26.0", "path": ["members", 0, "byte_length"], "raw_token": "26.0", "classification": LIMITATION_CLASSIFICATION},
    {
        "name": "embedded artifact content_type.version raw 9.0",
        "path": [*EMB, "content_type", "version"],
        "raw_token": "9.0",
        "classification": LIMITATION_CLASSIFICATION,
    },
    {
        "name": "embedded artifact identity.schema_version raw 17.0",
        "path": [*EMB, "identity", "schema_version"],
        "raw_token": "17.0",
        "classification": LIMITATION_CLASSIFICATION,
    },
]


def build_limitation_document(baseline: Any, limitation: dict[str, Any]) -> tuple[str, Any]:
    """Return (raw JSON text, parsed document) with the raw numeric lexeme kept verbatim.

    One unique quoted placeholder is set at the chosen field of an in-memory copy, then
    replaced textually by the raw token, and the result goes through the strict decoder.
    """
    document = apply_op(copy.deepcopy(baseline), {"op": "set", "path": limitation["path"], "value": LIMITATION_PLACEHOLDER})
    quoted = json.dumps(LIMITATION_PLACEHOLDER)
    text = json.dumps(document)
    if text.count(quoted) != 1:
        raise N4aContainerContractError(f"limitation {limitation['name']!r}: placeholder is not unique in the diagnostic document")
    raw_text = text.replace(quoted, limitation["raw_token"])
    try:
        return raw_text, parse_json_text(raw_text)
    except ValueError as error:
        raise N4aContainerContractError(f"limitation {limitation['name']!r}: diagnostic document is malformed JSON: {error}") from error


def run_known_lexical_limitations(validator: Draft202012Validator, baseline: Any) -> dict[str, Any]:
    """Require the schema to accept each known case; count them apart from the mutation bank."""
    names = [entry["name"] for entry in KNOWN_LEXICAL_LIMITATIONS]
    if len(set(names)) != len(names):
        raise N4aContainerContractError("known lexical limitations have duplicate names")
    for limitation in KNOWN_LEXICAL_LIMITATIONS:
        _, document = build_limitation_document(baseline, limitation)
        value = _walk(document, list(limitation["path"]))
        if not isinstance(value, float) or isinstance(value, bool) or not value.is_integer():
            raise N4aContainerContractError(f"limitation {limitation['name']!r}: parsed value {value!r} is not an integral float")
        problems = validate_instance(validator, document)
        if problems:
            raise N4aContainerContractError(
                f"limitation {limitation['name']!r} is no longer accepted by the schema; "
                "update the known-limitation table:\n  " + "\n  ".join(problems)
            )
    return {"count": len(KNOWN_LEXICAL_LIMITATIONS), "classification": LIMITATION_CLASSIFICATION, "limitation": LIMITATION_TEXT}


def validate_manifests(validator: Draft202012Validator, paths: list[Path]) -> dict[Path, Any]:
    if not paths:
        raise N4aContainerContractError("no manifest inputs: zero-input success is refused")
    loaded: dict[Path, Any] = {}
    for path in paths:
        instance = load_json(path, "manifest")
        problems = validate_instance(validator, instance)
        if problems:
            shown = "\n  ".join(problems)
            raise N4aContainerContractError(f"manifest {path} refused by schema ({len(problems)} violation(s)):\n  {shown}")
        loaded[path] = instance
    return loaded


def run_gate(fixtures_root: Path = FIXTURES_ROOT, schema_path: Path = SCHEMA_PATH) -> dict[str, Any]:
    validator = build_validator(load_json(schema_path, "schema"))
    paths = default_manifest_paths(fixtures_root)
    loaded = validate_manifests(validator, paths)
    baseline = loaded[fixtures_root / BASELINE_NAME / "manifest.json"]
    counts = run_mutation_bank(validator, baseline)
    known = run_known_lexical_limitations(validator, baseline)
    return {
        "manifests": len(paths),
        **counts,
        "known_lexical_limitations": known["count"],
        "known_lexical_limitation_classification": known["classification"],
        "known_lexical_limitation_text": known["limitation"],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures-root", type=Path, default=FIXTURES_ROOT, help="directory holding the six <name>/manifest.json fixtures")
    parser.add_argument("--schema", type=Path, default=SCHEMA_PATH, help="n4a container JSON Schema")
    args = parser.parse_args(argv)
    try:
        counts = run_gate(args.fixtures_root, args.schema)
    except N4aContainerContractError as error:
        print(f"n4a container contract FAILED: {error}", file=sys.stderr)
        return 1
    print(
        "n4a container contract OK (JSON shape only): "
        f"{counts['manifests']} manifests validated, "
        f"{counts['positive']} positive and {counts['negative']} negative mutations as expected\n"
        f"known lexical limitations (separate category, not a pass): {counts['known_lexical_limitations']} "
        f"[{counts['known_lexical_limitation_classification']}]: {counts['known_lexical_limitation_text']}; "
        "native qualification is separate"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
