//! Runtime-free canonical `.n4a` owner actions shared by native and WASM hosts.
//!
//! IO owns decoding and materialization; DAG owns admission, request assembly,
//! facet semantics and replay; Methods owns fitted state. This module validates
//! and prepares data, seals owner facets and builds the plans a host executes.
//! It loads no Methods runtime, reads no file and starts no thread, so it holds
//! no executable authority: a plan is data, and the executing host re-admits it.
//!
//! The record, admission and seal paths apply the unchanged native policy. The
//! closed WASM profile is added only by the `*_portable` entrypoints below.
#![allow(dead_code)] // Shared by two crates; each uses a subset.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use dag_ml_core::artifact_facets::*;
use dag_ml_core::{
    methods_pls_predict_feature_content_fingerprint, ArtifactLoadMode, ControllerManifest,
    ExternalDataPlanEnvelope, MethodsPlsDataset, MethodsPlsMatrix, NativeLinearPipelineFacts,
    NativePipelineRecipe, Phase, PortablePredictorPackage, PredictCohortConstructionRequest,
    PredictCohortRole, SampleId, SampleRelationSet, TrainingOutcome, TrainingReplayRequest,
    TrainingRequest, TRAINING_REPLAY_REQUEST_SCHEMA_VERSION,
};
use nirs4all_io_core::{canonical_json::parse_json_value, public_dataset as io};
use nirs4all_io_dagml::{methods_pls_predict_cohort, DatasetPackage, PackageProvider};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::archive_codec::sha256;
use crate::n4a::*;
use crate::n4a_owner::{inspect_n4a_semantics, owner_snapshot, OwnerContentInspection};
use crate::n4a_physical::{scan_json_charged, try_copy, Ledger};
use crate::native_pipeline_contract::validate_pipeline;

// File-relative path: this helper follows the module into every crate mounting it.
#[path = "io_package_provider.rs"]
pub mod io_package_provider;
use io_package_provider::{
    convert_relations, validate_disjoint_calibration_relations, DatasetPackageMethodsProvider,
};

const TRAINING_RELATIONS_METADATA: &str = "native_pipeline_training_relations";

/// Producer audit retained inside the owner-fingerprinted campaign, rather
/// than an unbound attachment or a relation set supplied at prediction time.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativePipelineTrainingRelations {
    schema_version: u32,
    source_id: String,
    requirement_key: String,
    relations: SampleRelationSet,
}

/// The existing training adapters return text errors. Facet validation retains
/// its typed owner refusal; this type is not the native action admission API.
#[derive(Debug)]
pub enum NativePipelineProducerError {
    Training(String),
    Owner(FacetRefusal),
}

impl std::fmt::Display for NativePipelineProducerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Training(detail) => f.write_str(detail),
            Self::Owner(source) => std::fmt::Display::fmt(source, f),
        }
    }
}

impl std::error::Error for NativePipelineProducerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Training(_) => None,
            Self::Owner(source) => Some(source),
        }
    }
}

/// Closed dispatch implemented here; qualification remains cohort-specific.
/// Neither inspection nor a caller-supplied list can extend this set.
pub const NATIVE_ACTION_FEATURES: &[&str] = &["native-workflow@1", "native-conformal@1"];

#[derive(Debug)]
pub enum NativeActionError {
    Operation(OperationRefusal),
    Owner(FacetRefusal),
    Container(N4aError),
    Record(String),
    Schema(String),
    Identity(String),
    Producer(NativePipelineProducerError),
    Conformal(dag_ml_core::ConformalError),
}
impl std::fmt::Display for NativeActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation(e) => e.fmt(f),
            Self::Owner(e) => e.fmt(f),
            Self::Container(e) => e.fmt(f),
            Self::Producer(e) => e.fmt(f),
            Self::Conformal(e) => e.fmt(f),
            Self::Record(e) | Self::Schema(e) | Self::Identity(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for NativeActionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Operation(e) => Some(e),
            Self::Owner(e) => Some(e),
            Self::Container(e) => Some(e),
            Self::Producer(e) => Some(e),
            Self::Conformal(e) => Some(e),
            Self::Record(_) | Self::Schema(_) | Self::Identity(_) => None,
        }
    }
}
impl From<FacetRefusal> for NativeActionError {
    fn from(e: FacetRefusal) -> Self {
        Self::Owner(e)
    }
}
impl From<dag_ml_core::DagMlError> for NativeActionError {
    fn from(e: dag_ml_core::DagMlError) -> Self {
        Self::Owner(e.into())
    }
}
impl From<N4aError> for NativeActionError {
    fn from(e: N4aError) -> Self {
        Self::Container(e)
    }
}
pub(crate) type ActionResult<T> = Result<T, NativeActionError>;

/// This selects data only. External bytes cannot override a stored entrypoint.
#[derive(Clone, Copy)]
pub enum NativeInput<'a> {
    Stored,
    External(&'a [u8]),
}

/// Requesting a weaker schema guarantee never ignores a present bad anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSchemaPolicy {
    RequireTrainingSchema,
    AllowUnavailable,
}

/// Host-independent action selection. Hosts add their own runtime and labels.
#[derive(Clone, Copy)]
pub(crate) struct ActionInput<'a> {
    pub(crate) entrypoint: &'a str,
    pub(crate) input: NativeInput<'a>,
    pub(crate) source_id: &'a str,
    pub(crate) partition: &'a str,
    pub(crate) schema_policy: NativeSchemaPolicy,
    pub(crate) memory_budget: N4aMemoryBudget,
}

/// Calibration carries the host's independent-record policy; WASM never calibrates.
#[derive(Clone, Copy)]
pub(crate) enum PrepareMode {
    Predict,
    Calibrate(fn(&Value) -> Result<(), String>),
}

/// Current-cohort evidence is kept distinct from frozen training evidence.
/// All hashes prove consistency only; no producer authenticity is established.
#[derive(Debug, serde::Serialize)]
pub struct NativeActionReceipt {
    pub binding: NativeBindingReceipt,
    pub record_sha256: String,
    pub io_content_identity: Value,
    pub schema_validation: String,
    pub envelope: ExternalDataPlanEnvelope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_authority: Option<String>,
    pub authenticity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_id_disjointness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_content_disjointness: Option<String>,
}
#[derive(Debug, serde::Serialize)]
pub struct NativeBindingReceipt {
    pub container_sha256: String,
    pub entrypoint_id: String,
    pub operation: String,
    pub root_id: String,
}

pub(crate) fn unsupported(detail: impl Into<String>) -> NativeActionError {
    FacetRefusal::UnsupportedOperation {
        operation: detail.into(),
    }
    .into()
}
pub(crate) fn mismatch(detail: impl Into<String>) -> NativeActionError {
    FacetRefusal::Mismatch {
        detail: detail.into(),
    }
    .into()
}

/// Scan exact JSON bytes under `budget`. A memory refusal stays a container
/// refusal; anything else is a defect of the document, reported by `refuse`.
fn scan_owner_json(
    bytes: &[u8],
    budget: N4aMemoryBudget,
    refuse: impl FnOnce(String) -> NativeActionError,
) -> ActionResult<()> {
    match scan_json_charged(bytes, &mut Ledger::new(budget)) {
        Ok(()) => Ok(()),
        Err(error @ N4aError::Memory(_)) => Err(error.into()),
        Err(error) => Err(refuse(error.to_string())),
    }
}

pub(crate) fn decode_record(bytes: &[u8], budget: N4aMemoryBudget) -> ActionResult<Value> {
    scan_owner_json(bytes, budget, NativeActionError::Record)?;
    // The sole dataset decoder is IO. The scanner creates no dataset Value.
    let text = std::str::from_utf8(bytes).map_err(|e| NativeActionError::Record(e.to_string()))?;
    parse_json_value(text).map_err(|e| NativeActionError::Record(e.to_string()))
}

pub(crate) fn normalized_record(value: &Value) -> ActionResult<Value> {
    let normalized = io::normalize_dataset(value).map_err(NativeActionError::Record)?;
    let schema = normalized["schema"].as_str().unwrap_or("");
    let version = normalized["schema_version"].as_u64().unwrap_or(0);
    if !io::PUBLIC_RECORD_CONTENT_TYPES
        .iter()
        .any(|(id, v)| *id == schema && u64::from(*v) == version)
    {
        return Err(unsupported("IO record content type/version"));
    }
    Ok(normalized)
}

fn validate_dataset_descriptor(
    artifact: &N4aArtifact,
    bytes: &[u8],
    value: &Value,
) -> ActionResult<()> {
    let normalized = normalized_record(value)?;
    let identity = &artifact.identity;
    if artifact.owner != io::OWNER_ID
        || identity.owner != io::OWNER_ID
        || artifact.kind != ArtifactKind::Dataset
        || !io::PUBLIC_RECORD_CONTENT_TYPES.contains(&(
            artifact.content_type.id.as_str(),
            artifact.content_type.version,
        ))
        || normalized["schema"].as_str() != Some(artifact.content_type.id.as_str())
        || normalized["schema_version"].as_u64() != Some(u64::from(artifact.content_type.version))
        || identity.schema != artifact.content_type.id
        || identity.schema_version != artifact.content_type.version
        || !io::CONTENT_IDENTITY_PROFILES.contains(&identity.fingerprint_profile.as_str())
    {
        return Err(mismatch("IO dataset descriptor/content contract differs"));
    }
    let expected = if identity.fingerprint_profile == "sha256.raw.v1" {
        sha256(bytes)
    } else {
        io::labelled_dataset_content_identity(value).map_err(NativeActionError::Identity)?
            ["fingerprint"]
            .as_str()
            .ok_or_else(|| NativeActionError::Identity("IO content fingerprint absent".into()))?
            .to_string()
    };
    if identity.fingerprint != expected {
        return Err(mismatch("IO dataset identity differs from immutable bytes"));
    }
    Ok(())
}

fn embedded_record<'a>(
    container: &'a OwnedN4a,
    id: &str,
) -> ActionResult<(&'a N4aArtifact, &'a [u8])> {
    let artifact = container
        .manifest()
        .artifacts
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| mismatch("IO dataset anchor absent"))?;
    let N4aStorage::Embedded {
        root_member,
        member_paths,
    } = &artifact.storage
    else {
        return Err(FacetRefusal::UnresolvedOwnerContent {
            artifact_id: id.into(),
        }
        .into());
    };
    if member_paths.as_slice() != [root_member.clone()] {
        return Err(unsupported("multi-member IO record"));
    }
    let bytes = container
        .member(root_member)
        .ok_or_else(|| mismatch("IO dataset bytes absent"))?
        .bytes;
    Ok((artifact, bytes))
}

fn compare_schema(normalized: &Value, source: &str, anchor: &Value) -> ActionResult<()> {
    let current =
        io::public_source_schema(normalized, source).map_err(NativeActionError::Schema)?;
    if io::canonical_content_bytes(&current).map_err(NativeActionError::Schema)?
        != io::canonical_content_bytes(anchor).map_err(NativeActionError::Schema)?
    {
        return Err(NativeActionError::Schema(
            "IO source schema differs from retained training anchor".into(),
        ));
    }
    Ok(())
}

pub(crate) struct Prepared {
    pub(crate) snapshot: ArtifactFacetExport,
    pub(crate) root_id: String,
    pub(crate) outcome_id: Option<String>,
    pub(crate) binding_id: String,
    pub(crate) package: DatasetPackage,
    pub(crate) relations: SampleRelationSet,
    pub(crate) cohort_key: String,
    pub(crate) cohort: MethodsPlsDataset,
    pub(crate) receipt: NativeActionReceipt,
    pub(crate) sample_ids: Vec<String>,
}

/// The owner export keeps every member, so its complete copy is charged together
/// with retained metadata and resident archive bytes under the supplied budget.
pub(crate) fn complete_owner_snapshot<C: N4aMemberBytes>(
    container: &C,
    budget: N4aMemoryBudget,
) -> ActionResult<(ArtifactFacetExport, Vec<String>)> {
    let metadata = container.metadata();
    let mut ledger = Ledger::new(budget);
    ledger.charge(metadata.retained_bytes())?;
    ledger.charge(container.resident_bytes())?;
    for member in &metadata.manifest().members {
        ledger.charge(member.byte_length)?;
    }
    let inspected = owner_snapshot(container, budget)?;
    let mut members = BTreeMap::new();
    for member in &metadata.manifest().members {
        let bytes = match container.member_bytes(&member.path)? {
            Cow::Borrowed(bytes) => try_copy(bytes)?,
            Cow::Owned(bytes) => bytes,
        };
        members.insert(member.path.clone(), bytes);
    }
    Ok((
        ArtifactFacetExport {
            graph: inspected.graph,
            context: inspected.context,
            members,
        },
        inspected.opaque_link_ids,
    ))
}

/// Resolve same-identity references only from already validated embedded owner
/// bytes. The owner rechecks descriptor kind, identity and optional raw hash.
/// No reference fetch, serialized token or fabricated companion is involved.
pub(crate) fn resolve_local_owner_references(
    snapshot: &mut ArtifactFacetExport,
) -> FacetResult<()> {
    resolve_owner_references(&snapshot.graph, &mut snapshot.context)
}

fn resolve_owner_references(
    graph: &ArtifactFacetGraph,
    context: &mut FacetValidationContext<'_>,
) -> FacetResult<()> {
    for artifact in &graph.artifacts {
        let ArtifactFacetStorage::Reference { root_sha256 } = &artifact.storage else {
            continue;
        };
        if artifact.owner != "dag-ml" {
            continue;
        }
        let proof = context
            .owner_proofs
            .values()
            .find(|proof| {
                proof.content_type() == &artifact.content_type
                    && proof.identity() == &artifact.identity
                    && root_sha256
                        .as_ref()
                        .is_none_or(|sha| sha == proof.raw_sha256())
            })
            .cloned()
            .ok_or_else(|| FacetRefusal::UnresolvedOwnerContent {
                artifact_id: artifact.id.clone(),
            })?;
        context.owner_proofs.insert(artifact.id.clone(), proof);
    }
    Ok(())
}

/// Declared `EvidenceFor` links into one model root. Selection by role only;
/// no body is scanned for a convenient training record.
fn declared_evidence_ids<'a>(graph: &'a ArtifactFacetGraph, root_id: &str) -> BTreeSet<&'a str> {
    graph
        .links
        .iter()
        .filter(|link| {
            link.owner == "dag-ml"
                && link.relation == ArtifactFacetRelation::EvidenceFor
                && link.target_artifact == root_id
        })
        .map(|link| link.source_artifact.as_str())
        .collect()
}

/// Closed native profile shared by actions and the read-only training view:
/// the complete recipe/parameter/graph/owner-state contract, portable-only
/// artifact loading and native controller families. All checks are static and
/// precede any MethodsRuntime loading.
fn validate_closed_native_profile(
    package: &dag_ml_core::PortablePredictorPackage,
    outcome: &dag_ml_core::TrainingOutcome,
    recipe_value: &Value,
    source_id: &str,
) -> ActionResult<()> {
    validate_pipeline(&json!({
        "schema":"nirs4all.native-pipeline.v1", "schema_version":1,
        "config":{"source_id":source_id,"pipeline":recipe_value},
        "training_outcome":outcome,"package":package,
    }))
    .map_err(|detail| mismatch(format!("canonical recipe contract: {detail}")))?;
    if package
        .artifact_bindings
        .iter()
        .any(|b| b.load_mode != ArtifactLoadMode::NativePortable)
    {
        return Err(FacetRefusal::MissingTrust.into());
    }
    // Controller family checks are static and precede MethodsRuntime loading.
    let native = dag_ml_core::hpo::methods_native_controller_ids();
    if package
        .effective_plan
        .node_plans
        .values()
        .any(|node| !native.contains(&node.controller_id))
    {
        return Err(unsupported("non-native controller in canonical pipeline"));
    }
    Ok(())
}

/// Actual owner training view of a sealed native-produced model: the complete
/// `TrainingOutcome` selected by the declared `EvidenceFor` link into the model,
/// and the single source id bound by the owner package.
#[derive(Clone, Debug)]
pub struct N4aTrainingView {
    pub outcome: dag_ml_core::TrainingOutcome,
    pub source_id: String,
}

/// Read-only view over an already opened container. It reuses the owner
/// snapshot/reference resolution and the closed profile checks of the actions.
/// It never trains, replays, loads MethodsRuntime or reads the ZIP directly,
/// and it refuses every shape outside the closed native-produced model slice.
pub fn read_n4a_training_view<C: N4aMemberBytes>(container: &C) -> ActionResult<N4aTrainingView> {
    // The opened container proves complete physical ownership. owner_snapshot
    // additionally validates every recognized embedded facet's exact members,
    // reading/charging file members on demand without retaining an export map.
    let mut snapshot = owner_snapshot(container, container.metadata().memory_budget())?;
    resolve_owner_references(&snapshot.graph, &mut snapshot.context)?;
    // Export validation also requires proofs for ALL embedded Models/FullRefits,
    // including foreign ones. Require those through the owner's inspection API
    // with the complete graph/context; opaque attachments need no member bytes.
    let required: Vec<_> = snapshot
        .graph
        .artifacts
        .iter()
        .filter(|artifact| {
            matches!(
                artifact.kind,
                ArtifactFacetKind::Model | ArtifactFacetKind::FullRefit
            ) && matches!(artifact.storage, ArtifactFacetStorage::Embedded { .. })
        })
        .map(|artifact| artifact.id.clone())
        .collect();
    if let Some((root, inputs)) = required.split_first() {
        require_facet_operation("inspect", root, inputs, &snapshot.graph, &snapshot.context)?;
    } else {
        validate_artifact_facet_links(&snapshot.graph, &snapshot.context)?;
    }
    stored_training_from_owner_context(
        &snapshot.graph,
        &snapshot.context,
        &snapshot.opaque_link_ids,
    )
    .map(|stored| stored.view)
}

/// The validated training view together with the recipe it was trained from.
pub(crate) struct StoredTraining {
    pub(crate) view: N4aTrainingView,
    pub(crate) recipe: NativePipelineRecipe,
}

/// Retrain retains the complete RAM export. Its recipe/source extraction is
/// shared with inspection and uses the validated package only, never a caller.
pub(crate) fn read_stored_training<C: N4aMemberBytes>(
    container: &C,
    budget: N4aMemoryBudget,
) -> ActionResult<StoredTraining> {
    let (mut snapshot, opaque_links) = complete_owner_snapshot(container, budget)?;
    resolve_local_owner_references(&mut snapshot)?;
    // Includes unrelated/foreign Models; never select a smaller proof map.
    snapshot.validate()?;
    stored_training_from_owner_context(&snapshot.graph, &snapshot.context, &opaque_links)
}

/// Shared extraction after complete owner member/proof/link validation.
fn stored_training_from_owner_context(
    graph: &ArtifactFacetGraph,
    context: &FacetValidationContext<'_>,
    opaque_links: &[String],
) -> ActionResult<StoredTraining> {
    if !opaque_links.is_empty() {
        return Err(unsupported("unprovided foreign link validation"));
    }
    let mut models = graph
        .artifacts
        .iter()
        .filter(|a| matches!(a.kind, ArtifactFacetKind::Model));
    let (Some(model), None) = (models.next(), models.next()) else {
        return Err(unsupported("exactly one canonical model artifact"));
    };
    let root_id = model.id.clone();
    let proof = context
        .owner_proofs
        .get(&root_id)
        .ok_or_else(|| mismatch("selected owner proof absent"))?;
    let package = proof
        .predictor_package()
        .ok_or_else(|| unsupported("FullRefit/ChildRefit canonical action remains unqualified"))?;
    package.validate()?;
    let [_requirement] = package.execution_bundle.data_requirements.as_slice() else {
        return Err(unsupported("exactly one native data requirement"));
    };
    let [_output] = package.output_bindings.as_slice() else {
        return Err(unsupported("exactly one validated output binding"));
    };
    let bindings: Vec<_> = package
        .template
        .campaign
        .data_bindings
        .values()
        .flatten()
        .collect();
    let [source_binding] = bindings.as_slice() else {
        return Err(unsupported("single source native pipeline"));
    };
    let [source_id] = source_binding.source_ids.as_slice() else {
        return Err(unsupported("single source native pipeline"));
    };
    let recipe_value = package
        .template
        .campaign
        .metadata
        .get("native_pipeline_recipe")
        .ok_or_else(|| unsupported("canonical linear pipeline recipe required"))?;
    let evidence_ids = declared_evidence_ids(graph, &root_id);
    if evidence_ids.len() != 1 {
        return Err(unsupported(
            "one declared training evidence role for the canonical model",
        ));
    }
    let evidence = *evidence_ids
        .first()
        .ok_or_else(|| mismatch("training evidence role absent"))?;
    let outcome = context
        .owner_proofs
        .get(evidence)
        .and_then(|proof| proof.training_outcome())
        .ok_or_else(|| mismatch("canonical training evidence is unresolved"))?;
    validate_closed_native_profile(package, outcome, recipe_value, source_id)?;
    Ok(StoredTraining {
        view: N4aTrainingView {
            outcome: outcome.clone(),
            source_id: source_id.clone(),
        },
        recipe: serde_json::from_value(recipe_value.clone()).map_err(FacetRefusal::from)?,
    })
}

pub(crate) fn prepare_action(
    container: &OwnedN4a,
    request: &ActionInput<'_>,
    mode: PrepareMode,
) -> ActionResult<Prepared> {
    let calibrating = matches!(mode, PrepareMode::Calibrate(_));
    let binding = container
        .operation_binding(request.entrypoint)
        .map_err(NativeActionError::Operation)?;
    let entry = container
        .manifest()
        .entrypoints
        .iter()
        .find(|e| e.id == request.entrypoint)
        .ok_or(NativeActionError::Operation(
            OperationRefusal::UnknownEntrypoint,
        ))?;
    let missing: Vec<_> = container
        .manifest()
        .required_features
        .iter()
        .chain(&entry.required_features)
        .filter(|feature| !NATIVE_ACTION_FEATURES.contains(&feature.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(NativeActionError::Operation(
            OperationRefusal::UnknownRequiredFeatures(missing),
        ));
    }
    if calibrating != (binding.operation == "calibrate-external") {
        return Err(unsupported(
            "action and exact entrypoint operation disagree",
        ));
    }
    let (mut snapshot, opaque_links) = complete_owner_snapshot(container, request.memory_budget)?;
    resolve_local_owner_references(&mut snapshot)?;
    // Includes unrelated/foreign Models; never select a smaller proof map.
    snapshot.validate()?;
    if !opaque_links.is_empty() {
        return Err(unsupported("unprovided foreign link validation"));
    }
    let role = resolve_facet_entrypoint(
        &binding.owner,
        &binding.operation,
        &binding.root.id,
        &entry.input_artifacts,
        entry.requirements.as_ref().map(|r| r.artifact_id.as_str()),
        &snapshot.graph,
        &FacetDatasetContract {
            owner: io::OWNER_ID,
            content_types: io::PUBLIC_RECORD_CONTENT_TYPES,
        },
    )?;
    let (root_id, outcome_id, input_id, schema_id) = match role {
        FacetEntrypointRole::StoredPredict { root_id, input_ids } => {
            if !matches!(request.input, NativeInput::Stored) {
                return Err(mismatch("stored prediction refuses external override"));
            }
            (root_id, None, Some(input_ids[0].clone()), None)
        }
        FacetEntrypointRole::ExternalPredict {
            root_id,
            schema_anchor,
        } => (root_id, None, None, schema_anchor),
        FacetEntrypointRole::ExternalCalibrate {
            model_id,
            outcome_id,
        } => (model_id, Some(outcome_id), None, None),
    };
    let proof = snapshot
        .context
        .owner_proofs
        .get(&root_id)
        .ok_or_else(|| mismatch("selected owner proof absent"))?;
    let package = proof
        .predictor_package()
        .ok_or_else(|| unsupported("FullRefit/ChildRefit canonical action remains unqualified"))?;
    package.validate()?;
    let [requirement] = package.execution_bundle.data_requirements.as_slice() else {
        return Err(unsupported("exactly one native data requirement"));
    };
    let [output] = package.output_bindings.as_slice() else {
        return Err(unsupported("exactly one validated output binding"));
    };
    let bindings: Vec<_> = package
        .template
        .campaign
        .data_bindings
        .values()
        .flatten()
        .collect();
    let [source_binding] = bindings.as_slice() else {
        return Err(unsupported("single source native pipeline"));
    };
    if source_binding.source_ids.as_slice() != [request.source_id.to_string()] {
        return Err(mismatch("selected source differs from owner binding"));
    }
    let recipe_value = package
        .template
        .campaign
        .metadata
        .get("native_pipeline_recipe")
        .ok_or_else(|| unsupported("canonical linear pipeline recipe required"))?;
    let recipe: NativePipelineRecipe =
        serde_json::from_value(recipe_value.clone()).map_err(FacetRefusal::from)?;
    // Reuse the existing complete recipe/parameter/graph/owner-state contract.
    // Select the outcome by its declared evidence role, never by scanning bodies
    // for a convenient training record or accepting a caller-provided witness.
    let evidence_ids = declared_evidence_ids(&snapshot.graph, &root_id);
    let evidence = if let Some(id) = &outcome_id {
        id.as_str()
    } else {
        if evidence_ids.len() != 1 {
            return Err(unsupported(
                "one declared training evidence role for the canonical model",
            ));
        }
        *evidence_ids
            .first()
            .ok_or_else(|| mismatch("training evidence role absent"))?
    };
    let outcome = snapshot
        .context
        .owner_proofs
        .get(evidence)
        .and_then(|proof| proof.training_outcome())
        .ok_or_else(|| mismatch("canonical training evidence is unresolved"))?;
    validate_closed_native_profile(package, outcome, recipe_value, request.source_id)?;
    let training_relations = if let Some(outcome_id) = &outcome_id {
        preflight_facet_calibration(
            &snapshot.graph,
            &snapshot.context,
            &snapshot.members,
            outcome_id,
            &root_id,
            &output.binding_id,
        )?;
        let outcome = snapshot
            .context
            .owner_proofs
            .get(outcome_id)
            .and_then(|p| p.training_outcome())
            .ok_or_else(|| mismatch("selected calibration outcome proof absent"))?;
        Some(recover_native_pipeline_training_relations(
            outcome, package,
        )?)
    } else {
        None
    };

    let bytes = match (&request.input, &input_id) {
        (NativeInput::Stored, Some(id)) => embedded_record(container, id)?.1,
        (NativeInput::External(bytes), None) => bytes,
        _ => {
            return Err(mismatch(
                "entrypoint requires exact stored or external input mode",
            ))
        }
    };
    let value = decode_record(bytes, request.memory_budget)?;
    let normalized = normalized_record(&value)?;
    if normalized["dataset"]["sources"]
        .as_array()
        .is_none_or(|sources| sources.len() != 1)
    {
        return Err(unsupported("single source IO record required"));
    }
    if let Some(id) = &input_id {
        validate_dataset_descriptor(embedded_record(container, id)?.0, bytes, &value)?;
    }
    let metadata_anchor = package.template.campaign.metadata.get("raw_source_schema");
    let mut schema_validation = "unavailable";
    if let Some(anchor) = metadata_anchor {
        compare_schema(&normalized, request.source_id, anchor)?;
        schema_validation = "io_source_schema";
    } else if calibrating {
        return Err(FacetRefusal::MissingCapability {
            capability: "canonical training schema anchor".into(),
        }
        .into());
    }
    let (dataset, _) = io::masked_matrix_dataset_package(&value, request.source_id)
        .map_err(NativeActionError::Record)?;
    // Validate every embedded IO anchor, including training/stored anchors not
    // consumed by this entrypoint. Present bad evidence never falls back.
    for artifact in &container.manifest().artifacts {
        if artifact.kind != ArtifactKind::Dataset {
            continue;
        }
        let (_, anchor_bytes) = embedded_record(container, &artifact.id)?;
        let decoded_anchor = if input_id.as_ref() == Some(&artifact.id) {
            None
        } else {
            Some(decode_record(anchor_bytes, request.memory_budget)?)
        };
        let anchor = decoded_anchor.as_ref().unwrap_or(&value);
        validate_dataset_descriptor(artifact, anchor_bytes, anchor)?;
        let anchor_normalized = normalized_record(anchor)?;
        if let Some(expected) = metadata_anchor {
            compare_schema(&anchor_normalized, request.source_id, expected)?;
        }
        let other_package = if decoded_anchor.is_some() {
            Some(
                io::masked_matrix_dataset_package(anchor, request.source_id)
                    .map_err(NativeActionError::Record)?
                    .0,
            )
        } else {
            None
        };
        let anchor_package = other_package.as_ref().unwrap_or(&dataset);
        let anchor_provider = DatasetPackageMethodsProvider::for_product_pipeline(
            anchor_package,
            request.source_id,
            recipe
                .steps
                .last()
                .is_some_and(|step| step.role == "classifier"),
        )
        .map_err(NativeActionError::Record)?;
        let anchor_identity = dag_ml_core::TrainingDataIdentity::from_binding_envelope(
            source_binding,
            anchor_provider.external_envelope(),
        )?;
        if package.data_identities.as_slice() != [anchor_identity] {
            return Err(mismatch(
                "embedded training IO identity differs from saved owner training evidence",
            ));
        }
        if schema_id.as_ref() == Some(&artifact.id) {
            let expected = io::public_source_schema(&anchor_normalized, request.source_id)
                .map_err(NativeActionError::Schema)?;
            compare_schema(&normalized, request.source_id, &expected)?;
            schema_validation = "io_source_schema";
        }
    }
    if schema_validation == "unavailable"
        && request.schema_policy == NativeSchemaPolicy::RequireTrainingSchema
    {
        return Err(FacetRefusal::MissingCapability {
            capability: "requested training schema validation".into(),
        }
        .into());
    }
    if let PrepareMode::Calibrate(validate_independent_record) = mode {
        validate_independent_record(&normalized).map_err(NativeActionError::Identity)?;
    }
    let provider = PackageProvider::from_package_source(&dataset, request.source_id)
        .map_err(|e| NativeActionError::Record(e.to_string()))?;
    if requirement.feature_set_id.as_deref() != Some(provider.feature_set_id())
        || requirement.output_representation
            != provider.envelope().plan.output_representation.as_str()
    {
        return Err(mismatch(
            "IO feature set/representation differs from owner requirement",
        ));
    }
    let physical = methods_pls_predict_cohort(
        &dataset.to_assembled(),
        request.partition,
        request.source_id,
    )
    .map_err(|e| NativeActionError::Record(e.to_string()))?;
    let x = MethodsPlsMatrix {
        rows: physical.rows,
        cols: physical.cols,
        values: physical.values,
    };
    if physical.data_content_profile != "n4a-matrix-f64-le.v1"
        || methods_pls_predict_feature_content_fingerprint(&x)? != physical.data_content_fingerprint
    {
        return Err(mismatch("IO/Methods matrix content identity differs"));
    }
    let all_relations = convert_relations(
        provider
            .envelope()
            .coordinator_relations
            .as_ref()
            .ok_or_else(|| NativeActionError::Identity("IO relations absent".into()))?,
    )
    .map_err(NativeActionError::Identity)?;
    let ids: BTreeSet<_> = physical.sample_ids.iter().map(String::as_str).collect();
    let relations = SampleRelationSet {
        records: all_relations
            .records
            .iter()
            .filter(|r| ids.contains(r.sample_id.as_str()))
            .cloned()
            .collect(),
    };
    relations.validate()?;
    if relations
        .records
        .iter()
        .map(|r| r.sample_id.as_str())
        .collect::<BTreeSet<_>>()
        != ids
    {
        return Err(mismatch(
            "selected partition relations differ from actual IO samples",
        ));
    }
    if calibrating && (dataset.partitions.len() != 1 || relations != all_relations) {
        return Err(unsupported(
            "calibration must consume the whole target-fingerprinted IO record",
        ));
    }
    let truth_provider = if calibrating {
        Some(
            DatasetPackageMethodsProvider::new(&dataset, request.source_id)
                .map_err(NativeActionError::Record)?,
        )
    } else {
        None
    };
    let target_fingerprint = if calibrating {
        Some(
            provider
                .envelope()
                .target_content_fingerprint
                .clone()
                .ok_or_else(|| {
                    NativeActionError::Identity("calibration requires IO target identity".into())
                })?,
        )
    } else {
        None
    };
    let cohort = PredictCohortConstructionRequest {
        role: if calibrating {
            PredictCohortRole::ExternalTest
        } else {
            PredictCohortRole::Inference
        },
        relations: relations.clone(),
        target_names: output.target_names.clone(),
        data_content_fingerprint: physical.data_content_fingerprint.clone(),
        target_content_fingerprint: target_fingerprint.clone(),
    }
    .derive()?;
    let coordinator = training_relations.as_ref().unwrap_or(&relations);
    let canonical_relation_fingerprint = coordinator.fingerprint()?;
    let envelope = ExternalDataPlanEnvelope {
        schema_version: 2,
        schema_fingerprint: requirement.schema_fingerprint.clone(),
        plan_fingerprint: requirement.plan_fingerprint.clone(),
        relation_fingerprint: if training_relations.is_some() {
            // Calibration keeps S from the sealed requirement; C is separate.
            requirement.relation_fingerprint.clone()
        } else {
            Some(canonical_relation_fingerprint.clone())
        },
        data_content_fingerprint: Some(physical.data_content_fingerprint),
        target_content_fingerprint: target_fingerprint,
        coordinator_relations: Some(coordinator.clone()),
        predict_cohort: Some(cohort),
    };
    envelope.validate()?;
    if let Some(training) = &training_relations {
        // S comes from the sealed requirement. C comes from the independently
        // recovered training records above, never the incoming calibration cohort.
        envelope.validate_external_test_training_authority(
            requirement.relation_fingerprint.as_deref(),
            Some(canonical_relation_fingerprint.as_str()),
            package.effective_plan.fold_set.as_ref(),
        )?;
        package
            .effective_plan
            .campaign
            .validate_data_envelope_relations(&envelope)?;
        dag_ml_core::public_robustness::validate_independent_uncertainty_cohort(
            package, &relations,
        )?;
        validate_disjoint_calibration_relations(training, &relations)
            .map_err(NativeActionError::Identity)?;
        if truth_provider
            .as_ref()
            .is_none_or(|p| p.relations() != &relations)
        {
            return Err(mismatch("truth provider and replay relations differ"));
        }
    }
    let binding_id = output.binding_id.clone();
    // IO physical order for the selected partition, kept before the dataset
    // below consumes the IDs.
    let sample_ids = physical.sample_ids.clone();
    let cohort_key = requirement.key();
    let cohort = MethodsPlsDataset {
        sample_ids: physical
            .sample_ids
            .into_iter()
            .map(SampleId::new)
            .collect::<dag_ml_core::Result<_>>()?,
        x,
        y: None,
        target_names: output.target_names.clone(),
        y_validity_masks: None,
    };
    let receipt = NativeActionReceipt {
        binding: NativeBindingReceipt {
            container_sha256: binding.container_sha256,
            entrypoint_id: binding.entrypoint_id,
            operation: binding.operation,
            root_id: root_id.clone(),
        },
        record_sha256: sha256(bytes),
        io_content_identity: io::labelled_dataset_content_identity(&value)
            .map_err(NativeActionError::Identity)?,
        schema_validation: schema_validation.into(),
        envelope,
        relation_authority: calibrating.then(|| "fingerprint_bound_self_consistent".into()),
        authenticity: "not_established".into(),
        observation_id_disjointness: calibrating
            .then(|| "checked_against_recorded_universe".into()),
        // Distinct IDs cannot prove spectra differ. No content claim is made.
        observation_content_disjointness: calibrating.then(|| "unavailable".into()),
    };
    Ok(Prepared {
        snapshot,
        root_id,
        outcome_id,
        binding_id,
        package: dataset,
        relations,
        cohort_key,
        cohort,
        receipt,
        sample_ids,
    })
}

/// The signed target-free PREDICT request of one validated predictor binding.
pub(crate) fn predict_replay_request(
    predictor: &PortablePredictorPackage,
    binding_id: &str,
    request_id: &str,
) -> ActionResult<TrainingReplayRequest> {
    let mut replay_request = TrainingReplayRequest {
        schema_version: TRAINING_REPLAY_REQUEST_SCHEMA_VERSION,
        request_id: request_id.into(),
        source_outcome_fingerprint: predictor.training_outcome.outcome_fingerprint.clone(),
        phase: Phase::Predict,
        data_envelope_keys: vec![predictor.execution_bundle.data_requirements[0].key()],
        output_binding_ids: vec![binding_id.to_string()],
        request_fingerprint: String::new(),
    };
    replay_request.request_fingerprint = replay_request.compute_fingerprint()?;
    Ok(replay_request)
}

pub(crate) fn validate_carry(
    container: &OwnedN4a,
    baseline: &ArtifactFacetExport,
) -> ActionResult<()> {
    baseline.validate()?;
    let expected_artifacts: Vec<N4aArtifact> =
        serde_json::from_value(json!(baseline.graph.artifacts)).map_err(FacetRefusal::from)?;
    let expected_links: Vec<N4aLink> =
        serde_json::from_value(json!(baseline.graph.links)).map_err(FacetRefusal::from)?;
    let actual: Vec<_> = container
        .manifest()
        .artifacts
        .iter()
        .filter(|a| a.owner == "dag-ml")
        .cloned()
        .collect();
    if actual != expected_artifacts || container.manifest().links != expected_links {
        return Err(unsupported(
            "fresh export cannot preserve extra artifacts or unprovided links",
        ));
    }
    let complete_graph: ArtifactFacetGraph = serde_json::from_value(json!({
        "artifacts":container.manifest().artifacts,"links":container.manifest().links,
    }))
    .map_err(FacetRefusal::from)?;
    // Unsupported retained entrypoint roles are knowable before replay. Their
    // feature lists remain untouched and grant no authority to this selection.
    for entry in &container.manifest().entrypoints {
        resolve_facet_entrypoint(
            &entry.owner,
            &entry.operation,
            &entry.root_artifact,
            &entry.input_artifacts,
            entry.requirements.as_ref().map(|r| r.artifact_id.as_str()),
            &complete_graph,
            &FacetDatasetContract {
                owner: io::OWNER_ID,
                content_types: io::PUBLIC_RECORD_CONTENT_TYPES,
            },
        )?;
    }
    let mut claimed: BTreeSet<String> = baseline.members.keys().cloned().collect();
    for (path, bytes) in &baseline.members {
        if container.member(path).is_none_or(|m| m.bytes != bytes) {
            return Err(mismatch(
                "source owner member differs from complete regenerated closure",
            ));
        }
    }
    for artifact in &container.manifest().artifacts {
        if artifact.owner == "dag-ml" {
            continue;
        }
        let named = container.manifest().entrypoints.iter().any(|e| {
            e.input_artifacts.contains(&artifact.id)
                || e.requirements
                    .as_ref()
                    .is_some_and(|r| r.artifact_id == artifact.id)
        });
        if !named || artifact.owner != io::OWNER_ID || artifact.kind != ArtifactKind::Dataset {
            return Err(unsupported("unprovided attachment/anchor preservation"));
        }
        let (_, bytes) = embedded_record(container, &artifact.id)?;
        // prepare() already validated the IO descriptor, source schema and
        // training identity from these immutable bytes. Recheck raw transport
        // without decoding another Value or constructing another package.
        let N4aStorage::Embedded { root_member, .. } = &artifact.storage else {
            unreachable!()
        };
        let declared = container
            .manifest()
            .members
            .iter()
            .find(|m| &m.path == root_member)
            .ok_or_else(|| mismatch("carried IO member inventory absent"))?;
        if sha256(bytes) != declared.sha256 {
            return Err(mismatch("carried IO raw hash differs"));
        }
        let N4aStorage::Embedded { member_paths, .. } = &artifact.storage else {
            unreachable!()
        };
        for path in member_paths {
            if !claimed.insert(path.clone()) {
                return Err(mismatch("carried member collision"));
            }
        }
    }
    if claimed
        != container
            .manifest()
            .members
            .iter()
            .map(|m| m.path.clone())
            .collect()
    {
        return Err(mismatch("unprovided source member preservation"));
    }
    Ok(())
}

pub(crate) fn compose(
    source: Option<&OwnedN4a>,
    before: &ArtifactFacetExport,
    mut fresh: ArtifactFacetExport,
    container_id: &str,
    budget: N4aMemoryBudget,
) -> ActionResult<OwnedN4a> {
    fresh.validate()?;
    let mut artifacts: Vec<N4aArtifact> =
        serde_json::from_value(json!(fresh.graph.artifacts)).map_err(FacetRefusal::from)?;
    let links: Vec<N4aLink> =
        serde_json::from_value(json!(fresh.graph.links)).map_err(FacetRefusal::from)?;
    let (required_features, entrypoints) = if let Some(source) = source {
        validate_carry(source, before)?;
        // Exact fitted-state hashes/paths, not a promise inferred from roots.
        for artifact in &before.graph.artifacts {
            if !matches!(
                artifact.kind,
                ArtifactFacetKind::Model | ArtifactFacetKind::FullRefit
            ) {
                continue;
            }
            let proof = before
                .context
                .owner_proofs
                .get(&artifact.id)
                .ok_or_else(|| mismatch("source state proof absent"))?;
            for state in validate_facet_members(artifact, proof, &before.members)? {
                if fresh.members.get(&state.member_path) != before.members.get(&state.member_path) {
                    return Err(mismatch(
                        "calibration changed fitted-state member bytes/path",
                    ));
                }
            }
        }
        for artifact in &source.manifest().artifacts {
            if artifact.owner == "dag-ml" {
                continue;
            }
            if artifacts.iter().any(|a| a.id == artifact.id) {
                return Err(mismatch("fresh artifact/anchor collision"));
            }
            let N4aStorage::Embedded { member_paths, .. } = &artifact.storage else {
                return Err(unsupported("reference anchor carry"));
            };
            for path in member_paths {
                let bytes = source
                    .member(path)
                    .ok_or_else(|| mismatch("carried anchor bytes absent"))?
                    .bytes;
                if fresh.members.insert(path.clone(), bytes.to_vec()).is_some() {
                    return Err(mismatch("fresh member/anchor collision"));
                }
            }
            artifacts.push(artifact.clone());
        }
        (
            source.manifest().required_features.clone(),
            source.manifest().entrypoints.clone(),
        )
    } else {
        (vec![], external_entrypoints())
    };
    let manifest = N4aManifest {
        schema: N4A_SCHEMA.into(),
        schema_version: 1,
        minimum_reader_version: 1,
        container_id: container_id.into(),
        required_features,
        physical_profile: N4A_PHYSICAL_PROFILE.into(),
        members: fresh
            .members
            .iter()
            .map(|(path, bytes)| N4aMember {
                path: path.clone(),
                byte_length: bytes.len() as u64,
                sha256: sha256(bytes),
            })
            .collect(),
        artifacts,
        links,
        entrypoints,
    };
    let owned = write_n4a_owned(
        N4aWriteRequest {
            manifest,
            members: fresh
                .members
                .iter()
                .map(|(path, bytes)| N4aMemberInput { path, bytes })
                .collect(),
        },
        budget,
    )?;
    let (mut snapshot, opaque) = complete_owner_snapshot(&owned, budget)?;
    resolve_local_owner_references(&mut snapshot)?;
    snapshot.validate()?;
    if !opaque.is_empty() {
        return Err(unsupported("fresh foreign links"));
    }
    // Every fresh entrypoint must resolve against the complete fresh graph.
    for entry in &owned.manifest().entrypoints {
        resolve_facet_entrypoint(
            &entry.owner,
            &entry.operation,
            &entry.root_artifact,
            &entry.input_artifacts,
            entry.requirements.as_ref().map(|r| r.artifact_id.as_str()),
            &snapshot.graph,
            &FacetDatasetContract {
                owner: io::OWNER_ID,
                content_types: io::PUBLIC_RECORD_CONTENT_TYPES,
            },
        )?;
    }
    Ok(owned)
}

fn external_entrypoints() -> Vec<N4aEntrypoint> {
    vec![
        N4aEntrypoint {
            id: "predict-external".into(),
            owner: "dag-ml".into(),
            operation: "predict-external".into(),
            root_artifact: "model".into(),
            input_artifacts: vec![],
            required_features: vec!["native-workflow@1".into()],
            requirements: None,
        },
        N4aEntrypoint {
            id: "calibrate-external".into(),
            owner: "dag-ml".into(),
            operation: "calibrate-external".into(),
            root_artifact: "model".into(),
            input_artifacts: vec![],
            required_features: vec!["native-conformal@1".into()],
            requirements: Some(RequirementRef {
                artifact_id: "result".into(),
            }),
        },
    ]
}

/// Recover only after role resolution and owner calibration preflight. The
/// reserved metadata supplies a preimage, not authenticity or caller authority.
pub(crate) fn recover_native_pipeline_training_relations(
    outcome: &TrainingOutcome,
    package: &PortablePredictorPackage,
) -> FacetResult<SampleRelationSet> {
    outcome.validate()?;
    package.validate()?;
    if package.effective_plan != outcome.effective_plan
        || package.training_outcome != outcome.to_reference()?
        || package.data_identities != outcome.data_identities
        || package.training_influence != outcome.training_influence
    {
        return Err(FacetRefusal::Mismatch {
            detail: "training outcome/package closure differs".into(),
        });
    }
    let raw = package
        .template
        .campaign
        .metadata
        .get(TRAINING_RELATIONS_METADATA)
        .ok_or_else(|| FacetRefusal::MissingCapability {
            capability: "fingerprint-bound native pipeline training relations audit".into(),
        })?;
    let version = raw
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| FacetRefusal::InvalidPayload {
            detail: "training relations schema_version must be an integer".into(),
        })?;
    if version != 1 {
        return Err(FacetRefusal::MissingCapability {
            capability: format!("native pipeline training relations v{version}"),
        });
    }
    let audit: NativePipelineTrainingRelations = serde_json::from_value(raw.clone())?;
    if outcome
        .effective_plan
        .campaign
        .metadata
        .get(TRAINING_RELATIONS_METADATA)
        != Some(raw)
        || package
            .effective_plan
            .campaign
            .metadata
            .get(TRAINING_RELATIONS_METADATA)
            != Some(raw)
    {
        return Err(FacetRefusal::Mismatch {
            detail: "training relation metadata retention differs".into(),
        });
    }
    let [identity] = package.data_identities.as_slice() else {
        return Err(FacetRefusal::UnsupportedOperation {
            operation: "training relations require one data identity".into(),
        });
    };
    let [requirement] = package.execution_bundle.data_requirements.as_slice() else {
        return Err(FacetRefusal::UnsupportedOperation {
            operation: "training relations require one data requirement".into(),
        });
    };
    let bindings: Vec<_> = package
        .template
        .campaign
        .data_bindings
        .values()
        .flatten()
        .collect();
    let [binding] = bindings.as_slice() else {
        return Err(FacetRefusal::UnsupportedOperation {
            operation: "training relations require one source binding".into(),
        });
    };
    let fingerprint = audit.relations.fingerprint()?;
    if audit.requirement_key != identity.requirement_key
        || audit.requirement_key != requirement.key()
        || audit.requirement_key
            != dag_ml_core::data_binding_requirement_key(&binding.node_id, &binding.input_name)
        || binding.source_ids.len() != 1
        || binding.source_ids[0] != audit.source_id
        || fingerprint != identity.relation_fingerprint
        || Some(fingerprint.as_str()) != requirement.relation_fingerprint.as_deref()
        || Some(fingerprint.as_str()) != binding.relation_fingerprint.as_deref()
        || fingerprint != package.training_influence.relation_fingerprint
    {
        return Err(FacetRefusal::Mismatch {
            detail: "retained IO relations differ from owner identity/influence/binding/source"
                .into(),
        });
    }
    let folds = package.effective_plan.fold_set.as_ref().ok_or_else(|| {
        FacetRefusal::MissingCapability {
            capability: "training CV FoldSet evidence".into(),
        }
    })?;
    audit
        .relations
        .validate_against_fold_set(folds, &package.effective_plan.campaign.leakage_policy)?;
    if let Some(split) = &package.effective_plan.campaign.split_invocation {
        audit
            .relations
            .validate_against_fold_set(folds, &split.leakage_policy)?;
    }
    let samples = audit
        .relations
        .records
        .iter()
        .map(|record| &record.sample_id)
        .collect::<std::collections::BTreeSet<_>>();
    if folds.sample_ids.iter().any(|id| !samples.contains(id)) {
        return Err(FacetRefusal::Mismatch {
            detail: "training relations omit a CV fold sample".into(),
        });
    }
    // Added by the separately accepted DAG owner overlay. Never duplicate its
    // private influence closure or fall back to reconstructed relations here.
    package
        .training_influence
        .validate_identity_closure(&audit.relations)?;
    Ok(audit.relations)
}

/// One sealed export and the actual owner training view of the SAME fit.
pub struct NativeProduction {
    pub container: OwnedN4a,
    pub training: N4aTrainingView,
}

/// Parse a native pipeline recipe through the owner's strict path: the
/// structural scanner (duplicate keys, escaped aliases, nesting) and the IO
/// canonical JSON parser run before the closed `deny_unknown_fields` recipe
/// type. A parameter is preserved or the recipe is refused; none is dropped.
pub fn parse_native_recipe_json(
    recipe_json: &str,
    budget: N4aMemoryBudget,
) -> ActionResult<NativePipelineRecipe> {
    let refuse = |detail: String| {
        NativeActionError::Producer(NativePipelineProducerError::Training(format!(
            "native pipeline recipe refused: {detail}"
        )))
    };
    scan_owner_json(recipe_json.as_bytes(), budget, refuse)?;
    let value = parse_json_value(recipe_json).map_err(|e| refuse(e.to_string()))?;
    serde_json::from_value(value).map_err(|e| refuse(e.to_string()))
}

/// One training export ready to seal: the owner facets of a single fit and the
/// exact IO record bytes it was trained on.
pub(crate) struct TrainingSeal<'a> {
    pub(crate) record: &'a [u8],
    pub(crate) value: &'a Value,
    pub(crate) facets: ArtifactFacetExport,
    pub(crate) training: N4aTrainingView,
    pub(crate) container_id: &'a str,
    pub(crate) embed_training_record: bool,
    pub(crate) memory_budget: N4aMemoryBudget,
}

/// Bind one fit to the request that planned it and to its reserved metadata.
/// Both hosts seal through this check, native or externally executed.
pub(crate) fn check_training_retention(
    request: &TrainingRequest,
    training: &TrainingOutcome,
    package: &PortablePredictorPackage,
) -> Result<(), NativePipelineProducerError> {
    if training.training_request_fingerprint != request.request_fingerprint {
        return Err(NativePipelineProducerError::Owner(FacetRefusal::Mismatch {
            detail: "training outcome differs from its planned request".into(),
        }));
    }
    for key in [
        "raw_source_schema",
        TRAINING_RELATIONS_METADATA,
        "native_pipeline_recipe",
    ] {
        let expected = request.campaign.metadata.get(key);
        if expected.is_none()
            || training.effective_plan.campaign.metadata.get(key) != expected
            || package.template.campaign.metadata.get(key) != expected
        {
            return Err(NativePipelineProducerError::Owner(
                FacetRefusal::MissingCapability {
                    capability: format!("owner retention of canonical pipeline metadata `{key}`"),
                },
            ));
        }
    }
    Ok(())
}

/// Seal the complete owner facets and, on request, embed the exact IO record as
/// the training anchor and stored prediction input.
pub(crate) fn seal_training_facets(seal: TrainingSeal<'_>) -> ActionResult<NativeProduction> {
    let TrainingSeal {
        record,
        value,
        facets,
        training,
        container_id,
        embed_training_record,
        memory_budget,
    } = seal;
    let empty = ArtifactFacetExport {
        graph: ArtifactFacetGraph::default(),
        members: BTreeMap::new(),
        context: FacetValidationContext::default(),
    };
    let owned = compose(None, &empty, facets, container_id, memory_budget)?;
    if !embed_training_record {
        return Ok(NativeProduction {
            container: owned,
            training,
        });
    }
    let mut manifest = owned.manifest().clone();
    let path = "io/training.json";
    let schema = value["schema"]
        .as_str()
        .ok_or_else(|| NativeActionError::Record("IO schema absent".into()))?;
    let version = value["schema_version"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| NativeActionError::Record("IO schema version absent".into()))?;
    let content_identity =
        io::labelled_dataset_content_identity(value).map_err(NativeActionError::Identity)?;
    let artifact = N4aArtifact {
        id: "training-data".into(),
        kind: ArtifactKind::Dataset,
        owner: io::OWNER_ID.into(),
        content_type: ContentType {
            id: schema.into(),
            version,
        },
        identity: crate::n4a::OwnerIdentity {
            owner: io::OWNER_ID.into(),
            schema: schema.into(),
            schema_version: version,
            fingerprint_profile: content_identity["profile"]
                .as_str()
                .ok_or_else(|| mismatch("IO profile absent"))?
                .into(),
            fingerprint: content_identity["fingerprint"]
                .as_str()
                .ok_or_else(|| mismatch("IO digest absent"))?
                .into(),
        },
        storage: N4aStorage::Embedded {
            root_member: path.into(),
            member_paths: vec![path.into()],
        },
        requirements: None,
    };
    validate_dataset_descriptor(&artifact, record, value)?;
    if manifest.artifacts.iter().any(|a| a.id == artifact.id) || owned.member(path).is_some() {
        return Err(mismatch("producer IO anchor collision"));
    }
    manifest.artifacts.push(artifact);
    manifest.members.push(N4aMember {
        path: path.into(),
        byte_length: record.len() as u64,
        sha256: sha256(record),
    });
    manifest.entrypoints[0].requirements = Some(RequirementRef {
        artifact_id: "training-data".into(),
    });
    manifest.entrypoints.push(N4aEntrypoint {
        id: "predict".into(),
        owner: "dag-ml".into(),
        operation: "predict".into(),
        root_artifact: "model".into(),
        input_artifacts: vec!["training-data".into()],
        required_features: vec!["native-workflow@1".into()],
        requirements: None,
    });
    let mut members: Vec<_> = owned
        .manifest()
        .members
        .iter()
        .map(|m| N4aMemberInput {
            path: &m.path,
            bytes: owned.member(&m.path).expect("validated owned member").bytes,
        })
        .collect();
    members.push(N4aMemberInput {
        path,
        bytes: record,
    });
    let container = write_n4a_owned(N4aWriteRequest { manifest, members }, memory_budget)?;
    Ok(NativeProduction {
        container,
        training,
    })
}

/// IO audit order restricted to the training relations: declared-fold index domain.
fn training_input_sample_ids(
    dataset: &DatasetPackage,
    relations: &SampleRelationSet,
) -> Result<Option<Vec<String>>, String> {
    let train_ids: BTreeSet<_> = relations
        .records
        .iter()
        .map(|record| record.sample_id.as_str())
        .collect();
    dataset
        .audits
        .iter()
        .find_map(|audit| audit.get("input_sample_ids"))
        .map(|value| {
            let ids: Vec<String> =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            Ok(ids
                .into_iter()
                .filter(|id| train_ids.contains(id.as_str()))
                .collect())
        })
        .transpose()
}

/// IO identities and declared folds of the selected training source, read
/// through the exact provider that serves training. No buffer is carried.
pub(crate) fn native_linear_facts(
    dataset: &DatasetPackage,
    source_id: &str,
    recipe: &NativePipelineRecipe,
) -> Result<NativeLinearPipelineFacts, String> {
    let classification = recipe
        .steps
        .last()
        .is_some_and(|step| step.role == "classifier");
    let provider =
        DatasetPackageMethodsProvider::for_product_pipeline(dataset, source_id, classification)?;
    let input_sample_ids = training_input_sample_ids(dataset, provider.relations())?;
    let mixed = provider
        .provider
        .envelope()
        .coordinator_relations
        .as_ref()
        .is_some_and(|relations| relations.records.len() != provider.relations().records.len());
    if mixed && !dataset.folds.is_empty() {
        // IO fold positions are train-relative in audit order. Check their
        // provenance once here, mapping observation IDs through the relations.
        let ids = input_sample_ids
            .as_ref()
            .ok_or("Mixed native folds require IO input sample order")?;
        let samples_by_observation: BTreeMap<_, _> = provider
            .relations()
            .records
            .iter()
            .map(|record| (record.observation_id.as_str(), record.sample_id.as_str()))
            .collect();
        let resolve = |indices: &[i64]| -> Result<Vec<&str>, String> {
            indices
                .iter()
                .map(|index| {
                    usize::try_from(*index)
                        .ok()
                        .and_then(|position| ids.get(position))
                        .map(String::as_str)
                        .ok_or("Declared fold index outside native training cohort".into())
                })
                .collect()
        };
        let provenance_samples = |observations: &[String]| -> Result<Vec<&str>, String> {
            observations
                .iter()
                .map(|id| {
                    samples_by_observation
                        .get(id.as_str())
                        .copied()
                        .ok_or("Declared fold provenance outside native training cohort".into())
                })
                .collect()
        };
        if dataset.folds.len() != dataset.fold_provenance.len() {
            return Err("Declared folds disagree with IO fold provenance".into());
        }
        for ((train, validation), provenance) in dataset.folds.iter().zip(&dataset.fold_provenance)
        {
            if resolve(train)? != provenance_samples(&provenance.train_observation_ids)?
                || resolve(validation)?
                    != provenance_samples(&provenance.validation_observation_ids)?
            {
                return Err("Declared folds disagree with IO fold provenance".into());
            }
        }
    }
    Ok(NativeLinearPipelineFacts {
        envelope: provider.external_envelope().clone(),
        relations: provider.relations().clone(),
        feature_set_id: provider.provider.feature_set_id().to_string(),
        source_id: provider.source_id().to_string(),
        target_names: provider.provider.target_names().to_vec(),
        input_sample_ids,
        declared_folds: dataset.folds.clone(),
    })
}

/// Prepare the canonical single-source CV producer using IO's public record
/// projection. This separate path never changes the legacy transport request.
/// The returned package is the same package whose provider attests training.
///
/// `assemble` builds the request from the package; hosts differ only in where
/// their controller manifests come from.
pub(crate) fn canonical_export_training_request(
    dataset_record: &Value,
    source_id: &str,
    recipe: &NativePipelineRecipe,
    assemble: impl FnOnce(&DatasetPackage) -> Result<TrainingRequest, String>,
) -> Result<(DatasetPackage, TrainingRequest), String> {
    let normalized = io::normalize_dataset(dataset_record)?;
    if normalized["dataset"]["sources"]
        .as_array()
        .is_none_or(|sources| sources.len() != 1)
    {
        return Err("canonical pipeline producer requires exactly one IO source".into());
    }
    let (dataset, _) = io::masked_matrix_dataset_package(dataset_record, source_id)?;
    // Only IO's canonical producer supplies this audit. Do not accept a
    // caller-created DatasetPackage audit as a schema declaration here.
    let mut audits = dataset
        .audits
        .iter()
        .filter(|audit| audit.get("selected_source").and_then(Value::as_str) == Some(source_id));
    let schema = audits
        .next()
        .and_then(|audit| audit.get("raw_source_schema"))
        .filter(|schema| !schema.is_null())
        .ok_or("canonical IO producer did not retain raw_source_schema")?
        .clone();
    if audits.next().is_some() {
        return Err("canonical IO producer returned ambiguous source audits".into());
    }
    let mut request = assemble(&dataset)?;
    let provider = DatasetPackageMethodsProvider::for_product_pipeline(
        &dataset,
        source_id,
        recipe
            .steps
            .last()
            .is_some_and(|step| step.role == "classifier"),
    )?;
    let [identity] = request.data_identities.as_slice() else {
        return Err("canonical pipeline producer requires one training data identity".into());
    };
    let relations = provider.relations().clone();
    if relations.fingerprint().map_err(|e| e.to_string())? != identity.relation_fingerprint {
        return Err(
            "IO training relations differ from fingerprint-bound training data identity".into(),
        );
    }
    let projection = request.project().map_err(|e| e.to_string())?;
    let folds = projection
        .plan
        .fold_set
        .as_ref()
        .ok_or("canonical pipeline producer requires genuine CV FoldSet evidence")?;
    relations
        .validate_against_fold_set(folds, &projection.plan.campaign.leakage_policy)
        .map_err(|e| e.to_string())?;
    let audit = NativePipelineTrainingRelations {
        schema_version: 1,
        source_id: source_id.into(),
        requirement_key: identity.requirement_key.clone(),
        relations,
    };
    for key in ["raw_source_schema", TRAINING_RELATIONS_METADATA] {
        if request.campaign.metadata.contains_key(key) {
            return Err(format!(
                "canonical producer refuses existing reserved metadata `{key}`"
            ));
        }
    }
    request
        .campaign
        .metadata
        .insert("raw_source_schema".into(), schema);
    request.campaign.metadata.insert(
        TRAINING_RELATIONS_METADATA.into(),
        serde_json::to_value(audit).map_err(|e| e.to_string())?,
    );
    // Both audits must precede training and every owner-derived fingerprint.
    request.request_fingerprint = request.compute_fingerprint().map_err(|e| e.to_string())?;
    request.validate().map_err(|e| e.to_string())?;
    Ok((dataset, request))
}

/// Serialize the closed native-produced model view. Every field comes from the
/// typed Core contracts: the full serde manifest, the container receipt and the
/// actual owner `TrainingOutcome`. A host recomputes no identity or hash.
pub fn n4a_view_json(container: &OwnedN4a, training: &N4aTrainingView) -> Result<String, String> {
    let receipt = container.receipt();
    let view = json!({
        "manifest": serde_json::to_value(container.manifest()).map_err(|e| e.to_string())?,
        "receipt": {
            "container_sha256": receipt.container_sha256,
            "manifest_sha256": receipt.manifest_sha256,
            "byte_length": receipt.byte_length,
        },
        "training": {
            "outcome": serde_json::to_value(&training.outcome).map_err(|e| e.to_string())?,
            "source_id": training.source_id,
        },
    });
    serde_json::to_string(&view).map_err(|e| e.to_string())
}

/// One charged, fallible copy: `archive_bytes` is the caller's and stays borrowed.
fn open_portable(archive_bytes: &[u8], budget: N4aMemoryBudget) -> ActionResult<OwnedN4a> {
    budget.admit(archive_bytes.len() as u64)?;
    Ok(open_owned_n4a_bytes(
        try_copy(archive_bytes)?.into_boxed_slice(),
        budget,
    )?)
}

/// Open externally supplied bytes at the owner boundary and return the closed
/// native-produced model view as JSON. Any other shape is refused.
pub fn inspect_n4a_view_json(
    archive_bytes: &[u8],
    budget: N4aMemoryBudget,
) -> ActionResult<String> {
    let container = open_portable(archive_bytes, budget)?;
    let training = read_n4a_training_view(&container)?;
    n4a_view_json(&container, &training).map_err(NativeActionError::Record)
}

/// Report-only owner semantics of any opened container. A refusal is reported,
/// never raised: readability is not executability. Unknown owners are opaque.
fn n4a_semantic_report<C: N4aMemberBytes>(container: &C) -> Value {
    let refused = |detail: String| json!({"status": "refused", "detail": detail});
    let report = match inspect_n4a_semantics(container) {
        Ok(report) => report,
        Err(refusal) => return refused(refusal.to_string()),
    };
    let build = || -> Result<Value, serde_json::Error> {
        let mut artifacts = Vec::new();
        for artifact in &report.artifacts {
            artifacts.push(match &artifact.content {
                OwnerContentInspection::Validated {
                    state_members,
                    model_states,
                } => json!({
                    "artifact_id": artifact.artifact_id, "content": "validated",
                    "state_members": serde_json::to_value(state_members)?,
                    "model_states": serde_json::to_value(model_states)?,
                }),
                OwnerContentInspection::Opaque => {
                    json!({"artifact_id": artifact.artifact_id, "content": "opaque"})
                }
                OwnerContentInspection::Unresolved => {
                    json!({"artifact_id": artifact.artifact_id, "content": "unresolved"})
                }
            });
        }
        let entrypoints: Vec<Value> = report
            .entrypoints
            .iter()
            .map(|entry| match &entry.content {
                Ok(()) => json!({"entrypoint_id": entry.entrypoint_id, "content": "resolved"}),
                Err(refusal) => json!({
                    "entrypoint_id": entry.entrypoint_id, "content": "refused",
                    "detail": refusal.to_string(),
                }),
            })
            .collect();
        Ok(json!({
            "status": "report_only", "artifacts": artifacts,
            "links": serde_json::to_value(&report.links)?,
            "opaque_link_ids": report.opaque_link_ids, "entrypoints": entrypoints,
        }))
    };
    build().unwrap_or_else(|error| refused(error.to_string()))
}

/// Static owner view of native training evidence. Containers outside the closed
/// native-produced model shape report `not_available`; they remain readable.
fn n4a_training_view_report<C: N4aMemberBytes>(container: &C) -> Value {
    // The native profile always carries a dag-ml owned Model artifact (the owner
    // test used throughout this module). Without one the manifest is outside the
    // closed native model shape, so no training owner data is scanned or parsed.
    let native_shape = container
        .metadata()
        .manifest()
        .artifacts
        .iter()
        .any(|a| a.owner == "dag-ml" && matches!(a.kind, ArtifactKind::Model));
    if !native_shape {
        return json!({
            "status": "not_available",
            "detail": "no dag-ml model artifact: outside the native-produced model profile",
        });
    }
    match read_n4a_training_view(container) {
        Ok(view) => {
            let outcome = serde_json::to_value(&view.outcome).unwrap_or(Value::Null);
            json!({
                "status": "readable", "source_id": view.source_id,
                "run_id": outcome.get("run_id"),
                "selected_variant_id": outcome.get("selected_variant_id"),
                "outcome_fingerprint": outcome.get("outcome_fingerprint"),
            })
        }
        Err(refusal) => json!({"status": "not_available", "detail": refusal.to_string()}),
    }
}

/// Generic, owner-neutral view of any opened canonical container.
///
/// This is the one source of the `n4a-inspect` report body: storage identity,
/// the exact typed manifest, report-only owner semantics and the optional native
/// training view. It loads no runtime, deserializes no host payload and grants
/// no operation, so `executability` is always `not_evaluated`. Unknown owners
/// stay `opaque`. On a file handle, only member bytes the owner reports need are
/// read, each verified by that read; a failed read is a reported refusal.
pub fn n4a_container_view<C: N4aMemberBytes>(container: &C) -> Value {
    storage_view(
        container.metadata(),
        n4a_semantic_report(container),
        n4a_training_view_report(container),
    )
}

/// The view of a host that retains its own source and scanned only the metadata.
/// No member bytes were read, so owner reports are honestly `not_evaluated`.
pub fn n4a_metadata_view(metadata: &N4aMetadata) -> Value {
    let unread = json!({
        "status": "not_evaluated",
        "detail": "member bytes of this external source were not read",
    });
    storage_view(metadata, unread.clone(), unread)
}

fn storage_view(metadata: &N4aMetadata, semantics: Value, training_view: Value) -> Value {
    let inspection = metadata.inspect();
    json!({
        "readability": {
            "storage": "verified",
            "container_sha256": inspection.receipt.container_sha256,
            "manifest_sha256": inspection.receipt.manifest_sha256,
            "byte_length": inspection.receipt.byte_length,
            "manifest": inspection.manifest,
            "required_features": inspection.required_features,
            "semantics": semantics,
            "training_view": training_view,
        },
        "executability": "not_evaluated",
    })
}

/// Closed bounded profile of hosts executing outside the native runtime. This
/// is a policy of the `*_portable` entrypoints only, keyed on roles and fitted
/// state capabilities rather than on method names.
fn refuse_outside_portable_recipe(recipe: &NativePipelineRecipe) -> ActionResult<()> {
    if let Some(last) = recipe.steps.last() {
        if last.role != "regressor" {
            return Err(unsupported("portable profile requires a final regressor"));
        }
        if last.params.contains_key("native_target_index") {
            return Err(unsupported(
                "portable profile refuses native_target_index selection",
            ));
        }
    }
    Ok(())
}

fn refuse_outside_portable_states(package: &PortablePredictorPackage) -> ActionResult<()> {
    let retains_rows = package
        .execution_bundle
        .refit_artifacts
        .iter()
        .filter_map(|record| record.artifact.native_estimator_descriptor.as_ref())
        .any(|descriptor| {
            descriptor
                .capabilities
                .iter()
                .any(|capability| capability == "retains_training_rows")
        });
    if retains_rows {
        return Err(unsupported(
            "portable profile refuses training-row state retention",
        ));
    }
    Ok(())
}

/// One prediction selected by a host without a native runtime. `record: None`
/// selects the stored input and `Some` exact external IO bytes.
pub struct PortablePredictRequest<'a> {
    pub entrypoint: &'a str,
    pub record: Option<&'a [u8]>,
    pub source_id: &'a str,
    pub partition: &'a str,
    pub require_training_schema: bool,
    pub request_id: &'a str,
    pub memory_budget: N4aMemoryBudget,
}

/// Read-only replay inputs. It has no target field: nothing downstream of
/// prediction can reach Y. `x` is row-major in `sample_ids` order, which is IO's
/// physical record order of the selected partition; replay keeps its own order,
/// so a host joins by sample ID.
pub struct PortablePredictPlan {
    pub package_json: String,
    pub request_json: String,
    pub envelopes_json: String,
    pub receipt_json: String,
    pub sample_ids: Vec<String>,
    pub target_names: Vec<String>,
    pub rows: usize,
    pub cols: usize,
    pub x: Vec<f64>,
}

struct PortableCohort {
    envelopes: BTreeMap<String, ExternalDataPlanEnvelope>,
}
impl FacetReplayCohort for PortableCohort {
    fn envelopes(&self) -> &BTreeMap<String, ExternalDataPlanEnvelope> {
        &self.envelopes
    }
}

/// Every static owner, IO and schema check of native prediction, then owner
/// admission, without loading anything. The admission value stays here: the
/// executing DAG runtime re-admits the returned strings before any callback.
pub fn prepare_predict_portable(
    archive_bytes: &[u8],
    request: PortablePredictRequest<'_>,
) -> ActionResult<PortablePredictPlan> {
    let container = open_portable(archive_bytes, request.memory_budget)?;
    let Prepared {
        snapshot,
        root_id,
        binding_id,
        cohort_key,
        cohort,
        receipt,
        sample_ids,
        ..
    } = prepare_action(
        &container,
        &ActionInput {
            entrypoint: request.entrypoint,
            input: match request.record {
                Some(bytes) => NativeInput::External(bytes),
                None => NativeInput::Stored,
            },
            source_id: request.source_id,
            partition: request.partition,
            schema_policy: if request.require_training_schema {
                NativeSchemaPolicy::RequireTrainingSchema
            } else {
                NativeSchemaPolicy::AllowUnavailable
            },
            memory_budget: request.memory_budget,
        },
        PrepareMode::Predict,
    )?;
    let proof = snapshot
        .context
        .owner_proofs
        .get(&root_id)
        .ok_or_else(|| mismatch("selected owner proof absent"))?;
    let package = proof
        .predictor_package()
        .ok_or_else(|| unsupported("predictor required"))?;
    let recipe = package
        .template
        .campaign
        .metadata
        .get("native_pipeline_recipe")
        .ok_or_else(|| unsupported("canonical linear pipeline recipe required"))?;
    refuse_outside_portable_recipe(
        &serde_json::from_value(recipe.clone()).map_err(FacetRefusal::from)?,
    )?;
    refuse_outside_portable_states(package)?;
    let replay_request = predict_replay_request(package, &binding_id, request.request_id)?;
    let request_json = serde_json::to_string(&replay_request).map_err(FacetRefusal::from)?;
    let envelopes = BTreeMap::from([(cohort_key, receipt.envelope.clone())]);
    let envelopes_json = serde_json::to_string(&envelopes).map_err(FacetRefusal::from)?;
    admit_facet_predict(
        &snapshot.graph,
        &snapshot.context,
        &snapshot.members,
        &root_id,
        FacetPredictRequest::Replay(replay_request),
        PortableCohort { envelopes },
    )?;
    Ok(PortablePredictPlan {
        package_json: std::str::from_utf8(proof.original_bytes())
            .map_err(|e| NativeActionError::Record(e.to_string()))?
            .to_string(),
        request_json,
        envelopes_json,
        receipt_json: serde_json::to_string(&receipt).map_err(FacetRefusal::from)?,
        sample_ids,
        target_names: cohort.target_names,
        rows: cohort.x.rows,
        cols: cohort.x.cols,
        x: cohort.x.values,
    })
}

/// One planned fit: everything an external DAG runtime executes, and what the
/// seal needs to bind its result. Rows of `x` and `y` follow `sample_ids`, IO's
/// physical record order; a host joins them to scheduler views by sample ID.
pub struct PortableTrainPlan {
    record: Vec<u8>,
    request: TrainingRequest,
    request_json: String,
    envelopes_json: String,
    relations_json: String,
    source_id: String,
    run_id: String,
    package_id: String,
    outcome_id: String,
    bundle_id: String,
    sample_ids: Vec<String>,
    target_names: Vec<String>,
    x: MethodsPlsMatrix,
    y: MethodsPlsMatrix,
}

/// Sealed bytes with the view of the same fit, in `inspect_n4a_view_json` shape.
pub struct PortableSealed {
    pub bytes: Vec<u8>,
    pub view_json: String,
}

impl PortableTrainPlan {
    pub fn source_id(&self) -> &str {
        &self.source_id
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn package_id(&self) -> &str {
        &self.package_id
    }
    pub fn outcome_id(&self) -> &str {
        &self.outcome_id
    }
    pub fn bundle_id(&self) -> &str {
        &self.bundle_id
    }
    pub fn request_json(&self) -> &str {
        &self.request_json
    }
    pub fn envelopes_json(&self) -> &str {
        &self.envelopes_json
    }
    pub fn relations_json(&self) -> &str {
        &self.relations_json
    }
    pub fn sample_ids(&self) -> &[String] {
        &self.sample_ids
    }
    pub fn target_names(&self) -> &[String] {
        &self.target_names
    }
    pub fn rows(&self) -> usize {
        self.x.rows
    }
    pub fn cols(&self) -> usize {
        self.x.cols
    }
    pub fn x(&self) -> &[f64] {
        &self.x.values
    }
    pub fn y(&self) -> &[f64] {
        &self.y.values
    }

    /// Seal the outcome and package a host produced from this plan. They must
    /// carry this plan's request fingerprint and reserved metadata, satisfy the
    /// closed native profile and stay inside the portable profile.
    pub fn seal(
        &self,
        outcome_json: &str,
        package_json: &str,
        container_id: &str,
        embed_training_record: bool,
        budget: N4aMemoryBudget,
    ) -> ActionResult<PortableSealed> {
        let outcome = TrainingOutcome::from_json(outcome_json)?;
        let package = PortablePredictorPackage::from_json(package_json)?;
        check_training_retention(&self.request, &outcome, &package)
            .map_err(NativeActionError::Producer)?;
        refuse_outside_portable_states(&package)?;
        let recipe = package
            .template
            .campaign
            .metadata
            .get("native_pipeline_recipe")
            .ok_or_else(|| unsupported("canonical linear pipeline recipe required"))?;
        validate_closed_native_profile(&package, &outcome, recipe, &self.source_id)?;
        let facets = build_training_artifact_facets_from_bytes(
            outcome_json.as_bytes(),
            package_json.as_bytes(),
        )?;
        let value = decode_record(&self.record, budget)?;
        let production = seal_training_facets(TrainingSeal {
            record: &self.record,
            value: &value,
            facets,
            training: N4aTrainingView {
                outcome,
                source_id: self.source_id.clone(),
            },
            container_id,
            embed_training_record,
            memory_budget: budget,
        })?;
        let view_json = n4a_view_json(&production.container, &production.training)
            .map_err(NativeActionError::Record)?;
        Ok(PortableSealed {
            bytes: production.container.into_archive_bytes().into_vec(),
            view_json,
        })
    }
}

fn plan_portable_training(
    record: &[u8],
    source_id: &str,
    recipe: &NativePipelineRecipe,
    manifests_json: &str,
    run_id: &str,
    budget: N4aMemoryBudget,
) -> ActionResult<PortableTrainPlan> {
    refuse_outside_portable_recipe(recipe)?;
    let value = decode_record(record, budget)?;
    normalized_record(&value)?;
    let manifests: Vec<ControllerManifest> =
        serde_json::from_value(decode_record(manifests_json.as_bytes(), budget)?)
            .map_err(FacetRefusal::from)?;
    let producer =
        |detail| NativeActionError::Producer(NativePipelineProducerError::Training(detail));
    let (dataset, request) =
        canonical_export_training_request(&value, source_id, recipe, |dataset| {
            dag_ml_core::native_pipeline_training_request(
                &native_linear_facts(dataset, source_id, recipe)?,
                recipe,
                &manifests,
            )
        })
        .map_err(producer)?;
    let provider = DatasetPackageMethodsProvider::for_product_pipeline(&dataset, source_id, false)
        .map_err(producer)?;
    provider.require_complete_targets().map_err(producer)?;
    let sample_ids = training_input_sample_ids(&dataset, provider.relations())
        .map_err(producer)?
        .unwrap_or_else(|| {
            provider
                .relations()
                .records
                .iter()
                .map(|record| record.sample_id.to_string())
                .collect()
        });
    let matrices = provider
        .training_matrices(run_id, &sample_ids)
        .map_err(producer)?;
    let [identity] = request.data_identities.as_slice() else {
        return Err(unsupported("exactly one native data requirement"));
    };
    let envelopes = BTreeMap::from([(
        identity.requirement_key.clone(),
        provider.external_envelope().clone(),
    )]);
    Ok(PortableTrainPlan {
        record: try_copy(record)?,
        request_json: serde_json::to_string(&request).map_err(FacetRefusal::from)?,
        envelopes_json: serde_json::to_string(&envelopes).map_err(FacetRefusal::from)?,
        relations_json: serde_json::to_string(provider.relations()).map_err(FacetRefusal::from)?,
        source_id: source_id.to_string(),
        run_id: run_id.to_string(),
        package_id: format!("package:{run_id}"),
        outcome_id: format!("outcome:{run_id}"),
        bundle_id: format!("bundle:{run_id}"),
        sample_ids,
        target_names: provider.provider.target_names().to_vec(),
        x: matrices.x,
        y: matrices.y,
        request,
    })
}

/// Plan one fit from exact IO record bytes and a recipe. `manifests_json` are
/// the controller manifests the executing host trusts; the request embeds
/// exactly these, so a host running another catalog is refused by DAG.
pub fn prepare_train_portable(
    record: &[u8],
    source_id: &str,
    recipe_json: &str,
    manifests_json: &str,
    run_id: &str,
    budget: N4aMemoryBudget,
) -> ActionResult<PortableTrainPlan> {
    let recipe = parse_native_recipe_json(recipe_json, budget)?;
    plan_portable_training(record, source_id, &recipe, manifests_json, run_id, budget)
}

/// Phase one of a retrain: the stored recipe and source ID, read once from the
/// validated archive. The host obtains its trusted controller manifests only
/// afterwards, then calls [`PortableRetrainFacts::plan`]. No caller supplies a
/// recipe or a source ID, and the archive is neither reopened nor revalidated.
pub struct PortableRetrainFacts {
    source_id: String,
    recipe: NativePipelineRecipe,
}

impl PortableRetrainFacts {
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Phase two: plan the retrain with the host's trusted manifests.
    pub fn plan(
        &self,
        record: &[u8],
        manifests_json: &str,
        run_id: &str,
        budget: N4aMemoryBudget,
    ) -> ActionResult<PortableTrainPlan> {
        plan_portable_training(
            record,
            &self.source_id,
            &self.recipe,
            manifests_json,
            run_id,
            budget,
        )
    }
}

/// Validate the archive once and capture its stored training facts. Anything
/// outside the closed native-produced model profile is refused here.
pub fn read_retrain_facts(
    archive_bytes: &[u8],
    budget: N4aMemoryBudget,
) -> ActionResult<PortableRetrainFacts> {
    let container = open_portable(archive_bytes, budget)?;
    let stored = read_stored_training(&container, container.memory_budget())?;
    Ok(PortableRetrainFacts {
        source_id: stored.view.source_id,
        recipe: stored.recipe,
    })
}

/// Plan a retrain: recipe and source come from the validated archive only.
pub fn prepare_retrain_portable(
    archive_bytes: &[u8],
    record: &[u8],
    manifests_json: &str,
    run_id: &str,
    budget: N4aMemoryBudget,
) -> ActionResult<PortableTrainPlan> {
    read_retrain_facts(archive_bytes, budget)?.plan(record, manifests_json, run_id, budget)
}
