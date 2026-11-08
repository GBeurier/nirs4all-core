# Native n4a actions: Stage D source

This describes the native n4a actions, including the portable split,
`retrain_n4a_native` and the portable plans under
[Portable plans and native retrain](#portable-plans-and-native-retrain).
Source availability of an entrypoint is not parity. Qualification names the
profile, operation, artifact, build and platform that were actually executed,
and it is never inferred for another one. The text below states contracts and
points to the test targets that exercise them. The controller owns dependency
composition, runtime admission and WORK review.

`produce_n4a_native` accepts immutable IO record bytes, one source, a native
linear recipe, explicit runtime path/digest and container limits. It trains via
the existing DAG/Methods pipeline and seals the owner's complete facet export.
It may embed the exact original IO bytes as a training/schema anchor and stored
prediction input. No DAG dataset link is fabricated. Recipe parameters,
selection, scores, results, identities and fitted-state members come from the
owner builder. Existing NativePipeline JSON transports retain their request
path and wire format.

`predict_n4a_native` and `calibrate_n4a_native` borrow an `OwnedN4a` and take an
exact entrypoint plus `NativeInput::Stored` or immutable external bytes, source,
partition, labels, record limits and `NativeRuntime`. They accept no dataset
path, caller JSON Value, provider, admission assertion, arbitrary relations,
matrices, registry or capability list. Their new public types are exported by
`nirs4all::n4a_owner_native` and the crate root.

`NativeSchemaPolicy::RequireTrainingSchema` requests the full schema check.
Prediction alone may explicitly request `AllowUnavailable`; its receipt then
states `schema_validation: unavailable` if neither anchor exists. A present
anchor must always match. Calibration always requires the canonical producer's
metadata schema anchor. An embedded original training anchor must also match
the saved owner data identity, independently of its descriptor and raw hash.

Admission validates the exact Core operation binding, global and selected
features, the complete owner member/proof map, local same-identity references,
roles, one requirement/output/source and the existing complete native recipe
contract. An unrelated unproven embedded Model refuses. FullRefit/ChildRefit,
multimodal, a manifest `retrain` operation and unsupported native controller
families refuse before runtime. An invalid descriptor or present schema anchor never triggers fallback.

Records and recipes have no Core size or nesting cap: `memory_budget` on each
request is the only Core bound, and `None` leaves only allocator and host limits.
IO's JSON parser keeps its own stack bound on deep documents. The portable
manifest's existing iterative scanner remains live and rejects trailing commas, invalid primitive tokens and invalid strings,
as well as exact and escaped-alias duplicate keys. IO alone decodes dataset
JSON and constructs its immutable package. IO's compiled owner, record versions
and identity profiles are authoritative. Methods receives the IO matrix's
`n4a-matrix-f64-le.v1` identity and `y: None` for both operations. The calibration
truth accessor reads sample IDs and selected target names from that same
retained package after replay, releasing its temporary IO handles.

For calibration, the producer inserts `raw_source_schema` and versioned
`native_pipeline_training_relations` metadata before computing the owner request
fingerprint, refusing existing reserved keys. After role resolution and owner
calibration preflight, private recovery receives the validated outcome and
package. It checks audit version before decoding its closed structure, owner
cross-binding, one identity/requirement/source binding, all relation fingerprints,
exact FoldSet coverage and both campaign and split leakage policies. The owner
influence-closure method validates the entries; Core contains no closure copy.

The V2 ExternalTest envelope carries the recovered CV universe as coordinator
relations and the independent calibration cohort in `predict_cohort`. Its data
and target identities describe the calibration cohort. The action explicitly
calls the owner authority API before runtime, followed by campaign relation
policy, independent-uncertainty policy and sample/origin/observation disjointness.
Calibration consumes the whole target-fingerprinted record and retains the
existing record gate for groups, origin aliases, experimental units and
repetitions. Inference uses its own cohort as coordinator relations, requires
no calibration relation audit and makes no disjointness claim.

Receipts distinguish current IO/cohort evidence from frozen training evidence:
`relation_authority: fingerprint_bound_self_consistent`,
`authenticity: not_established`,
`observation_id_disjointness: checked_against_recorded_universe`, and
`observation_content_disjointness: unavailable` for calibration. Different IDs
do not establish different spectra. A fully self-consistent renaming/forgery is
not detectable by unkeyed fingerprints. No receipt claims producer authenticity.

All static owner admission and independence checks precede the attested Methods
runtime. `NativeActionError::Owner` retains `FacetRefusal`, including
`UnsupportedOperation`, `MissingCapability`, `MissingTrust` and
`RuntimeUnavailable`; Core operation, IO/schema/identity and container errors
remain distinct. Runtime retries retain the actual owner ABI error source.

Calibration delegates Marginal/JointMax and Error/Unbounded policies to DAG;
Error is the default. It builds a fresh complete owner facet set and seals a new
container. Fitted-state paths and bytes remain exact. Only named, validated IO
anchors/stored inputs carry across. Extra owner artifacts, attachments, foreign
links, collisions and unprovided member/link preservation refuse before replay
where structurally knowable, and before output in all cases. The original
container remains immutable. Evaluation population semantics remain unverified
where the owner reports them so; carrying a score is no independent evaluation
claim. Generic exact-copy storage remains separate.

| Family | Producer | Prediction | External calibration |
| --- | --- | --- | --- |
| Raw PLS; transformed regressors; selector chains | Implemented source | Implemented source | RegressionPoint source |
| Multiple targets; selected index 0 or higher | Implemented source | Implemented source | Name-aligned truth source |
| Native classifier | Implemented source | Implemented source | Static UnsupportedOperation |
| FullRefit / ChildRefit roots | Deferred product producer | Explicit unsupported/unqualified | Refused by role |
| Multimodal | Existing separate transport | New action refuses | Refused |

These are dispatch descriptions, not executed qualification claims. Actual
N4MM/N4ME/role state kinds must be recorded from the qualified cohort; no kind
alone grants execution. No new artifact version or dependency pin is declared.

## Source bridge: training view, single-fit export and Python

This slice describes the training view, single-fit export and Python bridge.
Its test target is `n4a_owner_native_predict`; qualification is per profile,
operation and platform, not universal.

**Owner view.** `read_n4a_training_view(&OwnedN4a)` returns
`N4aTrainingView { outcome: TrainingOutcome, source_id: String }`. It reuses the
owner snapshot, local same-identity reference resolution, the `EvidenceFor` link
into the model, the proof's `training_outcome` and the single package source
binding. It shares the closed-profile checks of the actions: exactly one model,
one requirement/output/source, the complete native recipe contract,
portable-only artifact loading and native controllers. It never trains,
replays, loads MethodsRuntime or reads the ZIP. Foreign or extra models,
FullRefit/ChildRefit roots, missing or multiple evidence and unproven links
refuse explicitly.

**Single fit.** `produce_n4a_native_with_view` trains once and returns the
sealed `OwnedN4a` plus the view built from the `TrainingOutcome` and `source_id`
of that same owner result. `produce_n4a_native` keeps its signature and
behavior as a wrapper. Reserved recipe, schema and relation metadata still
precede the owner fingerprint. Exporting the sealed bytes never trains,
replays, reopens or revalidates them. `parse_native_recipe_json` is the strict
recipe parser: the structural scanner (duplicate keys, escaped aliases,
nesting), IO canonical JSON, then the closed `deny_unknown_fields` recipe type.
An unknown key or duplicate refuses and no parameter is silently dropped.

**Python (`nirs4all_core`).** The Rust extension exposes
`produce_n4a_native_with_view`, `inspect_n4a_bytes` and `predict_n4a_native_json`.
Each releases the GIL for the native work. `_n4a.py` only forwards arguments
and decodes returned JSON text; it has no ZIP or recipe parser, no fingerprint
math and no fallback path. The public functions are:

- `produce_n4a_native(record, source_id, recipe_json, *, methods_library_path,
  methods_library_sha256, run_id, container_id, embed_training_record=True)`
  returns `{"archive_bytes", "view"}`.
- `inspect_n4a_bytes(archive_bytes)` returns `view`, for the closed
  native-produced model slice only. Other shapes are refused.
- `predict_n4a_native(archive_bytes, *, record, entrypoint, source_id,
  partition, request_id, run_id, methods_library_path, methods_library_sha256,
  require_training_schema=True)` returns `{"replay", "receipt", "sample_ids"}`. `record=None`
  selects `NativeInput::Stored` and bytes select `External`. The boolean maps to
  `RequireTrainingSchema` or `AllowUnavailable` and never ignores a present bad
  anchor.

`sample_ids` are the selected partition's IDs in IO physical record order; the
signed `replay` keeps its own sorted order unchanged, so callers align by ID.

`view` has `manifest` (the full typed `N4aManifest`, including every artifact
identity and the model/run/result/score links), `receipt`
(`container_sha256`, `manifest_sha256`, `byte_length`), `training`
(`outcome`, `source_id`). Python recomputes no hash or identity.

**Memory.** Native actions keep record and model state in memory, because DAG's
facet export, IO decoding and the Methods ABI take in-memory bytes. A file becomes
an `OwnedN4a` through `N4aFile::into_owned`, which verifies the bytes it returns
and reuses the opening metadata. Peak memory is about the archive plus the owner
snapshot's copy of its members, and Core charges both to the declared budget.
Allocations made inside IO, DAG and Methods are outside that budget and can still
fail as a true out-of-memory condition. The bridge adds no host limit. Archives
given to inspection and prediction are validated once at the owner boundary; the
facade adds no validation loop and no admission bypass.

**Capability record.** `compat/capabilities.toml` declares the profile
`native-n4a-canonical@1` with these actions and their real entrypoints. It
separates source support from qualification and does not alias `inspect` to
import or `calibrate` to fit; WASM has no calibration.

**Qualification scope.** Executed evidence is bounded to the named profile,
operation, artifact and platform; it does not extend to a whole-system path or
to universal support. Calibration, a CLI route and any new engine stay out of
this slice. The Rust test
`training_view_of_a_sealed_producer_export_matches_its_model_evidence` in
`n4a_owner_native_predict` covers the training view of a sealed producer export.

## Portable plans and native retrain

This section states the portable and retrain contract. Its test targets are
named at the end of the section.

**Split.** `n4a_owner_portable.rs` holds everything that needs no Methods
runtime: record decoding, owner snapshot, schema and identity checks, the
training view, `prepare_action`, `compose` and the shared seal. The file loads
no library and reads no path, so both the native crate and `wasm-native` compile
it. `n4a_owner_native.rs` keeps runtime attestation, cohort freezing, replay,
calibration and the producer. `io_package_provider.rs` holds the unchanged
`DatasetPackageMethodsProvider`; the portable module mounts it with a
file-relative path, so a host mounts one module only.

**Request.** DAG assembles the linear training request:
`dag_ml_core::native_pipeline_training_request(&facts, &recipe, &manifests)`.
Core supplies IO facts and the host's controller manifests, and re-exports the
recipe types from DAG. The historical V2 PLS request stays in `io_training.rs`
until its callers migrate. `request_transition.rs` compares the moved request
with the pre-move source on three fold paths and both target selections. The
pre-move copy is test-only and is deleted with the transition.

With an invalid recipe or a non-train relation in the supplied training facts
(not the full record) and a broken runtime, the
`native_pipeline_training_request` refusal may now name the runtime first,
because manifests are read before DAG validates. Invalid training facts still
refuse. A full record that mixes train and test rows is scoped to its train
rows before the facts are built (see "Mixed train/test records" below).

**Seal.** Native and portable fits seal through the same two steps.
`check_training_retention` requires `outcome.training_request_fingerprint` to
equal the planned request and refuses lost reserved metadata.
`seal_training_facets` then composes the owner facets and optionally embeds the
exact IO record.

**Retrain.** `retrain_n4a_native(&OwnedN4a, NativeRetrainRequest)` reads the
recipe and source from the validated owner package, then calls
`produce_n4a_native_with_view` once. The caller supplies new record bytes, the
runtime, labels and limits, and neither a source nor a recipe. The source
container stays immutable and the result is a new model. No manifest entrypoint
or feature is added; a manifest `retrain` operation still refuses. Native policy
applies unchanged, so `native_target_index`, raw PLS/Ridge and classification
recipes retrain natively. Python exposes `retrain_n4a_native(archive_bytes, *,
record, methods_library_path, methods_library_sha256, run_id, container_id,
embed_training_record=True)`, which returns `{"archive_bytes", "view"}`.

**Portable plans.** `prepare_predict_portable`, `prepare_train_portable`,
`prepare_retrain_portable`, `PortableTrainPlan::seal` and
`inspect_n4a_view_json` give a host without a native runtime the same static
checks. A plan is data: the executing DAG runtime re-admits it. A predict plan
has no target field. `x`, `y` and `sample_ids` follow IO physical record order,
and hosts join rows by sample ID.

Only these entrypoints add the closed portable profile: a final regressor, no
`native_target_index`, complete targets and no `retains_training_rows` state.
The profile keys on roles and fitted-state capabilities, not on method names,
and it does not narrow any native action.

The test targets are `n4a_owner_portable` and the library test
`pipeline_workflow::request_transition`. Execution by DAG WASM belongs to
`n4a_wasm_exchange`.

## Mixed train/test records

This section is the contract of the admitted numeric IO-backed canonical
production path. The mixed-record regression, numeric oracle and stored plus
external cold-replay cases exist as Core tests, and the local Linux native and
WASM mixed cases have been qualified. That does not extend to other families,
platforms or operations. The transport `run_pipeline` route is separate: it
scopes the record by this contract or refuses it, and makes no claim beyond
that.

- The complete original record, with its train and test rows, is stored
  byte-identically. The underlying IO package and full record are neither
  reduced nor relabelled.
- The engine training universe, coordinator relations, external envelope and
  FoldSet hold only the train IDs, in original IO order. Native DAG facts stay
  train-only; the scheduler and DAG schemas are unchanged.
- Declared fold indices are relative to the train block in original IO order
  and are aligned with the IO fold provenance by sample ID. They are never read
  against the full-record order, and a fold that disagrees with that
  provenance is refused.
- CV fits, validation/OOF selection and the final refit exclude held-out test
  rows, and preprocessing is fitted on the same training scope. Held-out rows
  are kept for stored and external prediction. They are not scored
  automatically and take no part in model selection.
- All-train records keep their existing behavior, examples and frozen
  references. Rows without exactly one partition tag, or a record without a
  train row, are still refused. No intrinsic size or entry-count ceiling is
  added.

## Required owner source cohort

Frozen DAG414 does not contain these new APIs:

```rust
TrainingInfluenceManifest::validate_identity_closure(
    &self, relations: &SampleRelationSet,
) -> dag_ml_core::Result<()>;
ExternalDataPlanEnvelope::validate_external_test_training_authority(
    &self, training_relation_fingerprint: Option<&str>, fold_set: Option<&FoldSet>,
) -> dag_ml_core::Result<()>;
```

Core calls them directly. The separate DAG author owns their implementation
and shared replay/initial-refit enforcement. The controller must compose and
accept that new owner cohort before Core compilation. Frozen DAG414, IO46a,
shipped pins and locks are unchanged. There is no fallback or copied owner math.
The V3 target-relation/child-requirement open item remains that owner's gate.

## Qualification targets

The new integration targets are `n4a_owner_calibrate` and
`n4a_owner_native_predict`. F10 keeps the exact test name
`admitted_replay_to_calibration` and now uses production actions in cold child
processes, including wide and selected-index-1 cases. Historical
`F10_ATTEMPT06.stderr` and `F10_REAL_RED_TEST.rs` remain external, unchanged.
The temporary public F10 bridge is removed.

Private unit tests in `authority_controls.rs` construct genuine IO/owner-trained
artifacts with deliberately changed request metadata; they export no bridge.
They cover absent/future/malformed audits, partial relation edits, stale
influence, owner C1 authority, FoldSet/policy mutations, observation/origin and
augmentation controls, float relation roundtrip, explicit unavailable schema,
opaque foreign metadata, and the authenticity/content limitation. Integration
tests cover strict raw/member JSON, mixed outcomes, foreign Models/attachments,
features/roles, schema changes, masks/experimental units, whole-record refusal,
missing runtime, classifier refusal, policies, fresh member/state closure,
size limits and cold Linux mapping checks.

Run the accepted controller commands with its frozen config, lock, exact
Methods digest, admitted target/scratch and resource window. The commands
below are a reference, not a record of execution:

```sh
cargo +1.88 --config "$CORE_COHORT_CONFIG" test --locked --offline -p nirs4all --test n4a_owner_calibrate admitted_replay_to_calibration -- --exact --nocapture
cargo +1.88 --config "$CORE_COHORT_CONFIG" test --locked --offline -p nirs4all --test n4a_owner_native_predict --test n4a_owner_calibrate -- --nocapture
cargo +1.88 --config "$CORE_COHORT_CONFIG" test --locked --offline -p nirs4all --lib n4a_owner_native::authority_controls -- --nocapture
cargo +1.88 --config "$CORE_COHORT_CONFIG" test --locked --offline -p nirs4all --test n4a_container --test n4a_owner_roundtrip --test n4a_owned --test n4a_owner_semantic -- --nocapture
cargo +1.88 --config "$CORE_COHORT_CONFIG" clippy --locked --offline --workspace --all-targets -- -D warnings
cargo +1.88 --config "$CORE_COHORT_CONFIG" test --locked --offline --workspace -- --nocapture
cargo +1.88 --config "$CORE_COHORT_CONFIG" check --locked --offline --manifest-path bindings/wasm-native/Cargo.toml --target wasm32-unknown-unknown --lib
```

`CORE_F10_EXECUTION_SCRATCH` is mandatory for process evidence. Keep TMPDIR
within controller-admitted scratch when executing the existing runtime snapshot
mechanism. G1–G5, the 17 owner checks, all 13 reviewed adversarial controls,
native F64/mask captures, independent scientific references, selector-family
qualification, direct-owner replay comparison, byte-identical legacy transport,
package inclusion, and independent Core/DAG WORK remain mandatory controller
gates. A source assertion or a test that was not executed is not a pass. Stage P
is separate.

## Generic containers are not native actions

The actions on this page apply to closed native-produced model containers.
`inspect_n4a_bytes` keeps refusing every other shape. A container holding
owner-neutral members, such as a host model, recipe, dataset, scores or results,
is read and written through the separate `n4a-container@1` storage profile
(`open_n4a_container`, `write_n4a_container`, `n4a-write`, `n4a-read`). That
route returns exact bytes and reports `executability: not_evaluated`; it never
deserializes a host payload. A container that mixes a native model with a second
`Model` artifact or a foreign link stays readable there, but produce, predict
and retrain refuse it.
