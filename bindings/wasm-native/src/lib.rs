//! WASM projection over Core's canonical Archive V2 reader.
//!
//! The ZIP, manifest and inventory implementation is included from the exact
//! Core-owned Rust source used by the native binding. This crate adds only a
//! bounded Methods/DAG-ML projection and wasm-bindgen ownership glue; it is not
//! a second archive parser and contains no numerical code.

use std::collections::{BTreeMap, BTreeSet};

use dag_ml_core::{
    methods_n4mm_abi_requirement, ArtifactBackend, ArtifactLoadMode, ControllerId,
    FittedArtifactMode, NativePredictorDescriptorV1, NativePredictorDimensionsV1,
    NativePredictorPipelineV1, NativePredictorWriterAbiV1, OutputOrder, Phase, PredictionKind,
    PredictionLevel, NATIVE_PREDICTOR_DESCRIPTOR_SCHEMA_VERSION_V1,
    NATIVE_PREDICTOR_DESCRIPTOR_TYPE_V1, NATIVE_PREDICTOR_FORMAT_N4MM,
    NATIVE_PREDICTOR_PIPELINE_FINGERPRINT_FNV1A64_V1, NATIVE_PREDICTOR_PIPELINE_TYPE_SNV_SAVGOL_V1,
};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

#[path = "../../rust/nirs4all/src/native_pipeline_contract.rs"]
mod native_pipeline_contract;
#[path = "../../rust/nirs4all/src/conformal_json.rs"]
mod public_conformal;
mod public_generation;
#[path = "../../rust/nirs4all/src/result_projection.rs"]
mod public_results;
#[path = "../../rust/nirs4all/src/robustness_json.rs"]
mod public_robustness;

#[wasm_bindgen]
pub fn project_training_predictions_json(
    outcome_json: &str,
    dataset_label: &str,
    task_type_label: &str,
) -> Result<String, JsValue> {
    public_results::project_training_predictions_json(outcome_json, dataset_label, task_type_label)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn validate_training_predictions_json(
    outcome_json: &str,
    predictions_json: &str,
) -> Result<(), JsValue> {
    public_results::validate_training_predictions_json(outcome_json, predictions_json)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn validate_robustness_scenarios_json(scenarios: &str) -> Result<String, JsValue> {
    public_robustness::validate_robustness_scenarios_json(scenarios)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn robustness_report_json(
    package: &str,
    scenarios: &str,
    replays: &str,
    truth: &str,
) -> Result<String, JsValue> {
    public_robustness::robustness_report_json(package, scenarios, replays, truth)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn calibrate_workflow_replay_json(
    source: &str,
    replay: &str,
    relations: &str,
    truth: &str,
    coverages: &str,
    small_sample_policy: &str,
) -> Result<String, JsValue> {
    public_conformal::calibrate_workflow_replay_json(
        source,
        replay,
        relations,
        truth,
        coverages,
        small_sample_policy,
    )
    .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn calibrated_prediction_json(
    package: &str,
    request: &str,
    replay: &str,
) -> Result<String, JsValue> {
    public_conformal::calibrated_prediction_json(package, request, replay)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn conformal_metrics_json(
    calibration: &str,
    intervals: &str,
    truth: &str,
) -> Result<String, JsValue> {
    public_conformal::conformal_metrics_json(calibration, intervals, truth)
        .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn calibrated_methods_points_json(
    package: &str,
    archive_sha: &str,
    sample_ids: &str,
    values: &str,
    descriptor: &str,
) -> Result<String, JsValue> {
    public_conformal::calibrated_methods_points_json(
        package,
        archive_sha,
        sample_ids,
        values,
        descriptor,
    )
    .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn frozen_methods_points_json(
    package: &str,
    archive_sha: &str,
    sample_ids: &str,
    values: &str,
    descriptor: &str,
) -> Result<String, JsValue> {
    public_robustness::frozen_methods_points_json(
        package,
        archive_sha,
        sample_ids,
        values,
        descriptor,
    )
    .map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn robustness_methods_points_json(
    package: &str,
    scenarios: &str,
    points: &str,
    truth: &str,
) -> Result<String, JsValue> {
    public_robustness::robustness_methods_points_json(package, scenarios, points, truth)
        .map_err(|error| JsValue::from_str(&error))
}

/// Validate the complete native portable payload closure for any supported
/// DAG predictor profile, without selecting a host execution backend.
#[wasm_bindgen]
pub fn validate_portable_archive_v2(bytes: &[u8]) -> Result<String, JsValue> {
    let archive = n4a_native_package::load_native_package_bytes(bytes, None, memory(None))
        .map_err(refusal)?;
    Ok(
        std::str::from_utf8(archive.portable_predictor_package().map_err(refusal)?)
            .map_err(refusal)?
            .to_owned(),
    )
}

/// Synchronous artifact hashing for native DAG controller callbacks.
#[wasm_bindgen]
pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[path = "../../rust/nirs4all/src/archive_storage.rs"]
mod archive_storage;
pub use archive_storage::ArchiveStoreError;

#[allow(dead_code)]
#[path = "../../rust/nirs4all/src/archive_codec.rs"]
mod archive_codec;

// Sans-IO budget, scanner, hasher and writer plan shared by every host. It must be
// mounted before `n4a`, which re-exports its public items.
#[path = "../../rust/nirs4all/src/n4a.rs"]
pub mod n4a;
#[cfg(not(target_arch = "wasm32"))]
#[path = "../../rust/nirs4all/src/n4a_file.rs"]
pub mod n4a_file;
#[path = "../../rust/nirs4all/src/n4a_native_package.rs"]
pub mod n4a_native_package;
#[path = "../../rust/nirs4all/src/n4a_physical.rs"]
pub mod n4a_physical;
// Compile the shared report-only inspector; no host/JS exports or native loader.
#[path = "../../rust/nirs4all/src/n4a_owner.rs"]
pub mod n4a_owner;
// Runtime-free portable plans for the canonical V1 `.n4a` L2 actions. Execution
// stays in dag-ml-wasm and the Methods WASM controllers (see `bindings/wasm/src/n4a.js`).
#[path = "../../rust/nirs4all/src/n4a_owner_portable.rs"]
pub mod n4a_owner_portable;

#[cfg(not(target_arch = "wasm32"))]
#[path = "../../rust/nirs4all/src/durability.rs"]
mod durability;

/// Core-validated storage inventory, not a controller authorization.
/// DAG-ML must validate the opaque package/member links before replay.
#[wasm_bindgen]
pub struct ValidatedPortableArchiveV2 {
    archive: n4a_native_package::LoadedNativePackage,
}

#[wasm_bindgen]
impl ValidatedPortableArchiveV2 {
    #[wasm_bindgen(constructor)]
    pub fn new(archive_bytes: &[u8]) -> Result<ValidatedPortableArchiveV2, JsValue> {
        Self::with_entrypoint(archive_bytes, None, None)
    }
    /// Storage/strict scientific inspection supports Package3; execution is separate.
    pub fn with_entrypoint(
        archive_bytes: &[u8],
        entrypoint: Option<String>,
        memory_budget_bytes: Option<u64>,
    ) -> Result<ValidatedPortableArchiveV2, JsValue> {
        n4a_native_package::load_native_package_bytes(
            archive_bytes,
            entrypoint.as_deref(),
            memory(memory_budget_bytes),
        )
        .map(|archive| Self { archive })
        .map_err(refusal)
    }
    #[wasm_bindgen(getter)]
    pub fn archive_id(&self) -> String {
        self.archive.reference().archive_id().to_owned()
    }
    #[wasm_bindgen(getter)]
    pub fn archive_sha256(&self) -> String {
        self.archive.reference().archive_sha256().to_owned()
    }
    #[wasm_bindgen(getter)]
    pub fn package_schema_version(&self) -> u32 {
        self.archive.reference().schema_version()
    }
    pub fn manifest_json(&self) -> String {
        serde_json::to_string(self.archive.manifest()).expect("validated manifest")
    }
    pub fn view_json(&self) -> String {
        self.archive.view().to_string()
    }
    pub fn package_bytes(&self) -> Vec<u8> {
        self.archive.closure().root().original_bytes().to_vec()
    }
    pub fn training_outcome_bytes(&self) -> Result<Vec<u8>, JsValue> {
        self.archive
            .training_outcome_bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(refusal)
    }
    pub fn state_member_bytes(&self, artifact_id: &str) -> Result<Vec<u8>, JsValue> {
        self.archive
            .state_member(artifact_id)
            .map(|bytes| bytes.to_vec())
            .map_err(refusal)
    }
    pub fn member_paths_json(&self) -> String {
        serde_json::to_string(&self.archive.members().keys().collect::<Vec<_>>())
            .expect("validated member paths")
    }
    pub fn member_bytes(&self, path: &str) -> Result<Vec<u8>, JsValue> {
        self.archive
            .member(path)
            .map(|bytes| bytes.to_vec())
            .map_err(refusal)
    }
}

/// Exact owner documents sealed into neutral Core V1 bytes, with no fit.
#[wasm_bindgen]
pub fn write_native_predictor_package_bytes(
    container_id: &str,
    outcome_bytes: &[u8],
    package_bytes: &[u8],
    memory_budget_bytes: Option<u64>,
) -> Result<Vec<u8>, JsValue> {
    n4a_native_package::native_package_container(
        Some(outcome_bytes),
        package_bytes,
        container_id,
        memory(memory_budget_bytes),
    )
    .map(|container| container.into_archive_bytes().into_vec())
    .map_err(refusal)
}
#[wasm_bindgen]
pub fn write_native_refit_package_bytes(
    container_id: &str,
    package_bytes: &[u8],
    memory_budget_bytes: Option<u64>,
) -> Result<Vec<u8>, JsValue> {
    n4a_native_package::native_package_container(
        None,
        package_bytes,
        container_id,
        memory(memory_budget_bytes),
    )
    .map(|container| container.into_archive_bytes().into_vec())
    .map_err(refusal)
}

/// Calibrate from a known immutable base; retained foreign content is carried
/// byte-for-byte and retained references cannot be rebound to changed facets.
#[wasm_bindgen]
pub fn calibrate_native_package_bytes(
    base_bytes: &[u8],
    entrypoint: Option<String>,
    outcome_bytes: &[u8],
    package_bytes: &[u8],
    container_id: &str,
    memory_budget_bytes: Option<u64>,
) -> Result<Vec<u8>, JsValue> {
    let budget = memory(memory_budget_bytes);
    let base = n4a::open_n4a_bytes(base_bytes, budget).map_err(refusal)?;
    let before = n4a_native_package::read_native_package_for_operation(
        &base,
        entrypoint.as_deref(),
        "calibrate-external",
        budget,
    )
    .map_err(refusal)?;
    let (manifest, members) = n4a_native_package::calibration_update(
        &base,
        &before,
        outcome_bytes,
        package_bytes,
        container_id,
    )
    .map_err(refusal)?;
    n4a::write_n4a_update(
        &base,
        manifest,
        members
            .iter()
            .map(|(path, bytes)| n4a::N4aMemberInput { path, bytes })
            .collect(),
        budget,
    )
    .map(|(_, bytes)| bytes)
    .map_err(refusal)
}

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
    format_version: u32,
    abi_min_minor: u32,
    owner_controller: ControllerId,
    embedded_descriptor: Option<NativePredictorDescriptorV1>,
}

#[wasm_bindgen]
impl ValidatedMethodsArchiveV2 {
    /// Validate through Core before returning any package or model bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(archive_bytes: &[u8]) -> Result<ValidatedMethodsArchiveV2, JsValue> {
        project_archive(archive_bytes)
            .map_err(|error| JsValue::from_str(&format!("Core Archive V2 refusal: {error}")))
    }

    pub fn with_entrypoint(
        archive_bytes: &[u8],
        entrypoint: Option<String>,
        memory_budget_bytes: Option<u64>,
    ) -> Result<ValidatedMethodsArchiveV2, JsValue> {
        let archive = n4a_native_package::load_native_package_bytes(
            archive_bytes,
            entrypoint.as_deref(),
            memory(memory_budget_bytes),
        )
        .map_err(refusal)?;
        project_native_archive(&archive).map_err(|error| JsValue::from_str(&error))
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

    #[wasm_bindgen(getter)]
    pub fn abi_min_minor(&self) -> u32 {
        self.abi_min_minor
    }

    #[wasm_bindgen(getter)]
    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Bind authoritative Methods/WASM inspection fields to the inventoried
    /// N4MM bytes and return DAG-ML's typed descriptor JSON.
    ///
    /// The public JavaScript facade obtains these primitive fields only from
    /// `@nirs4all/methods.inspectN4mm`. Core supplies the artifact hash and
    /// controller from the validated archive, while DAG-ML owns all pure
    /// controller/algorithm/capability/dimension policy and TCV1 identity.
    #[allow(clippy::too_many_arguments)]
    pub fn bind_inspected_native_predictor_v1(
        &self,
        inspection_schema_version: u32,
        format_version: u32,
        writer_abi_major: u32,
        writer_abi_minor: u32,
        writer_abi_patch: u32,
        storage_algorithm: i32,
        training_samples: i64,
        n_features: i32,
        n_targets: i32,
        n_components: i32,
        capabilities: u64,
        pipeline_present: bool,
        pipeline_schema_version: u32,
        pipeline_operator_count: u32,
        pipeline_first_operator: i32,
        pipeline_second_operator: i32,
        savgol_window: i32,
        savgol_poly_degree: i32,
        savgol_derivative: i32,
        pipeline_semantic_profile: u32,
        savgol_delta: f64,
        pipeline_raw_n_features: i32,
        pipeline_model_n_features: i32,
        pipeline_fingerprint_algorithm: u32,
        pipeline_fingerprint: u64,
        snv_axis: i32,
        snv_with_mean: bool,
        snv_with_std: bool,
        snv_ddof: i32,
        savgol_mode: i32,
        savgol_cval: f64,
    ) -> Result<String, JsValue> {
        if inspection_schema_version != 1 {
            return Err(JsValue::from_str(
                "Methods returned an unsupported serialized-model inspection schema",
            ));
        }
        if format_version != self.format_version {
            return Err(JsValue::from_str(
                "Methods inspection does not match the Archive V2 N4MM format declaration",
            ));
        }
        let pipeline = if pipeline_present {
            if pipeline_schema_version != 1
                || pipeline_operator_count != 2
                || pipeline_first_operator != 4
                || pipeline_second_operator != 8
                || savgol_derivative != 0
                || pipeline_semantic_profile != 1
                || savgol_delta.to_bits() != 1.0f64.to_bits()
                || pipeline_fingerprint_algorithm != 1
                || snv_axis != 1
                || !snv_with_mean
                || !snv_with_std
                || snv_ddof != 0
                || savgol_mode != 4
                || savgol_cval.to_bits() != 0.0f64.to_bits()
            {
                return Err(JsValue::from_str(
                    "Methods returned an unsupported native predictor pipeline",
                ));
            }
            Some(NativePredictorPipelineV1 {
                pipeline_type: NATIVE_PREDICTOR_PIPELINE_TYPE_SNV_SAVGOL_V1.to_owned(),
                schema_version: pipeline_schema_version,
                operator_count: pipeline_operator_count,
                raw_n_features: pipeline_raw_n_features,
                model_n_features: pipeline_model_n_features,
                fingerprint_algorithm: NATIVE_PREDICTOR_PIPELINE_FINGERPRINT_FNV1A64_V1.to_owned(),
                native_fingerprint: format!("{pipeline_fingerprint:016x}"),
                savgol_window,
                savgol_poly_degree,
            })
        } else {
            None
        };
        let mut descriptor = NativePredictorDescriptorV1 {
            descriptor_type: NATIVE_PREDICTOR_DESCRIPTOR_TYPE_V1.to_owned(),
            schema_version: NATIVE_PREDICTOR_DESCRIPTOR_SCHEMA_VERSION_V1,
            artifact_sha256: format!("{:x}", Sha256::digest(&self.model_bytes)),
            owner_controller: self.owner_controller.clone(),
            format: NATIVE_PREDICTOR_FORMAT_N4MM.to_owned(),
            format_version,
            writer_abi: NativePredictorWriterAbiV1 {
                major: writer_abi_major,
                minor: writer_abi_minor,
                patch: writer_abi_patch,
            },
            storage_algorithm,
            capabilities,
            dimensions: NativePredictorDimensionsV1 {
                training_samples,
                n_features,
                n_targets,
                n_components,
            },
            pipeline,
            descriptor_fingerprint: String::new(),
        };
        descriptor.descriptor_fingerprint = descriptor
            .compute_fingerprint()
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        descriptor
            .validate()
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        if self
            .embedded_descriptor
            .as_ref()
            .is_some_and(|embedded| embedded != &descriptor)
        {
            return Err(JsValue::from_str(
                "Archive V2 native predictor descriptor does not match Methods inspection",
            ));
        }
        serde_json::to_string(&descriptor)
            .map_err(|error| JsValue::from_str(&format!("cannot serialize descriptor: {error}")))
    }
}

fn project_archive(bytes: &[u8]) -> Result<ValidatedMethodsArchiveV2, String> {
    let archive = n4a_native_package::load_native_package_bytes(bytes, None, memory(None))
        .map_err(|error| error.to_string())?;
    project_native_archive(&archive)
}

fn project_native_archive(
    archive: &n4a_native_package::LoadedNativePackage,
) -> Result<ValidatedMethodsArchiveV2, String> {
    if archive.reference().schema_version() == 3 {
        return refuse(
            "Package3 WASM execution is unsupported; storage and inspection remain available",
        );
    }
    let package_bytes = archive
        .portable_predictor_package()
        .map_err(|error| error.to_string())?;
    let package_json = std::str::from_utf8(package_bytes)
        .map_err(|_| "portable predictor package is not UTF-8".to_owned())?
        .to_owned();
    let package = archive
        .predictor_package()
        .map_err(|error| error.to_string())?;

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
    if binding.load_mode != ArtifactLoadMode::NativePortable {
        return refuse("package artifact binding does not match portable N4MM");
    }

    let output = &package.output_bindings[0];
    let unique_target_names: BTreeSet<&str> =
        output.target_names.iter().map(String::as_str).collect();
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
        || output.target_names.len() > 256
        || unique_target_names.len() != output.target_names.len()
        || output
            .target_names
            .iter()
            .any(|name| name.is_empty() || name.len() > 128 || name.chars().any(char::is_control))
    {
        return refuse("output binding is outside multi-target regression replay");
    }

    let record = &package.execution_bundle.refit_artifacts[0];
    let artifact = &record.artifact;
    let (artifact_abi_major, artifact_abi_min_minor) =
        methods_n4mm_abi_requirement(artifact).map_err(|error| error.to_string())?;
    if record.node_id != *node_id
        || artifact.id != binding.artifact_id
        || artifact.kind != "n4m_model"
        || artifact.backend != Some(ArtifactBackend::Raw)
        || artifact.plugin.is_some()
        || artifact.plugin_version.is_some()
        || artifact_abi_major != 2
        || !archive.methods_requirements().iter().any(|r| {
            r.artifact_id == artifact.id
                && r.abi_major == artifact_abi_major
                && r.abi_min_minor == artifact_abi_min_minor
        })
    {
        return refuse("refit artifact does not cross-link the portable N4MM member");
    }
    let format_version = artifact
        .native_predictor_descriptor
        .as_ref()
        .map_or(1, |descriptor| descriptor.format_version);
    match (
        format_version,
        artifact.native_predictor_descriptor.as_ref(),
    ) {
        (1, None) => {}
        (1, Some(descriptor))
            if descriptor.format_version == 1 && descriptor.pipeline.is_none() => {}
        (2, Some(descriptor))
            if descriptor.format_version == 2 && descriptor.pipeline.is_some() => {}
        (3, Some(descriptor))
            if descriptor.format_version == 3 && descriptor.writer_abi.minor >= 20 => {}
        _ => {
            return refuse(
                "N4MM format declaration does not match its content-bound predictor descriptor",
            )
        }
    }
    let embedded = package
        .execution_bundle
        .raw_artifact_payloads
        .get(&artifact.id)
        .ok_or_else(|| "execution bundle lacks detached N4MM bytes".to_owned())?;
    let model_bytes = archive
        .state_member(artifact.id.as_str())
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
        format_version,
        abi_min_minor: artifact_abi_min_minor,
        owner_controller: artifact.controller_id.clone(),
        embedded_descriptor: artifact.native_predictor_descriptor.clone(),
    })
}

fn refuse<T>(detail: &str) -> Result<T, String> {
    Err(detail.to_owned())
}

#[cfg(test)]
mod native_transport_tests {
    #[test]
    fn exact_byte_writers_and_static_readers_match_the_facade_contract() {
        let outcome = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/training_outcome.json"
        );
        let package = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/predictor_package.json"
        );
        let bytes =
            super::write_native_predictor_package_bytes("wasm:byte-api", outcome, package, None)
                .unwrap();
        let reader = super::ValidatedPortableArchiveV2::with_entrypoint(
            &bytes,
            Some("predict-external".into()),
            None,
        )
        .unwrap();
        assert_eq!(reader.package_bytes().as_slice(), package.as_slice());
        assert_eq!(
            reader.training_outcome_bytes().unwrap().as_slice(),
            outcome.as_slice()
        );
        let view: serde_json::Value = serde_json::from_str(&reader.view_json()).unwrap();
        assert_eq!(view["package_schema_version"], 2);
        assert_eq!(view["schema_version"], 1);
        let child = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/child_refit_v3.json"
        );
        let bytes =
            super::write_native_refit_package_bytes("wasm:child-byte-api", child, None).unwrap();
        let reader =
            super::ValidatedPortableArchiveV2::with_entrypoint(&bytes, None, None).unwrap();
        assert_eq!(reader.package_bytes().as_slice(), child.as_slice());
        assert_eq!(reader.package_schema_version(), 3);
    }

    #[test]
    fn package_three_remains_inspectable_with_explicit_execution_refusal() {
        let bytes = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/child_refit_v3.json"
        );
        let container = super::n4a_native_package::native_package_container(
            None,
            bytes,
            "wasm:child",
            super::memory(None),
        )
        .unwrap();
        let loaded = super::n4a_native_package::read_native_package(&container, None).unwrap();
        assert_eq!(loaded.portable_refit_package().unwrap(), bytes);
        let error = super::project_native_archive(&loaded).err().unwrap();
        assert!(
            error.contains("Package3 WASM execution is unsupported"),
            "{error}"
        );
    }

    #[test]
    fn shared_reader_retains_role_states_without_narrowing_storage_to_pls() {
        let outcome = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/training_outcome.json"
        );
        let package = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/predictor_package.json"
        );
        let container = super::n4a_native_package::native_package_container(
            Some(outcome),
            package,
            "wasm:role",
            super::memory(None),
        )
        .unwrap();
        let loaded = super::n4a_native_package::read_native_package(&container, None).unwrap();
        assert_eq!(loaded.portable_predictor_package().unwrap(), package);
        assert_eq!(loaded.methods_requirements()[0].abi_min_minor, 14);
        for state in loaded.closure().state_members() {
            assert_eq!(
                loaded.state_member(state.artifact_id.as_str()).unwrap(),
                container.member(&state.member_path).unwrap().bytes
            );
        }
        // The existing bounded PLS execution adapter still refuses other
        // numerical families; their portable storage has already succeeded.
        assert!(super::project_native_archive(&loaded).is_err());
    }

    #[test]
    fn sans_io_native_reader_reuses_scanned_metadata_and_verified_scientific_members() {
        let outcome = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/training_outcome.json"
        );
        let package = include_bytes!(
            "../../rust/nirs4all/tests/fixtures/native_package_transport_v1/predictor_package.json"
        );
        let container = super::n4a_native_package::native_package_container(
            Some(outcome),
            package,
            "wasm:external-source",
            super::memory(None),
        )
        .unwrap();
        let bytes = container.archive_bytes();
        let mut scanner =
            super::n4a::N4aScanner::new(bytes.len() as u64, super::memory(None)).unwrap();
        while let Some(request) = scanner.next_read() {
            let start = request.offset as usize;
            scanner
                .feed(&bytes[start..start + request.length as usize])
                .unwrap();
        }
        let scanned = scanner.finish().unwrap();
        let source = super::N4aSourceHandle {
            metadata: scanned.metadata,
            manifest: scanned.manifest_bytes,
        };
        let mut reader = super::NativePackageMemberReader::new(&source, None).unwrap();
        for member in &container.manifest().members {
            let bytes = container.member(&member.path).unwrap().bytes;
            reader
                .add_member(member.path.clone(), bytes.to_vec().into_boxed_slice())
                .unwrap();
        }
        let loaded = reader
            .finish(Some("predict-external".into()), "predict-external")
            .unwrap();
        assert_eq!(loaded.archive.closure().root().original_bytes(), package);
        assert_eq!(
            loaded.archive_sha256(),
            container.receipt().container_sha256
        );
        assert!(reader.members.is_none());
    }
}

/// Validate the shared CPU/browser envelope in Rust before host presentation.
/// JSON fragments stay strings so JavaScript never rounds native uint64 seeds.
#[wasm_bindgen]
pub fn native_pipeline_fragments_json(input: &str) -> Result<String, JsValue> {
    let record = native_pipeline_contract::parse_native_pipeline_record(input)
        .map_err(|error| JsValue::from_str(&error))?;
    let fragments = serde_json::json!({
        "config_json": serde_json::to_string(&record["config"]).map_err(|error| JsValue::from_str(&error.to_string()))?,
        "package_json": serde_json::to_string(&record["package"]).map_err(|error| JsValue::from_str(&error.to_string()))?,
        "training_outcome_json": serde_json::to_string(&record["training_outcome"]).map_err(|error| JsValue::from_str(&error.to_string()))?
    });
    serde_json::to_string(&fragments).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn refusal(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Open externally supplied `.n4a` bytes at the owner boundary and return the
/// closed native-produced model view (manifest, receipt, training outcome) as
/// JSON text. Inspection is not executability. `memory_budget` is the optional
/// working-memory budget in bytes; `undefined` leaves only the allocator.
#[wasm_bindgen]
pub fn inspect_n4a_bytes(
    archive_bytes: &[u8],
    memory_budget: Option<u64>,
) -> Result<String, JsValue> {
    n4a_owner_portable::inspect_n4a_view_json(archive_bytes, memory(memory_budget)).map_err(refusal)
}

/// The optional working-memory budget of one operation. `None` declares no Core
/// ceiling: only the allocator and real host limits remain.
fn memory(working_bytes: Option<u64>) -> n4a::N4aMemoryBudget {
    n4a::N4aMemoryBudget::new(working_bytes)
}

/// Core's admission of `bytes` against a budget. Hosts call this BEFORE a
/// wasm-bindgen argument copy or any other allocation Core cannot make fallible.
#[wasm_bindgen]
pub fn n4a_admit(memory_budget: Option<u64>, bytes: u64) -> Result<(), JsValue> {
    memory(memory_budget).admit(bytes).map_err(refusal)
}

/// Streaming buffer size Core derives from a budget: a buffer size, never a
/// content limit.
#[wasm_bindgen]
pub fn n4a_chunk_bytes(memory_budget: Option<u64>) -> u64 {
    memory(memory_budget).chunk_bytes()
}

/// Read-only prediction plan. Core validates the container and record, IO
/// normalises the record, and DAG-ML re-admits the exact strings below inside
/// `replay_training_package_json`. The plan carries features and sample IDs
/// only: it has no target accessor at all.
#[wasm_bindgen]
pub struct N4aPredictPlan {
    plan: n4a_owner_portable::PortablePredictPlan,
}

#[wasm_bindgen]
impl N4aPredictPlan {
    /// `record` is `undefined` for the stored input or exact external IO bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(
        archive_bytes: &[u8],
        entrypoint: &str,
        record: Option<Vec<u8>>,
        source_id: &str,
        partition: &str,
        require_training_schema: bool,
        request_id: &str,
        memory_budget: Option<u64>,
    ) -> Result<N4aPredictPlan, JsValue> {
        n4a_owner_portable::prepare_predict_portable(
            archive_bytes,
            n4a_owner_portable::PortablePredictRequest {
                entrypoint,
                record: record.as_deref(),
                source_id,
                partition,
                require_training_schema,
                request_id,
                memory_budget: memory(memory_budget),
            },
        )
        .map(|plan| Self { plan })
        .map_err(refusal)
    }

    pub fn package_json(&self) -> String {
        self.plan.package_json.clone()
    }
    pub fn request_json(&self) -> String {
        self.plan.request_json.clone()
    }
    pub fn envelopes_json(&self) -> String {
        self.plan.envelopes_json.clone()
    }
    pub fn receipt_json(&self) -> String {
        self.plan.receipt_json.clone()
    }
    pub fn sample_ids_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.plan.sample_ids).map_err(refusal)
    }
    pub fn target_names_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.plan.target_names).map_err(refusal)
    }
    #[wasm_bindgen(getter)]
    pub fn rows(&self) -> usize {
        self.plan.rows
    }
    #[wasm_bindgen(getter)]
    pub fn cols(&self) -> usize {
        self.plan.cols
    }
    /// Row-major features in `sample_ids` order. Copied once per call; the
    /// facade reads it once per plan.
    pub fn x(&self) -> Vec<f64> {
        self.plan.x.clone()
    }
}

/// Phase one of a stored-recipe retrain: Core validates the archive once and
/// keeps only the stored recipe and source ID. The host then supplies its
/// trusted controller manifests to `plan`. Anything outside the closed native
/// model profile is refused by the constructor.
#[wasm_bindgen]
pub struct N4aRetrainFacts {
    facts: n4a_owner_portable::PortableRetrainFacts,
}

#[wasm_bindgen]
impl N4aRetrainFacts {
    #[wasm_bindgen(constructor)]
    pub fn new(
        archive_bytes: &[u8],
        memory_budget: Option<u64>,
    ) -> Result<N4aRetrainFacts, JsValue> {
        n4a_owner_portable::read_retrain_facts(archive_bytes, memory(memory_budget))
            .map(|facts| Self { facts })
            .map_err(refusal)
    }

    /// Plan the retrain from this archive's stored recipe and source.
    pub fn plan(
        &self,
        record: &[u8],
        manifests_json: &str,
        run_id: &str,
        memory_budget: Option<u64>,
    ) -> Result<N4aTrainPlan, JsValue> {
        self.facts
            .plan(record, manifests_json, run_id, memory(memory_budget))
            .map(|plan| N4aTrainPlan { plan })
            .map_err(refusal)
    }
}

/// Training plan produced from an IO record plus a recipe, or from the recipe
/// stored in a validated archive. DAG-ML request assembly, IO identities and
/// folds are Rust-owned; JavaScript only executes the exact strings and seals
/// the result through Core.
#[wasm_bindgen]
pub struct N4aTrainPlan {
    plan: n4a_owner_portable::PortableTrainPlan,
}

#[wasm_bindgen]
impl N4aTrainPlan {
    /// `manifests_json` are the trusted controller manifests of the executing
    /// host; the request embeds exactly these.
    #[wasm_bindgen(constructor)]
    pub fn new(
        record: &[u8],
        source_id: &str,
        recipe_json: &str,
        manifests_json: &str,
        run_id: &str,
        memory_budget: Option<u64>,
    ) -> Result<N4aTrainPlan, JsValue> {
        n4a_owner_portable::prepare_train_portable(
            record,
            source_id,
            recipe_json,
            manifests_json,
            run_id,
            memory(memory_budget),
        )
        .map(|plan| Self { plan })
        .map_err(refusal)
    }

    #[wasm_bindgen(getter)]
    pub fn source_id(&self) -> String {
        self.plan.source_id().to_owned()
    }
    #[wasm_bindgen(getter)]
    pub fn run_id(&self) -> String {
        self.plan.run_id().to_owned()
    }
    #[wasm_bindgen(getter)]
    pub fn package_id(&self) -> String {
        self.plan.package_id().to_owned()
    }
    #[wasm_bindgen(getter)]
    pub fn outcome_id(&self) -> String {
        self.plan.outcome_id().to_owned()
    }
    #[wasm_bindgen(getter)]
    pub fn bundle_id(&self) -> String {
        self.plan.bundle_id().to_owned()
    }
    #[wasm_bindgen(getter)]
    pub fn rows(&self) -> usize {
        self.plan.rows()
    }
    #[wasm_bindgen(getter)]
    pub fn cols(&self) -> usize {
        self.plan.cols()
    }
    pub fn request_json(&self) -> String {
        self.plan.request_json().to_owned()
    }
    pub fn envelopes_json(&self) -> String {
        self.plan.envelopes_json().to_owned()
    }
    pub fn relations_json(&self) -> String {
        self.plan.relations_json().to_owned()
    }
    pub fn sample_ids_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(self.plan.sample_ids()).map_err(refusal)
    }
    pub fn target_names_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(self.plan.target_names()).map_err(refusal)
    }
    /// Row-major features and targets in `sample_ids` order; each is copied
    /// once per plan by the facade.
    pub fn x(&self) -> Vec<f64> {
        self.plan.x().to_vec()
    }
    pub fn y(&self) -> Vec<f64> {
        self.plan.y().to_vec()
    }

    /// Core's owner seal of the DAG-ML outcome and package this plan requested.
    pub fn seal(
        &self,
        outcome_json: &str,
        package_json: &str,
        container_id: &str,
        embed_training_record: bool,
        memory_budget: Option<u64>,
    ) -> Result<N4aSealed, JsValue> {
        let sealed = self
            .plan
            .seal(
                outcome_json,
                package_json,
                container_id,
                embed_training_record,
                memory(memory_budget),
            )
            .map_err(refusal)?;
        Ok(N4aSealed {
            bytes: sealed.bytes,
            view_json: sealed.view_json,
        })
    }
}

/// Sealed canonical `.n4a` bytes with the view of the same fit.
#[wasm_bindgen]
pub struct N4aSealed {
    bytes: Vec<u8>,
    view_json: String,
}

#[wasm_bindgen]
impl N4aSealed {
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
    pub fn view_json(&self) -> String {
        self.view_json.clone()
    }
}

fn host_refusal(message: &str) -> JsValue {
    JsValue::from_str(message)
}

/// `usize` is the real addressing limit of in-memory routes on wasm32.
fn addressable(value: u64) -> Result<usize, JsValue> {
    usize::try_from(value).map_err(|_| {
        refusal(n4a::N4aError::Memory(
            "value is not addressable on this host".into(),
        ))
    })
}

/// Bounds-checked sub-slice for chunked reads of in-memory bytes.
fn window(bytes: &[u8], offset: u64, length: u64) -> Result<&[u8], JsValue> {
    let start = addressable(offset)?;
    let end = start
        .checked_add(addressable(length)?)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| host_refusal("n4a read window lies outside the stored bytes"))?;
    Ok(&bytes[start..end])
}

fn receipt_to_json(receipt: &n4a::N4aReceipt) -> Result<String, JsValue> {
    serde_json::to_string(&serde_json::json!({
        "container_sha256": receipt.container_sha256,
        "manifest_sha256": receipt.manifest_sha256,
        "byte_length": receipt.byte_length,
        "members": receipt.members,
    }))
    .map_err(refusal)
}

/// Validated generic `.n4a` storage held in WASM memory. The container is opened
/// once, owns the caller's allocation without a Rust-side copy and is immutable
/// afterwards. Readers return exact bytes. Owner semantics stay report-only and
/// no Methods or DAG-ML runtime is involved. The working-memory budget is the
/// optional caller declaration; Core adds no content cap of its own.
#[wasm_bindgen]
pub struct N4aContainerHandle {
    container: n4a::OwnedN4a,
}

#[wasm_bindgen]
impl N4aContainerHandle {
    /// Consumes the transferred bytes. Any physical, manifest, memory or
    /// integrity failure is a refusal and no handle exists. Hosts admit the
    /// byte length with `n4a_admit` before the wasm-bindgen copy.
    #[wasm_bindgen(constructor)]
    pub fn new(
        archive_bytes: Box<[u8]>,
        memory_budget: Option<u64>,
    ) -> Result<N4aContainerHandle, JsValue> {
        n4a::open_owned_n4a_bytes(archive_bytes, memory(memory_budget))
            .map(|container| Self { container })
            .map_err(refusal)
    }

    /// Core's container view as JSON text (storage status, receipt, manifest,
    /// semantic report). Reading is not executability.
    pub fn view_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&n4a_owner_portable::n4a_container_view(&self.container))
            .map_err(refusal)
    }

    /// Admission of `bytes` against the budget this handle was opened with.
    pub fn admit(&self, bytes: u64) -> Result<(), JsValue> {
        self.container.memory_budget().admit(bytes).map_err(refusal)
    }

    /// Streaming buffer size derived from this handle's budget.
    pub fn chunk_bytes(&self) -> u64 {
        self.container.memory_budget().chunk_bytes()
    }

    /// The exact stored `manifest.json` bytes.
    pub fn manifest_bytes(&self) -> Result<Vec<u8>, JsValue> {
        let bytes = self.container.manifest_bytes();
        self.admit(bytes.len() as u64)?;
        Ok(bytes.to_vec())
    }

    /// Byte length of one inventoried member, or `undefined`.
    pub fn member_length(&self, path: &str) -> Option<u64> {
        self.container
            .member(path)
            .map(|member| member.bytes.len() as u64)
    }

    /// The exact bytes of one inventoried member, or `undefined`.
    pub fn member(&self, path: &str) -> Result<Option<Vec<u8>>, JsValue> {
        let Some(member) = self.container.member(path) else {
            return Ok(None);
        };
        self.admit(member.bytes.len() as u64)?;
        Ok(Some(member.bytes.to_vec()))
    }

    /// `length` bytes of a member from `offset`, or `undefined` for an unknown
    /// member. The bytes were validated when the container was opened.
    pub fn member_chunk(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<Option<Vec<u8>>, JsValue> {
        let Some(member) = self.container.member(path) else {
            return Ok(None);
        };
        let slice = window(member.bytes, offset, length)?;
        self.admit(length)?;
        Ok(Some(slice.to_vec()))
    }

    /// JSON array of the member paths an embedded artifact owns (empty for a
    /// reference artifact), or `undefined` for an unknown artifact id.
    pub fn artifact_member_paths_json(&self, artifact_id: &str) -> Result<Option<String>, JsValue> {
        match self.container.artifact_members(artifact_id) {
            None => Ok(None),
            Some(members) => {
                let paths: Vec<&str> = members.iter().map(|member| member.path).collect();
                serde_json::to_string(&paths).map(Some).map_err(refusal)
            }
        }
    }

    pub fn archive_length(&self) -> u64 {
        self.container.archive_bytes().len() as u64
    }

    /// `length` bytes of the original archive from `offset`.
    pub fn archive_chunk(&self, offset: u64, length: u64) -> Result<Vec<u8>, JsValue> {
        let slice = window(self.container.archive_bytes(), offset, length)?;
        self.admit(length)?;
        Ok(slice.to_vec())
    }

    /// An independent copy of the original archive bytes.
    pub fn archive_bytes(&self) -> Result<Vec<u8>, JsValue> {
        let bytes = self.container.archive_bytes();
        self.admit(bytes.len() as u64)?;
        Ok(bytes.to_vec())
    }
}

/// Incremental hash of bytes a host actually read from a retained source. It is
/// compared with the validated receipt by `N4aSourceHandle::verify_*`, or turned
/// into a member digest by `N4aStreamWriter::add_member`. One use only.
#[wasm_bindgen]
pub struct N4aStreamVerifier {
    hasher: Option<n4a::N4aMemberHasher>,
}

#[wasm_bindgen]
impl N4aStreamVerifier {
    #[wasm_bindgen(constructor)]
    pub fn new() -> N4aStreamVerifier {
        Self {
            hasher: Some(n4a::N4aMemberHasher::new()),
        }
    }

    pub fn update(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.hasher
            .as_mut()
            .ok_or_else(|| host_refusal("n4a stream verifier was already used"))?
            .update(bytes);
        Ok(())
    }
}

impl Default for N4aStreamVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl N4aStreamVerifier {
    fn digest(&mut self) -> Result<n4a::N4aMemberDigest, JsValue> {
        self.hasher
            .take()
            .map(n4a::N4aMemberHasher::finish)
            .ok_or_else(|| host_refusal("n4a stream verifier was already used"))
    }
}

/// Drives Core's one sans-IO scanner over a host-owned random-access source. The
/// host reads exactly the requested ranges and feeds them back; all physical
/// parsing, CRC, SHA and manifest checks stay in Core.
#[wasm_bindgen]
pub struct N4aSourceScan {
    scanner: Option<n4a::N4aScanner>,
}

#[wasm_bindgen]
impl N4aSourceScan {
    /// `archive_len` is the exact source size. A budget too small for the
    /// scanner's buffer is refused here.
    #[wasm_bindgen(constructor)]
    pub fn new(archive_len: u64, memory_budget: Option<u64>) -> Result<N4aSourceScan, JsValue> {
        n4a::N4aScanner::new(archive_len, memory(memory_budget))
            .map(|scanner| Self {
                scanner: Some(scanner),
            })
            .map_err(refusal)
    }

    /// Offset of the pending read, or `undefined` when the scan is ready to finish.
    pub fn next_offset(&self) -> Result<Option<u64>, JsValue> {
        Ok(self.scanner()?.next_read().map(|request| request.offset))
    }

    /// Length of the pending read, or `undefined` when the scan is ready to finish.
    pub fn next_length(&self) -> Result<Option<u64>, JsValue> {
        Ok(self.scanner()?.next_read().map(|request| request.length))
    }

    /// The bytes of exactly the pending read. A short or long read is refused.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.scanner
            .as_mut()
            .ok_or_else(|| host_refusal("n4a source scan is already finished"))?
            .feed(bytes)
            .map_err(refusal)
    }

    /// Complete validation. The handle keeps metadata only, never member bytes.
    pub fn finish(&mut self) -> Result<N4aSourceHandle, JsValue> {
        let scanner = self
            .scanner
            .take()
            .ok_or_else(|| host_refusal("n4a source scan is already finished"))?;
        let scanned = scanner.finish().map_err(refusal)?;
        Ok(N4aSourceHandle {
            metadata: scanned.metadata,
            manifest: scanned.manifest_bytes,
        })
    }
}

impl N4aSourceScan {
    fn scanner(&self) -> Result<&n4a::N4aScanner, JsValue> {
        self.scanner
            .as_ref()
            .ok_or_else(|| host_refusal("n4a source scan is already finished"))
    }
}

/// Validated metadata of an externally held archive. Member and archive bytes
/// stay with the host's source; every later read is verified by the host feeding
/// the bytes it actually read into an `N4aStreamVerifier`. Semantics are not
/// evaluated because no member bytes were read here.
#[wasm_bindgen]
pub struct N4aSourceHandle {
    metadata: n4a::N4aMetadata,
    manifest: Box<[u8]>,
}

/// Sans-IO scientific reader over one already scanned external source. Hosts
/// feed only scientific members they materialize; the strict DAG read requests
/// determine which are required, and foreign payloads need not be loaded.
#[wasm_bindgen]
pub struct NativePackageMemberReader {
    metadata: n4a::N4aMetadata,
    members: Option<BTreeMap<String, Box<[u8]>>>,
    resident: u64,
    budget: n4a::N4aMemoryBudget,
}

impl n4a::N4aMemberBytes for NativePackageMemberReader {
    fn metadata(&self) -> &n4a::N4aMetadata {
        &self.metadata
    }
    fn member_bytes(&self, path: &str) -> Result<std::borrow::Cow<'_, [u8]>, n4a::N4aError> {
        self.members
            .as_ref()
            .and_then(|members| members.get(path))
            .map(|bytes| std::borrow::Cow::Borrowed(bytes.as_ref()))
            .ok_or_else(|| {
                n4a::N4aError::Manifest(format!(
                    "selected scientific member `{path}` has not been supplied"
                ))
            })
    }
    fn resident_bytes(&self) -> u64 {
        self.resident
    }
}

#[wasm_bindgen]
impl NativePackageMemberReader {
    #[wasm_bindgen(constructor)]
    pub fn new(
        source: &N4aSourceHandle,
        memory_budget: Option<u64>,
    ) -> Result<NativePackageMemberReader, JsValue> {
        let budget = memory_budget
            .map(n4a::N4aMemoryBudget::bytes)
            .unwrap_or_else(|| source.metadata.memory_budget());
        budget
            .admit(source.metadata.retained_bytes())
            .map_err(refusal)?;
        Ok(Self {
            metadata: source.metadata.clone(),
            members: Some(BTreeMap::new()),
            resident: 0,
            budget,
        })
    }
    /// Call before a wasm-bindgen buffer transfer. It is an authorized working
    /// set check, independent of physical content size.
    pub fn admit_member(&self, byte_length: u64) -> Result<(), JsValue> {
        let total = self
            .resident
            .checked_add(byte_length)
            .and_then(|total| total.checked_add(self.metadata.retained_bytes()))
            .ok_or_else(|| host_refusal("native member working set overflow"))?;
        self.budget.admit(total).map_err(refusal)
    }
    pub fn add_member(&mut self, path: String, bytes: Box<[u8]>) -> Result<(), JsValue> {
        self.admit_member(bytes.len() as u64)?;
        let members = self
            .members
            .as_mut()
            .ok_or_else(|| host_refusal("native package reader already finished"))?;
        if members.contains_key(&path) {
            return Err(host_refusal("duplicate native member"));
        }
        let mut hasher = n4a::N4aMemberHasher::new();
        hasher.update(&bytes);
        self.metadata
            .verify_member(&path, &hasher.finish())
            .map_err(refusal)?;
        self.resident += bytes.len() as u64;
        members.insert(path, bytes);
        Ok(())
    }
    /// Reuse the scanned metadata and verified member buffers; no archive
    /// reconstruction, whole-file copy, rescan or second member hash occurs.
    pub fn finish(
        &mut self,
        entrypoint: Option<String>,
        operation: &str,
    ) -> Result<ValidatedPortableArchiveV2, JsValue> {
        if self.members.is_none() {
            return Err(host_refusal("native package reader already finished"));
        }
        let archive = n4a_native_package::read_native_package_for_operation(
            self,
            entrypoint.as_deref(),
            operation,
            self.budget,
        )
        .map_err(refusal)?;
        self.members = None;
        self.resident = 0;
        Ok(ValidatedPortableArchiveV2 { archive })
    }
}

#[wasm_bindgen]
impl N4aSourceHandle {
    /// Core's metadata view: `semantics` and `training_view` are `not_evaluated`.
    pub fn view_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&n4a_owner_portable::n4a_metadata_view(&self.metadata))
            .map_err(refusal)
    }

    pub fn admit(&self, bytes: u64) -> Result<(), JsValue> {
        self.metadata.memory_budget().admit(bytes).map_err(refusal)
    }

    pub fn chunk_bytes(&self) -> u64 {
        self.metadata.memory_budget().chunk_bytes()
    }

    pub fn manifest_bytes(&self) -> Result<Vec<u8>, JsValue> {
        self.admit(self.manifest.len() as u64)?;
        Ok(self.manifest.to_vec())
    }

    /// `[data_offset, byte_length]` of a member in the source, or `undefined`.
    pub fn member_range(&self, path: &str) -> Option<Vec<u64>> {
        self.metadata
            .member_location(path)
            .map(|location| vec![location.data_offset, location.byte_length])
    }

    pub fn artifact_member_paths_json(&self, artifact_id: &str) -> Result<Option<String>, JsValue> {
        match self.metadata.artifact_member_paths(artifact_id) {
            None => Ok(None),
            Some(paths) => serde_json::to_string(paths).map(Some).map_err(refusal),
        }
    }

    pub fn archive_length(&self) -> u64 {
        self.metadata.receipt().byte_length
    }

    /// Compare the bytes actually read for `path` with the validated receipt.
    pub fn verify_member(
        &self,
        path: &str,
        verifier: &mut N4aStreamVerifier,
    ) -> Result<(), JsValue> {
        let digest = verifier.digest()?;
        self.metadata.verify_member(path, &digest).map_err(refusal)
    }

    /// Compare the bytes actually read for the whole archive with the receipt.
    pub fn verify_archive(&self, verifier: &mut N4aStreamVerifier) -> Result<(), JsValue> {
        let digest = verifier.digest()?;
        self.metadata.verify_archive(&digest).map_err(refusal)
    }
}

/// Transfers member byte arrays into WASM memory one call at a time so no
/// `js-sys` dependency is needed. Each array is moved, not cloned. The builder
/// is consumed by `finish` / `finish_update`; the owner writer enforces every
/// duplicate-path, closure and memory rule. Hosts admit each array with
/// `n4a_admit` before `add_member`.
#[wasm_bindgen]
pub struct N4aWriter {
    members: Vec<(String, Box<[u8]>)>,
}

#[wasm_bindgen]
impl N4aWriter {
    #[wasm_bindgen(constructor)]
    pub fn new() -> N4aWriter {
        Self {
            members: Vec::new(),
        }
    }

    pub fn add_member(&mut self, path: String, bytes: Box<[u8]>) {
        self.members.push((path, bytes));
    }

    /// Write a new container from exactly the added members.
    pub fn finish(
        self,
        manifest: &[u8],
        memory_budget: Option<u64>,
    ) -> Result<N4aWritten, JsValue> {
        let budget = memory(memory_budget);
        let manifest = n4a::parse_n4a_manifest(manifest, budget).map_err(refusal)?;
        let request = n4a::N4aWriteRequest {
            manifest,
            members: self.inputs(),
        };
        n4a::write_n4a_bytes(request, budget)
            .map_err(refusal)
            .and_then(N4aWritten::from_owner)
    }

    /// Write a new container: every member the manifest declares takes the
    /// added bytes, otherwise the base's. The base handle is never changed.
    pub fn finish_update(
        self,
        base: &N4aContainerHandle,
        manifest: &[u8],
        memory_budget: Option<u64>,
    ) -> Result<N4aWritten, JsValue> {
        let budget = memory(memory_budget);
        let manifest = n4a::parse_n4a_manifest(manifest, budget).map_err(refusal)?;
        n4a::write_n4a_update(&base.container, manifest, self.inputs(), budget)
            .map_err(refusal)
            .and_then(N4aWritten::from_owner)
    }
}

impl Default for N4aWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl N4aWriter {
    fn inputs(&self) -> Vec<n4a::N4aMemberInput<'_>> {
        self.members
            .iter()
            .map(|(path, bytes)| n4a::N4aMemberInput { path, bytes })
            .collect()
    }
}

/// A written container: the receipt as JSON text and the archive bytes,
/// which `into_bytes` hands over without cloning.
#[wasm_bindgen]
pub struct N4aWritten {
    bytes: Vec<u8>,
    receipt_json: String,
}

impl N4aWritten {
    fn from_owner((receipt, bytes): (n4a::N4aReceipt, Vec<u8>)) -> Result<N4aWritten, JsValue> {
        Ok(Self {
            receipt_json: receipt_to_json(&receipt)?,
            bytes,
        })
    }
}

#[wasm_bindgen]
impl N4aWritten {
    pub fn receipt_json(&self) -> String {
        self.receipt_json.clone()
    }

    /// Consumes this object; read `receipt_json` first.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Two-pass streaming writer driven over Core's sans-IO `N4aWritePlan`. Pass 1:
/// the host hashes every new member through an `N4aStreamVerifier` and registers
/// it with `add_member`. `build*` completes the physical set (members the
/// manifest declares and the host did not supply come from the base inventory,
/// without hashing) and fixes the exact archive size. Pass 2: the host loops
/// `next_step`, appending every frame to its sink and streaming every member's
/// payload through `update_member`; a source that changed between the passes is
/// refused by Core, and the sink must then be discarded.
#[wasm_bindgen]
pub struct N4aStreamWriter {
    manifest: Option<n4a::N4aManifest>,
    budget: n4a::N4aMemoryBudget,
    planned: Vec<n4a::N4aPlannedMember>,
    plan: Option<n4a::N4aWritePlan>,
    step: Option<n4a::N4aWriteStep>,
}

#[wasm_bindgen]
impl N4aStreamWriter {
    /// `manifest` is the exact manifest JSON; it is parsed by Core.
    #[wasm_bindgen(constructor)]
    pub fn new(manifest: &[u8], memory_budget: Option<u64>) -> Result<N4aStreamWriter, JsValue> {
        let budget = memory(memory_budget);
        Ok(Self {
            manifest: Some(n4a::parse_n4a_manifest(manifest, budget).map_err(refusal)?),
            budget,
            planned: Vec::new(),
            plan: None,
            step: None,
        })
    }

    /// Register one new member by the digest of the bytes the host just read.
    pub fn add_member(
        &mut self,
        path: String,
        verifier: &mut N4aStreamVerifier,
    ) -> Result<(), JsValue> {
        let digest = verifier.digest()?;
        self.planned.push(n4a::N4aPlannedMember { path, digest });
        Ok(())
    }

    /// Plan from the added members alone.
    pub fn build(&mut self) -> Result<(), JsValue> {
        self.plan_with(|_| None)
    }

    /// Plan an update of a bytes-held base. Its retained digests come from the
    /// base's validated inventory; the base is never changed.
    pub fn build_from_bytes_base(&mut self, base: &N4aContainerHandle) -> Result<(), JsValue> {
        self.plan_with(|path| base.container.member_digest(path))
    }

    /// Plan an update of an externally held base (same rules).
    pub fn build_from_source_base(&mut self, base: &N4aSourceHandle) -> Result<(), JsValue> {
        self.plan_with(|path| base.metadata.member_digest(path))
    }

    /// Exact final archive size, known before any byte is written.
    pub fn byte_length(&self) -> Result<u64, JsValue> {
        Ok(self.plan_ref()?.byte_length())
    }

    /// Advance the plan: 0 = complete, 1 = a frame to append (`frame`),
    /// 2 = a member payload to stream (`member_path`, `member_length`).
    pub fn next_step(&mut self) -> Result<u8, JsValue> {
        let plan = self
            .plan
            .as_mut()
            .ok_or_else(|| host_refusal("n4a stream writer has no plan yet"))?;
        self.step = plan.next_step().map_err(refusal)?;
        Ok(match &self.step {
            None => 0,
            Some(n4a::N4aWriteStep::Frame(_)) => 1,
            Some(n4a::N4aWriteStep::Member { .. }) => 2,
        })
    }

    /// The frame bytes of the current step, to append to the sink unchanged.
    pub fn frame(&mut self) -> Result<Vec<u8>, JsValue> {
        match self.step.take() {
            Some(n4a::N4aWriteStep::Frame(bytes)) => Ok(bytes),
            other => {
                self.step = other;
                Err(host_refusal("the current n4a write step is not a frame"))
            }
        }
    }

    pub fn member_path(&self) -> Result<String, JsValue> {
        match &self.step {
            Some(n4a::N4aWriteStep::Member { path, .. }) => Ok(path.clone()),
            _ => Err(host_refusal("the current n4a write step is not a member")),
        }
    }

    pub fn member_length(&self) -> Result<u64, JsValue> {
        match &self.step {
            Some(n4a::N4aWriteStep::Member { byte_length, .. }) => Ok(*byte_length),
            _ => Err(host_refusal("the current n4a write step is not a member")),
        }
    }

    /// Account one payload chunk of the current member. Core compares the member's
    /// digest at the following `next_step`.
    pub fn update_member(&mut self, chunk: &[u8]) -> Result<(), JsValue> {
        self.plan
            .as_mut()
            .ok_or_else(|| host_refusal("n4a stream writer has no plan yet"))?
            .update_member(chunk)
            .map_err(refusal)
    }

    /// Complete the write; the container SHA-256 comes from the bytes written.
    /// Returns the receipt as JSON text.
    pub fn finish(&mut self) -> Result<String, JsValue> {
        let plan = self
            .plan
            .take()
            .ok_or_else(|| host_refusal("n4a stream writer has no plan yet"))?;
        receipt_to_json(&plan.finish().map_err(refusal)?)
    }
}

impl N4aStreamWriter {
    fn plan_ref(&self) -> Result<&n4a::N4aWritePlan, JsValue> {
        self.plan
            .as_ref()
            .ok_or_else(|| host_refusal("n4a stream writer has no plan yet"))
    }

    fn plan_with(
        &mut self,
        retained: impl Fn(&str) -> Option<n4a::N4aMemberDigest>,
    ) -> Result<(), JsValue> {
        let manifest = self
            .manifest
            .take()
            .ok_or_else(|| host_refusal("n4a stream writer was already planned"))?;
        let mut planned = std::mem::take(&mut self.planned);
        let supplied: std::collections::BTreeSet<String> =
            planned.iter().map(|member| member.path.clone()).collect();
        // A declared member the host did not supply comes from the base. A path
        // neither side holds stays missing and Core's plan refuses the mismatch.
        for member in &manifest.members {
            if !supplied.contains(&member.path) {
                if let Some(digest) = retained(&member.path) {
                    planned.push(n4a::N4aPlannedMember {
                        path: member.path.clone(),
                        digest,
                    });
                }
            }
        }
        self.plan = Some(n4a::N4aWritePlan::new(manifest, planned, self.budget).map_err(refusal)?);
        Ok(())
    }
}
