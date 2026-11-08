//! Thin native aggregate composition. DAG owns admission, variants and SELECT;
//! IO owns records/views; Methods owns KS and every numerical operation.
use std::collections::BTreeMap;
use std::path::Path;

use dag_ml_core as dag;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    #[serde(default)]
    profile: Option<String>,
    definition: Value,
    #[serde(default)]
    dataset: Option<Value>,
    #[serde(default = "default_source")]
    source_id: String,
}
fn default_source() -> String {
    "spectra".into()
}
fn request(value: &Value) -> Result<Request, String> {
    let request: Request = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if request.schema_version != 1
        || request
            .profile
            .as_deref()
            .is_some_and(|p| p != dag::PORTABLE_LINEAR_PROFILE)
    {
        return Err("unsupported portable request schema/profile".into());
    }
    Ok(request)
}

/// Pure owner admission. No native library, data materialization, import or fit.
pub fn plan(value: &Value) -> Result<Value, String> {
    let request = request(value)?;
    if request.dataset.is_some() {
        return Err("portable-pipeline-plan forbids dataset; inspection never executes".into());
    }
    serde_json::to_value(
        dag::admit_portable_linear(&request.definition).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Marshals a dense host matrix to IO's existing public record. Validation and
/// package materialization use IO, including preservation of F64 storage.
fn record(
    value: &Value,
    source: &str,
) -> Result<(Value, String, Value, BTreeMap<String, Value>), String> {
    if value.get("schema").is_some() {
        let normalized = nirs4all_io_crate::core::public_dataset::normalize_dataset(value)?;
        let raw = &normalized["dataset"];
        let sources = raw["sources"].as_array().ok_or("record requires sources")?;
        if sources.len() != 1 {
            return Err("portable profile refuses multiple sources".into());
        }
        let axis = json!({"axes":sources[0]["axes"],"axis_units":sources[0]["axis_units"],"axis_coordinates":sources[0]["axis_coordinates"],"feature_names":sources[0]["feature_names"]});
        return Ok((normalized, "declared".into(), axis, BTreeMap::new()));
    }
    let object = value
        .as_object()
        .ok_or("dataset must be a dense object or IO record")?;
    for key in object.keys() {
        if ![
            "X",
            "y",
            "rows",
            "cols",
            "n_samples",
            "n_features",
            "sample_ids",
            "axis",
            "metadata",
            "provenance",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("dataset.{key}: unsupported dense dataset field"));
        }
    }
    let mut x = object
        .get("X")
        .and_then(Value::as_array)
        .ok_or("dataset.X must be an array")?
        .clone();
    if x.first().is_some_and(|v| !v.is_array()) {
        let rows = object
            .get("rows")
            .or_else(|| object.get("n_samples"))
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("flat X requires rows")?;
        let cols = object
            .get("cols")
            .or_else(|| object.get("n_features"))
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("flat X requires cols")?;
        if cols == 0 || rows.checked_mul(cols) != Some(x.len()) {
            return Err("flat X dimensions disagree".into());
        }
        x = x.chunks(cols).map(|r| json!(r)).collect();
    }
    let rows = x.len();
    let cols = x
        .first()
        .and_then(Value::as_array)
        .map(Vec::len)
        .filter(|n| *n > 0)
        .ok_or("nonempty matrix required")?;
    if x.iter()
        .any(|r| r.as_array().is_none_or(|r| r.len() != cols))
    {
        return Err("rectangular X required".into());
    }
    let mut y = object
        .get("y")
        .and_then(Value::as_array)
        .ok_or("dataset.y must be one numeric target")?
        .clone();
    if y.first().is_some_and(Value::is_array) {
        y = y
            .iter()
            .map(|r| {
                r.as_array()
                    .filter(|r| r.len() == 1)
                    .map(|r| r[0].clone())
                    .ok_or("one target required")
            })
            .collect::<Result<Vec<_>, _>>()?;
    }
    if y.len() != rows {
        return Err("X/y row counts disagree".into());
    }
    let mut row_annotations = BTreeMap::new();
    for key in ["metadata", "provenance"] {
        if let Some(value) = object.get(key) {
            let cells = value
                .as_array()
                .ok_or_else(|| format!("dataset.{key}: row-aligned array required"))?;
            if cells.len() != rows {
                return Err(format!("dataset.{key}: row count disagrees with X"));
            }
            for (index, cell) in cells.iter().enumerate() {
                if !cell.is_object() && !cell.is_array() {
                    return Err(format!(
                        "dataset.{key}[{index}]: opaque object or list required"
                    ));
                }
            }
            row_annotations.insert(key.into(), value.clone());
        }
    }
    for (keys, actual) in [
        (["rows", "n_samples"], rows),
        (["cols", "n_features"], cols),
    ] {
        for key in keys {
            if let Some(value) = object.get(key) {
                if value.as_u64().and_then(|v| usize::try_from(v).ok()) != Some(actual) {
                    return Err(format!(
                        "dataset.{key}: declared matrix dimension disagrees with X"
                    ));
                }
            }
        }
    }
    let kind = if object.contains_key("sample_ids") {
        "declared"
    } else {
        "positional"
    };
    let ids = object.get("sample_ids").cloned().unwrap_or_else(|| {
        json!((0..rows)
            .map(|i| format!("position_{i}"))
            .collect::<Vec<_>>())
    });
    let axis = object.get("axis").cloned().unwrap_or(Value::Null);
    if !axis.is_null() && !axis.is_object() {
        return Err("dataset.axis must be an object".into());
    }
    let raw = json!({"schema":"nirs4all.dataset.v1","schema_version":1,"origin_ids":ids,"fold_ids":vec![Value::Null;rows],
        "dataset":{"schema":"nirs4all.multimodal-dataset","schema_version":1,"name":"portable","sample_ids":ids,
        "sources":[{"name":source,"sample_ids":ids,"representation_id":"tabular_numeric","axes":["sample","feature"],"feature_names":null,
            "axis_units":{},"axis_coordinates":{},"array":{"dtype":"float64","shape":[rows,cols],"values":x}}],
        "y":{"dtype":"float64","shape":[rows],"values":y},"groups":null,
        "partitions":{"dtype":"<U5","shape":[rows],"values":vec!["train";rows]},"target_names":["y"],"task_type":"regression"}});
    Ok((
        nirs4all_io_crate::core::public_dataset::normalize_dataset(&raw)?,
        kind.into(),
        axis,
        row_annotations,
    ))
}

fn cohort(record: &Value) -> Result<Vec<String>, String> {
    let raw = &record["dataset"];
    if raw["task_type"] != "regression" || raw["y"]["shape"].as_array().is_none_or(|s| s.len() != 1)
    {
        return Err("portable profile requires one numeric regression target".into());
    }
    for key in ["groups", "independent_unit_ids", "repetition_ids"] {
        if raw.get(key).is_some_and(|v| !v.is_null()) {
            return Err(format!("portable profile refuses declared {key}"));
        }
    }
    if raw["partitions"]["values"]
        .as_array()
        .is_none_or(|v| v.iter().any(|v| v != "train"))
    {
        return Err("portable profile refuses non-train partitions".into());
    }
    if record["fold_ids"]
        .as_array()
        .is_none_or(|v| v.iter().any(|v| !v.is_null()))
    {
        return Err("portable profile refuses declared folds".into());
    }
    let ids: Vec<String> =
        serde_json::from_value(raw["sample_ids"].clone()).map_err(|e| e.to_string())?;
    let origins: Vec<String> =
        serde_json::from_value(record["origin_ids"].clone()).map_err(|e| e.to_string())?;
    if origins != ids {
        return Err("portable profile refuses dependent origin identities".into());
    }
    Ok(ids)
}

/// Adds an exact no-split refit order to the existing IO provider. Buffers,
/// content attestations and every scheduler view remain delegated to IO/Core.
struct PortableProvider<'a> {
    inner: &'a crate::DatasetPackageMethodsProvider,
    ordered_ids: Vec<dag::SampleId>,
    // Opaque dense input-row evidence, independent of numerical views/relations.
    row_annotations: BTreeMap<String, Value>,
}
impl dag::RuntimeDataProvider for PortableProvider<'_> {
    fn materialize(&self, r: &dag::DataMaterializationRequest) -> dag::Result<dag::HandleRef> {
        self.inner.materialize(r)
    }
    fn make_view(&self, r: &dag::DataViewRequest) -> dag::Result<dag::HandleRef> {
        self.inner.make_view(r)
    }
    fn training_data_identity(
        &self,
        b: &dag::DataBinding,
    ) -> dag::Result<Option<dag::TrainingDataIdentity>> {
        self.inner.training_data_identity(b)
    }
    fn coordinator_relations(
        &self,
        b: &dag::DataBinding,
    ) -> dag::Result<Option<dag::SampleRelationSet>> {
        self.inner.coordinator_relations(b)
    }
    fn methods_pls_capability(&self) -> dag::Result<()> {
        self.inner.methods_pls_capability()
    }
    fn preflight_methods_pls(&self, r: &dag::MethodsPlsDataRequest) -> dag::Result<()> {
        self.inner.preflight_methods_pls(r)
    }
    fn methods_pls_data(&self, r: &dag::MethodsPlsDataRequest) -> dag::Result<dag::MethodsPlsData> {
        self.inner.methods_pls_data(r)
    }
    fn refit_sample_ids(&self, b: &dag::DataBinding) -> dag::Result<Option<Vec<dag::SampleId>>> {
        self.inner.training_data_identity(b)?;
        Ok(Some(self.ordered_ids.clone()))
    }
}

fn registry() -> Result<dag::ControllerRegistry, String> {
    let mut spec = dag::HostControllerSpec::new(
        dag::PORTABLE_STEPS_CONTROLLER,
        dag::PORTABLE_STEPS_VERSION,
        dag::NodeKind::Model,
    );
    spec.rng_policy = dag::RngPolicy::ExternallyDeterministic;
    spec.output_ports=Some(serde_json::from_value(json!([{"name":"oof","kind":"prediction","representation":null,"cardinality":"one","description":""}])).map_err(|e|e.to_string())?);
    let mut registry = dag::ControllerRegistry::new();
    registry
        .register(spec.derive().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(registry)
}

fn in_order(
    block: &dag::PredictionBlock,
    targets: &dag::RegressionTargetBlock,
    ids: &[dag::SampleId],
) -> Result<(Vec<f64>, Vec<f64>), String> {
    let positions = block
        .sample_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect::<BTreeMap<_, _>>();
    let truth = targets
        .unit_ids
        .iter()
        .enumerate()
        .filter_map(|(i, id)| match id {
            dag::PredictionUnitId::Sample(id) => Some((id, i)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut predicted = Vec::new();
    let mut actual = Vec::new();
    for id in ids {
        let p = positions
            .get(id)
            .ok_or("owner predictions omit a requested evaluation ID")?;
        let t = truth
            .get(id)
            .ok_or("owner targets omit a requested evaluation ID")?;
        if block.values[*p].len() != 1 || targets.values[*t].len() != 1 {
            return Err("portable result requires one target".into());
        }
        predicted.push(block.values[*p][0]);
        actual.push(targets.values[*t][0]);
    }
    Ok((predicted, actual))
}

/// Sole common owner executor for CLI/native extension and public Rust APIs.
pub fn run(value: &Value, library: &Path) -> Result<Value, String> {
    let request = request(value)?;
    let admission = dag::admit_portable_linear(&request.definition).map_err(|e| e.to_string())?;
    let (record, identity_kind, axis, row_annotations) = record(
        request.dataset.as_ref().ok_or("run requires dataset")?,
        &request.source_id,
    )?;
    let ids = cohort(&record)?;
    let package = nirs4all_io_crate::core::public_dataset::dense_dataset_package(
        &record,
        &request.source_id,
    )?;
    let provider = crate::DatasetPackageMethodsProvider::new(&package, &request.source_id)?;
    let matrices = provider.training_matrices("portable:materialize", &ids)?;
    let sample_ids = ids
        .iter()
        .map(dag::SampleId::new)
        .collect::<dag::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let runtime = crate::native_methods_replay::configure_methods_runtime_for_source(library)
        .map_err(|e| e.to_string())?;
    for params in &admission.candidates {
        let mut selected = BTreeMap::from([("pipeline_steps_v1".into(), json!(admission.recipe))]);
        selected.extend(params.clone());
        let steps = dag::pipeline_steps_recipe(&selected).map_err(|e| e.to_string())?;
        dag::preflight_pipeline_steps(&steps).map_err(|e| e.to_string())?;
    }
    let (train, test) = if let Some(splitter) = &admission.splitter {
        let context = n4m::Context::new().map_err(|e| e.to_string())?;
        let mut params = n4m::roles::Params::new(&context, "splitters.kennard_stone")
            .map_err(|e| e.to_string())?;
        params
            .set_double(
                "test_size",
                splitter["params"]["test_size"]
                    .as_f64()
                    .ok_or("invalid owner splitter")?,
            )
            .map_err(|e| e.to_string())?;
        let x = n4m::MatrixRef::row_major(&matrices.x.values, matrices.x.rows, matrices.x.cols)
            .map_err(|e| e.to_string())?;
        let folds = n4m::roles::run_procedure(
            &context,
            "splitters.kennard_stone",
            Some(&params),
            &n4m::roles::FitInputs::new(x),
        )
        .map_err(|e| e.to_string())?
        .folds()
        .map_err(|e| e.to_string())?;
        let [fold] = folds.as_slice() else {
            return Err("Methods KS must return exactly one split".into());
        };
        let convert = |indexes: &[i64]| {
            indexes
                .iter()
                .map(|i| {
                    usize::try_from(*i)
                        .ok()
                        .filter(|i| *i < ids.len())
                        .ok_or("Methods KS index outside cohort")
                })
                .collect::<Result<Vec<_>, _>>()
        };
        (convert(&fold.train)?, convert(&fold.test)?)
    } else {
        let all = (0..ids.len()).collect::<Vec<_>>();
        (all.clone(), all)
    };
    let select_ids = |indexes: &[usize]| {
        indexes
            .iter()
            .map(|i| sample_ids[*i].clone())
            .collect::<Vec<_>>()
    };
    let training_ids = select_ids(&train);
    let evaluation_ids = select_ids(&test);
    let fold_set = if admission.splitter.is_some() {
        Some(serde_json::from_value::<dag::FoldSet>(json!({"id":"portable:ks","sample_ids":sample_ids,
        "partition_mode":"resampled","sample_groups":{},"folds":[{"fold_id":"portable:holdout","train_sample_ids":training_ids,"validation_sample_ids":evaluation_ids,"metadata":{}}]})).map_err(|e|e.to_string())?)
    } else {
        None
    };
    let envelope = provider.external_envelope();
    let binding:dag::DataBinding=serde_json::from_value(json!({"node_id":"model:portable","input_name":"x","request_id":"io:portable",
        "schema_fingerprint":envelope.schema_fingerprint,"plan_fingerprint":envelope.plan_fingerprint,"relation_fingerprint":envelope.relation_fingerprint,
        "output_representation":provider.provider.envelope().plan.output_representation,"feature_set_id":provider.provider.feature_set_id(),"source_ids":[request.source_id],"require_relations":true,
        "view_policy":{"fit_partition":"fold_train","predict_partition":"fold_validation","include_augmented_train":false,"include_augmented_validation":false,"include_excluded":false,"require_sample_ids":true},"metadata":{}})).map_err(|e|e.to_string())?;
    let registry = registry()?;
    let compiled = dag::compile_portable_linear(&admission, binding, fold_set, &registry)
        .map_err(|e| e.to_string())?;
    let plan = dag::build_execution_plan(
        "plan:portable",
        compiled.graph,
        compiled.campaign_template,
        &registry,
    )
    .map_err(|e| e.to_string())?;
    let mut controllers = dag::RuntimeControllerRegistry::new();
    controllers
        .register(Box::new(
            dag::MethodsNativeRegressionController::pipeline_steps_v1(runtime),
        ))
        .map_err(|e| e.to_string())?;
    let run_id = dag::RunId::new("run:portable").map_err(|e| e.to_string())?;
    let target = dag::NodeId::new("model:portable").map_err(|e| e.to_string())?;
    let scoped_provider = PortableProvider {
        inner: &provider,
        ordered_ids: sample_ids.clone(),
        row_annotations,
    };
    let mut variants = Vec::new();
    let mut targets = None;
    let (owner, selected_ordinal) = if admission.splitter.is_some() {
        let bank = dag::execute_single_holdout_bank(
            &plan,
            &dag::SingleHoldoutBankRequest {
                schema_version: 1,
                profile: dag::SINGLE_HOLDOUT_BANK_PROFILE.into(),
                report_profile: dag::RegressionReportProfile::PopulationRmseRatiosV1,
                failure_group_dimension: "chain".into(),
                target_node: target,
            },
            &run_id,
            admission.root_seed,
            &controllers,
            &scoped_provider,
        )
        .map_err(|e| e.to_string())?;
        let winner = bank.selected.as_ref().ok_or_else(|| {
            format!(
                "portable candidate chain refused: {}",
                serde_json::to_string(&bank.chain_issues).unwrap_or_default()
            )
        })?;
        for entry in &bank.variants {
            if !entry.eligible {
                return Err("portable chain has an ineligible candidate".into());
            }
            let (pred, truth) = in_order(
                entry
                    .validation_predictions
                    .as_ref()
                    .ok_or("owner prediction missing")?,
                entry
                    .validation_targets
                    .as_ref()
                    .ok_or("owner targets missing")?,
                &evaluation_ids,
            )?;
            targets = Some(truth);
            variants.push(json!({"n_components":entry.identity.n_components,"rmse":entry.selection_report.as_ref().ok_or("owner score missing")?.metrics["rmse"],"predictions":pred,
                "params":entry.identity.params,"ordinal":entry.identity.ordinal,"variant_id":entry.identity.variant_id}));
        }
        let ordinal = winner.identity.ordinal.clone();
        (
            json!({"profile":bank.profile,"decision":bank.decision,"evidence":bank}),
            ordinal,
        )
    } else {
        let bank = dag::execute_training_bank(
            &plan,
            &dag::TrainingBankRequest {
                schema_version: 1,
                profile: dag::TRAINING_BANK_PROFILE.into(),
                failure_group_dimension: "chain".into(),
                target_node: target,
            },
            envelope,
            &sample_ids,
            &run_id,
            admission.root_seed,
            &controllers,
            &scoped_provider,
        )
        .map_err(|e| e.to_string())?;
        for entry in &bank.variants {
            let (pred, truth) = in_order(&entry.predictions, &entry.targets, &evaluation_ids)?;
            targets = Some(truth);
            variants.push(json!({"n_components":entry.identity.n_components,"rmse":entry.report.metrics["rmse"],"predictions":pred,"params":entry.identity.params,"ordinal":entry.identity.ordinal,"variant_id":entry.identity.variant_id}));
        }
        let ordinal = bank.selected.ordinal.clone();
        // The evaluated winner package is native evidence, not an extra deployment fit.
        (
            json!({"profile":bank.profile,"decision":bank.decision,"evidence":bank}),
            ordinal,
        )
    };
    let selected = variants
        .iter()
        .find(|v| v["ordinal"] == selected_ordinal)
        .ok_or("native winner absent from owner evidence")?
        .clone();
    let mut identities = json!({"kind":identity_kind,"sample_ids":ids,"training_sample_ids":training_ids,"evaluation_sample_ids":evaluation_ids,"axis":axis});
    identities
        .as_object_mut()
        .expect("identity evidence object")
        .extend(scoped_provider.row_annotations);
    Ok(
        json!({"schema_version":1,"profile":admission.profile,"owner_profile":admission.owner_profile,"mode":admission.mode,"name":admission.name,
        "rows":matrices.x.rows,"cols":matrices.x.cols,"split":{"kind":if admission.splitter.is_some(){"KennardStone"}else{"all"},"trainIndices":train,"testIndices":test},
        "preprocessing":admission.preprocessing,"variants":variants,"selected":selected,"targets":targets.ok_or("native target evidence absent")?,"evaluation":admission.evaluation,
        "identities":identities,"owner":owner}),
    )
}

pub(crate) fn legacy_result(value: &Value) -> Result<crate::PortablePipelineResult, String> {
    let variant = |v: &Value| -> Result<crate::PortableVariant, String> {
        Ok(crate::PortableVariant {
            n_components: v["n_components"]
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or("Rust portable PLS surface requires components")?,
            rmse: v["rmse"].as_f64().ok_or("missing native RMSE")?,
            predictions: serde_json::from_value(v["predictions"].clone())
                .map_err(|e| e.to_string())?,
        })
    };
    Ok(crate::PortablePipelineResult {
        name: value["name"].as_str().unwrap_or("pipeline").into(),
        rows: value["rows"].as_u64().ok_or("missing rows")? as usize,
        cols: value["cols"].as_u64().ok_or("missing cols")? as usize,
        split: crate::PortableSplitResult {
            kind: value["split"]["kind"]
                .as_str()
                .ok_or("missing split")?
                .into(),
            train_indices: serde_json::from_value(value["split"]["trainIndices"].clone())
                .map_err(|e| e.to_string())?,
            test_indices: serde_json::from_value(value["split"]["testIndices"].clone())
                .map_err(|e| e.to_string())?,
        },
        preprocessing: value["preprocessing"]
            .as_array()
            .ok_or("missing preprocessing")?
            .iter()
            .map(|v| crate::PortablePreprocessingResult {
                kind: v["type"].as_str().unwrap_or("").into(),
                params: legacy_preprocessing_params(v),
            })
            .collect(),
        variants: value["variants"]
            .as_array()
            .ok_or("missing variants")?
            .iter()
            .map(variant)
            .collect::<Result<_, _>>()?,
        selected: variant(&value["selected"])?,
        targets: serde_json::from_value(value["targets"].clone()).map_err(|e| e.to_string())?,
    })
}

fn legacy_preprocessing_params(value: &Value) -> Vec<f64> {
    let p = &value["params"];
    if value["type"] == "SavitzkyGolay" {
        let modes = ["mirror", "constant", "nearest", "wrap", "interp"];
        vec![
            p["window_length"].as_f64().unwrap_or(0.0),
            p["polyorder"].as_f64().unwrap_or(0.0),
            p["deriv"].as_f64().unwrap_or(0.0),
            modes
                .iter()
                .position(|s| Some(*s) == p["mode"].as_str())
                .unwrap_or(4) as f64,
            p["cval"].as_f64().unwrap_or(0.0),
        ]
    } else {
        Vec::new()
    }
}

pub(crate) fn decode_definition(input: &str) -> Result<Value, String> {
    match serde_json::from_str(input) {
        Ok(value) => Ok(value),
        Err(_) => serde_yaml::from_str::<serde_yaml::Value>(input)
            .map_err(|e| e.to_string())
            .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string())),
    }
}

pub(crate) fn admission_plan(
    admission: dag::PortableLinearAdmission,
) -> Result<crate::ExecutionPlan, String> {
    if admission.recipe.last().and_then(|s| s["methodId"].as_str())
        != Some("models.pls.pls_regression")
    {
        return Err("compact Rust portable plan requires PLS; generic regression uses portable-pipeline-run".into());
    }
    let mut preprocessing = Vec::new();
    for step in &admission.preprocessing {
        match step["type"].as_str() {
            Some("StandardNormalVariate")=>preprocessing.push(crate::PortablePreprocessing::StandardNormalVariate),
            Some("SavitzkyGolay")=>{
                let p=&step["params"];let modes=["mirror","constant","nearest","wrap","interp"];
                preprocessing.push(crate::PortablePreprocessing::SavitzkyGolay(crate::SavitzkyGolayParams {
                    window_length:p["window_length"].as_i64().ok_or("owner SG window missing")? as i32,
                    polyorder:p["polyorder"].as_i64().ok_or("owner SG degree missing")? as i32,
                    deriv:p["deriv"].as_i64().ok_or("owner SG derivative missing")? as i32,
                    mode:modes.iter().position(|m|Some(*m)==p["mode"].as_str()).ok_or("owner SG mode missing")? as i32,
                    cval:p["cval"].as_f64().ok_or("owner SG cval missing")?,
                }));
            },
            _=>return Err("compact Rust portable plan admits SNV/SG; generic regression uses portable-pipeline-run".into()),
        }
    }
    let splitter = admission
        .splitter
        .as_ref()
        .map(|v| crate::PortableSplitter {
            kind: "KennardStone".into(),
            test_size: v["params"]["test_size"]
                .as_f64()
                .expect("owner checked size"),
        });
    Ok(crate::ExecutionPlan {
        splitter,
        preprocessing,
        n_components: admission.n_components.clone(),
        owner_admission: admission,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_needs_no_dataset_library_or_execution() {
        let result=plan(&json!({"schema_version":1,"definition":[{"model":{"class":"n4m.PLS","params":{"scale":false}}}]})).unwrap();
        assert_eq!(result["mode"], "training");
        assert_eq!(result["recipe"][0]["params"]["scale_x"], false);
        assert!(plan(&json!({"schema_version":1,"definition":[],"dataset":{}})).is_err());
    }
    #[test]
    fn dense_row_evidence_keeps_opaque_objects_lists_and_input_order() {
        let input = json!({"X":[[1.0,2.0],[3.0,4.0]],"y":[1.0,2.0],"sample_ids":["z","a"],
            "metadata":[{"groups":["opaque"],"nested":[[],{}]},[]],
            "provenance":[{},["source",{"rows":[2,1],"optional":null}]]});
        let (normalized, kind, _, annotations) = record(&input, "spectra").unwrap();
        assert_eq!(kind, "declared");
        assert_eq!(cohort(&normalized).unwrap(), vec!["z", "a"]);
        assert!(normalized["dataset"]["groups"].is_null());
        assert_eq!(annotations["metadata"], input["metadata"]);
        assert_eq!(annotations["provenance"], input["provenance"]);
        let (_, _, _, absent) = record(&json!({"X":[[1.0]],"y":[1.0]}), "spectra").unwrap();
        assert!(absent.is_empty());
    }
    #[test]
    fn malformed_dense_row_evidence_is_refused_before_native_load() {
        for key in ["metadata", "provenance"] {
            for (cells, expected) in [
                (json!([{}]), format!("dataset.{key}: row count")),
                (Value::Null, format!("dataset.{key}: row-aligned array")),
                (
                    json!([{}, null]),
                    format!("dataset.{key}[1]: opaque object or list"),
                ),
            ] {
                let mut dataset = json!({"X":[[1.0],[2.0]],"y":[1.0,2.0]});
                dataset[key] = cells;
                let error = run(&json!({"schema_version":1,"definition":[{"model":{"class":"n4m.PLS"}}],"dataset":dataset}), Path::new("/missing/native/library")).unwrap_err();
                assert!(error.contains(&expected), "{error}");
            }
        }
    }
}
