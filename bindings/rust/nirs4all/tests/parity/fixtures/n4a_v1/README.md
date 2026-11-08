# Authoritative packaged n4a V1 owner fixtures

Status: current frozen-source qualification executed and passed; final joint
Core/DAG adversarial review accepted. The immutable qualification is
workspace `_implementation/2026-11-03-ecosystem-consolidation/phase2a-c-qualification/runs/final-cohort-03/receipt.json`.
The prior complete corpus and receipt are preserved as superseded evidence in
that run. `published-fixture.json` records the exact publication;
`corpus-comparison.json` compares the original and current raw payload
occurrences explicitly. Run03 `comparison-to-prior-cohorts.json` verifies
that all six complete containers and all owner raw members are byte-identical
to Run01 and Run02; only provenance identifies the final guarded source and execution.
Publication copies owner bytes, without handwritten scientific payloads or
transport reserialization of those payloads.

Six default tests in `tests/n4a_owner_roundtrip.rs` read the exact recipe,
dataset, model, result-only, score-only and combined manifests and their declared
physical members. They assert provenance/raw hashes, exactly-one member
ownership, retained external owner identities and links, exact manifest bytes,
exact raw payloads and exact producer container length/SHA from
`PROVENANCE.containers`. Model root and child paths come from the actual owner
inventory and `storage.member_paths`. The combined shape has nine payloads;
the model facet owns the unchanged DAG package JSON and its declared
content-addressed raw state once. Prediction-cache and plan identities use
the current DAG facet profiles. Tests do not parse scientific JSON/native state,
regenerate payloads, fit, or depend on unreleased DAG APIs.

The actual Rust 1.88 qualification normalizes a float32 toy package through IO,
uses Core's public provider/request adapter, executes DAG training and direct
new owner facet export/validation, then writes/opens/copies/extracts canonical
containers. The harness calls general `validate_facet_members` on every known embedded DAG
facet (seven) in both producer and fresh reader, and `export.validate()` invokes
the current DAG D1/D2/D3 relationship guards. In the fresh OS process, live DAG
state-member validation and actual Methods inspection validate the owner raw
closure and model proof. The process
matches the actual IO-normalized raw-byte identity, obtains exact bound live
Core admission and calls DAG loaded-predictor replay with targets absent.
Producer PID 560933 and fresh replay PID 560934 are provenance metadata; the
runtime evidence is the immutable run's actual commands/results, not PID values
alone. Diagnostic JSON is never trusted or deserialized as live admission.

The actual Cargo resolution is local patched Core 0.4.7 plus `dag-ml`,
`dag-ml-core` and `dag-ml-results` 0.3.44 only. IO 0.2.14, DMD 0.2.16, Formats
0.2.14 and the n4m Rust binding 0.4.1 are locked registry sources, not dirty
sibling checkouts. The actual C++ Methods library is explicitly selected ABI
2.20, SHA-256 `05ae1a2e82491439f0e71ada0ea500ed79d03af227a11c4a2dbb317a99558529`.
Run03 `resolved-owner-source-inventory.json` records the 13 actual owner packages
and 271 source/input hashes with Cargo metadata. Native library, executable and
qualification inputs are archived with the immutable run.


Replay intentionally uses stored training features as Inference:
`in_sample_prediction`, state roundtrip only, with no independent test or
generalization claim. The actual original two-fold OOF ScoreSet/outcome bytes
remain owner bytes, with no Core recomputation. Dataset `sha256.raw.v1` means
byte identity only; `dataset_of` and `evaluates` remain explicitly unverified.
Recipe identity is an actual compiled GraphSpec; no original DSL equivalence
is claimed.

All six shapes passed Python standard ZIP/schema/datetime checks. Canonical
Python/WASM/R/MATLAB host-facade execution remains unsupported until 2a-B;
no SDK/Python full pipeline or cross-language pipeline parity is claimed.
Tracked wasm32 canonical Rust compilation and retained scratch WASI transport
execution are distinct proofs. External source patches/locks are qualification
configuration, not product dependency configuration. No binding/consumer cutover
is part of this corpus integration.

Synthetic hostile binary/JSON and 450-member capacity tests in
`tests/n4a_container.rs` carry no scientific model or fit-count claim. This
is the sole packaged owner corpus for canonical V1.
