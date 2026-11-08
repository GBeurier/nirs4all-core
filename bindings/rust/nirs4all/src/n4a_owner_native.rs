//! Native same-byte actions. IO owns decoding and materialization; DAG owns
//! admission, replay and calibration. No executable authority is deserialized.
//!
//! Runtime-free preparation and sealing live in `n4a_owner_portable`. This
//! module adds what needs the attested native Methods runtime: producing,
//! replaying, calibrating and retraining.
use std::collections::BTreeMap;
use std::path::Path;

use dag_ml_core::artifact_facets::*;
#[cfg(test)]
use dag_ml_core::{ArtifactLoadMode, SampleId};
use dag_ml_core::{
    ExternalDataPlanEnvelope, MethodsPlsDataset, RunId, SampleRelationSet, TrainingReplayOutcome,
};
use nirs4all_io_core::materialize::DatasetPackage;
#[cfg(test)]
use serde_json::json;

#[cfg(test)]
use crate::archive_codec::sha256;
use crate::conformal::validate_independent_record;
#[cfg(test)]
use crate::io_training::validate_disjoint_calibration_relations;
use crate::io_training::DatasetPackageMethodsProvider;
use crate::n4a::*;
use crate::n4a_owner_portable::*;
pub use crate::n4a_owner_portable::{
    parse_native_recipe_json, read_n4a_training_view, N4aTrainingView, NativeActionError,
    NativeActionReceipt, NativeBindingReceipt, NativeInput, NativeProduction, NativeSchemaPolicy,
    NATIVE_ACTION_FEATURES,
};
use crate::native_methods_replay::configure_attested_methods_runtime;
use crate::pipeline_workflow::{train_native_pipeline_for_export, NativePipelineRecipe};

/// Explicit runtime identity; evaluated only after all static checks.
#[derive(Clone, Copy)]
pub struct NativeRuntime<'a> {
    pub library: &'a Path,
    pub expected_sha256: &'a str,
}

pub struct NativeActionRequest<'a> {
    pub entrypoint: &'a str,
    pub input: NativeInput<'a>,
    pub source_id: &'a str,
    pub partition: &'a str,
    pub schema_policy: NativeSchemaPolicy,
    pub request_id: &'a str,
    pub run_id: &'a str,
    pub memory_budget: N4aMemoryBudget,
    pub runtime: NativeRuntime<'a>,
}

pub struct NativeCalibrationOptions {
    pub coverages: Vec<f64>,
    pub multi_target_policy: dag_ml_core::ConformalMultiTargetPolicy,
    pub small_sample_policy: dag_ml_core::ConformalSmallSamplePolicy,
}
impl Default for NativeCalibrationOptions {
    fn default() -> Self {
        Self {
            coverages: vec![0.95],
            multi_target_policy: dag_ml_core::ConformalMultiTargetPolicy::Marginal,
            small_sample_policy: dag_ml_core::ConformalSmallSamplePolicy::Error,
        }
    }
}

pub struct NativePrediction {
    pub replay: TrainingReplayOutcome,
    pub receipt: NativeActionReceipt,
    /// Host presentation axis, outside the signed replay: the sample IDs of
    /// the selected source and `request.partition`, exactly as IO emitted them
    /// in physical record order. The replay keeps the owner's own (sorted)
    /// order and is never reordered or rewritten. IO guarantees these IDs are
    /// unique within the partition and cover exactly its rows.
    pub sample_ids: Vec<String>,
}
pub struct NativeCalibration {
    pub container: OwnedN4a,
    pub replay: TrainingReplayOutcome,
    pub receipt: NativeActionReceipt,
}

/// Private consuming adapter. No caller can supply a provider, cohort, matrix
/// or envelope; these fields are built together from the retained IO package.
struct IoSnapshot {
    key: String,
    dataset: MethodsPlsDataset,
    envelope: ExternalDataPlanEnvelope,
}
impl MethodsFacetCohortOwner for IoSnapshot {
    fn into_bindings(self) -> FacetResult<BTreeMap<String, MethodsFacetBinding>> {
        Ok(BTreeMap::from([(
            self.key,
            MethodsFacetBinding {
                dataset: self.dataset,
                envelope: self.envelope,
            },
        )]))
    }
}

fn prepare(
    container: &OwnedN4a,
    request: &NativeActionRequest<'_>,
    calibrating: bool,
) -> ActionResult<Prepared> {
    prepare_action(
        container,
        &ActionInput {
            entrypoint: request.entrypoint,
            input: request.input,
            source_id: request.source_id,
            partition: request.partition,
            schema_policy: request.schema_policy,
            memory_budget: request.memory_budget,
        },
        if calibrating {
            PrepareMode::Calibrate(validate_independent_record)
        } else {
            PrepareMode::Predict
        },
    )
}

fn replay(
    prepared: Prepared,
    request: &NativeActionRequest<'_>,
) -> ActionResult<(PreparedReplay, TrainingReplayOutcome)> {
    let Prepared {
        snapshot,
        root_id,
        outcome_id,
        binding_id,
        package,
        relations,
        cohort_key,
        cohort,
        receipt,
        sample_ids,
    } = prepared;
    let cohort = freeze_methods_facet_cohort(IoSnapshot {
        key: cohort_key,
        envelope: receipt.envelope.clone(),
        dataset: cohort,
    })?;
    let predictor = snapshot.context.owner_proofs[&root_id]
        .predictor_package()
        .ok_or_else(|| unsupported("predictor required"))?;
    let replay_request = predict_replay_request(predictor, &binding_id, request.request_id)?;
    let run_id = RunId::new(request.run_id)?;
    let admitted = admit_facet_predict(
        &snapshot.graph,
        &snapshot.context,
        &snapshot.members,
        &root_id,
        FacetPredictRequest::Replay(replay_request),
        cohort,
    )?;
    // All owner/static preflight, schema, record and independence work precedes
    // attestation and runtime configuration; preserve the closed owner error.
    let runtime = configure_attested_methods_runtime(
        request.runtime.library,
        request.runtime.expected_sha256,
    )?;
    let result = execute_admitted_methods_facet_replay(
        admitted,
        runtime,
        format!("outcome:{}", request.request_id),
        run_id,
    )?;
    let FacetMethodsReplayOutcome::Predictor(result) = result else {
        return Err(unsupported("non-predictor native result"));
    };
    Ok((
        PreparedReplay {
            snapshot,
            root_id,
            outcome_id,
            binding_id,
            package,
            relations,
            receipt,
            sample_ids,
        },
        *result,
    ))
}
struct PreparedReplay {
    snapshot: ArtifactFacetExport,
    root_id: String,
    outcome_id: Option<String>,
    binding_id: String,
    package: DatasetPackage,
    relations: SampleRelationSet,
    receipt: NativeActionReceipt,
    sample_ids: Vec<String>,
}

pub fn predict_n4a_native(
    container: &OwnedN4a,
    request: NativeActionRequest<'_>,
) -> ActionResult<NativePrediction> {
    let prepared = prepare(container, &request, false)?;
    let (prepared, replay) = replay(prepared, &request)?;
    Ok(NativePrediction {
        replay,
        receipt: prepared.receipt,
        sample_ids: prepared.sample_ids,
    })
}

pub fn calibrate_n4a_native(
    container: &OwnedN4a,
    request: NativeActionRequest<'_>,
    options: NativeCalibrationOptions,
    container_id: &str,
) -> ActionResult<NativeCalibration> {
    dag_ml_core::validate_conformal_coverages(&options.coverages)
        .map_err(NativeActionError::Conformal)?;
    let prepared = prepare(container, &request, true)?;
    // Refuse unsupported carry BEFORE native execution, with a genuine owner
    // export as the baseline. Do not merge, rename or drop extra owner facets.
    let before = selected_training_export(
        &prepared.snapshot,
        &prepared.root_id,
        prepared
            .outcome_id
            .as_deref()
            .ok_or_else(|| mismatch("calibration outcome absent"))?,
    )?;
    validate_carry(container, &before)?;
    let (prepared, replay) = replay(prepared, &request)?;
    let truth = DatasetPackageMethodsProvider::new(&prepared.package, request.source_id)
        .map_err(NativeActionError::Record)?
        .conformal_truth_by_name(&replay, &prepared.binding_id)
        .map_err(NativeActionError::Record)?;
    let fresh = calibrate_training_artifact_facets(
        &prepared.snapshot.graph,
        &prepared.snapshot.context,
        &prepared.snapshot.members,
        prepared
            .outcome_id
            .as_deref()
            .ok_or_else(|| mismatch("outcome absent"))?,
        &prepared.root_id,
        FacetCalibrationRequest {
            replay: &replay,
            binding_id: &prepared.binding_id,
            relations: &prepared.relations,
            truth,
            coverages: options.coverages,
            multi_target_policy: options.multi_target_policy,
            small_sample_policy: options.small_sample_policy,
        },
    )?;
    let container = compose(
        Some(container),
        &before,
        fresh,
        container_id,
        request.memory_budget,
    )?;
    Ok(NativeCalibration {
        container,
        replay,
        receipt: prepared.receipt,
    })
}

fn selected_training_export(
    snapshot: &ArtifactFacetExport,
    model: &str,
    result: &str,
) -> ActionResult<ArtifactFacetExport> {
    let root = |id: &str| {
        snapshot
            .context
            .owner_proofs
            .get(id)
            .map(|p| p.original_bytes())
            .ok_or_else(|| mismatch("owner root proof absent"))
    };
    Ok(build_training_artifact_facets_from_bytes(
        root(result)?,
        root(model)?,
    )?)
}

pub struct NativeProduceRequest<'a> {
    pub record: &'a [u8],
    pub source_id: &'a str,
    pub recipe: &'a NativePipelineRecipe,
    pub runtime: NativeRuntime<'a>,
    pub run_id: &'a str,
    pub container_id: &'a str,
    pub embed_training_record: bool,
    pub memory_budget: N4aMemoryBudget,
}

/// Train once via DAG/Methods and seal its complete owner export. Only this
/// byte-facing producer adds the reserved schema/relation metadata. Historical
/// NativePipeline JSON transport continues through its existing request path.
pub fn produce_n4a_native(request: NativeProduceRequest<'_>) -> ActionResult<OwnedN4a> {
    Ok(produce_n4a_native_with_view(request)?.container)
}

/// Same single fit as [`produce_n4a_native`]. The training view is retained
/// from that fit's owner result; the sealed bytes are neither reopened nor
/// revalidated and nothing is trained or replayed a second time.
pub fn produce_n4a_native_with_view(
    request: NativeProduceRequest<'_>,
) -> ActionResult<NativeProduction> {
    let value = decode_record(request.record, request.memory_budget)?;
    normalized_record(&value)?;
    // Explicit identity attestation uses the same process snapshot as replay
    // and historical training. This does not add a second native loader.
    configure_attested_methods_runtime(request.runtime.library, request.runtime.expected_sha256)?;
    let produced = train_native_pipeline_for_export(
        &value,
        request.source_id,
        request.recipe,
        request.runtime.library,
        request.run_id,
    )
    .map_err(NativeActionError::Producer)?;
    seal_training_facets(TrainingSeal {
        record: request.record,
        value: &value,
        facets: produced.facets,
        training: N4aTrainingView {
            outcome: produced.training,
            source_id: request.source_id.to_string(),
        },
        container_id: request.container_id,
        embed_training_record: request.embed_training_record,
        memory_budget: request.memory_budget,
    })
}

/// New training bytes for a stored model. Recipe and source come from the
/// validated owner package of `container`, never from the caller.
pub struct NativeRetrainRequest<'a> {
    pub record: &'a [u8],
    pub runtime: NativeRuntime<'a>,
    pub run_id: &'a str,
    pub container_id: &'a str,
    pub embed_training_record: bool,
    pub memory_budget: N4aMemoryBudget,
}

/// Fit the stored recipe on `request.record` and seal a new archive, through
/// the producer and its native policy. `container` stays immutable; the result
/// is a new model, not a continuation of the stored fit.
pub fn retrain_n4a_native(
    container: &OwnedN4a,
    request: NativeRetrainRequest<'_>,
) -> ActionResult<NativeProduction> {
    let stored = read_stored_training(container, request.memory_budget)?;
    produce_n4a_native_with_view(NativeProduceRequest {
        record: request.record,
        source_id: &stored.view.source_id,
        recipe: &stored.recipe,
        runtime: request.runtime,
        run_id: request.run_id,
        container_id: request.container_id,
        embed_training_record: request.embed_training_record,
        memory_budget: request.memory_budget,
    })
}

#[cfg(test)]
#[path = "../tests/fixtures/n4a_native_actions_v1/authority_controls.rs"]
mod authority_controls;
