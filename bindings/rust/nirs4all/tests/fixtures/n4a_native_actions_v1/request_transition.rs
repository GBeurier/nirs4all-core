//! Transition regression: the DAG-owned request must equal the request Core
//! assembled before the move, on real IO packages and the live catalog.
//!
//! `legacy_native_pipeline_training_request` is the pre-move Core source, kept
//! here for this comparison only. Its one edit takes the manifests as an
//! argument, so both paths see the same catalog. Delete this file with the
//! transition; it is not a second production compiler.
use std::collections::BTreeMap;

use dag_ml_core::{
    ControllerManifest, DataBinding, NodeId, Phase, TrainingDataIdentity, TrainingRequest,
};
use serde_json::{json, Value};

use super::{native_pipeline_training_request, DatasetPackage, NativePipelineRecipe};
use crate::io_training::{
    base_regression_training_request, CanonicalPlsProfile, DatasetPackageMethodsProvider,
};

use crate::native_action_test_data as data;

/// Construct a fingerprint-bound, catalog-resolved training request for a linear native
/// pipeline. Each fitted transform executes separately inside every CV fold.
fn legacy_native_pipeline_training_request(
    dataset: &DatasetPackage,
    source_id: &str,
    recipe: &NativePipelineRecipe,
    manifests: &[ControllerManifest],
) -> Result<TrainingRequest, String> {
    if recipe.steps.is_empty()
        || recipe.steps.len() > 32
        || recipe.candidates.is_empty()
        || recipe.candidates.len() > 32
    {
        return Err(
            "native pipeline requires 1..32 steps and 1..32 candidate parameter maps".into(),
        );
    }
    let classification = recipe
        .steps
        .last()
        .is_some_and(|step| step.role == "classifier");
    let provider =
        DatasetPackageMethodsProvider::for_product_pipeline(dataset, source_id, classification)?;
    let base = base_regression_training_request(&provider, CanonicalPlsProfile::Raw, true)?;
    let mut request = serde_json::to_value(base).map_err(|e| e.to_string())?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut ids = Vec::new();
    for (index, step) in recipe.steps.iter().enumerate() {
        let final_step = index + 1 == recipe.steps.len();
        let kind = match (step.role.as_str(), final_step) {
            ("transformer" | "selector", false) => "transform",
            ("regressor" | "classifier", true) => "model",
            _ => return Err(
                "pipeline requires transformer/selector steps followed by one regressor/classifier"
                    .into(),
            ),
        };
        if step.method_id.trim().is_empty()
            || step.params.contains_key("method_id")
            || step.params.contains_key("unsafe_flags")
        {
            return Err(
                "method_id and unsafe_flags cannot be overridden by user parameters".into(),
            );
        }
        let id = if final_step {
            "model:pls".to_string()
        } else {
            format!("transform:{index}")
        };
        NodeId::new(&id).map_err(|e| e.to_string())?;
        let mut params = step.params.clone();
        params.insert("method_id".into(), json!(step.method_id));
        nodes.push(json!({"id":id,"kind":kind,"operator":format!("n4m:{}",step.method_id),"params":params,
            "ports":{"inputs":[{"name":"x","kind":"data","representation":"tabular_numeric","cardinality":"one","description":""}],
            "outputs":[{"name":if final_step {"oof"} else {"x_out"},"kind":if final_step {"prediction"} else {"data"},"representation":if final_step {Value::Null} else {json!("tabular_numeric")},"cardinality":"one","description":""}]},"metadata":{},"seed_label":null}));
        if let Some(previous) = ids.last() {
            edges.push(json!({"source":{"node_id":previous,"port_name":"x_out"},"target":{"node_id":id,"port_name":"x"},
                "contract":{"kind":"data","representation":"tabular_numeric","requires_oof":false,"requires_fold_alignment":true,"propagates_lineage":true}}));
        }
        ids.push(id);
    }
    request["campaign"]["metadata"]["native_pipeline_target_names"] =
        request["options"]["outputs"][0]["target_names"].clone();
    if let Some(index) = recipe
        .steps
        .last()
        .and_then(|step| step.params.get("native_target_index"))
    {
        let index = index
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("native_target_index requires addressable nonnegative integer")?;
        for key in ["target_names", "target_units", "class_labels"] {
            let values = request["options"]["outputs"][0][key]
                .as_array()
                .ok_or("Invalid native output target schema")?;
            let value = values
                .get(index)
                .ok_or("native_target_index exceeds declared targets")?
                .clone();
            request["options"]["outputs"][0][key] = json!([value]);
        }
    }
    if classification {
        if request["options"]["outputs"][0]["target_names"]
            .as_array()
            .is_none_or(|names| names.len() != 1)
        {
            return Err("native classification requires exactly one target".into());
        }
        request["options"]["outputs"][0]["prediction_kind"] = json!("class_label");
        request["options"]["selection"]["metric"] =
            json!({"name":"balanced_accuracy","objective":"maximize"});
    }
    request["graph"]["nodes"] = json!(nodes);
    request["graph"]["edges"] = json!(edges);
    request["controller_manifests"] = json!(manifests);
    // Only the first node reads raw IO buffers; subsequent nodes consume their
    // upstream native identity-keyed feature handles.
    let mut binding = request["campaign"]["data_bindings"]["model:pls"][0].clone();
    binding["node_id"] = json!(ids[0]);
    request["campaign"]["data_bindings"] = json!({ids[0].clone():[binding]});
    let shape = request["campaign"]["shape_plans"]["model:pls"].clone();
    let mut shapes = serde_json::Map::new();
    for id in &ids {
        let mut declaration = shape.clone();
        declaration["node_id"] = json!(id);
        shapes.insert(id.clone(), declaration);
    }
    request["campaign"]["shape_plans"] = Value::Object(shapes);
    let typed_binding: DataBinding =
        serde_json::from_value(request["campaign"]["data_bindings"][&ids[0]][0].clone())
            .map_err(|e| e.to_string())?;
    request["data_identities"] = json!([TrainingDataIdentity::from_binding_envelope(
        &typed_binding,
        provider.external_envelope()
    )
    .map_err(|e| e.to_string())?]);
    let sample_ids: Vec<String> = dataset
        .audits
        .iter()
        .find_map(|audit| audit.get("input_sample_ids"))
        .map(|value| serde_json::from_value(value.clone()).map_err(|e| e.to_string()))
        .transpose()?
        .unwrap_or_else(|| {
            provider
                .relations()
                .records
                .iter()
                .map(|r| r.sample_id.to_string())
                .collect()
        });
    let groups: BTreeMap<String, String> = provider
        .relations()
        .records
        .iter()
        .map(|r| {
            (
                r.sample_id.to_string(),
                r.group_id
                    .as_ref()
                    .map(|g| g.to_string())
                    .unwrap_or_else(|| r.sample_id.to_string()),
            )
        })
        .collect();
    let dependent = groups
        .values()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != groups.len()
        || provider
            .relations()
            .records
            .iter()
            .any(|r| r.origin_sample_id.is_some());
    if dependent && dataset.folds.is_empty() {
        return Err("dependent/grouped samples require explicit leakage-safe fold_ids".into());
    }
    if !dataset.folds.is_empty() {
        if dataset.folds.len() < 2 {
            return Err("at least two declared CV folds required".into());
        }
        let fold_set = json!({"id":"native:declared-folds","sample_ids":sample_ids,
            "sample_groups":if dependent {json!(groups)} else {json!({})},
            "folds":dataset.folds.iter().enumerate().map(|(i,(train,validation))|{
                let select = |indexes:&Vec<i64>| -> Result<Vec<String>,String> {
                    indexes.iter().map(|index| usize::try_from(*index).ok().and_then(|position|sample_ids.get(position)).cloned().ok_or("declared fold index outside native training cohort".into())).collect()
                };
                Ok(json!({"fold_id":format!("native.fold.{i}"),"train_sample_ids":select(train)?,"validation_sample_ids":select(validation)?,"metadata":{}}))
            }).collect::<Result<Vec<_>,String>>()?});
        request["campaign"]["split_invocation"]["fold_set"] = fold_set;
        request["campaign"]["split_invocation"]["id"] = json!("native:declared-folds");
    }
    if dependent {
        request["campaign"]["leakage_policy"]["split_unit"] = json!("group");
        request["campaign"]["leakage_policy"]["require_group_ids"] = json!(true);
        request["campaign"]["split_invocation"]["leakage_policy"] =
            request["campaign"]["leakage_policy"].clone();
    }
    request["campaign"]["metadata"]["native_pipeline_recipe"] = json!(recipe);
    request["campaign"]["metadata"]["input_sample_ids"] = json!(provider
        .relations()
        .records
        .iter()
        .map(|r| r.sample_id.to_string())
        .collect::<Vec<_>>());
    for candidate in &recipe.candidates {
        if candidate.contains_key("method_id")
            || candidate.contains_key("unsafe_flags")
            || candidate.contains_key("native_target_index")
        {
            return Err("candidate parameters cannot override method identity or safety".into());
        }
    }
    request["campaign"]["generation"] = json!({"strategy":"cartesian","max_variants":recipe.candidates.len(),
        "dimensions":[{"name":"native_model_params","choices":recipe.candidates.iter().enumerate().map(|(i,params)|json!({
            "label":format!("candidate_{i}"),"value":i,"param_overrides":if params.is_empty() {vec![]} else {vec![json!({"node_id":"model:pls","params":params})]},"active_subsequence":null})).collect::<Vec<_>>()}]});
    let mut request: TrainingRequest =
        serde_json::from_value(request).map_err(|e| e.to_string())?;
    request.request_fingerprint = request.compute_fingerprint().map_err(|e| e.to_string())?;
    request.validate().map_err(|e| e.to_string())?;
    let projected = request.project().map_err(|e| e.to_string())?;
    for (id, step) in ids.iter().zip(&recipe.steps) {
        let node_id = NodeId::new(id).map_err(|e| e.to_string())?;
        let expected = format!("controller:n4m.{}", step.role);
        if projected
            .plan
            .node_plans
            .get(&node_id)
            .is_none_or(|node| node.controller_id.as_str() != expected)
        {
            return Err(format!(
                "native catalog role differs from declared role for {id}"
            ));
        }
    }
    if projected
        .plan
        .node_plans
        .values()
        .any(|node| !node.supported_phases.contains(&Phase::Refit))
    {
        return Err("every native pipeline step must support portable full refit".into());
    }
    Ok(request)
}

fn package(record: &Value) -> DatasetPackage {
    nirs4all_io_crate::core::public_dataset::masked_matrix_dataset_package(record, "spectra")
        .unwrap()
        .0
}

fn live_manifests(library: &std::path::Path) -> Vec<ControllerManifest> {
    let runtime =
        crate::native_methods_replay::configure_methods_runtime_for_source(library).unwrap();
    let mut manifests = dag_ml_core::methods_estimator_host_controller_specs(&runtime)
        .unwrap()
        .iter()
        .map(|spec| spec.derive())
        .collect::<dag_ml_core::Result<Vec<_>>>()
        .unwrap();
    manifests.sort_by(|a, b| a.controller_id.cmp(&b.controller_id));
    manifests
}

/// Declared folds, default folds and group-dependent folds take distinct paths.
fn records() -> Vec<(&'static str, Value)> {
    let declared = data::record("train", 18, true, true);
    let mut default_folds = declared.clone();
    default_folds["fold_ids"] = json!(vec![Value::Null; 18]);
    let mut grouped = declared.clone();
    grouped["dataset"]["groups"] = json!({"dtype":"<U2","shape":[18],
        "values":(0..18).map(|row| format!("g{}", row / 2)).collect::<Vec<_>>()});
    grouped["fold_ids"] = json!((0..18)
        .map(|row| format!("fold:{}", (row / 2) % 3))
        .collect::<Vec<_>>());
    vec![
        ("declared", declared),
        ("default", default_folds),
        ("grouped", grouped),
    ]
}

#[test]
fn dag_owned_request_equals_the_pre_move_core_request() {
    let library = std::path::PathBuf::from(
        std::env::var_os("N4M_LIBRARY_PATH").expect("controller runtime required"),
    );
    let manifests = live_manifests(&library);
    for (label, record) in records() {
        let dataset = package(&record);
        for selected in [false, true] {
            let recipe: NativePipelineRecipe =
                serde_json::from_value(data::recipe_value(selected)).unwrap();
            let legacy =
                legacy_native_pipeline_training_request(&dataset, "spectra", &recipe, &manifests)
                    .unwrap();
            let moved =
                native_pipeline_training_request(&dataset, "spectra", &recipe, &library).unwrap();
            assert_eq!(
                moved.request_fingerprint, legacy.request_fingerprint,
                "{label}/{selected}"
            );
            assert_eq!(
                serde_json::to_value(&moved).unwrap(),
                serde_json::to_value(&legacy).unwrap(),
                "{label}/{selected}"
            );
        }
    }
}

#[test]
fn mixed_training_facts_preserve_io_order_masks_and_fold_provenance() {
    use crate::n4a_owner_portable::native_linear_facts;
    use sha2::{Digest, Sha256};
    let record = data::interleaved_record();
    let dataset =
        nirs4all_io_crate::core::public_dataset::masked_matrix_dataset_package(&record, "nir")
            .unwrap()
            .0;
    let recipe = serde_json::from_value(data::recipe_value(false)).unwrap();
    let facts = native_linear_facts(&dataset, "nir", &recipe).unwrap();
    let expected = data::training_cohort(&record);
    let expected_ids: Vec<String> =
        serde_json::from_value(expected["dataset"]["sample_ids"].clone()).unwrap();
    assert_eq!(facts.input_sample_ids.as_ref().unwrap(), &expected_ids);
    assert_eq!(expected_ids.len(), 32);
    assert_eq!(expected_ids[5], "sample-002");
    assert_eq!(record["dataset"]["sample_ids"][5], "sample-035");
    assert_eq!(facts.relations.records.len(), 32);
    assert!(facts.relations.records.iter().all(|r| {
        expected_ids.contains(&r.sample_id.to_string())
            && r.tags.iter().any(|tag| tag == "partition:train")
    }));
    let provider =
        DatasetPackageMethodsProvider::for_product_pipeline(&dataset, "nir", false).unwrap();
    let full = provider.provider.envelope();
    assert_eq!(
        full.coordinator_relations.as_ref().unwrap().records.len(),
        40
    );
    assert_eq!(
        facts.envelope.data_content_fingerprint,
        full.data_content_fingerprint
    );
    assert_eq!(
        facts.envelope.relation_fingerprint.as_ref().unwrap(),
        &facts.relations.fingerprint().unwrap()
    );
    let masks: BTreeMap<_, _> = expected_ids.iter().map(|id| (id, vec![true])).collect();
    let identity = serde_json::to_vec(&json!({
        "target_content_fingerprint":full.target_content_fingerprint,
        "target_names":provider.provider.target_names(), "observed_by_sample":masks,
    }))
    .unwrap();
    assert_eq!(
        facts.envelope.target_content_fingerprint,
        Some(format!("{:x}", Sha256::digest(identity)))
    );
    assert_eq!(facts.declared_folds.len(), 4);
    let mapping: BTreeMap<_, _> = facts
        .relations
        .records
        .iter()
        .map(|r| (r.observation_id.as_str(), r.sample_id.as_str()))
        .collect();
    for ((train, validation), provenance) in
        facts.declared_folds.iter().zip(&dataset.fold_provenance)
    {
        assert_eq!((train.len(), validation.len()), (24, 8));
        for (indices, observations) in [
            (train, &provenance.train_observation_ids),
            (validation, &provenance.validation_observation_ids),
        ] {
            assert_eq!(
                indices
                    .iter()
                    .map(|i| expected_ids[*i as usize].as_str())
                    .collect::<Vec<_>>(),
                observations
                    .iter()
                    .map(|id| mapping[id.as_str()])
                    .collect::<Vec<_>>()
            );
        }
    }
    // The complete-target legacy constructor remains full-record and unchanged.
    let all_train = package(&data::record("train", 18, true, true));
    let legacy = DatasetPackageMethodsProvider::new(&all_train, "spectra").unwrap();
    let scoped =
        DatasetPackageMethodsProvider::for_product_pipeline(&all_train, "spectra", false).unwrap();
    assert_eq!(
        serde_json::to_vec(legacy.relations()).unwrap(),
        serde_json::to_vec(scoped.relations()).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(legacy.external_envelope()).unwrap(),
        serde_json::to_vec(scoped.external_envelope()).unwrap()
    );

    let mut prediction_rows = record.clone();
    prediction_rows["dataset"]["partitions"]["dtype"] = json!("<U7");
    for partition in prediction_rows["dataset"]["partitions"]["values"]
        .as_array_mut()
        .unwrap()
    {
        if *partition == "test" {
            *partition = json!("predict");
        }
    }
    let prediction_package =
        nirs4all_io_crate::core::public_dataset::masked_matrix_dataset_package(
            &prediction_rows,
            "nir",
        )
        .unwrap()
        .0;
    assert_eq!(
        native_linear_facts(&prediction_package, "nir", &recipe)
            .unwrap()
            .input_sample_ids,
        facts.input_sample_ids
    );
    let mut no_train = record.clone();
    no_train["dataset"]["partitions"]["values"] = json!(vec!["test"; 40]);
    no_train["fold_ids"] = json!(vec![Value::Null; 40]);
    let no_train =
        nirs4all_io_crate::core::public_dataset::masked_matrix_dataset_package(&no_train, "nir")
            .unwrap()
            .0;
    assert!(native_linear_facts(&no_train, "nir", &recipe)
        .unwrap_err()
        .contains("at least one train"));

    let mut wrong_order = dataset.clone();
    wrong_order.audits[0]["input_sample_ids"]
        .as_array_mut()
        .unwrap()
        .swap(0, 6);
    assert!(native_linear_facts(&wrong_order, "nir", &recipe)
        .unwrap_err()
        .contains("fold provenance"));
    let mut wrong_provenance = dataset.clone();
    wrong_provenance.fold_provenance[0].train_observation_ids[0] = "sample-035".into();
    assert_eq!(
        native_linear_facts(&wrong_provenance, "nir", &recipe).unwrap_err(),
        "Whole-dataset owner record does not correspond to materialized package"
    );
    let mut wrong_mask = dataset.clone();
    wrong_mask.audits[0]["input_sample_ids"][5] = json!("unknown-heldout");
    // Owner mismatch or identity validation may refuse before Core's train filtering.
    assert!(native_linear_facts(&wrong_mask, "nir", &recipe).is_err());
}

#[test]
fn mixed_training_execution_views_and_influence_exclude_heldout() {
    use crate::n4a_owner_portable::native_linear_facts;
    let library = std::path::PathBuf::from(
        std::env::var_os("N4M_LIBRARY_PATH").expect("controller runtime required"),
    );
    let recipe = serde_json::from_value(data::recipe_value(false)).unwrap();
    let (dataset, request) = super::native_pipeline_export_training_request(
        &data::interleaved_record(),
        "nir",
        &recipe,
        &library,
    )
    .unwrap();
    let facts = native_linear_facts(&dataset, "nir", &recipe).unwrap();
    let expected = facts.input_sample_ids.unwrap();
    let projection = request.project().unwrap();
    let folds = projection.plan.fold_set.as_ref().unwrap();
    assert_eq!(
        folds
            .sample_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(folds.folds.len(), 4);
    for (fold, (train, validation)) in folds.folds.iter().zip(&dataset.folds) {
        assert_eq!(
            fold.train_sample_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            train
                .iter()
                .map(|i| expected[*i as usize].clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            fold.validation_sample_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            validation
                .iter()
                .map(|i| expected[*i as usize].clone())
                .collect::<Vec<_>>()
        );
    }
    let audit = &request.campaign.metadata["native_pipeline_training_relations"];
    assert_eq!(audit["relations"], json!(facts.relations));
    let provider =
        DatasetPackageMethodsProvider::for_product_pipeline(&dataset, "nir", false).unwrap();
    let outcome = crate::io_training::execute_dataset_package_methods_training(
        &provider,
        &request,
        "outcome:mixed-views",
        dag_ml_core::RunId::new("run:mixed-views").unwrap(),
        dag_ml_core::BundleId::new("bundle:mixed-views").unwrap(),
        &library,
        &mut dag_ml_core::InMemoryArtifactStore::new(),
    )
    .unwrap();
    outcome
        .training_influence
        .validate_identity_closure(provider.relations())
        .unwrap();
    let influence = serde_json::to_string(&outcome.training_influence).unwrap();
    for i in 32..40 {
        assert!(!influence.contains(&format!("sample-{i:03}")));
    }
    let views = provider.training_views.lock().unwrap();
    assert!(!views.is_empty());
    let mut saw_fit = false;
    let mut saw_validation = false;
    let mut saw_refit = false;
    for (phase, view) in views.iter() {
        let ids = view.sample_ids.as_ref().expect("explicit scheduled IDs");
        assert!(ids.iter().all(|id| expected.contains(&id.to_string())));
        if *phase == Phase::FitCv {
            saw_fit |= ids.len() == 24;
            saw_validation |= ids.len() == 8;
            assert!(folds
                .folds
                .iter()
                .any(|f| ids == &f.train_sample_ids || ids == &f.validation_sample_ids));
        }
        if *phase == Phase::Refit {
            saw_refit = true;
            assert_eq!(
                ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                expected
            );
        }
    }
    assert!(saw_fit && saw_validation && saw_refit);
}

#[test]
fn mixed_fold_provenance_maps_distinct_observation_ids_to_samples() {
    use crate::n4a_owner_portable::native_linear_facts;
    use nirs4all_io_crate::core::materialize::{assemble_in_memory, InMemorySource, SourcePayload};
    use nirs4all_io_crate::core::spec::DatasetSpec;

    // Public Dataset v1/v2 assigns the same observation and sample IDs. Use
    // IO's actual DatasetSpec assembler to produce distinct identities without
    // modifying a package attached to an immutable public owner record.
    let record = data::interleaved_record();
    let raw = &record["dataset"];
    let rows = raw["sample_ids"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(row, id)| {
            json!({
                "sid":id, "oid":format!("observation-{}", id.as_str().unwrap()),
                "set":raw["partitions"]["values"][row], "fold":record["fold_ids"][row],
                "x0":raw["sources"][0]["array"]["values"][row][0],
                "x1":raw["sources"][0]["array"]["values"][row][1],
                "x2":raw["sources"][0]["array"]["values"][row][2],
                "response":raw["y"]["values"][row][0],
            })
        })
        .collect();
    let spec = DatasetSpec::from_value(&json!({
        "name":"distinct-observation-cohort", "task_type":"regression",
        "sample_index":{"by":"id", "key":"sid", "observation_id":"oid"},
        "sources":[{"id":"nir", "role":"mixed", "input":"cohort.json", "columns":[
            {"role":"features", "select":["x0","x1","x2"]},
            {"role":"targets", "select":["response"]},
            {"role":"metadata", "select":["sid","oid","set","fold"]}
        ]}],
        "partitions":{"by":"column", "column":"set",
            "train_values":["train"], "test_values":["test"]},
        "folds":{"column":"fold"}
    }))
    .unwrap();
    let assembled = assemble_in_memory(
        &spec,
        &[InMemorySource {
            name: "cohort.json".into(),
            payload: SourcePayload::Rows(rows),
        }],
        &std::collections::HashMap::new(),
        None,
    )
    .unwrap();
    let package = DatasetPackage::from_assembled(&assembled).unwrap();
    assert_eq!(package.audits, assembled.audits);
    let provider =
        DatasetPackageMethodsProvider::for_product_pipeline(&package, "nir", false).unwrap();
    assert_eq!(
        provider
            .provider
            .envelope()
            .coordinator_relations
            .as_ref()
            .unwrap()
            .records
            .len(),
        40
    );
    assert_eq!(provider.relations().records.len(), 32);
    assert!(provider
        .relations()
        .records
        .iter()
        .all(|r| r.observation_id.as_str() != r.sample_id.as_str()));
    let expected = data::training_cohort(&record);
    let ids: Vec<String> =
        serde_json::from_value(expected["dataset"]["sample_ids"].clone()).unwrap();
    assert_eq!(ids[5], "sample-002");
    let mapping: BTreeMap<_, _> = provider
        .relations()
        .records
        .iter()
        .map(|r| (r.observation_id.as_str(), r.sample_id.as_str()))
        .collect();
    assert_eq!(package.folds.len(), 4);
    assert_eq!(package.fold_provenance.len(), 4);
    for ((train, validation), provenance) in package.folds.iter().zip(&package.fold_provenance) {
        assert_eq!((train.len(), validation.len()), (24, 8));
        for (indices, observations) in [
            (train, &provenance.train_observation_ids),
            (validation, &provenance.validation_observation_ids),
        ] {
            assert_eq!(
                indices
                    .iter()
                    .map(|i| ids[*i as usize].as_str())
                    .collect::<Vec<_>>(),
                observations
                    .iter()
                    .map(|id| mapping[id.as_str()])
                    .collect::<Vec<_>>()
            );
        }
    }
    // This assembly profile does not emit the public-record input-order audit.
    // Keep the native boundary refusal: no audit is invented to certify it.
    assert!(package
        .audits
        .iter()
        .all(|audit| audit.get("input_sample_ids").is_none()));
    let recipe = serde_json::from_value(data::recipe_value(false)).unwrap();
    assert_eq!(
        native_linear_facts(&package, "nir", &recipe).unwrap_err(),
        "Mixed native folds require IO input sample order"
    );
}
