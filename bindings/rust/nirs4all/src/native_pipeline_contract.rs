//! Pure product recipe/envelope contracts, reusable in CPU and WASM bindings.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePipelineStep {
    pub method_id: String,
    pub role: String,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePipelineRecipe {
    pub steps: Vec<NativePipelineStep>,
    pub candidates: Vec<BTreeMap<String, Value>>,
}

/// Validate the product envelope and its signed upstream package/outcome closure.
pub fn validate_pipeline(record: &Value) -> Result<dag_ml_core::PortablePredictorPackage, String> {
    let keys = [
        "schema",
        "schema_version",
        "config",
        "training_outcome",
        "package",
    ];
    if record
        .as_object()
        .is_none_or(|o| o.len() != keys.len() || keys.iter().any(|k| !o.contains_key(*k)))
        || record["schema"] != "nirs4all.native-pipeline.v1"
        || record["schema_version"] != 1
    {
        return Err("unsupported native pipeline envelope".into());
    }
    let package = dag_ml_core::PortablePredictorPackage::from_json(
        &serde_json::to_string(&record["package"]).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let outcome = dag_ml_core::TrainingOutcome::from_json(
        &serde_json::to_string(&record["training_outcome"]).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    outcome.validate().map_err(|e| e.to_string())?;
    if package.training_outcome != outcome.to_reference().map_err(|e| e.to_string())?
        || package.execution_bundle != outcome.execution_bundle
        || package.effective_plan != outcome.effective_plan
    {
        return Err("pipeline package differs from its native training outcome".into());
    }
    let config = &record["config"];
    let recipe: NativePipelineRecipe =
        serde_json::from_value(config["pipeline"].clone()).map_err(|e| e.to_string())?;
    if recipe.steps.is_empty()
        || recipe.steps.len() > 32
        || recipe.candidates.is_empty()
        || recipe.candidates.len() > 32
    {
        return Err("invalid native recipe dimensions".into());
    }
    if config
        .as_object()
        .is_none_or(|o| o.len() != 2 || !o.contains_key("source_id") || !o.contains_key("pipeline"))
        || package
            .template
            .campaign
            .metadata
            .get("native_pipeline_recipe")
            != Some(&config["pipeline"])
    {
        return Err("pipeline choices differ from signed training recipe".into());
    }
    let sources: Vec<_> = package
        .template
        .campaign
        .data_bindings
        .values()
        .flatten()
        .flat_map(|b| b.source_ids.iter())
        .collect();
    if sources.len() != 1 || config["source_id"].as_str() != Some(sources[0].as_str()) {
        return Err("pipeline source differs from signed IO binding".into());
    }
    if package
        .execution_bundle
        .refit_artifacts
        .iter()
        .any(|r| r.artifact.kind != "n4m_estimator")
    {
        return Err("native pipeline accepts only native N4ME estimator states".into());
    }
    Ok(package)
}

/// Native parse/validation preserves uint64 values before any host presentation.
pub fn parse_native_pipeline_record(input: &str) -> Result<Value, String> {
    let record: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    validate_pipeline(&record)?;
    Ok(record)
}
