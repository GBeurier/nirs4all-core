//! Rust surface for the nirs4all-core aggregate distribution.
//!
//! This crate intentionally starts as a registry and namespace layer. Runtime
//! functionality must delegate to the owning upstream crates.

#[cfg(test)]
use std::cell::Cell;
use std::ffi::CStr;
use std::os::raw::{c_char, c_double, c_int, c_void};
use std::path::Path;
use std::ptr;

use libloading::{Library, Symbol};
use serde_json::Value;

mod archive_codec;
pub mod archive_command;
mod archive_storage;
mod archive_view;
mod capability_profiles_data;
pub mod conformal;
pub mod conformal_json;
mod durability;
mod formats_io;
pub mod generation;
mod io_training;
pub mod n4a_owner_native;
pub use n4a_owner_native::{
    calibrate_n4a_native, parse_native_recipe_json, predict_n4a_native, produce_n4a_native,
    produce_n4a_native_with_view, read_n4a_training_view, retrain_n4a_native, N4aTrainingView,
    NativeActionError, NativeActionReceipt, NativeActionRequest, NativeCalibration,
    NativeCalibrationOptions, NativeInput, NativePrediction, NativeProduceRequest,
    NativeProduction, NativeRetrainRequest, NativeRuntime, NativeSchemaPolicy,
};
pub mod multimodal_workflow;
pub mod n4a;
pub mod n4a_file;
pub mod n4a_native_package;
pub use n4a_native_package::{LoadedNativePackage, NativePackageReference};
pub mod n4a_owner;
pub mod n4a_owner_portable;
pub mod n4a_physical;
mod n4m_roles;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/fixtures/n4a_native_actions_v1/data.rs"]
pub(crate) mod native_action_test_data;
mod native_methods_replay;
pub mod native_pipeline_contract;
pub mod pipeline_workflow;
pub mod portable_execution;
mod portable_session;
mod publication;
pub mod result_projection;
pub mod results;
pub mod robustness;
pub mod robustness_json;
#[cfg(test)]
mod test_fixture;
pub mod tuning;
mod workflow;
pub use archive_storage::ArchiveStoreError;
pub use archive_view::{native_package_view, ArchiveViewError};
pub use dag_ml_core::NativePredictorDescriptorV1;
pub use formats_io::{
    load_spectrum_dataset_package, load_spectrum_methods_provider, FormatsIoError,
    LoadedSpectrumDataset,
};
pub use io_training::{
    canonical_pls_training_request, train_dataset_package_methods_archive_v2,
    train_dataset_package_methods_conformal_archive_v2, CanonicalPlsProfile, DatasetPackage,
    DatasetPackageMethodsArchiveV2Outcome, DatasetPackageMethodsArchiveV2Request,
    DatasetPackageMethodsConformalArchiveV2Outcome, DatasetPackageMethodsConformalArchiveV2Request,
    DatasetPackageMethodsProvider,
};
pub use n4m_roles::{
    n4m_role_capabilities, n4m_role_method_id, resolve_n4m_role, ClassLabel, N4mRolePipeline,
    RolePredictions, RoleTarget, N4M_ROLE_PREFIX, N4M_TRAINED_PIPELINE_SCHEMA,
};
pub use native_methods_replay::{
    inspect_methods_archive_v2_predictors, inspect_methods_archive_v2_predictors_json,
    load_methods_archive_v2_conformal_presentation_v2, predict_methods_archive_v2_matrix,
    predict_methods_archive_v2_matrix_conformal_presentation_v2,
    predict_methods_archive_v2_matrix_json, predict_tuning_archive_v2_matrix,
    predict_tuning_archive_v2_matrix_json, preflight_methods_archive_v2_library,
    replay_methods_archive_v2, replay_methods_archive_v2_conformal_presentation_v1,
    replay_methods_archive_v2_conformal_presentation_v1_json,
    replay_methods_archive_v2_conformal_presentation_v1_loaded_json,
    replay_methods_archive_v2_conformal_presentation_v2,
    replay_methods_archive_v2_conformal_presentation_v2_json,
    replay_methods_archive_v2_conformal_presentation_v2_loaded_json,
    replay_methods_archive_v2_json, replay_methods_archive_v2_loaded_json,
    replay_methods_archive_v3, replay_methods_archive_v3_json,
    replay_methods_archive_v3_loaded_json, MethodsArchiveMatrixPredictJsonRequest,
    MethodsArchiveMatrixPredictRequest, MethodsArchivePredictRequest, MethodsArchiveRefitRequestV3,
    MethodsArchiveReplayJsonRequest, NativeMethodsReplayError,
};
pub use portable_session::{
    PortableSession, PortableSessionError, PortableSessionState, PORTABLE_SESSION_EXPORT_SCHEMA,
    PORTABLE_SESSION_EXPORT_SCHEMA_V2,
};
pub use publication::publish_directory_noreplace;
pub use workflow::{
    dense_regression_training_request, dense_regression_workflow_config,
    preflight_workflow_archive_path, run_dense_regression_workflow, DenseRegressionPreprocessing,
    DenseRegressionWorkflowRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upstream {
    pub key: &'static str,
    pub package: &'static str,
    pub role: &'static str,
}

pub const UPSTREAMS: &[Upstream] = &[
    Upstream {
        key: "dag_ml",
        package: "dag-ml",
        role: "Leakage-safe DAG/ML execution coordinator",
    },
    Upstream {
        key: "dag_ml_data",
        package: "dag-ml-data",
        role: "Sample-aligned data contracts for DAG/ML runtimes",
    },
    Upstream {
        key: "formats",
        package: "nirs4all-formats",
        role: "Spectroscopy/NIRS vendor file readers",
    },
    Upstream {
        key: "io",
        package: "nirs4all-io",
        role: "Dataset assembly bridge",
    },
    Upstream {
        key: "datasets",
        package: "nirs4all-datasets",
        role: "DOI-pinned NIRS dataset catalog",
    },
    Upstream {
        key: "methods",
        package: "nirs4all-methods",
        role: "Portable C ABI PLS/NIRS numerical engine",
    },
];

// Private runner constant: the temporary portable runner/compiler still validates
// against it until its callers migrate to owner APIs. It is not a capability claim.
const PORTABLE_OPERATOR_CLASSES: &[&str] = &[
    "nirs4all.operators.splitters.KennardStoneSplitter",
    "nirs4all.operators.splitters.splitters.KennardStoneSplitter",
    "n4m.KennardStone",
    "nirs4all.operators.transforms.SNV",
    "nirs4all.operators.transforms.StandardNormalVariate",
    "nirs4all.operators.transforms.scalers.StandardNormalVariate",
    "n4m.SNV",
    "nirs4all.operators.transforms.SavitzkyGolay",
    "nirs4all.operators.transforms.nirs.SavitzkyGolay",
    "n4m.SavitzkyGolay",
    "sklearn.cross_decomposition.PLSRegression",
    "sklearn.cross_decomposition._pls.PLSRegression",
    "n4m.PLS",
    "n4m.PLSRegression",
];

pub const RUNTIME_SURFACES: &[&str] = &["python", "javascript_wasm", "rust", "matlab_octave"];

/// Versioned native execution profiles and the external R federation record,
/// parsed from the data generated from `compat/capabilities.toml`.
pub fn execution_profiles() -> Value {
    serde_json::from_str(capability_profiles_data::EXECUTION_PROFILES_JSON)
        .expect("generated execution profiles are valid JSON")
}

pub fn capability_manifest() -> Value {
    serde_json::json!({
        "schema": "nirs4all-core.capabilities.v1",
        "aggregate": "nirs4all-core",
        "runtime_surfaces": RUNTIME_SURFACES,
        "execution_profiles": execution_profiles(),
    })
}

pub fn upstream(key: &str) -> Option<&'static Upstream> {
    UPSTREAMS.iter().find(|item| item.key == key)
}

pub type LocalImplementationRegistry<T> = dag_ml::LocalImplementationRegistry<T>;

pub fn local_implementation_registry<T>() -> LocalImplementationRegistry<T> {
    LocalImplementationRegistry::new()
}

pub fn load_pipeline_definition_str(input: &str) -> Result<Value, String> {
    let mut value = match serde_json::from_str::<Value>(input) {
        Ok(value) => value,
        Err(json_error) => {
            let yaml_value =
                serde_yaml::from_str::<serde_yaml::Value>(input).map_err(|yaml_error| {
                    format!("invalid JSON ({json_error}) and YAML ({yaml_error})")
                })?;
            serde_json::to_value(yaml_value)
                .map_err(|error| format!("could not normalize YAML to JSON value: {error}"))?
        }
    };

    normalize_pipeline_root(&mut value)?;

    let pipeline = value.get_mut("pipeline").ok_or_else(|| {
        "pipeline definition must contain a 'pipeline' or 'steps' key".to_string()
    })?;
    if !pipeline.is_array() {
        return Err(
            "pipeline definition key 'pipeline' or 'steps' must contain an array of steps"
                .to_string(),
        );
    }
    strip_comments_in_place(pipeline);

    let classes = portable_class_names(&value);
    let mut unsupported = Vec::new();
    for name in &classes {
        if n4m_role_method_id(name).is_some() {
            resolve_n4m_role(name)?;
        } else if !PORTABLE_OPERATOR_CLASSES.contains(&name.as_str()) {
            unsupported.push(name.clone());
        }
    }
    if !unsupported.is_empty() {
        return Err(format!(
            "pipeline uses operators outside the current nirs4all-core portable subset: {}",
            unsupported.join(", ")
        ));
    }

    Ok(value)
}

pub fn portable_class_names(value: &Value) -> Vec<String> {
    let root = value.get("pipeline").unwrap_or(value);
    let mut classes = Vec::new();
    collect_classes(root, &mut classes);
    classes
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortableDataset {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub rows: usize,
    pub cols: usize,
}

impl PortableDataset {
    pub fn from_json_value(value: &Value) -> Result<Self, String> {
        let rows = value
            .get("rows")
            .or_else(|| value.get("n_samples"))
            .and_then(Value::as_u64)
            .ok_or_else(|| "dataset must provide rows or n_samples".to_string())?
            as usize;
        let cols = value
            .get("cols")
            .or_else(|| value.get("n_features"))
            .and_then(Value::as_u64)
            .ok_or_else(|| "dataset must provide cols or n_features".to_string())?
            as usize;
        let x = flatten_matrix(
            value
                .get("X")
                .ok_or_else(|| "dataset must contain X".to_string())?,
            rows,
            cols,
            "X",
        )?;
        let y = flatten_matrix(
            value
                .get("y")
                .ok_or_else(|| "dataset must contain y".to_string())?,
            rows,
            1,
            "y",
        )?;
        Ok(Self { x, y, rows, cols })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionPlan {
    pub splitter: Option<PortableSplitter>,
    pub preprocessing: Vec<PortablePreprocessing>,
    pub n_components: Vec<i32>,
    /// Complete owner admission; compact compatibility fields never erase flags.
    pub owner_admission: dag_ml_core::PortableLinearAdmission,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortableSplitter {
    pub kind: String,
    pub test_size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PortablePreprocessing {
    StandardNormalVariate,
    SavitzkyGolay(SavitzkyGolayParams),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SavitzkyGolayParams {
    pub window_length: i32,
    pub polyorder: i32,
    pub deriv: i32,
    pub mode: i32,
    pub cval: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmbeddedPipelineSpec {
    savgol_window: i32,
    savgol_poly_degree: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortablePipelineResult {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    pub split: PortableSplitResult,
    pub preprocessing: Vec<PortablePreprocessingResult>,
    pub variants: Vec<PortableVariant>,
    pub selected: PortableVariant,
    pub targets: Vec<f64>,
}

impl PortablePipelineResult {
    /// The reported RMSE is a training or selection score, never an independent test.
    pub fn evaluation_scope(&self) -> &'static str {
        if self.split.kind == "all" {
            "training"
        } else {
            "selection_validation"
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortableSplitResult {
    pub kind: String,
    pub train_indices: Vec<usize>,
    pub test_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortablePreprocessingResult {
    pub kind: String,
    pub params: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortableVariant {
    pub n_components: i32,
    pub rmse: f64,
    pub predictions: Vec<f64>,
}

pub fn parse_execution_plan_str(input: &str) -> Result<ExecutionPlan, String> {
    parse_execution_plan(&portable_execution::decode_definition(input)?)
}

pub fn parse_execution_plan(definition: &Value) -> Result<ExecutionPlan, String> {
    let admission = dag_ml_core::admit_portable_linear(definition).map_err(|e| e.to_string())?;
    portable_execution::admission_plan(admission)
}

pub fn run_portable_pipeline_with_library(
    input: &str,
    dataset: &PortableDataset,
    library_path: impl AsRef<Path>,
) -> Result<PortablePipelineResult, String> {
    let definition = portable_execution::decode_definition(input)?;
    // Preserve the historical Rust PLS surface; the generic dispatcher shares
    // the same executor while carrying declared IO identities when supplied.
    parse_execution_plan(&definition)?;
    let result = portable_execution::run(
        &serde_json::json!({"schema_version":1,"definition":definition,
        "dataset":{"X":dataset.x,"y":dataset.y,"rows":dataset.rows,"cols":dataset.cols}}),
        library_path.as_ref(),
    )?;
    portable_execution::legacy_result(&result)
}

/// Run the portable subset and export the selected native PLS model as an
/// `N4MM` payload owned by libn4m. Methods selects the current payload format;
/// exact default SNV -> Savitzky-Golay smoothing may be embedded, while other
/// admitted stateless recipes replay their complete parameters externally.
pub fn run_portable_pipeline_with_exported_model(
    input: &str,
    dataset: &PortableDataset,
    library_path: impl AsRef<Path>,
) -> Result<(PortablePipelineResult, Vec<u8>), String> {
    // N4MM representability is decided before any evaluation fit.
    let plan = parse_execution_plan_str(input)?;
    let model_params = plan
        .owner_admission
        .recipe
        .last()
        .ok_or("owner model missing")?["params"]
        .as_object()
        .ok_or("owner model params missing")?
        .clone();
    let embedded_pipeline = embedded_pipeline_spec(&plan);
    let result = run_portable_pipeline_with_library(input, dataset, &library_path)?;
    let methods = MethodsLibrary::load(library_path)?;
    let train = &result.split.train_indices;
    let mut x_train = select_rows(&dataset.x, dataset.rows, dataset.cols, train)?;
    let y_train = select_rows(&dataset.y, dataset.rows, 1, train)?;
    if embedded_pipeline.is_none() {
        x_train =
            transform_portable_preprocessing(&plan, &methods, &x_train, train.len(), dataset.cols)?;
    }
    let (_, model) = methods.fit_predict_pls(
        PlsFitInput {
            x_train: &x_train,
            y_train: &y_train,
            train_rows: train.len(),
            cols: dataset.cols,
            n_components: result.selected.n_components,
            x_test: &x_train,
            test_rows: train.len(),
            embedded_pipeline,
            model_params: &model_params,
        },
        true,
    )?;
    Ok((result, model.expect("model export requested")))
}

/// Replay a selected `N4MM` model under the content-inspected preprocessing
/// contract. Historical model-only payloads apply the portable definition;
/// embedded Methods pipelines consume raw input directly. This is intentionally
/// limited to the same executable subset accepted by
/// [`run_portable_pipeline_with_library`].
pub fn predict_exported_portable_model_with_library(
    input: &str,
    model_n4mm: &[u8],
    x: &[f64],
    rows: usize,
    cols: usize,
    library_path: impl AsRef<Path>,
) -> Result<Vec<f64>, String> {
    let plan = parse_execution_plan_str(input)?;
    let mut transformed = x.to_vec();
    if transformed.len()
        != rows
            .checked_mul(cols)
            .ok_or("prediction matrix dimensions overflow")?
    {
        return Err(format!(
            "prediction matrix length {} does not match {rows}x{cols}",
            transformed.len()
        ));
    }
    let library_path = library_path.as_ref();
    let descriptor = inspect_portable_model_descriptor(model_n4mm, library_path)?;
    validate_portable_model_plan(&plan, &descriptor)?;
    let methods = MethodsLibrary::load(library_path)?;
    if descriptor.pipeline.is_none() {
        transformed = transform_portable_preprocessing(&plan, &methods, &transformed, rows, cols)?;
    }
    methods.import_predict_pls(model_n4mm, &transformed, rows, cols)
}

/// Preflight a persisted selected model against an explicit libn4m runtime.
pub fn validate_exported_portable_model_with_library(
    input: &str,
    model_n4mm: &[u8],
    cols: usize,
    library_path: impl AsRef<Path>,
) -> Result<(), String> {
    let plan = parse_execution_plan_str(input)?;
    let library_path = library_path.as_ref();
    let descriptor = inspect_portable_model_descriptor(model_n4mm, library_path)?;
    validate_portable_model_plan(&plan, &descriptor)?;
    let methods = MethodsLibrary::load(library_path)?;
    methods
        .import_predict_pls(model_n4mm, &vec![0.0; cols], 1, cols)
        .map(|_| ())
}

fn embedded_pipeline_spec(plan: &ExecutionPlan) -> Option<EmbeddedPipelineSpec> {
    let snv = plan.owner_admission.recipe.first()?;
    if snv["methodId"] != "preprocessing.scatter.snv"
        || snv["params"]["with_mean"] != true
        || snv["params"]["with_std"] != true
        || snv["params"]["ddof"] != 0
    {
        return None;
    }
    let [PortablePreprocessing::StandardNormalVariate, PortablePreprocessing::SavitzkyGolay(sg)] =
        plan.preprocessing.as_slice()
    else {
        return None;
    };
    (sg.deriv == 0
        && sg.mode == 4
        && sg.cval.to_bits() == 0.0f64.to_bits()
        && (3..=501).contains(&sg.window_length)
        && sg.window_length % 2 == 1
        && sg.polyorder >= 0
        && sg.polyorder < sg.window_length)
        .then_some(EmbeddedPipelineSpec {
            savgol_window: sg.window_length,
            savgol_poly_degree: sg.polyorder,
        })
}

fn transform_portable_preprocessing(
    plan: &ExecutionPlan,
    methods: &MethodsLibrary,
    input: &[f64],
    rows: usize,
    cols: usize,
) -> Result<Vec<f64>, String> {
    let mut transformed = input.to_vec();
    for (index, step) in plan.preprocessing.iter().enumerate() {
        transformed = match step {
            PortablePreprocessing::StandardNormalVariate => {
                let params = &plan.owner_admission.recipe[index]["params"];
                methods.snv_transform(
                    &transformed,
                    rows,
                    cols,
                    params["with_mean"]
                        .as_bool()
                        .ok_or("owner SNV with_mean missing")?,
                    params["with_std"]
                        .as_bool()
                        .ok_or("owner SNV with_std missing")?,
                    i32::try_from(params["ddof"].as_i64().ok_or("owner SNV ddof missing")?)
                        .map_err(|_| "SNV ddof outside i32")?,
                )?
            }
            PortablePreprocessing::SavitzkyGolay(params) => {
                methods.savgol_transform(&transformed, rows, cols, params)?
            }
        };
    }
    Ok(transformed)
}

pub(crate) fn inspect_portable_model_descriptor(
    model_n4mm: &[u8],
    library_path: &Path,
) -> Result<NativePredictorDescriptorV1, String> {
    let canonical = std::fs::canonicalize(library_path).map_err(|error| {
        format!(
            "cannot resolve libn4m at {} for N4MM inspection: {error}",
            library_path.display()
        )
    })?;
    native_methods_replay::configure_methods_runtime_for_source(&canonical)
        .map_err(|error| format!("cannot configure Methods for N4MM inspection: {error}"))?;
    let controller = dag_ml_core::ControllerId::new("controller:methods.pls")
        .map_err(|error| error.to_string())?;
    dag_ml_core::inspect_methods_native_predictor_descriptor_v1(&controller, model_n4mm)
        .map_err(|error| format!("native Methods predictor inspection failed: {error}"))
}

pub(crate) fn validate_portable_model_plan(
    plan: &ExecutionPlan,
    descriptor: &NativePredictorDescriptorV1,
) -> Result<(), String> {
    if let Some(pipeline) = &descriptor.pipeline {
        let expected = embedded_pipeline_spec(plan).ok_or_else(|| {
            "embedded Methods preprocessing contradicts the portable definition".to_string()
        })?;
        if pipeline.savgol_window != expected.savgol_window
            || pipeline.savgol_poly_degree != expected.savgol_poly_degree
        {
            return Err(
                "embedded Methods pipeline fingerprint parameters contradict the portable definition"
                    .to_string(),
            );
        }
    }
    Ok(())
}

pub mod dag_ml {
    pub use dag_ml_crate::*;

    pub const UPSTREAM_KEY: &str = "dag_ml";
}

pub mod dag_ml_data {
    pub use dag_ml_data_crate::*;

    pub const UPSTREAM_KEY: &str = "dag_ml_data";
}

/// Optional `nirs4all-datasets` surface.
///
/// nirs4all-datasets is kept EXTERNAL/OPTIONAL (to avoid bloat), so this
/// convenience module is gated behind the off-by-default `datasets` Cargo
/// feature. The bundled aggregate is methods + formats + io (+ dag-ml); the
/// datasets host is delegated to at runtime and never vendored.
#[cfg(feature = "datasets")]
pub mod datasets {
    pub const UPSTREAM_KEY: &str = "datasets";
}

pub mod formats {
    pub use nirs4all_formats_crate::*;

    pub const UPSTREAM_KEY: &str = "formats";
}

pub mod io {
    pub use nirs4all_io_crate::*;

    pub const UPSTREAM_KEY: &str = "io";
}

pub mod methods {
    pub const UPSTREAM_KEY: &str = "methods";
}

fn normalize_pipeline_root(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Array(items) => {
            let pipeline = Value::Array(std::mem::take(items));
            let mut root = serde_json::Map::new();
            root.insert("pipeline".to_string(), pipeline);
            *value = Value::Object(root);
            Ok(())
        }
        Value::Object(map) => {
            if !map.contains_key("pipeline") {
                let steps = map.remove("steps").ok_or_else(|| {
                    "invalid pipeline definition format: expected an array or an object with a 'pipeline' or 'steps' key"
                        .to_string()
                })?;
                map.insert("pipeline".to_string(), steps);
            }
            Ok(())
        }
        _ => Err(
            "pipeline definition must be an array or an object with a 'pipeline'/'steps' key"
                .to_string(),
        ),
    }
}

fn strip_comments_in_place(value: &mut Value) {
    match value {
        Value::Array(items) => {
            items.retain(|item| !is_comment_step(item));
            for item in items {
                strip_comments_in_place(item);
            }
        }
        Value::Object(map) => {
            map.remove("_comment");
            for item in map.values_mut() {
                strip_comments_in_place(item);
            }
        }
        _ => {}
    }
}

fn is_comment_step(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.len() == 1 && map.contains_key("_comment"))
}

fn collect_classes(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(token) if n4m_role_method_id(token).is_some() => {
                        output.push(token.clone())
                    }
                    _ => collect_classes(item, output),
                }
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(class_name)) = map.get("class") {
                output.push(class_name.clone());
            }
            for item in map.values() {
                collect_classes(item, output);
            }
        }
        _ => {}
    }
}

fn flatten_matrix(
    value: &Value,
    rows: usize,
    cols: usize,
    label: &str,
) -> Result<Vec<f64>, String> {
    let expected = rows
        .checked_mul(cols)
        .ok_or_else(|| format!("dataset {label} shape overflows"))?;
    let array = value
        .as_array()
        .ok_or_else(|| format!("dataset {label} must be an array"))?;

    let out = if array.first().is_some_and(Value::is_array) {
        let mut flattened = Vec::with_capacity(expected);
        for row in array {
            let row_items = row
                .as_array()
                .ok_or_else(|| format!("dataset {label} rows must be arrays"))?;
            for item in row_items {
                flattened.push(
                    item.as_f64()
                        .ok_or_else(|| format!("dataset {label} contains a non-numeric value"))?,
                );
            }
        }
        flattened
    } else {
        array
            .iter()
            .map(|item| {
                item.as_f64()
                    .ok_or_else(|| format!("dataset {label} contains a non-numeric value"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    if out.len() != expected {
        return Err(format!(
            "dataset {label} length {} does not match {rows}x{cols}",
            out.len()
        ));
    }
    Ok(out)
}

fn select_rows(
    data: &[f64],
    rows: usize,
    cols: usize,
    indices: &[usize],
) -> Result<Vec<f64>, String> {
    let mut out = Vec::with_capacity(indices.len() * cols);
    for &index in indices {
        if index >= rows {
            return Err(format!("row index {index} is outside 0..{}", rows - 1));
        }
        let start = index * cols;
        out.extend_from_slice(&data[start..start + cols]);
    }
    Ok(out)
}

type N4mStatus = c_int;
type N4mHandle = *mut c_void;

/// Owns one opaque C-ABI handle after its create/import call succeeds.
///
/// The guard is deliberately used around every fallible Rust allocation that
/// follows a native allocation, so an early return or unwind cannot leak it.
struct NativeHandleGuard {
    handle: N4mHandle,
    destroy: unsafe extern "C" fn(N4mHandle),
}

impl NativeHandleGuard {
    fn new(destroy: unsafe extern "C" fn(N4mHandle)) -> Self {
        Self {
            handle: ptr::null_mut(),
            destroy,
        }
    }
}

impl Drop for NativeHandleGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.destroy)(self.handle) };
        }
    }
}

const N4M_OK: N4mStatus = 0;
const N4M_DTYPE_F64: c_int = 1;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct N4mMatrixView {
    data: *mut c_void,
    rows: i64,
    cols: i64,
    row_stride: i64,
    col_stride: i64,
    dtype: c_int,
    reserved0: i32,
}

struct MethodsLibrary {
    library: Library,
}

#[cfg(test)]
thread_local! {
    static HOST_SNV_TRANSFORMS: Cell<usize> = const { Cell::new(0) };
    static HOST_SAVGOL_TRANSFORMS: Cell<usize> = const { Cell::new(0) };
    static NATIVE_MODEL_IMPORTS: Cell<usize> = const { Cell::new(0) };
    static NATIVE_MODEL_PREDICTS: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_portable_replay_call_counts() {
    HOST_SNV_TRANSFORMS.set(0);
    HOST_SAVGOL_TRANSFORMS.set(0);
    NATIVE_MODEL_IMPORTS.set(0);
    NATIVE_MODEL_PREDICTS.set(0);
}

#[cfg(test)]
pub(crate) fn portable_replay_call_counts() -> (usize, usize, usize, usize) {
    (
        HOST_SNV_TRANSFORMS.get(),
        HOST_SAVGOL_TRANSFORMS.get(),
        NATIVE_MODEL_IMPORTS.get(),
        NATIVE_MODEL_PREDICTS.get(),
    )
}

struct PlsFitInput<'a> {
    x_train: &'a [f64],
    y_train: &'a [f64],
    train_rows: usize,
    cols: usize,
    n_components: i32,
    x_test: &'a [f64],
    test_rows: usize,
    embedded_pipeline: Option<EmbeddedPipelineSpec>,
    model_params: &'a serde_json::Map<String, Value>,
}

impl MethodsLibrary {
    fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("could not load libn4m at {}: {error}", path.display()))?;
        Ok(Self { library })
    }

    fn snv_transform(
        &self,
        input: &[f64],
        rows: usize,
        cols: usize,
        with_mean: bool,
        with_std: bool,
        ddof: i32,
    ) -> Result<Vec<f64>, String> {
        #[cfg(test)]
        HOST_SNV_TRANSFORMS.set(HOST_SNV_TRANSFORMS.get() + 1);
        let create: Symbol<unsafe extern "C" fn(*mut N4mHandle, c_int, c_int, c_int) -> N4mStatus> =
            self.symbol(b"n4m_transform_snv_create")?;
        let transform: Symbol<
            unsafe extern "C" fn(N4mHandle, N4mMatrixView, N4mMatrixView) -> N4mStatus,
        > = self.symbol(b"n4m_transform_snv_transform")?;
        let destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
            self.symbol(b"n4m_transform_snv_destroy")?;

        let mut x = input.to_vec();
        let mut out = vec![0.0; input.len()];
        let x_view = matrix_view(&mut x, rows, cols)?;
        let out_view = matrix_view(&mut out, rows, cols)?;
        let mut handle = NativeHandleGuard::new(*destroy);
        unsafe {
            self.check(
                create(
                    &mut handle.handle,
                    i32::from(with_mean),
                    i32::from(with_std),
                    ddof,
                ),
                "n4m_transform_snv_create",
                None,
            )?;
        }
        let status = unsafe { transform(handle.handle, x_view, out_view) };
        self.check(status, "n4m_transform_snv_transform", None)?;
        Ok(out)
    }

    fn savgol_transform(
        &self,
        input: &[f64],
        rows: usize,
        cols: usize,
        params: &SavitzkyGolayParams,
    ) -> Result<Vec<f64>, String> {
        #[cfg(test)]
        HOST_SAVGOL_TRANSFORMS.set(HOST_SAVGOL_TRANSFORMS.get() + 1);
        let create: Symbol<
            unsafe extern "C" fn(
                *mut N4mHandle,
                i32,
                i32,
                i32,
                c_double,
                i32,
                c_double,
            ) -> N4mStatus,
        > = self.symbol(b"n4m_transform_savitzky_golay_create")?;
        let transform: Symbol<
            unsafe extern "C" fn(N4mHandle, N4mMatrixView, N4mMatrixView) -> N4mStatus,
        > = self.symbol(b"n4m_transform_savitzky_golay_transform")?;
        let destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
            self.symbol(b"n4m_transform_savitzky_golay_destroy")?;

        let mut x = input.to_vec();
        let mut out = vec![0.0; input.len()];
        let x_view = matrix_view(&mut x, rows, cols)?;
        let out_view = matrix_view(&mut out, rows, cols)?;
        let mut handle = NativeHandleGuard::new(*destroy);
        unsafe {
            self.check(
                create(
                    &mut handle.handle,
                    params.window_length,
                    params.polyorder,
                    params.deriv,
                    1.0,
                    params.mode,
                    params.cval,
                ),
                "n4m_transform_savitzky_golay_create",
                None,
            )?;
        }
        let status = unsafe { transform(handle.handle, x_view, out_view) };
        self.check(status, "n4m_transform_savitzky_golay_transform", None)?;
        Ok(out)
    }

    fn fit_predict_pls(
        &self,
        input: PlsFitInput<'_>,
        export_model: bool,
    ) -> Result<(Vec<f64>, Option<Vec<u8>>), String> {
        let context_create: Symbol<unsafe extern "C" fn(*mut N4mHandle) -> N4mStatus> =
            self.symbol(b"n4m_context_create")?;
        let context_destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
            self.symbol(b"n4m_context_destroy")?;
        let config_create: Symbol<unsafe extern "C" fn(*mut N4mHandle) -> N4mStatus> =
            self.symbol(b"n4m_config_create")?;
        let config_destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
            self.symbol(b"n4m_config_destroy")?;
        let set_algorithm: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_algorithm")?;
        let set_solver: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_solver")?;
        let set_deflation: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_deflation")?;
        let set_n_components: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_n_components")?;
        let set_center_x: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_center_x")?;
        let set_scale_x: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_scale_x")?;
        let set_center_y: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_center_y")?;
        let set_scale_y: Symbol<unsafe extern "C" fn(N4mHandle, i32) -> N4mStatus> =
            self.symbol(b"n4m_config_set_scale_y")?;
        let model_fit: Symbol<
            unsafe extern "C" fn(
                N4mHandle,
                N4mHandle,
                *const N4mMatrixView,
                *const N4mMatrixView,
                *mut N4mHandle,
            ) -> N4mStatus,
        > = self.symbol(b"n4m_model_fit")?;
        let model_predict: Symbol<
            unsafe extern "C" fn(
                N4mHandle,
                N4mHandle,
                *const N4mMatrixView,
                *mut N4mMatrixView,
            ) -> N4mStatus,
        > = self.symbol(b"n4m_model_predict")?;
        let model_destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
            self.symbol(b"n4m_model_destroy")?;

        let mut x_train = input.x_train.to_vec();
        let mut y_train = input.y_train.to_vec();
        let x_view = matrix_view(&mut x_train, input.train_rows, input.cols)?;
        let y_view = matrix_view(&mut y_train, input.train_rows, 1)?;
        let mut x_test = input.x_test.to_vec();
        let mut predictions = vec![0.0; input.test_rows];
        let test_view = matrix_view(&mut x_test, input.test_rows, input.cols)?;
        let mut out_view = matrix_view(&mut predictions, input.test_rows, 1)?;
        let mut ctx = NativeHandleGuard::new(*context_destroy);
        let mut cfg = NativeHandleGuard::new(*config_destroy);
        let mut model = NativeHandleGuard::new(*model_destroy);
        unsafe {
            self.check(context_create(&mut ctx.handle), "n4m_context_create", None)?;
        }
        macro_rules! checked {
            ($status:expr, $name:literal) => {
                self.check(unsafe { $status }, $name, Some(ctx.handle))?;
            };
        }
        checked!(config_create(&mut cfg.handle), "n4m_config_create");
        checked!(set_algorithm(cfg.handle, 0), "n4m_config_set_algorithm");
        let solver = match input.model_params["solver"].as_str() {
            Some("nipals") => 0,
            Some("simpls") => 1,
            Some("orthogonal_scores") => 2,
            Some("kernel_algorithm") => 3,
            Some("wide_kernel") => 4,
            Some("svd") => 5,
            Some("power") => 6,
            Some("randomized_svd") => 7,
            _ => return Err("owner PLS solver is not representable by N4MM export".into()),
        };
        checked!(set_solver(cfg.handle, solver), "n4m_config_set_solver");
        checked!(set_deflation(cfg.handle, 0), "n4m_config_set_deflation");
        checked!(
            set_n_components(cfg.handle, input.n_components),
            "n4m_config_set_n_components"
        );
        checked!(
            set_center_x(
                cfg.handle,
                i32::from(
                    input.model_params["center_x"]
                        .as_bool()
                        .ok_or("owner center_x missing")?
                )
            ),
            "n4m_config_set_center_x"
        );
        checked!(
            set_scale_x(
                cfg.handle,
                i32::from(
                    input.model_params["scale_x"]
                        .as_bool()
                        .ok_or("owner scale_x missing")?
                )
            ),
            "n4m_config_set_scale_x"
        );
        checked!(
            set_center_y(
                cfg.handle,
                i32::from(
                    input.model_params["center_y"]
                        .as_bool()
                        .ok_or("owner center_y missing")?
                )
            ),
            "n4m_config_set_center_y"
        );
        checked!(
            set_scale_y(
                cfg.handle,
                i32::from(
                    input.model_params["scale_y"]
                        .as_bool()
                        .ok_or("owner scale_y missing")?
                )
            ),
            "n4m_config_set_scale_y"
        );

        let _pipeline = if let Some(spec) = input.embedded_pipeline {
            let pipeline_create: Symbol<unsafe extern "C" fn(*mut N4mHandle) -> N4mStatus> =
                self.symbol(b"n4m_pipeline_create")?;
            let pipeline_destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
                self.symbol(b"n4m_pipeline_destroy")?;
            let pipeline_add: Symbol<
                unsafe extern "C" fn(N4mHandle, c_int, *const c_double, c_int) -> N4mStatus,
            > = self.symbol(b"n4m_pipeline_add_operator")?;
            let config_set_pipeline: Symbol<
                unsafe extern "C" fn(N4mHandle, N4mHandle) -> N4mStatus,
            > = self.symbol(b"n4m_config_set_pipeline")?;
            let mut pipeline = NativeHandleGuard::new(*pipeline_destroy);
            checked!(pipeline_create(&mut pipeline.handle), "n4m_pipeline_create");
            checked!(
                pipeline_add(pipeline.handle, 4, ptr::null(), 0),
                "n4m_pipeline_add_operator(SNV)"
            );
            let savgol = [
                f64::from(spec.savgol_window),
                f64::from(spec.savgol_poly_degree),
            ];
            checked!(
                pipeline_add(pipeline.handle, 8, savgol.as_ptr(), savgol.len() as c_int,),
                "n4m_pipeline_add_operator(SavitzkyGolay)"
            );
            checked!(
                config_set_pipeline(cfg.handle, pipeline.handle),
                "n4m_config_set_pipeline"
            );
            Some(pipeline)
        } else {
            None
        };

        let fit_status =
            unsafe { model_fit(ctx.handle, cfg.handle, &x_view, &y_view, &mut model.handle) };
        self.check(fit_status, "n4m_model_fit", Some(ctx.handle))?;

        let predict_status =
            unsafe { model_predict(ctx.handle, model.handle, &test_view, &mut out_view) };
        let predict_check = self.check(predict_status, "n4m_model_predict", Some(ctx.handle));
        let export_result = if export_model && predict_check.is_ok() {
            (|| -> Result<Vec<u8>, String> {
                let export_size: Symbol<unsafe extern "C" fn(N4mHandle, *mut usize) -> N4mStatus> =
                    self.symbol(b"n4m_model_export_size")?;
                let export_to: Symbol<
                    unsafe extern "C" fn(N4mHandle, *mut c_void, usize, *mut usize) -> N4mStatus,
                > = self.symbol(b"n4m_model_export_to_buffer")?;
                let mut len = 0usize;
                unsafe {
                    self.check(
                        export_size(model.handle, &mut len),
                        "n4m_model_export_size",
                        None,
                    )?;
                }
                if len == 0 || len > 64 * 1024 * 1024 {
                    return Err("n4m_model_export_size returned an invalid N4MM length".to_string());
                }
                let mut bytes = vec![0u8; len];
                let mut written = 0usize;
                unsafe {
                    self.check(
                        export_to(
                            model.handle,
                            bytes.as_mut_ptr().cast(),
                            bytes.len(),
                            &mut written,
                        ),
                        "n4m_model_export_to_buffer",
                        None,
                    )?;
                }
                if written == 0 || written > bytes.len() {
                    return Err(
                        "n4m_model_export_to_buffer returned an invalid N4MM length".to_string()
                    );
                }
                bytes.truncate(written);
                Ok(bytes)
            })()
        } else {
            Ok(Vec::new())
        };
        predict_check?;
        let exported = export_result?;
        Ok((predictions, export_model.then_some(exported)))
    }

    fn import_predict_pls(
        &self,
        model_n4mm: &[u8],
        input: &[f64],
        rows: usize,
        cols: usize,
    ) -> Result<Vec<f64>, String> {
        if model_n4mm.is_empty() || model_n4mm.len() > 64 * 1024 * 1024 {
            return Err("N4MM model payload is empty or exceeds the portable-session limit".into());
        }
        let mut x = input.to_vec();
        let mut out = vec![0.0; rows];
        let x_view = matrix_view(&mut x, rows, cols)?;
        let mut out_view = matrix_view(&mut out, rows, 1)?;
        let context_create: Symbol<unsafe extern "C" fn(*mut N4mHandle) -> N4mStatus> =
            self.symbol(b"n4m_context_create")?;
        let context_destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
            self.symbol(b"n4m_context_destroy")?;
        let import: Symbol<
            unsafe extern "C" fn(N4mHandle, *const c_void, usize, *mut N4mHandle) -> N4mStatus,
        > = self.symbol(b"n4m_model_import_from_buffer")?;
        let predict: Symbol<
            unsafe extern "C" fn(
                N4mHandle,
                N4mHandle,
                *const N4mMatrixView,
                *mut N4mMatrixView,
            ) -> N4mStatus,
        > = self.symbol(b"n4m_model_predict")?;
        let model_destroy: Symbol<unsafe extern "C" fn(N4mHandle)> =
            self.symbol(b"n4m_model_destroy")?;
        let mut ctx = NativeHandleGuard::new(*context_destroy);
        unsafe {
            self.check(context_create(&mut ctx.handle), "n4m_context_create", None)?;
        }
        let mut model = NativeHandleGuard::new(*model_destroy);
        let import_result = unsafe {
            self.check(
                import(
                    ctx.handle,
                    model_n4mm.as_ptr().cast(),
                    model_n4mm.len(),
                    &mut model.handle,
                ),
                "n4m_model_import_from_buffer",
                Some(ctx.handle),
            )
        };
        import_result?;
        #[cfg(test)]
        NATIVE_MODEL_IMPORTS.set(NATIVE_MODEL_IMPORTS.get() + 1);
        let prediction = unsafe {
            self.check(
                predict(ctx.handle, model.handle, &x_view, &mut out_view),
                "n4m_model_predict",
                Some(ctx.handle),
            )
        };
        prediction?;
        #[cfg(test)]
        NATIVE_MODEL_PREDICTS.set(NATIVE_MODEL_PREDICTS.get() + 1);
        Ok(out)
    }

    fn symbol<T>(&self, name: &[u8]) -> Result<Symbol<'_, T>, String> {
        unsafe { self.library.get::<T>(name) }.map_err(|error| {
            format!(
                "could not load symbol {}: {error}",
                String::from_utf8_lossy(name)
            )
        })
    }

    fn check(
        &self,
        status: N4mStatus,
        function_name: &str,
        ctx: Option<N4mHandle>,
    ) -> Result<(), String> {
        if status == N4M_OK {
            return Ok(());
        }
        let status_to_string: Result<Symbol<unsafe extern "C" fn(N4mStatus) -> *const c_char>, _> =
            unsafe { self.library.get(b"n4m_status_to_string") };
        let status_text = status_to_string
            .ok()
            .and_then(|func| unsafe { c_string(func(status)) })
            .unwrap_or_else(|| format!("status {status}"));
        let context_error = ctx.filter(|handle| !handle.is_null()).and_then(|handle| {
            let last_error: Result<Symbol<unsafe extern "C" fn(N4mHandle) -> *const c_char>, _> =
                unsafe { self.library.get(b"n4m_context_last_error") };
            last_error
                .ok()
                .and_then(|func| unsafe { c_string(func(handle)) })
                .filter(|message| !message.is_empty())
        });
        match context_error {
            Some(message) => Err(format!("{function_name} failed: {status_text}: {message}")),
            None => Err(format!("{function_name} failed: {status_text}")),
        }
    }
}

fn matrix_view(data: &mut [f64], rows: usize, cols: usize) -> Result<N4mMatrixView, String> {
    let expected = rows
        .checked_mul(cols)
        .ok_or_else(|| format!("matrix dimensions {rows}x{cols} overflow usize"))?;
    if data.len() != expected {
        return Err(format!(
            "matrix length {} does not match {rows}x{cols}",
            data.len()
        ));
    }
    Ok(N4mMatrixView {
        data: data.as_mut_ptr().cast::<c_void>(),
        rows: i64::try_from(rows).map_err(|_| format!("row count {rows} is outside i64 range"))?,
        cols: i64::try_from(cols)
            .map_err(|_| format!("column count {cols} is outside i64 range"))?,
        row_stride: i64::try_from(cols)
            .map_err(|_| format!("column count {cols} is outside i64 range"))?,
        col_stride: 1,
        dtype: N4M_DTYPE_F64,
        reserved0: 0,
    })
}

unsafe fn c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn portable_execution_contract_cases_are_shared_and_bounded() {
        let cases: Value = serde_json::from_str(include_str!(
            "../tests/parity/fixtures/execution_contract_cases.json"
        ))
        .unwrap();
        for case in cases["invalid"].as_array().unwrap() {
            assert!(
                parse_execution_plan(case).is_err(),
                "accepted {}",
                case["name"]
            );
        }
        for case in cases["valid"].as_array().unwrap() {
            let mut recipe = case.clone();
            recipe.as_object_mut().unwrap().remove("components");
            let plan = parse_execution_plan(&recipe).unwrap();
            assert_eq!(
                serde_json::json!(plan.n_components),
                case["components"],
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn exposes_expected_upstream_keys() {
        let keys: Vec<_> = UPSTREAMS.iter().map(|item| item.key).collect();
        assert_eq!(
            keys,
            vec![
                "dag_ml",
                "dag_ml_data",
                "formats",
                "io",
                "datasets",
                "methods"
            ]
        );
    }

    #[test]
    fn resolves_upstream_by_key() {
        assert_eq!(upstream("methods").unwrap().package, "nirs4all-methods");
        assert!(upstream("unknown").is_none());
    }

    #[test]
    fn rust_local_implementation_registry_facade_delegates_to_dag_ml() {
        type CriterionFn = fn(f64, f64) -> f64;

        let loss = rust_loss_reference();
        let metric = rust_metric_reference();
        let mut registry = local_implementation_registry::<CriterionFn>();

        registry
            .register_loss(&loss, |target, prediction| (prediction - target).abs())
            .unwrap();
        registry
            .register_metric(&metric, |target, prediction| prediction - target)
            .unwrap();

        assert_eq!(registry.resolve_loss(&loss).unwrap()(2.0, 5.5), 3.5);
        assert_eq!(registry.resolve_metric(&metric).unwrap()(2.0, 5.5), 3.5);
        assert_eq!(registry.len(), 2);
    }

    fn generated_profiles_fixture() -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tests/parity/expected/capability_profiles.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("generated profile fixture {path:?}: {error}"));
        serde_json::from_str(&text).expect("fixture is valid JSON")
    }

    fn profile<'a>(profiles: &'a Value, id: &str) -> &'a Value {
        profiles["profiles"]
            .as_array()
            .expect("profiles array")
            .iter()
            .find(|item| item["id"] == id)
            .unwrap_or_else(|| panic!("profile {id} is declared"))
    }

    const SURFACES: [&str; 4] = ["python", "javascript_wasm", "rust", "matlab_octave"];
    const OPERATIONS: [&str; 12] = [
        "compile",
        "fit",
        "predict",
        "import",
        "export",
        "produce",
        "inspect",
        "calibrate",
        "retrain",
        "read",
        "write",
        "update",
    ];
    const METHODS_PACKAGE: &str = "native-methods-package";
    const CANONICAL: &str = "native-n4a-canonical";
    const STORAGE: &str = "n4a-container";
    const METHODS_PACKAGE_SURFACES: [&str; 3] = ["python", "javascript_wasm", "rust"];

    fn expected_surfaces(profile_id: &str) -> &'static [&'static str] {
        if profile_id == METHODS_PACKAGE {
            &METHODS_PACKAGE_SURFACES
        } else {
            &SURFACES
        }
    }

    /// Exact operation declaration; statuses follow `SURFACES` order.
    fn expected_statuses(profile_id: &str, operation: &str) -> [&'static str; 4] {
        const S: &str = "supported";
        const U: &str = "unsupported";
        match (profile_id, operation) {
            // Owner-neutral storage executes nothing: only inspect, read, write and update exist.
            (STORAGE, "inspect" | "read" | "write" | "update") => [S, S, S, S],
            (STORAGE, _) => [U, U, U, U],
            // Canonical V1 `.n4a`: only its own actions are declared; calibration is Rust-only.
            (CANONICAL, "compile" | "fit" | "import" | "export" | "read" | "write" | "update") => {
                [U, U, U, U]
            }
            (CANONICAL, "predict" | "produce" | "inspect" | "retrain") => [S, S, S, S],
            (CANONICAL, "calibrate") => [U, U, S, U],
            // Methods Package V2/V3 in the neutral Core V1 container: import, export and predict on
            // Python, JavaScript/WASM and Rust; MATLAB/Octave declares no entrypoint here.
            (METHODS_PACKAGE, "import" | "export" | "predict") => [S, S, S, U],
            (METHODS_PACKAGE, _) => [U, U, U, U],
            // The envelope and Archive V2 profiles declare the canonical and storage actions unsupported.
            (_, "produce" | "inspect" | "calibrate" | "retrain" | "read" | "write" | "update") => {
                [U, U, U, U]
            }
            ("native-workflow", "compile") => [U, U, S, U],
            ("native-conformal", "compile") => [U, U, U, U],
            (_, "import") | (_, "export") => [S, S, U, S],
            (_, "fit") | (_, "predict") => [S, S, S, S],
            other => panic!("unexpected profile/operation {other:?}"),
        }
    }

    #[test]
    fn execution_profiles_equal_the_generated_owner_fixture() {
        let profiles = execution_profiles();
        assert_eq!(profiles, generated_profiles_fixture());
        assert_eq!(profiles["schema"], "nirs4all-core.execution-profiles.v1");
        assert_eq!(profiles["version"], 1);
        assert_eq!(profiles["runtime_surfaces"], serde_json::json!(SURFACES));
        assert_eq!(
            profiles["runtime_surfaces"],
            serde_json::json!(RUNTIME_SURFACES)
        );

        let manifest = capability_manifest();
        let mut keys: Vec<_> = manifest.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "aggregate",
                "execution_profiles",
                "runtime_surfaces",
                "schema"
            ]
        );
        assert_eq!(manifest["schema"], "nirs4all-core.capabilities.v1");
        assert_eq!(
            manifest["runtime_surfaces"],
            serde_json::json!(RUNTIME_SURFACES)
        );
        assert_eq!(manifest["execution_profiles"], profiles);
    }

    #[test]
    fn execution_profiles_are_independent_copies() {
        let mut first = execution_profiles();
        first["profiles"][0]["operations"]["fit"]["rust"]["status"] =
            Value::String("unsupported".into());
        first["profiles"][1]["content_types"][0]["schema"] = Value::String("mutated".into());
        first.as_object_mut().unwrap().remove("external_federation");
        assert_eq!(execution_profiles(), generated_profiles_fixture());
    }

    #[test]
    fn exactly_the_native_profiles_are_supported_with_exact_operations() {
        let profiles = execution_profiles();
        let ids: Vec<&str> = profiles["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec![
                "native-workflow",
                "native-conformal",
                METHODS_PACKAGE,
                CANONICAL,
                STORAGE
            ]
        );

        for id in &ids {
            let profile = profile(&profiles, id);
            assert_eq!(profile["version"], 1);
            assert_eq!(
                profile["surfaces"],
                serde_json::json!(expected_surfaces(id))
            );
            let operations = profile["operations"].as_object().unwrap();
            assert_eq!(operations.keys().collect::<Vec<_>>(), OPERATIONS.to_vec());
            for operation in OPERATIONS {
                let cells = operations[operation].as_object().unwrap();
                assert_eq!(cells.keys().collect::<Vec<_>>(), SURFACES.to_vec());
                let statuses: Vec<&str> = SURFACES
                    .iter()
                    .map(|surface| cells[*surface]["status"].as_str().unwrap())
                    .collect();
                assert_eq!(
                    statuses,
                    expected_statuses(id, operation),
                    "{id}.{operation}"
                );
                for surface in SURFACES {
                    let cell = &cells[surface];
                    if cell["status"] == "unsupported" {
                        assert!(!cell["reason"].as_str().unwrap().is_empty());
                        assert!(cell.get("entrypoints").is_none());
                    } else {
                        assert!(!cell["entrypoints"].as_array().unwrap().is_empty());
                        assert!(cell.get("reason").is_none());
                    }
                }
            }
        }

        for id in &ids {
            let requirements = profile(&profiles, id)["runtime_requirements"]
                .as_object()
                .unwrap();
            // The Methods Package profile declares no MATLAB/Octave entrypoint, hence no requirement.
            let expected_requirements = expected_surfaces(id).to_vec();
            assert_eq!(
                requirements.keys().collect::<Vec<_>>(),
                expected_requirements
            );
            let rust = requirements["rust"].to_string();
            assert!(rust.contains("no CLI"), "{rust}");
            let browser = requirements["javascript_wasm"].to_string();
            assert!(
                browser.contains("WASM builds")
                    && browser.contains("no native library path and no CLI")
            );
        }
        assert_eq!(
            profile(&profiles, "native-workflow")["content_types"],
            serde_json::json!([{
                "schema": "nirs4all.native-pipeline.v1",
                "owner": "nirs4all-core",
                "carried_in": "native pipeline JSON envelope"
            }])
        );
        assert_eq!(
            profile(&profiles, "native-conformal")["content_types"],
            serde_json::json!([{
                "schema": "nirs4all.calibrated.v1",
                "owner": "nirs4all-core",
                "carried_in": "Core calibration wrapper JSON, only on the content_routes that name this schema: the Rust calibrate_archive return value, the CLI fit/load responses and the MATLAB struct. The JS/WASM host object and the Python host object carry no schema id; import/export transfer the native DAG Package V2 (scientific package schema2) carried in the owner-neutral Core V1 container (physical schema1) via native-methods-package@1"
            }])
        );

        assert_eq!(
            profile(&profiles, CANONICAL)["content_types"],
            serde_json::json!([
                {
                    "schema": "nirs4all.n4a.container.v1",
                    "owner": "nirs4all-core",
                    "carried_in": "raw canonical n4a archive bytes (physical profile zip-stored.v1); the owner artifacts and metadata inside stay authoritative and hosts never re-encode them"
                },
                {
                    "schema": "nirs4all.dataset.v1",
                    "owner": "nirs4all-io",
                    "carried_in": "the exact IO dataset record JSON bytes passed unchanged to produce, retrain and predict; only this JSON record route is described here, not general ingestion"
                }
            ])
        );
        assert_eq!(
            profile(&profiles, METHODS_PACKAGE)["content_types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["schema"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "dagml.portable_predictor_package",
                "dagml.portable_refit_package"
            ]
        );
        assert!(profile(&profiles, METHODS_PACKAGE)["content_routes"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(profile(&profiles, CANONICAL)["content_routes"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(profile(&profiles, STORAGE)["content_routes"]
            .as_array()
            .unwrap()
            .is_empty());
        let storage_limits = profile(&profiles, STORAGE)["limits"].to_string();
        assert!(
            storage_limits.contains("Readable never implies executable")
                && storage_limits.contains("not parity")
        );
        let routes = profile(&profiles, "native-conformal")["content_routes"]
            .as_array()
            .unwrap();
        assert_eq!(routes.len(), 21);
        assert!(profile(&profiles, "native-workflow")["content_routes"]
            .as_array()
            .unwrap()
            .is_empty());
        for route in routes {
            if route["surface"] == "javascript_wasm" || route["route"] == "archive_transfer" {
                assert!(route.get("schema").is_none(), "{route}");
            }
        }
        let text = capability_manifest().to_string();
        for retired in [
            "legacy",
            "wasm-methods-extension",
            "portable_pipeline",
            "runtime_contract",
            "parity-validated",
            "conformal_store",
        ] {
            assert!(!text.contains(retired), "retired term {retired} reachable");
        }
    }

    #[test]
    fn rust_entrypoints_named_by_the_profiles_resolve_in_source() {
        let profiles = execution_profiles();
        let mut checked = 0;
        for profile in profiles["profiles"].as_array().unwrap() {
            for operation in OPERATIONS {
                let cell = &profile["operations"][operation]["rust"];
                if cell["status"] != "supported" {
                    continue;
                }
                for entry in cell["entrypoints"].as_array().unwrap() {
                    let source = entry["source"].as_str().unwrap();
                    assert!(
                        source.starts_with("bindings/rust/nirs4all/src/"),
                        "{source}"
                    );
                    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../..")
                        .join(source);
                    let text = std::fs::read_to_string(&path).unwrap();
                    let symbol = entry["symbol"].as_str().unwrap();
                    assert!(
                        text.contains(&format!("pub fn {symbol}")),
                        "{symbol} in {source}"
                    );
                    checked += 1;
                }
            }
        }
        // workflow: compile, fit, predict; conformal: fit, predict; methods package: predict,
        // import, export; canonical: produce, inspect, predict, calibrate, retrain;
        // storage: inspect, read, write, update. Every declared Rust source entry is checked above.
        assert_eq!(checked, 17);
    }

    #[test]
    fn canonical_profile_declares_real_actions_without_aliases() {
        let profiles = execution_profiles();
        let canonical = profile(&profiles, CANONICAL);
        let mut symbols = Vec::new();
        for operation in OPERATIONS {
            let cell = &canonical["operations"][operation]["rust"];
            if cell["status"] == "supported" {
                for entry in cell["entrypoints"].as_array().unwrap() {
                    symbols.push((operation, entry["symbol"].as_str().unwrap().to_string()));
                }
            }
        }
        assert_eq!(
            symbols,
            vec![
                ("predict", "predict_n4a_native".to_string()),
                ("produce", "produce_n4a_native".to_string()),
                ("inspect", "inspect_n4a_view_json".to_string()),
                ("calibrate", "calibrate_n4a_native".to_string()),
                ("retrain", "retrain_n4a_native".to_string()),
            ]
        );
        // Source support is not qualification; the profile says so itself.
        let limits = canonical["limits"].to_string();
        assert!(
            limits.contains("not parity") && limits.contains("Inspect is not executable admission")
        );
        for other in ["native-workflow", "native-conformal", METHODS_PACKAGE] {
            for operation in ["produce", "inspect", "calibrate", "retrain"] {
                assert!(
                    profile(&profiles, other)["operations"][operation]
                        .as_object()
                        .unwrap()
                        .values()
                        .all(|cell| cell["status"] == "unsupported"),
                    "{other}.{operation}"
                );
            }
        }
    }

    #[test]
    fn r_is_federated_externally_and_never_a_core_runtime_surface() {
        let profiles = execution_profiles();
        assert!(!RUNTIME_SURFACES.contains(&"r"));
        assert!(!profiles["runtime_surfaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "r"));
        let federation = profiles["external_federation"].as_array().unwrap();
        assert_eq!(federation.len(), 1);
        assert_eq!(federation[0]["surface"], "r");
        assert_eq!(federation[0]["core_runtime_surface"], false);
        assert_eq!(federation[0]["status"], "externally-federated");
        assert_eq!(
            federation[0]["profiles"],
            serde_json::json!([
                "native-workflow",
                "native-conformal",
                METHODS_PACKAGE,
                CANONICAL,
                STORAGE
            ])
        );
    }

    #[test]
    fn json_and_yaml_pipeline_fixtures_use_same_nirs4all_syntax() {
        let json = include_str!("../tests/parity/fixtures/portable_methods_pipeline.json");
        let yaml = include_str!("../tests/parity/fixtures/portable_methods_pipeline.yaml");
        let json_pipeline = load_pipeline_definition_str(json).unwrap();
        let yaml_pipeline = load_pipeline_definition_str(yaml).unwrap();

        assert_eq!(json_pipeline, yaml_pipeline);
        assert_eq!(
            portable_class_names(&json_pipeline),
            vec![
                "nirs4all.operators.splitters.KennardStoneSplitter",
                "nirs4all.operators.transforms.StandardNormalVariate",
                "nirs4all.operators.transforms.SavitzkyGolay",
                "sklearn.cross_decomposition.PLSRegression",
            ]
        );
        assert_eq!(
            json_pipeline["pipeline"][3]["_range_"],
            serde_json::json!([2, 11, 2])
        );
    }

    #[test]
    fn savgol_default_polyorder_matches_full_python_nirs4all() {
        let definition = serde_json::json!({
            "pipeline": [
                {
                    "class": "nirs4all.operators.transforms.SavitzkyGolay",
                    "params": { "window_length": 11 }
                },
                {
                    "model": {
                        "class": "sklearn.cross_decomposition.PLSRegression",
                        "params": { "n_components": 2 }
                    }
                }
            ]
        });
        let plan = parse_execution_plan(&definition).unwrap();

        assert_eq!(
            plan.preprocessing,
            vec![PortablePreprocessing::SavitzkyGolay(SavitzkyGolayParams {
                window_length: 11,
                polyorder: 3,
                deriv: 0,
                mode: 4,
                cval: 0.0,
            })]
        );
    }

    #[test]
    fn savgol_mode_and_cval_are_preserved() {
        let definition = serde_json::json!({
            "pipeline": [
                {
                    "class": "nirs4all.operators.transforms.SavitzkyGolay",
                    "params": { "window_length": 11, "mode": "constant", "cval": 7.25 }
                },
                {
                    "model": {
                        "class": "sklearn.cross_decomposition.PLSRegression",
                        "params": { "n_components": 2 }
                    }
                }
            ]
        });
        let plan = parse_execution_plan(&definition).unwrap();

        assert_eq!(
            plan.preprocessing,
            vec![PortablePreprocessing::SavitzkyGolay(SavitzkyGolayParams {
                window_length: 11,
                polyorder: 3,
                deriv: 0,
                mode: 1,
                cval: 7.25,
            })]
        );
    }

    #[test]
    fn matrix_view_rejects_invalid_input_before_any_native_handle_is_created() {
        let mut values = Vec::new();
        let error = matrix_view(&mut values, 1, 1).unwrap_err();
        assert!(error.contains("does not match"));
    }

    #[test]
    fn matrix_view_refuses_dimension_overflow_before_native_handle_creation() {
        let mut values = Vec::new();
        let error = matrix_view(&mut values, usize::MAX, 2).unwrap_err();
        assert!(error.contains("overflow"));
    }

    #[test]
    fn execution_plan_rejects_lossy_operator_parameter_coercions() {
        let definition = serde_json::json!({
            "pipeline": [
                {
                    "class": "nirs4all.operators.transforms.SavitzkyGolay",
                    "params": { "window_length": 10.5 }
                },
                {
                    "model": {
                        "class": "sklearn.cross_decomposition.PLSRegression",
                        "params": { "n_components": 2 }
                    }
                }
            ]
        });
        assert!(parse_execution_plan(&definition)
            .unwrap_err()
            .contains("$.pipeline[0].params.window_length: integer in 3..i32::MAX required"));

        let definition = serde_json::json!({
            "pipeline": [
                {
                    "model": {
                        "class": "sklearn.cross_decomposition.PLSRegression",
                        "params": { "n_components": 1.5 }
                    }
                }
            ]
        });
        assert!(parse_execution_plan(&definition)
            .unwrap_err()
            .contains("$.pipeline[0].model.params.n_components: integer in 1..i32::MAX required"));

        let definition = serde_json::json!({
            "pipeline": [
                {
                    "model": { "class": "sklearn.cross_decomposition.PLSRegression" },
                    "param": "n_components",
                    "_range_": [0, 4, 2]
                }
            ]
        });
        assert!(parse_execution_plan(&definition)
            .unwrap_err()
            .contains("$.pipeline[0]: integer in 1..i32::MAX required"));

        let definition = serde_json::json!({
            "pipeline": [
                {
                    "model": { "class": "sklearn.cross_decomposition.PLSRegression" },
                    "param": "n_components",
                    "_range_": [4, 2, 1]
                }
            ]
        });
        assert!(parse_execution_plan(&definition)
            .unwrap_err()
            .contains("$.pipeline[0]: range start exceeds stop"));
    }

    fn rust_loss_reference() -> dag_ml::LossReference {
        let spec = dag_ml::LossSpec::new(
            "example.loss.absolute@1",
            dag_ml::SemanticSpecKind::Custom,
            BTreeSet::from([dag_ml::LearningTaskKind::Regression]),
            BTreeSet::from([dag_ml::PredictionKind::RegressionPoint]),
            dag_ml::LossReduction::Mean,
            BTreeSet::from([
                dag_ml::CriterionInput::Target,
                dag_ml::CriterionInput::Prediction,
            ]),
            BTreeSet::new(),
            serde_json::json!({}),
        )
        .unwrap();
        let implementation = dag_ml::ImplementationDescriptor::new(
            dag_ml::ImplementationSemanticKind::Loss,
            &spec.loss_id,
            &spec.spec_fingerprint,
            "provider:rust-local",
            "binding:rust",
            "1.0.0",
            "2".repeat(64),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::from([dag_ml::ImplementationCapability::Deterministic]),
            dag_ml::PortabilityClass::HostLocal,
            dag_ml::ReplayabilityClass::RegistryRequired,
            Some("loss:nirs4all-core:rust:absolute".to_string()),
        )
        .unwrap();
        dag_ml::LossReference {
            spec,
            implementation,
        }
    }

    fn rust_metric_reference() -> dag_ml::MetricReference {
        let spec = dag_ml::MetricSpec::new(
            "example.metric.bias@1",
            dag_ml::SemanticSpecKind::Custom,
            BTreeSet::from([dag_ml::LearningTaskKind::Regression]),
            BTreeSet::from([dag_ml::PredictionKind::RegressionPoint]),
            dag_ml::MetricObjective::Minimize,
            BTreeSet::from([dag_ml::PredictionLevel::Sample]),
            dag_ml::MetricDecomposition::Global,
            dag_ml::MetricReduction::Global,
            BTreeSet::from([
                dag_ml::CriterionInput::Target,
                dag_ml::CriterionInput::Prediction,
            ]),
            BTreeSet::new(),
            serde_json::json!({}),
        )
        .unwrap();
        let implementation = dag_ml::ImplementationDescriptor::new(
            dag_ml::ImplementationSemanticKind::Metric,
            &spec.metric_id,
            &spec.spec_fingerprint,
            "provider:rust-local",
            "binding:rust",
            "1.0.0",
            "3".repeat(64),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::from([dag_ml::ImplementationCapability::Deterministic]),
            dag_ml::PortabilityClass::HostLocal,
            dag_ml::ReplayabilityClass::RegistryRequired,
            Some("metric:nirs4all-core:rust:bias".to_string()),
        )
        .unwrap();
        dag_ml::MetricReference {
            spec,
            implementation,
        }
    }

    #[test]
    fn all_shared_parity_fixtures_keep_json_and_yaml_in_lockstep() {
        let fixtures = [
            (
                include_str!("../tests/parity/fixtures/portable_kennard_stone_snv_pls.json"),
                include_str!("../tests/parity/fixtures/portable_kennard_stone_snv_pls.yaml"),
            ),
            (
                include_str!("../tests/parity/fixtures/portable_methods_pipeline.json"),
                include_str!("../tests/parity/fixtures/portable_methods_pipeline.yaml"),
            ),
            (
                include_str!("../tests/parity/fixtures/portable_savgol_pls.json"),
                include_str!("../tests/parity/fixtures/portable_savgol_pls.yaml"),
            ),
            (
                include_str!("../tests/parity/fixtures/portable_snv_pls.json"),
                include_str!("../tests/parity/fixtures/portable_snv_pls.yaml"),
            ),
        ];

        for (json, yaml) in fixtures {
            let json_pipeline = load_pipeline_definition_str(json).unwrap();
            let yaml_pipeline = load_pipeline_definition_str(yaml).unwrap();
            assert_eq!(json_pipeline, yaml_pipeline);
            assert!(!portable_class_names(&json_pipeline).is_empty());
        }
    }

    #[test]
    fn steps_alias_and_direct_arrays_match_nirs4all_loader_surface() {
        let json = include_str!("../tests/parity/fixtures/portable_methods_pipeline.json");
        let definition = load_pipeline_definition_str(json).unwrap();

        let from_steps = load_pipeline_definition_str(
            &serde_json::json!({ "steps": definition["pipeline"].clone() }).to_string(),
        )
        .unwrap();
        let from_list = load_pipeline_definition_str(&definition["pipeline"].to_string()).unwrap();

        assert_eq!(from_steps["pipeline"], definition["pipeline"]);
        assert_eq!(from_list["pipeline"], definition["pipeline"]);
    }

    #[test]
    fn rust_binding_execution_matches_full_python_nirs4all_oracle() {
        let library_path = std::env::var("NIRS4ALL_METHODS_LIB");
        let library_path = match library_path {
            Ok(path) => path,
            Err(error) => {
                if std::env::var("NIRS4ALL_CORE_REQUIRE_METHODS_PARITY").as_deref() == Ok("1") {
                    panic!("strict Rust parity requires NIRS4ALL_METHODS_LIB: {error}");
                }
                eprintln!("skipping Rust execution parity: NIRS4ALL_METHODS_LIB is not set");
                return;
            }
        };

        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/parity/expected/portable_python_oracle.json"
        ))
        .unwrap();
        let dataset = PortableDataset::from_json_value(&oracle["dataset"]).unwrap();
        let cases = oracle["cases"].as_array().unwrap();
        let tolerances = &oracle["metadata"]["tolerances"];
        let target_tol = tolerances["targets_abs"].as_f64().unwrap();
        let rmse_tol = tolerances["rmse_abs"].as_f64().unwrap();
        let prediction_tol = tolerances["predictions_abs"].as_f64().unwrap();

        assert!(cases.len() >= 4);
        for expected in cases {
            let name = expected["name"].as_str().unwrap();
            let fixture =
                fixture_for_name(name).unwrap_or_else(|| panic!("missing fixture {name}"));
            let actual =
                run_portable_pipeline_with_library(fixture, &dataset, &library_path).unwrap();
            if actual.split.kind != "all" {
                let mut alternate = load_pipeline_definition_str(fixture).unwrap();
                let steps = alternate["pipeline"].as_array_mut().unwrap();
                let splitter = steps.remove(0);
                steps.insert(steps.len() - 1, splitter);
                assert_eq!(
                    run_portable_pipeline_with_library(
                        &alternate.to_string(),
                        &dataset,
                        &library_path
                    )
                    .unwrap(),
                    actual
                );
            }
            assert_eq!(
                actual.evaluation_scope(),
                if actual.split.kind == "all" {
                    "training"
                } else {
                    "selection_validation"
                }
            );

            assert_eq!(
                actual.split.kind,
                expected["split"]["kind"].as_str().unwrap()
            );
            assert_eq!(
                actual.split.train_indices,
                value_usize_vec(&expected["split"]["trainIndices"])
            );
            assert_eq!(
                actual.split.test_indices,
                value_usize_vec(&expected["split"]["testIndices"])
            );
            assert!(
                max_abs_diff(&actual.targets, &value_f64_vec(&expected["targets"])) <= target_tol,
                "{name}: target diff exceeded tolerance"
            );
            let expected_variants = expected["variants"].as_array().unwrap();
            assert_eq!(actual.variants.len(), expected_variants.len(), "{name}");
            for (actual_variant, expected_variant) in actual.variants.iter().zip(expected_variants)
            {
                assert_eq!(
                    actual_variant.n_components,
                    expected_variant["n_components"].as_i64().unwrap() as i32,
                    "{name}: component mismatch"
                );
                assert!(
                    (actual_variant.rmse - expected_variant["rmse"].as_f64().unwrap()).abs()
                        <= rmse_tol,
                    "{name}: RMSE diff for n_components={}",
                    actual_variant.n_components
                );
                assert!(
                    max_abs_diff(
                        &actual_variant.predictions,
                        &value_f64_vec(&expected_variant["predictions"])
                    ) <= prediction_tol,
                    "{name}: prediction diff for n_components={}",
                    actual_variant.n_components
                );
            }
            assert_eq!(
                actual.selected.n_components,
                expected["selected"]["n_components"].as_i64().unwrap() as i32,
                "{name}: selected component mismatch"
            );
        }
    }

    fn fixture_for_name(name: &str) -> Option<&'static str> {
        match name {
            "portable_kennard_stone_snv_pls" => Some(include_str!(
                "../tests/parity/fixtures/portable_kennard_stone_snv_pls.json"
            )),
            "portable_methods_pipeline" => Some(include_str!(
                "../tests/parity/fixtures/portable_methods_pipeline.json"
            )),
            "portable_savgol_pls" => Some(include_str!(
                "../tests/parity/fixtures/portable_savgol_pls.json"
            )),
            "portable_snv_pls" => Some(include_str!(
                "../tests/parity/fixtures/portable_snv_pls.json"
            )),
            _ => None,
        }
    }

    fn value_usize_vec(value: &Value) -> Vec<usize> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_u64().unwrap() as usize)
            .collect()
    }

    fn value_f64_vec(value: &Value) -> Vec<f64> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_f64().unwrap())
            .collect()
    }

    fn max_abs_diff(actual: &[f64], expected: &[f64]) -> f64 {
        assert_eq!(actual.len(), expected.len());
        actual
            .iter()
            .zip(expected)
            .map(|(left, right)| (left - right).abs())
            .fold(0.0, f64::max)
    }
}

#[cfg(test)]
mod format3_qualification;
