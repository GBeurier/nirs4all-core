//! Core-owned Archive V2 to DAG-ML Methods replay composition.
//!
//! Archive parsing and integrity remain Core-owned. DAG-ML remains the sole
//! owner of package semantics, scheduling, N4MM hydration and conformal
//! intervals.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use dag_ml_core::{
    deserialize_external_contract, execute_loaded_methods_portable_refit_replay_v3,
    execute_loaded_methods_predictor_replay, ExternalDataPlanEnvelope, MethodsPlsDataset,
    MethodsPlsMatrix, MethodsPortablePredictorReplayInput, MethodsPortableRefitReplayInputV3,
    MethodsRuntime, PortablePredictorPackage, PortableRefitPackageV3, PortableRefitReplayOutcomeV3,
    RunId, RuntimeControllerRegistry, SampleId, TrainingReplayOutcome, TrainingReplayRequest,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

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
    let runtime = MethodsRuntime::configure(&input.methods_library_path).map_err(|error| {
        NativeMethodsReplayError(format!("cannot configure the Methods runtime: {error}"))
    })?;
    execute_loaded_methods_predictor_replay(MethodsPortablePredictorReplayInput {
        package: &package,
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
}
