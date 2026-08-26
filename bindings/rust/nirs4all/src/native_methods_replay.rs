//! Core-owned Archive V2 to DAG-ML Methods replay composition.
//!
//! Archive parsing/integrity remains Core-owned. DAG-ML remains the sole owner
//! of package semantics, scheduling, N4MM hydration and conformal intervals.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use dag_ml_core::{
    execute_loaded_methods_predictor_replay, ExternalDataPlanEnvelope, MethodsPlsDataset,
    MethodsPortablePredictorReplayInput, MethodsRuntime, PortablePredictorPackage, RunId,
    TrainingReplayOutcome, TrainingReplayRequest,
};

use crate::LoadedArchiveV2;

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

/// Fail-closed error at the aggregate composition boundary.
#[derive(Debug)]
pub struct NativeMethodsReplayError(String);

impl fmt::Display for NativeMethodsReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for NativeMethodsReplayError {}

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
