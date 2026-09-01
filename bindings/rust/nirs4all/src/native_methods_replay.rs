//! Core-owned Archive V2 to DAG-ML Methods replay composition.
//!
//! Archive parsing and integrity remain Core-owned. DAG-ML remains the sole
//! owner of package semantics, scheduling, N4MM hydration and conformal
//! intervals.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use dag_ml_core::training::PredictionSource;
use dag_ml_core::{
    build_conformal_presentation_v1, deserialize_external_contract,
    execute_loaded_methods_portable_refit_replay_v3, execute_loaded_methods_predictor_replay,
    methods_pls_predict_feature_content_fingerprint, ConformalPresentationV1,
    ExternalDataPlanEnvelope, MethodsPlsDataset, MethodsPlsMatrix,
    MethodsPortablePredictorReplayInput, MethodsPortableRefitReplayInputV3, MethodsRuntime,
    ObservationId, Phase, PortablePredictorPackage, PortableRefitPackageV3,
    PortableRefitReplayOutcomeV3, PredictionKind, PredictionPartition, RunId,
    RuntimeControllerRegistry, SampleId, SampleRelation, SampleRelationSet, TrainingReplayOutcome,
    TrainingReplayRequest, EXTERNAL_DATA_PLAN_ENVELOPE_SCHEMA_VERSION_V1,
    TRAINING_REPLAY_REQUEST_SCHEMA_VERSION,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{load_archive_v2, load_archive_v3, LoadedArchiveV2, LoadedArchiveV3};

/// Typed current-cohort input accepted by the callback-free DAG-ML replay.
pub struct MethodsArchivePredictRequest {
    pub request: TrainingReplayRequest,
    pub data_envelopes: BTreeMap<String, ExternalDataPlanEnvelope>,
    pub methods_inputs: BTreeMap<String, MethodsPlsDataset>,
    pub methods_library_path: PathBuf,
    pub outcome_id: String,
    pub run_id: RunId,
    pub warnings: Vec<String>,
    pub diagnostics: BTreeMap<String, serde_json::Value>,
}

/// Closed product input for one X-only, Methods-backed Archive V2 prediction.
///
/// Core derives the signed DAG-ML replay request, relation authority, external
/// data envelopes and Methods datasets from these host values. Callers cannot
/// inject a fit/refit phase, target values, controller callbacks, artifact
/// handles or a fallback engine.
pub struct MethodsArchiveMatrixPredictRequest {
    pub sample_ids: Vec<String>,
    pub x: Vec<Vec<f64>>,
    pub expected_target_names: Vec<String>,
    pub methods_library_path: PathBuf,
    pub methods_library_sha256: String,
    pub request_id: String,
    pub outcome_id: String,
    pub run_id: RunId,
    pub warnings: Vec<String>,
    pub diagnostics: BTreeMap<String, serde_json::Value>,
}

struct MethodsArchiveMatrixPredictComposition {
    input: MethodsArchivePredictRequest,
    sample_ids: Vec<SampleId>,
    target_names: Vec<String>,
    output_binding_id: String,
}

/// Typed current-cohort input for a target-bound Archive V3 full-refit replay.
///
/// `supplemental_controllers` is deliberately invocation-local and owns only
/// the non-Methods controllers declared by the persisted V3 plan. Core never
/// interprets, hydrates, or caches model artifacts itself: DAG-ML owns all
/// package validation, scheduler execution, and Methods N4MM lifecycle.
pub struct MethodsArchiveRefitRequestV3 {
    pub request: TrainingReplayRequest,
    pub data_envelopes: BTreeMap<String, ExternalDataPlanEnvelope>,
    pub methods_inputs: BTreeMap<String, MethodsPlsDataset>,
    pub methods_library_path: PathBuf,
    pub supplemental_controllers: RuntimeControllerRegistry,
    pub outcome_id: String,
    pub run_id: RunId,
    pub warnings: Vec<String>,
    pub diagnostics: BTreeMap<String, serde_json::Value>,
}

/// JSON inputs accepted by host-language bindings for callback-free replay.
///
/// The JSON documents remain DAG-ML contracts. Core only performs the strict
/// host-boundary conversion needed to call the typed aggregate entry point.
/// Archive parsing still happens first, and neither callbacks nor serialized
/// Python model handles are part of this surface.
pub struct MethodsArchiveReplayJsonRequest {
    pub request_json: String,
    pub data_envelopes_json: String,
    pub methods_inputs_json: String,
    pub methods_library_path: PathBuf,
    pub outcome_id: String,
    pub run_id: String,
    pub warnings_json: String,
    pub diagnostics_json: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MethodsDatasetJson {
    sample_ids: Vec<String>,
    x: Vec<Vec<f64>>,
    #[serde(default)]
    y: Option<Vec<Vec<f64>>>,
    target_names: Vec<String>,
}

/// Fail-closed error at the aggregate composition boundary.
#[derive(Debug)]
pub struct NativeMethodsReplayError(String);

impl fmt::Display for NativeMethodsReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for NativeMethodsReplayError {}

fn replay_error(detail: impl std::fmt::Display) -> NativeMethodsReplayError {
    NativeMethodsReplayError(detail.to_string())
}

fn parse_contract<T>(json: &str, label: &str) -> Result<T, NativeMethodsReplayError>
where
    T: DeserializeOwned + Serialize,
{
    deserialize_external_contract(json, label, |detail| {
        dag_ml_core::DagMlError::CampaignValidation(detail)
    })
    .map_err(|error| replay_error(format!("DAG-ML rejected {label}: {error}")))
}

fn matrix_from_rows(
    rows: Vec<Vec<f64>>,
    label: &str,
) -> Result<MethodsPlsMatrix, NativeMethodsReplayError> {
    let row_count = rows.len();
    let columns = rows.first().map(Vec::len).unwrap_or(0);
    if row_count == 0 || columns == 0 || rows.iter().any(|row| row.len() != columns) {
        return Err(replay_error(format!(
            "DAG-ML rejected {label}: expected a non-empty rectangular matrix"
        )));
    }
    Ok(MethodsPlsMatrix {
        values: rows.into_iter().flatten().collect(),
        rows: row_count,
        cols: columns,
    })
}

fn methods_dataset_from_json(
    input: MethodsDatasetJson,
    label: &str,
) -> Result<MethodsPlsDataset, NativeMethodsReplayError> {
    let sample_ids = input
        .sample_ids
        .into_iter()
        .map(SampleId::new)
        .collect::<dag_ml_core::Result<Vec<_>>>()
        .map_err(|error| replay_error(format!("DAG-ML rejected {label}: {error}")))?;
    let dataset = MethodsPlsDataset {
        sample_ids,
        x: matrix_from_rows(input.x, &format!("{label}.x"))?,
        y: input
            .y
            .map(|rows| matrix_from_rows(rows, &format!("{label}.y")))
            .transpose()?,
        target_names: input.target_names,
    };
    dataset
        .validate(label, false)
        .map_err(|error| replay_error(format!("DAG-ML rejected {label}: {error}")))?;
    Ok(dataset)
}

fn validate_methods_library_identity(
    path: &Path,
    expected_sha256: &str,
) -> Result<(), NativeMethodsReplayError> {
    if !path.is_absolute() {
        return Err(replay_error("libn4m path must be absolute"));
    }
    if expected_sha256.len() != 64
        || !expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(replay_error(
            "libn4m SHA-256 identity must be 64 lowercase hexadecimal characters",
        ));
    }
    let link_metadata = std::fs::symlink_metadata(path)
        .map_err(|error| replay_error(format!("cannot inspect libn4m identity: {error}")))?;
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return Err(replay_error(
            "libn4m identity must name a regular non-symlink file",
        ));
    }
    let mut file = File::open(path)
        .map_err(|error| replay_error(format!("cannot open attested libn4m: {error}")))?;
    let opened_metadata = file
        .metadata()
        .map_err(|error| replay_error(format!("cannot inspect opened libn4m: {error}")))?;
    if !opened_metadata.is_file() || opened_metadata.len() != link_metadata.len() {
        return Err(replay_error(
            "libn4m identity changed while opening the attested file",
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| replay_error(format!("cannot hash attested libn4m: {error}")))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected_sha256 {
        return Err(replay_error(format!(
            "libn4m SHA-256 identity mismatch: expected {expected_sha256}, got {actual}"
        )));
    }
    Ok(())
}

fn compose_methods_archive_matrix_predict(
    package: &PortablePredictorPackage,
    input: MethodsArchiveMatrixPredictRequest,
) -> Result<MethodsArchiveMatrixPredictComposition, NativeMethodsReplayError> {
    package.validate().map_err(|error| {
        replay_error(format!("DAG-ML rejected Core Archive V2 package: {error}"))
    })?;
    let [binding] = package.output_bindings.as_slice() else {
        return Err(replay_error(
            "Archive V2 matrix prediction requires exactly one output binding",
        ));
    };
    if binding.prediction_source != PredictionSource::FinalRefit
        || binding.prediction_kind != PredictionKind::RegressionPoint
    {
        return Err(replay_error(
            "Archive V2 matrix prediction requires one final-refit regression-point binding",
        ));
    }
    if input.expected_target_names != binding.target_names {
        return Err(replay_error(format!(
            "Archive V2 target order mismatch: expected {:?}, package binds {:?}",
            input.expected_target_names, binding.target_names
        )));
    }

    let sample_ids = input
        .sample_ids
        .into_iter()
        .map(SampleId::new)
        .collect::<dag_ml_core::Result<Vec<_>>>()
        .map_err(|error| replay_error(format!("DAG-ML rejected prediction sample ids: {error}")))?;
    if sample_ids.len() != sample_ids.iter().collect::<BTreeSet<_>>().len() {
        return Err(replay_error(
            "Archive V2 matrix prediction sample ids must be unique",
        ));
    }
    let x = matrix_from_rows(input.x, "Archive V2 matrix prediction X")?;
    if x.rows != sample_ids.len() {
        return Err(replay_error(format!(
            "Archive V2 matrix prediction has {} sample ids for {} X rows",
            sample_ids.len(),
            x.rows
        )));
    }
    x.validate("Archive V2 matrix prediction X")
        .map_err(|error| replay_error(format!("DAG-ML rejected prediction X: {error}")))?;

    let relations = SampleRelationSet {
        records: sample_ids
            .iter()
            .map(|sample_id| {
                let observation_id = ObservationId::new(sample_id.as_str()).map_err(|error| {
                    replay_error(format!(
                        "DAG-ML rejected prediction observation id: {error}"
                    ))
                })?;
                Ok(SampleRelation::new(observation_id, sample_id.clone()))
            })
            .collect::<Result<Vec<_>, NativeMethodsReplayError>>()?,
    };
    relations.validate().map_err(|error| {
        replay_error(format!(
            "DAG-ML rejected prediction relation authority: {error}"
        ))
    })?;
    let relation_fingerprint = relations.fingerprint().map_err(|error| {
        replay_error(format!(
            "DAG-ML could not fingerprint prediction relations: {error}"
        ))
    })?;
    let data_content_fingerprint =
        methods_pls_predict_feature_content_fingerprint(&x).map_err(|error| {
            replay_error(format!(
                "DAG-ML could not fingerprint prediction X: {error}"
            ))
        })?;

    if package.execution_bundle.data_requirements.is_empty() {
        return Err(replay_error(
            "Archive V2 matrix prediction requires at least one external data requirement",
        ));
    }
    let mut data_envelopes = BTreeMap::new();
    let mut methods_inputs = BTreeMap::new();
    for requirement in &package.execution_bundle.data_requirements {
        requirement.validate().map_err(|error| {
            replay_error(format!(
                "DAG-ML rejected Archive V2 data requirement: {error}"
            ))
        })?;
        if requirement.output_representation != "tabular_numeric" {
            return Err(replay_error(format!(
                "Archive V2 matrix prediction does not support requirement `{}` representation `{}`",
                requirement.key(),
                requirement.output_representation
            )));
        }
        let key = requirement.key();
        let envelope = ExternalDataPlanEnvelope {
            schema_version: EXTERNAL_DATA_PLAN_ENVELOPE_SCHEMA_VERSION_V1,
            schema_fingerprint: requirement.schema_fingerprint.clone(),
            plan_fingerprint: requirement.plan_fingerprint.clone(),
            relation_fingerprint: Some(relation_fingerprint.clone()),
            data_content_fingerprint: Some(data_content_fingerprint.clone()),
            target_content_fingerprint: None,
            coordinator_relations: Some(relations.clone()),
            predict_cohort: None,
        };
        envelope.validate().map_err(|error| {
            replay_error(format!(
                "DAG-ML rejected derived prediction envelope `{key}`: {error}"
            ))
        })?;
        let dataset = MethodsPlsDataset {
            sample_ids: sample_ids.clone(),
            x: x.clone(),
            y: None,
            target_names: binding.target_names.clone(),
        };
        dataset
            .validate(
                &format!("Archive V2 matrix prediction input `{key}`"),
                false,
            )
            .map_err(|error| {
                replay_error(format!("DAG-ML rejected derived Methods input: {error}"))
            })?;
        if data_envelopes.insert(key.clone(), envelope).is_some()
            || methods_inputs.insert(key.clone(), dataset).is_some()
        {
            return Err(replay_error(format!(
                "Archive V2 has ambiguous data requirement key `{key}`"
            )));
        }
    }

    let mut request = TrainingReplayRequest {
        schema_version: TRAINING_REPLAY_REQUEST_SCHEMA_VERSION,
        request_id: input.request_id,
        source_outcome_fingerprint: package.training_outcome.outcome_fingerprint.clone(),
        phase: Phase::Predict,
        data_envelope_keys: data_envelopes.keys().cloned().collect(),
        output_binding_ids: vec![binding.binding_id.clone()],
        request_fingerprint: String::new(),
    };
    request.request_fingerprint = request.compute_fingerprint().map_err(|error| {
        replay_error(format!(
            "DAG-ML could not sign the derived replay request: {error}"
        ))
    })?;
    request.validate().map_err(|error| {
        replay_error(format!("DAG-ML rejected derived replay request: {error}"))
    })?;

    Ok(MethodsArchiveMatrixPredictComposition {
        input: MethodsArchivePredictRequest {
            request,
            data_envelopes,
            methods_inputs,
            methods_library_path: input.methods_library_path,
            outcome_id: input.outcome_id,
            run_id: input.run_id,
            warnings: input.warnings,
            diagnostics: input.diagnostics,
        },
        sample_ids,
        target_names: binding.target_names.clone(),
        output_binding_id: binding.binding_id.clone(),
    })
}

fn validate_methods_archive_matrix_outcome(
    outcome: &TrainingReplayOutcome,
    sample_ids: &[SampleId],
    target_names: &[String],
    output_binding_id: &str,
) -> Result<(), NativeMethodsReplayError> {
    outcome.validate().map_err(|error| {
        replay_error(format!(
            "DAG-ML returned an invalid replay outcome: {error}"
        ))
    })?;
    let [output] = outcome.outputs.as_slice() else {
        return Err(replay_error(
            "Archive V2 matrix prediction returned an ambiguous output set",
        ));
    };
    if output.binding.binding_id != output_binding_id || output.binding.target_names != target_names
    {
        return Err(replay_error(
            "Archive V2 matrix prediction output binding or target order changed",
        ));
    }
    let [prediction] = output.predictions.as_slice() else {
        return Err(replay_error(
            "Archive V2 matrix prediction requires exactly one terminal prediction block",
        ));
    };
    if prediction.partition != PredictionPartition::Final
        || prediction.sample_ids != sample_ids
        || prediction.target_names != target_names
    {
        return Err(replay_error(
            "Archive V2 matrix prediction changed terminal partition, sample order or target order",
        ));
    }
    if prediction.values.len() != sample_ids.len()
        || prediction
            .values
            .iter()
            .any(|row| row.len() != target_names.len())
        || prediction
            .values
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return Err(replay_error(
            "Archive V2 matrix prediction returned an invalid or non-finite result matrix",
        ));
    }
    Ok(())
}

fn parse_json_request(
    input: MethodsArchiveReplayJsonRequest,
) -> Result<MethodsArchivePredictRequest, NativeMethodsReplayError> {
    let request = TrainingReplayRequest::from_json(&input.request_json)
        .map_err(|error| replay_error(format!("DAG-ML rejected replay request: {error}")))?;
    let data_envelopes = parse_contract::<BTreeMap<String, ExternalDataPlanEnvelope>>(
        &input.data_envelopes_json,
        "Methods replay data envelope map",
    )?;
    for (key, envelope) in &data_envelopes {
        envelope.validate().map_err(|error| {
            replay_error(format!(
                "DAG-ML rejected Methods replay data envelope `{key}`: {error}"
            ))
        })?;
    }
    let raw_inputs = parse_contract::<BTreeMap<String, MethodsDatasetJson>>(
        &input.methods_inputs_json,
        "Methods replay input map",
    )?;
    let methods_inputs = raw_inputs
        .into_iter()
        .map(|(key, dataset)| {
            let label = format!("Methods replay input `{key}`");
            Ok((key, methods_dataset_from_json(dataset, &label)?))
        })
        .collect::<Result<BTreeMap<_, _>, NativeMethodsReplayError>>()?;
    let warnings = parse_contract::<Vec<String>>(&input.warnings_json, "Methods replay warnings")?;
    let diagnostics = parse_contract::<BTreeMap<String, serde_json::Value>>(
        &input.diagnostics_json,
        "Methods replay diagnostics",
    )?;
    let run_id = RunId::new(&input.run_id)
        .map_err(|error| replay_error(format!("DAG-ML rejected replay run_id: {error}")))?;
    Ok(MethodsArchivePredictRequest {
        request,
        data_envelopes,
        methods_inputs,
        methods_library_path: input.methods_library_path,
        outcome_id: input.outcome_id,
        run_id,
        warnings,
        diagnostics,
    })
}

/// Replay a Core-validated Archive V2 through the registered Methods runtime.
///
/// The archive is opened before this function is called, so malformed ZIPs,
/// inventory drift and untrusted members are refused by Core before DAG-ML sees
/// package bytes. No Python callback, model handle or estimator is accepted.
pub fn replay_methods_archive_v2(
    archive: &LoadedArchiveV2,
    input: MethodsArchivePredictRequest,
) -> Result<TrainingReplayOutcome, NativeMethodsReplayError> {
    let package = load_v2_predictor_package(archive)?;
    replay_methods_predictor_package(&package, input)
}

/// Execute one closed, X-only Archive V2 Methods prediction.
///
/// This is the product-host surface for Studio and other Rust orchestrators.
/// It derives every DAG-ML replay contract through upstream constructors,
/// attests the exact native Methods library, then configures the process-global
/// runtime only after package, binding, target, identity and matrix validation.
/// It never fits, refits, calls Python or selects a fallback implementation.
pub fn predict_methods_archive_v2_matrix(
    archive: &LoadedArchiveV2,
    input: MethodsArchiveMatrixPredictRequest,
) -> Result<TrainingReplayOutcome, NativeMethodsReplayError> {
    let package = load_v2_predictor_package(archive)?;
    let expected_library_sha256 = input.methods_library_sha256.clone();
    let MethodsArchiveMatrixPredictComposition {
        input,
        sample_ids,
        target_names,
        output_binding_id,
    } = compose_methods_archive_matrix_predict(&package, input)?;
    validate_methods_library_identity(&input.methods_library_path, &expected_library_sha256)?;
    let outcome = replay_methods_predictor_package(&package, input)?;
    validate_methods_archive_matrix_outcome(
        &outcome,
        &sample_ids,
        &target_names,
        &output_binding_id,
    )?;
    Ok(outcome)
}

/// Replay an integrity-checked Archive V2 and project its already-calculated
/// split-conformal intervals through DAG-ML's closed presentation contract.
///
/// Core keeps the validated package, signed request and resulting replay
/// together only for this call. DAG-ML validates their complete provenance,
/// binding, sample-order and interval closure before returning the projection;
/// Core neither calculates an interval nor selects a target.
pub fn replay_methods_archive_v2_conformal_presentation_v1(
    archive: &LoadedArchiveV2,
    input: MethodsArchivePredictRequest,
) -> Result<ConformalPresentationV1, NativeMethodsReplayError> {
    let package = load_v2_predictor_package(archive)?;
    let request = input.request.clone();
    let replay = replay_methods_predictor_package(&package, input)?;
    build_conformal_presentation_v1(&package, &request, &replay).map_err(|error| {
        replay_error(format!(
            "DAG-ML could not build Core Archive V2 conformal presentation: {error}"
        ))
    })
}

fn load_v2_predictor_package(
    archive: &LoadedArchiveV2,
) -> Result<PortablePredictorPackage, NativeMethodsReplayError> {
    let package_bytes = archive.portable_predictor_package().map_err(|error| {
        NativeMethodsReplayError(format!("Core Archive V2 package read failed: {error}"))
    })?;
    let package_json = std::str::from_utf8(package_bytes).map_err(|error| {
        NativeMethodsReplayError(format!(
            "Core Archive V2 package is not UTF-8 JSON: {error}"
        ))
    })?;
    let package = PortablePredictorPackage::from_json(package_json).map_err(|error| {
        NativeMethodsReplayError(format!("DAG-ML rejected Core Archive V2 package: {error}"))
    })?;
    Ok(package)
}

fn replay_methods_predictor_package(
    package: &PortablePredictorPackage,
    input: MethodsArchivePredictRequest,
) -> Result<TrainingReplayOutcome, NativeMethodsReplayError> {
    // Keep structural/package/request/input validation ahead of process-global
    // libn4m configuration. Cross-contract scheduling and native N4MM hydration
    // remain DAG-ML-owned below.
    package.validate().map_err(|error| {
        replay_error(format!("DAG-ML rejected Core Archive V2 package: {error}"))
    })?;
    input
        .request
        .validate()
        .map_err(|error| replay_error(format!("DAG-ML rejected replay request: {error}")))?;
    if input.request.phase != Phase::Predict {
        return Err(replay_error(
            "DAG-ML rejected replay request: callback-free Methods package replay supports PREDICT only",
        ));
    }
    for (key, dataset) in &input.methods_inputs {
        dataset
            .validate(&format!("native Methods replay input `{key}`"), false)
            .map_err(|error| replay_error(format!("DAG-ML rejected Methods input: {error}")))?;
    }
    let runtime = MethodsRuntime::configure(&input.methods_library_path).map_err(|error| {
        NativeMethodsReplayError(format!("cannot configure the Methods runtime: {error}"))
    })?;
    execute_loaded_methods_predictor_replay(MethodsPortablePredictorReplayInput {
        package,
        request: &input.request,
        data_envelopes: &input.data_envelopes,
        methods_inputs: &input.methods_inputs,
        runtime,
        outcome_id: input.outcome_id,
        run_id: input.run_id,
        warnings: input.warnings,
        diagnostics: input.diagnostics,
    })
    .map_err(|error| NativeMethodsReplayError(format!("DAG-ML Methods replay failed: {error}")))
}

/// Replay a Core-validated Archive V3 target-bound refit package.
///
/// V3 remains a DAG-ML package family: Core provides only integrity-checked
/// bytes and the caller's attested current cohort. The public DAG-ML entry
/// point validates the complete package then registers a fresh Methods runtime
/// for this invocation, so no process-local N4MM handle can survive the call.
pub fn replay_methods_archive_v3(
    archive: &LoadedArchiveV3,
    input: MethodsArchiveRefitRequestV3,
) -> Result<PortableRefitReplayOutcomeV3, NativeMethodsReplayError> {
    let package_bytes = archive.portable_refit_package().map_err(|error| {
        NativeMethodsReplayError(format!("Core Archive V3 package read failed: {error}"))
    })?;
    let package_json = std::str::from_utf8(package_bytes).map_err(|error| {
        NativeMethodsReplayError(format!(
            "Core Archive V3 package is not UTF-8 JSON: {error}"
        ))
    })?;
    let package = PortableRefitPackageV3::from_json(package_json).map_err(|error| {
        NativeMethodsReplayError(format!("DAG-ML rejected Core Archive V3 package: {error}"))
    })?;
    let runtime = MethodsRuntime::configure(&input.methods_library_path).map_err(|error| {
        NativeMethodsReplayError(format!("cannot configure the Methods runtime: {error}"))
    })?;
    execute_loaded_methods_portable_refit_replay_v3(MethodsPortableRefitReplayInputV3 {
        package: &package,
        request: &input.request,
        data_envelopes: &input.data_envelopes,
        methods_inputs: &input.methods_inputs,
        runtime,
        supplemental_controllers: input.supplemental_controllers,
        outcome_id: input.outcome_id,
        run_id: input.run_id,
        warnings: input.warnings,
        diagnostics: input.diagnostics,
    })
    .map_err(|error| NativeMethodsReplayError(format!("DAG-ML Methods V3 replay failed: {error}")))
}

/// Open, validate, and replay an Archive V2 from strict host JSON contracts.
///
/// Archive validation is deliberately completed before any request parsing or
/// Methods runtime configuration. The returned JSON is the exact serialized
/// DAG-ML replay outcome.
pub fn replay_methods_archive_v2_json(
    archive_path: &Path,
    input: MethodsArchiveReplayJsonRequest,
) -> Result<String, NativeMethodsReplayError> {
    let archive = load_archive_v2(archive_path)
        .map_err(|error| replay_error(format!("Core Archive V2 validation refused: {error}")))?;
    let input = parse_json_request(input)?;
    let outcome = replay_methods_archive_v2(&archive, input)?;
    serde_json::to_string(&outcome).map_err(|error| {
        replay_error(format!(
            "cannot serialize DAG-ML V2 replay outcome: {error}"
        ))
    })
}

/// Open, validate and replay an Archive V2, returning DAG-ML's exact closed
/// conformal-presentation JSON for binding and Studio transport.
pub fn replay_methods_archive_v2_conformal_presentation_v1_json(
    archive_path: &Path,
    input: MethodsArchiveReplayJsonRequest,
) -> Result<String, NativeMethodsReplayError> {
    let archive = load_archive_v2(archive_path)
        .map_err(|error| replay_error(format!("Core Archive V2 validation refused: {error}")))?;
    let input = parse_json_request(input)?;
    let presentation = replay_methods_archive_v2_conformal_presentation_v1(&archive, input)?;
    serde_json::to_string(&presentation).map_err(|error| {
        replay_error(format!(
            "cannot serialize DAG-ML V2 conformal presentation: {error}"
        ))
    })
}

/// Open, validate, and replay an Archive V3 from strict host JSON contracts.
///
/// Python bindings intentionally get an empty supplemental controller registry:
/// the portable path is Methods-only and cannot hydrate a Python callback or a
/// joblib sidecar.
pub fn replay_methods_archive_v3_json(
    archive_path: &Path,
    input: MethodsArchiveReplayJsonRequest,
) -> Result<String, NativeMethodsReplayError> {
    let archive = load_archive_v3(archive_path)
        .map_err(|error| replay_error(format!("Core Archive V3 validation refused: {error}")))?;
    let input = parse_json_request(input)?;
    let outcome = replay_methods_archive_v3(
        &archive,
        MethodsArchiveRefitRequestV3 {
            request: input.request,
            data_envelopes: input.data_envelopes,
            methods_inputs: input.methods_inputs,
            methods_library_path: input.methods_library_path,
            supplemental_controllers: RuntimeControllerRegistry::new(),
            outcome_id: input.outcome_id,
            run_id: input.run_id,
            warnings: input.warnings,
            diagnostics: input.diagnostics,
        },
    )?;
    serde_json::to_string(&outcome).map_err(|error| {
        replay_error(format!(
            "cannot serialize DAG-ML V3 replay outcome: {error}"
        ))
    })
}

#[cfg(test)]
mod json_tests {
    use super::*;

    fn matrix_predict_request(
        methods_library_path: PathBuf,
        methods_library_sha256: String,
    ) -> MethodsArchiveMatrixPredictRequest {
        MethodsArchiveMatrixPredictRequest {
            sample_ids: vec!["predict.0".into(), "predict.1".into()],
            x: vec![vec![1.5, 0.5], vec![3.5, 1.5]],
            expected_target_names: vec!["protein".into(), "moisture".into()],
            methods_library_path,
            methods_library_sha256,
            request_id: "replay:nirs4all.rt-pred-001".into(),
            outcome_id: "outcome:nirs4all.rt-pred-001".into(),
            run_id: RunId::new("run:nirs4all.rt-pred-001").unwrap(),
            warnings: Vec::new(),
            diagnostics: BTreeMap::from([(
                "contract".into(),
                serde_json::Value::String("RT-PRED-001".into()),
            )]),
        }
    }

    fn invalid_json_input() -> MethodsArchiveReplayJsonRequest {
        MethodsArchiveReplayJsonRequest {
            request_json: "not-json".to_owned(),
            data_envelopes_json: "not-json".to_owned(),
            methods_inputs_json: "not-json".to_owned(),
            methods_library_path: PathBuf::from("/must-not-open-libn4m"),
            outcome_id: "outcome:must-not-run".to_owned(),
            run_id: "not a run id".to_owned(),
            warnings_json: "not-json".to_owned(),
            diagnostics_json: "not-json".to_owned(),
        }
    }

    #[test]
    fn json_replay_validates_archive_before_host_contracts() {
        let missing = std::env::temp_dir().join(format!(
            "nirs4all-core-missing-archive-{}-{}.n4a",
            std::process::id(),
            std::thread::current().name().unwrap_or("json-test")
        ));
        let v2 = replay_methods_archive_v2_json(&missing, invalid_json_input())
            .expect_err("missing V2 archive must be rejected first");
        assert!(v2
            .to_string()
            .starts_with("Core Archive V2 validation refused:"));
        let conformal = replay_methods_archive_v2_conformal_presentation_v1_json(
            &missing,
            invalid_json_input(),
        )
        .expect_err("missing conformal V2 archive must be rejected first");
        assert!(conformal
            .to_string()
            .starts_with("Core Archive V2 validation refused:"));
        let v3 = replay_methods_archive_v3_json(&missing, invalid_json_input())
            .expect_err("missing V3 archive must be rejected first");
        assert!(v3
            .to_string()
            .starts_with("Core Archive V3 validation refused:"));
    }

    #[test]
    fn methods_input_refuses_unknown_fields() {
        let error = parse_contract::<BTreeMap<String, MethodsDatasetJson>>(
            r#"{"input:predict":{"sample_ids":["sample:1"],"x":[[1.0]],"target_names":["y"],"artifact_handle":"python:model"}}"#,
            "Methods replay input map",
        )
        .expect_err("host artifact handles are not part of the portable input contract");
        assert!(error
            .to_string()
            .contains("unknown field `artifact_handle`"));
    }

    #[test]
    fn methods_input_refuses_ragged_matrix() {
        let raw = parse_contract::<BTreeMap<String, MethodsDatasetJson>>(
            r#"{"input:predict":{"sample_ids":["sample:1","sample:2"],"x":[[1.0],[2.0,3.0]],"target_names":["y"]}}"#,
            "Methods replay input map",
        )
        .expect("shape-valid JSON");
        let error = methods_dataset_from_json(
            raw.into_values().next().expect("one dataset"),
            "Methods replay input `input:predict`",
        )
        .expect_err("ragged matrices must fail before Methods runtime configuration");
        assert!(error.to_string().contains("non-empty rectangular matrix"));
    }

    #[test]
    #[ignore = "requires N4A_RT_PRED_ARCHIVE_V2, N4A_RT_PRED_METHODS_LIBRARY and N4A_RT_PRED_METHODS_SHA256"]
    fn real_multitarget_archive_matrix_product_contract() {
        let archive_path = PathBuf::from(
            std::env::var("N4A_RT_PRED_ARCHIVE_V2")
                .expect("N4A_RT_PRED_ARCHIVE_V2 must name the real multi-target witness"),
        );
        let methods_library_path = PathBuf::from(
            std::env::var("N4A_RT_PRED_METHODS_LIBRARY")
                .expect("N4A_RT_PRED_METHODS_LIBRARY must name the real libn4m"),
        );
        let methods_library_sha256 = std::env::var("N4A_RT_PRED_METHODS_SHA256")
            .expect("N4A_RT_PRED_METHODS_SHA256 must attest the real libn4m");
        let archive = load_archive_v2(&archive_path).expect("real Archive V2 witness validates");

        let mut wrong_targets =
            matrix_predict_request(methods_library_path.clone(), methods_library_sha256.clone());
        wrong_targets.expected_target_names.swap(0, 1);
        assert!(predict_methods_archive_v2_matrix(&archive, wrong_targets)
            .unwrap_err()
            .to_string()
            .contains("target order mismatch"));

        let mut duplicate_samples =
            matrix_predict_request(methods_library_path.clone(), methods_library_sha256.clone());
        duplicate_samples.sample_ids[1] = duplicate_samples.sample_ids[0].clone();
        assert!(
            predict_methods_archive_v2_matrix(&archive, duplicate_samples)
                .unwrap_err()
                .to_string()
                .contains("sample ids must be unique")
        );

        let mut ragged =
            matrix_predict_request(methods_library_path.clone(), methods_library_sha256.clone());
        ragged.x[1].pop();
        assert!(predict_methods_archive_v2_matrix(&archive, ragged)
            .unwrap_err()
            .to_string()
            .contains("non-empty rectangular matrix"));

        let mut non_finite =
            matrix_predict_request(methods_library_path.clone(), methods_library_sha256.clone());
        non_finite.x[0][0] = f64::NAN;
        assert!(predict_methods_archive_v2_matrix(&archive, non_finite)
            .unwrap_err()
            .to_string()
            .contains("non-finite"));

        let wrong_identity = matrix_predict_request(methods_library_path.clone(), "0".repeat(64));
        assert!(predict_methods_archive_v2_matrix(&archive, wrong_identity)
            .unwrap_err()
            .to_string()
            .contains("libn4m SHA-256 identity mismatch"));

        let outcome = predict_methods_archive_v2_matrix(
            &archive,
            matrix_predict_request(methods_library_path, methods_library_sha256),
        )
        .expect("real multi-target Archive V2 predicts without fallback");
        let output = outcome.outputs.first().expect("one output binding");
        let prediction = output.predictions.first().expect("one terminal prediction");
        assert_eq!(
            prediction.sample_ids,
            vec![
                SampleId::new("predict.0").unwrap(),
                SampleId::new("predict.1").unwrap(),
            ]
        );
        assert_eq!(prediction.target_names, ["protein", "moisture"]);
        let expected = [
            [1.636_363_636_363_636_5, 13.272_727_272_727_273],
            [2.499_999_999_999_999_6, 15.0],
        ];
        for (actual_row, expected_row) in prediction.values.iter().zip(expected) {
            for (actual, expected) in actual_row.iter().zip(expected_row) {
                assert!((actual - expected).abs() <= 1.0e-9);
            }
        }
    }
}
