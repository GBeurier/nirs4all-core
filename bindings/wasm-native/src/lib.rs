//! WASM projection over Core's canonical Archive V2 reader.
//!
//! The ZIP, manifest and inventory implementation is included from the exact
//! Core-owned Rust source used by the native binding. This crate adds only a
//! bounded Methods/DAG-ML projection and wasm-bindgen ownership glue; it is not
//! a second archive parser and contains no numerical code.

use std::path::{Path, PathBuf};

use dag_ml_core::{
    ArtifactBackend, ArtifactLoadMode, FittedArtifactMode, OutputOrder, Phase,
    PortablePredictorPackage, PredictionKind, PredictionLevel,
};
use wasm_bindgen::prelude::*;

// The canonical module's V1/V3 dispatch types live at the aggregate root. They
// are unavailable in this deliberately small wasm32 crate, so these private
// placeholders satisfy only the uncalled dual-dispatch signatures. Archive V2
// byte validation below executes the same source and never touches them.
#[derive(Clone, Debug)]
pub struct ArchivePayload {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum ArchiveStoreError {
    Io(std::io::Error),
    Format(String),
    Integrity(String),
    UnsupportedCapability(String),
    AlreadyExists(PathBuf),
    PublishedWithCleanupError { path: PathBuf, detail: String },
}

impl std::fmt::Display for ArchiveStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "archive I/O error: {error}"),
            Self::Format(detail) => detail.fmt(formatter),
            Self::Integrity(detail) => detail.fmt(formatter),
            Self::UnsupportedCapability(detail) => detail.fmt(formatter),
            Self::AlreadyExists(path) => {
                write!(
                    formatter,
                    "archive target already exists: {}",
                    path.display()
                )
            }
            Self::PublishedWithCleanupError { path, detail } => write!(
                formatter,
                "archive was published at {} but cleanup failed: {detail}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ArchiveStoreError {}

impl From<std::io::Error> for ArchiveStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug)]
pub struct LoadedArchiveV1;

#[derive(Clone, Debug)]
pub struct LoadedArchiveV3;

pub fn load_archive_v1(_path: &Path) -> Result<LoadedArchiveV1, ArchiveStoreError> {
    Err(ArchiveStoreError::UnsupportedCapability(
        "Archive V1 is not linked into the Archive V2 WASM validator".to_owned(),
    ))
}

pub fn load_archive_v3(_path: &Path) -> Result<LoadedArchiveV3, ArchiveStoreError> {
    Err(ArchiveStoreError::UnsupportedCapability(
        "Archive V3 is not linked into the Archive V2 WASM validator".to_owned(),
    ))
}

#[allow(dead_code, clippy::drop_non_drop)]
#[path = "../../rust/nirs4all/src/archive_v2.rs"]
mod core_archive_v2;

/// A fully validated, single-model Methods Archive V2 projection.
#[wasm_bindgen]
pub struct ValidatedMethodsArchiveV2 {
    archive_sha256: String,
    archive_id: String,
    package_json: String,
    model_bytes: Vec<u8>,
    artifact_id: String,
    binding_id: String,
    node_id: String,
    port_name: String,
    target_names_json: String,
}

#[wasm_bindgen]
impl ValidatedMethodsArchiveV2 {
    /// Validate through Core before returning any package or model bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(archive_bytes: &[u8]) -> Result<ValidatedMethodsArchiveV2, JsValue> {
        project_archive(archive_bytes)
            .map_err(|error| JsValue::from_str(&format!("Core Archive V2 refusal: {error}")))
    }

    #[wasm_bindgen(getter)]
    pub fn archive_sha256(&self) -> String {
        self.archive_sha256.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn archive_id(&self) -> String {
        self.archive_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn artifact_id(&self) -> String {
        self.artifact_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn binding_id(&self) -> String {
        self.binding_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn node_id(&self) -> String {
        self.node_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn port_name(&self) -> String {
        self.port_name.clone()
    }

    pub fn package_json(&self) -> String {
        self.package_json.clone()
    }

    pub fn model_bytes(&self) -> Vec<u8> {
        self.model_bytes.clone()
    }

    pub fn target_names_json(&self) -> String {
        self.target_names_json.clone()
    }
}

fn project_archive(bytes: &[u8]) -> Result<ValidatedMethodsArchiveV2, String> {
    let archive =
        core_archive_v2::load_archive_v2_bytes(bytes).map_err(|error| error.to_string())?;
    let declarations = archive
        .methods_n4mm_artifacts()
        .map_err(|error| error.to_string())?;
    if declarations.len() != 1 {
        return refuse("bounded WASM replay requires exactly one N4MM artifact");
    }
    let declaration = &declarations[0];
    let package_bytes = archive
        .portable_predictor_package()
        .map_err(|error| error.to_string())?;
    let package_json = std::str::from_utf8(package_bytes)
        .map_err(|_| "portable predictor package is not UTF-8".to_owned())?
        .to_owned();
    let package = PortablePredictorPackage::from_json(&package_json)
        .map_err(|error| format!("DAG-ML rejected predictor package: {error}"))?;

    if package.schema_version != 2
        || package.fitted_artifact_mode != FittedArtifactMode::PortableRequired
        || package.predictor_node_ids.len() != 1
        || package.artifact_bindings.len() != 1
        || package.output_bindings.len() != 1
        || package.effective_plan.node_plans.len() != 1
        || package.execution_bundle.refit_artifacts.len() != 1
        || package.execution_bundle.raw_artifact_payloads.len() != 1
    {
        return refuse("package is outside the bounded single-node Methods replay contract");
    }

    let node_id = &package.predictor_node_ids[0];
    let node = package
        .effective_plan
        .node_plans
        .get(node_id)
        .ok_or_else(|| "predictor node plan is absent".to_owned())?;
    if node.controller_id.as_str() != "controller:methods.pls"
        || !node.supported_phases.contains(&Phase::Predict)
    {
        return refuse("predictor node is not callback-free Methods PLS PREDICT");
    }

    let binding = &package.artifact_bindings[0];
    if binding.artifact_id.as_str() != declaration.artifact_id()
        || binding.load_mode != ArtifactLoadMode::NativePortable
    {
        return refuse("package artifact binding does not match portable N4MM");
    }

    let output = &package.output_bindings[0];
    if output.node_id != *node_id
        || output.prediction_level != PredictionLevel::Sample
        || output.prediction_kind != PredictionKind::RegressionPoint
        || serde_json::to_value(output.prediction_source)
            .ok()
            .as_ref()
            .and_then(serde_json::Value::as_str)
            != Some("final_refit")
        || output.output_order != OutputOrder::TargetOrder
        || output.target_space != "raw"
        || output.target_names.is_empty()
    {
        return refuse("output binding is outside multi-target regression replay");
    }

    let record = &package.execution_bundle.refit_artifacts[0];
    let artifact = &record.artifact;
    if record.node_id != *node_id
        || artifact.id != binding.artifact_id
        || artifact.id.as_str() != declaration.artifact_id()
        || artifact.kind != "n4m_model"
        || artifact.backend != Some(ArtifactBackend::Raw)
        || artifact.plugin.is_some()
        || artifact.plugin_version.is_some()
        || artifact.uri.as_deref() != Some(declaration.member_path())
    {
        return refuse("refit artifact does not cross-link the portable N4MM member");
    }
    let embedded = package
        .execution_bundle
        .raw_artifact_payloads
        .get(&artifact.id)
        .ok_or_else(|| "execution bundle lacks detached N4MM bytes".to_owned())?;
    let model_bytes = archive
        .member(declaration.member_path())
        .map_err(|error| error.to_string())?;
    if embedded.as_slice() != model_bytes {
        return refuse("DAG-ML N4MM bytes differ from the inventoried archive member");
    }

    let target_names_json = serde_json::to_string(&output.target_names)
        .map_err(|error| format!("cannot serialize target names: {error}"))?;
    Ok(ValidatedMethodsArchiveV2 {
        archive_sha256: archive.reference().archive_sha256().to_owned(),
        archive_id: archive.reference().archive_id().to_owned(),
        package_json,
        model_bytes: model_bytes.to_vec(),
        artifact_id: artifact.id.as_str().to_owned(),
        binding_id: output.binding_id.clone(),
        node_id: node_id.as_str().to_owned(),
        port_name: output.port_name.clone(),
        target_names_json,
    })
}

fn refuse<T>(detail: &str) -> Result<T, String> {
    Err(detail.to_owned())
}
