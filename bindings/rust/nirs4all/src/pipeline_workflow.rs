//! Product recipes compose the live native Methods catalog into DAG nodes.
//! Numerics, execution, leakage validation and portable state belong upstream.
//! DAG assembles the request; this module supplies the native manifests.
use std::collections::BTreeMap;
use std::path::Path;

use dag_ml_core::artifact_facets::{build_training_artifact_facets, ArtifactFacetExport};
use dag_ml_core::{ControllerManifest, PortablePredictorPackage, TrainingOutcome, TrainingRequest};
use serde_json::{json, Value};

pub use crate::n4a_owner_portable::NativePipelineProducerError;
pub use crate::native_pipeline_contract::{
    validate_pipeline, NativePipelineRecipe, NativePipelineStep,
};

use crate::io_training::{DatasetPackage, DatasetPackageMethodsProvider};
use crate::n4a_owner_portable::{
    canonical_export_training_request, check_training_retention, native_linear_facts,
};

/// Complete native producer output before container composition. DAG builds
/// every scientific facet/link and retains the exact fitted-state payloads.
pub struct NativePipelineTrainingExport {
    pub dataset: DatasetPackage,
    pub request: TrainingRequest,
    pub training: TrainingOutcome,
    pub package: PortablePredictorPackage,
    pub facets: ArtifactFacetExport,
}

/// Live native catalog manifests, as the DAG request assembler requires them.
fn native_controller_manifests(library: &Path) -> Result<Vec<ControllerManifest>, String> {
    let runtime = crate::native_methods_replay::configure_methods_runtime_for_source(library)
        .map_err(|e| e.to_string())?;
    let specs = dag_ml_core::methods_estimator_host_controller_specs(&runtime)
        .map_err(|e| e.to_string())?;
    specs
        .iter()
        .map(|s| s.derive())
        .collect::<dag_ml_core::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

/// Construct a fingerprint-bound, catalog-resolved training request for a linear native
/// pipeline. Each fitted transform executes separately inside every CV fold.
pub fn native_pipeline_training_request(
    dataset: &DatasetPackage,
    source_id: &str,
    recipe: &NativePipelineRecipe,
    library: &Path,
) -> Result<TrainingRequest, String> {
    let facts = native_linear_facts(dataset, source_id, recipe)?;
    let manifests = native_controller_manifests(library)?;
    dag_ml_core::native_pipeline_training_request(&facts, recipe, &manifests)
}

/// Prepare the canonical single-source CV producer using IO's public record
/// projection. This separate path never changes the legacy transport request.
/// The returned package is the same package whose provider attests training.
pub fn native_pipeline_export_training_request(
    dataset_record: &Value,
    source_id: &str,
    recipe: &NativePipelineRecipe,
    library: &Path,
) -> Result<(DatasetPackage, TrainingRequest), String> {
    canonical_export_training_request(dataset_record, source_id, recipe, |dataset| {
        native_pipeline_training_request(dataset, source_id, recipe, library)
    })
}

/// Train through the existing DAG/Methods execution and build its complete
/// owner facet closure. Container entrypoints and IO anchors are composed by
/// the native action module; this function does not invent dataset links.
pub fn train_native_pipeline_for_export(
    dataset_record: &Value,
    source_id: &str,
    recipe: &NativePipelineRecipe,
    library: &Path,
    run_id: &str,
) -> Result<NativePipelineTrainingExport, NativePipelineProducerError> {
    let (dataset, request) =
        native_pipeline_export_training_request(dataset_record, source_id, recipe, library)
            .map_err(NativePipelineProducerError::Training)?;
    let (training, package) =
        execute_pipeline_training(&dataset, source_id, recipe, library, run_id, &request)
            .map_err(NativePipelineProducerError::Training)?;
    check_training_retention(&request, &training, &package)?;
    let facets = build_training_artifact_facets(&training, &package)
        .map_err(NativePipelineProducerError::Owner)?;
    Ok(NativePipelineTrainingExport {
        dataset,
        request,
        training,
        package,
        facets,
    })
}

fn execute_pipeline_training(
    dataset: &DatasetPackage,
    source_id: &str,
    recipe: &NativePipelineRecipe,
    library: &Path,
    run_id: &str,
    request: &TrainingRequest,
) -> Result<(TrainingOutcome, PortablePredictorPackage), String> {
    let provider = DatasetPackageMethodsProvider::for_product_pipeline(
        dataset,
        source_id,
        recipe
            .steps
            .last()
            .is_some_and(|step| step.role == "classifier"),
    )?;
    let mut artifacts = dag_ml_core::InMemoryArtifactStore::new();
    let training = crate::io_training::execute_dataset_package_methods_training(
        &provider,
        request,
        &format!("outcome:{run_id}"),
        dag_ml_core::RunId::new(run_id).map_err(|e| e.to_string())?,
        dag_ml_core::BundleId::new(format!("bundle:{run_id}")).map_err(|e| e.to_string())?,
        library,
        &mut artifacts,
    )?;
    let package = training
        .to_portable_predictor_package(
            format!("package:{run_id}"),
            dag_ml_core::FittedArtifactMode::PortableRequired,
            dag_ml_core::ArtifactLoadMode::NativePortable,
        )
        .map_err(|e| e.to_string())?;
    Ok((training, package))
}

/// Train a portable native pipeline with CV/OOF selection and full refit.
/// The JSON transport is a DAG Package V2, distinct from a Core .n4a archive.
pub fn run_pipeline(
    record: &Value,
    source_id: &str,
    library: &Path,
    run_id: &str,
) -> Result<Value, String> {
    run_pipeline_with_contract(record, source_id, library, run_id, None)
}

pub(crate) fn run_pipeline_with_contract(
    record: &Value,
    source_id: &str,
    library: &Path,
    run_id: &str,
    contract: Option<&Value>,
) -> Result<Value, String> {
    if record
        .as_object()
        .is_none_or(|o| o.len() != 2 || !o.contains_key("dataset") || !o.contains_key("pipeline"))
    {
        return Err("pipeline-run requires exactly dataset and pipeline".into());
    }
    let recipe: NativePipelineRecipe =
        serde_json::from_value(record["pipeline"].clone()).map_err(|e| e.to_string())?;
    let (dataset, _) = nirs4all_io_crate::core::public_dataset::masked_matrix_dataset_package(
        &record["dataset"],
        source_id,
    )?;
    let mut request = native_pipeline_training_request(&dataset, source_id, &recipe, library)?;
    if let Some(contract) = contract {
        request
            .campaign
            .metadata
            .insert("native_multimodal_contract".into(), contract.clone());
        request.request_fingerprint = request.compute_fingerprint().map_err(|e| e.to_string())?;
        request.validate().map_err(|e| e.to_string())?;
    }
    let (training, package) =
        execute_pipeline_training(&dataset, source_id, &recipe, library, run_id, &request)?;
    Ok(
        json!({"schema":"nirs4all.native-pipeline.v1","schema_version":1,
        "config":{"source_id":source_id,"pipeline":recipe},"training_outcome":training,"package":package}),
    )
}

/// Cold, target-free replay is executed entirely by native DAG controllers.
pub fn predict_pipeline(record: &Value, library: &Path, run_id: &str) -> Result<Value, String> {
    let keys = ["model", "sample_ids", "x"];
    if record
        .as_object()
        .is_none_or(|o| o.len() != keys.len() || keys.iter().any(|k| !o.contains_key(*k)))
    {
        return Err("pipeline-predict requires model, sample_ids and x".into());
    }
    let package = validate_pipeline(&record["model"])?;
    let sample_ids: Vec<String> =
        serde_json::from_value(record["sample_ids"].clone()).map_err(|e| e.to_string())?;
    let x: Vec<Vec<f64>> =
        serde_json::from_value(record["x"].clone()).map_err(|e| e.to_string())?;
    let binding = package
        .output_bindings
        .first()
        .ok_or("missing pipeline output")?;
    let mut composition = crate::native_methods_replay::compose_methods_matrix_predict(
        &package,
        crate::MethodsArchiveMatrixPredictRequest {
            sample_ids,
            x,
            expected_target_names: binding.target_names.clone(),
            methods_library_path: library.to_path_buf(),
            methods_library_sha256: String::new(),
            request_id: format!("predict:{run_id}"),
            outcome_id: format!("outcome:{run_id}"),
            run_id: dag_ml_core::RunId::new(run_id).map_err(|e| e.to_string())?,
            warnings: Vec::new(),
            diagnostics: BTreeMap::new(),
        },
        true,
    )
    .map_err(|e| e.to_string())?;
    if let Some(names) = package
        .template
        .campaign
        .metadata
        .get("native_pipeline_target_names")
    {
        let names: Vec<String> =
            serde_json::from_value(names.clone()).map_err(|e| e.to_string())?;
        for rows in composition.input.methods_inputs.values_mut() {
            rows.target_names = names.clone();
        }
    }
    let replay =
        crate::native_methods_replay::replay_methods_predictor_package(&package, composition.input)
            .map_err(|e| e.to_string())?;
    serde_json::to_value(replay).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "../tests/fixtures/n4a_native_actions_v1/request_transition.rs"]
mod request_transition;
