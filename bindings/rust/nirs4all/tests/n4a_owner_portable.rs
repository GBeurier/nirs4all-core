//! Runtime-free plans and native retrain over the retained two-target fixture.
//! Execution of a portable plan by DAG WASM is covered by `n4a_wasm_exchange`.
use nirs4all::n4a::*;
use nirs4all::n4a_owner_native::*;
use nirs4all::n4a_owner_portable::*;
use nirs4all::pipeline_workflow::native_pipeline_export_training_request;
use serde_json::{json, Value};

#[path = "fixtures/n4a_native_actions_v1/support.rs"]
mod support;
use support::*;

fn retrain(source: &OwnedN4a, record: &Value, label: &str) -> NativeProduction {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    retrain_n4a_native(
        source,
        NativeRetrainRequest {
            record: &serde_json::to_vec(record).unwrap(),
            runtime: NativeRuntime {
                library: &library,
                expected_sha256: &sha,
            },
            run_id: label,
            container_id: label,
            embed_training_record: true,
            memory_budget: N4aMemoryBudget::default(),
        },
    )
    .unwrap()
}

/// The catalog a host would trust, taken from the native request it must match.
fn native_manifests(record: &Value, selected: bool) -> String {
    let recipe = serde_json::from_value(recipe_value(selected)).unwrap();
    let (_, request) =
        native_pipeline_export_training_request(record, "spectra", &recipe, &library()).unwrap();
    serde_json::to_string(&request.controller_manifests).unwrap()
}

fn close_to_record(actual: &[f64], rows: &Value, cols: usize) {
    let rows = rows.as_array().unwrap();
    assert_eq!(actual.len(), rows.len() * cols);
    for (row, expected) in rows.iter().enumerate() {
        for (col, value) in expected.as_array().unwrap().iter().enumerate() {
            let cell = actual[row * cols + col];
            let value = value.as_f64().unwrap();
            assert!(
                (cell - value).abs() <= 1e-5,
                "[{row}][{col}]: {cell} {value}"
            );
        }
    }
}

#[test]
fn retrain_takes_recipe_and_source_from_the_validated_package() {
    for selected in [false, true] {
        let source = source(selected, true, "retrain:source");
        let before = copy_n4a_bytes(&source);
        let fresh = record("fresh", 18, true, true);
        let produced = retrain(&source, &fresh, "retrain:result");
        assert_eq!(copy_n4a_bytes(&source), before);
        let (old, new) = (package(&source), package(&produced.container));
        assert_eq!(
            new.template.campaign.metadata["native_pipeline_recipe"],
            old.template.campaign.metadata["native_pipeline_recipe"]
        );
        assert_ne!(new.data_identities, old.data_identities);
        assert_eq!(produced.training.source_id, "spectra");
        assert_eq!(
            new.output_bindings[0].target_names,
            expected_names(selected)
        );
        // The view is the actual new fit, not the stored one.
        assert_eq!(
            produced.training.outcome.outcome_fingerprint,
            new.training_outcome.outcome_fingerprint
        );
        let external = serde_json::to_vec(&record("future", 5, false, false)).unwrap();
        let library = library();
        let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
        let predicted = predict_n4a_native(
            &produced.container,
            action(&external, "predict-external", &library, &sha),
        )
        .unwrap();
        assert_eq!(predicted.sample_ids.len(), 5);
    }
}

#[test]
fn portable_profile_narrows_only_the_portable_entrypoints() {
    let native = source(true, true, "portable:selected");
    let bytes = copy_n4a_bytes(&native);
    let training = record("train", 18, true, true);
    let manifests = native_manifests(&training, true);
    let plan = prepare_retrain_portable(
        &bytes,
        &serde_json::to_vec(&training).unwrap(),
        &manifests,
        "portable:refused",
        N4aMemoryBudget::default(),
    );
    assert!(plan
        .err()
        .is_some_and(|error| error.to_string().contains("native_target_index")));
    let external = serde_json::to_vec(&record("future", 5, false, false)).unwrap();
    let portable = prepare_predict_portable(
        &bytes,
        PortablePredictRequest {
            entrypoint: "predict-external",
            record: Some(&external),
            source_id: "spectra",
            partition: "predict",
            require_training_schema: true,
            request_id: "request:refused",
            memory_budget: N4aMemoryBudget::default(),
        },
    );
    assert!(portable
        .err()
        .is_some_and(|error| error.to_string().contains("native_target_index")));
    // The identical archive stays valid for the native runtime.
    retrain(&native, &record("fresh", 18, true, true), "portable:native");
}

#[test]
fn predict_plan_is_target_free_and_in_physical_record_order() {
    let source = source(false, true, "portable:predict");
    let labelled = record("future", 6, false, true);
    let bytes = serde_json::to_vec(&labelled).unwrap();
    let plan = prepare_predict_portable(
        &copy_n4a_bytes(&source),
        PortablePredictRequest {
            entrypoint: "predict-external",
            record: Some(&bytes),
            source_id: "spectra",
            partition: "predict",
            require_training_schema: true,
            request_id: "request:plan",
            memory_budget: N4aMemoryBudget::default(),
        },
    )
    .unwrap();
    assert_eq!(json!(plan.sample_ids), labelled["dataset"]["sample_ids"]);
    assert_eq!(plan.target_names, expected_names(false));
    assert_eq!((plan.rows, plan.cols), (6, 7));
    close_to_record(
        &plan.x,
        &labelled["dataset"]["sources"][0]["array"]["values"],
        plan.cols,
    );
    let envelopes: Value = serde_json::from_str(&plan.envelopes_json).unwrap();
    let envelopes = envelopes.as_object().unwrap();
    assert_eq!(envelopes.len(), 1);
    assert!(envelopes
        .values()
        .all(|e| e["target_content_fingerprint"].is_null()));
    assert_eq!(package(&source).package_fingerprint, {
        let exact: Value = serde_json::from_str(&plan.package_json).unwrap();
        exact["package_fingerprint"].as_str().unwrap().to_string()
    });
}

#[test]
fn train_plan_matches_native_identity_and_seal_binds_its_request() {
    let native = source(false, true, "portable:train");
    let training = record("train", 18, true, true);
    let bytes = serde_json::to_vec(&training).unwrap();
    let manifests = native_manifests(&training, false);
    let plan = prepare_retrain_portable(
        &copy_n4a_bytes(&native),
        &bytes,
        &manifests,
        "portable:train",
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let direct = prepare_train_portable(
        &bytes,
        "spectra",
        &recipe_value(false).to_string(),
        &manifests,
        "portable:train",
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert_eq!(plan.request_json(), direct.request_json());
    let request: Value = serde_json::from_str(plan.request_json()).unwrap();
    let expected = package(&native).training_request_fingerprint;
    assert_eq!(request["request_fingerprint"], json!(expected));
    assert_eq!(json!(plan.sample_ids()), training["dataset"]["sample_ids"]);
    assert_eq!(plan.target_names(), expected_names(false));
    close_to_record(
        plan.x(),
        &training["dataset"]["sources"][0]["array"]["values"],
        plan.cols(),
    );
    close_to_record(
        plan.y(),
        &training["dataset"]["y"]["values"],
        plan.target_names().len(),
    );

    let outcome = std::str::from_utf8(root(&native, "result")).unwrap();
    let model = std::str::from_utf8(root(&native, "model")).unwrap();
    let sealed = plan
        .seal(
            outcome,
            model,
            "portable:sealed",
            true,
            N4aMemoryBudget::default(),
        )
        .unwrap();
    let view: Value = serde_json::from_str(&sealed.view_json).unwrap();
    assert_eq!(
        view,
        serde_json::from_str::<Value>(
            &inspect_n4a_view_json(&sealed.bytes, N4aMemoryBudget::default()).unwrap()
        )
        .unwrap()
    );

    // Same record, another plan: the fit does not belong to this request.
    let mut other = recipe_value(false);
    other["candidates"] = json!([{"alpha": 0.5}]);
    let foreign = prepare_train_portable(
        &bytes,
        "spectra",
        &other.to_string(),
        &manifests,
        "portable:foreign",
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert!(matches!(
        foreign.seal(
            outcome,
            model,
            "portable:foreign",
            true,
            N4aMemoryBudget::default()
        ),
        Err(NativeActionError::Producer(_))
    ));
}

#[test]
fn mixed_training_plan_exports_only_train_rows_and_relations() {
    let record = interleaved_record();
    let train = training_cohort(&record);
    let recipe = serde_json::from_value(recipe_value(false)).unwrap();
    let (_, native) =
        native_pipeline_export_training_request(&record, "nir", &recipe, &library()).unwrap();
    let manifests = serde_json::to_string(&native.controller_manifests).unwrap();
    let prepare = |record: &Value| {
        prepare_train_portable(
            &serde_json::to_vec(record).unwrap(),
            "nir",
            &recipe_value(false).to_string(),
            &manifests,
            "portable:mixed",
            N4aMemoryBudget::default(),
        )
    };
    let plan = prepare(&record).unwrap();
    assert_eq!(plan.rows(), 32);
    assert_eq!(json!(plan.sample_ids()), train["dataset"]["sample_ids"]);
    close_to_record(
        plan.x(),
        &train["dataset"]["sources"][0]["array"]["values"],
        plan.cols(),
    );
    close_to_record(
        plan.y(),
        &train["dataset"]["y"]["values"],
        plan.target_names().len(),
    );
    let request: Value = serde_json::from_str(plan.request_json()).unwrap();
    assert_eq!(request, json!(native));
    let relations: Value = serde_json::from_str(plan.relations_json()).unwrap();
    assert_eq!(
        relations,
        request["campaign"]["metadata"]["native_pipeline_training_relations"]["relations"]
    );
    assert_eq!(relations["records"].as_array().unwrap().len(), 32);
    let envelopes: Value = serde_json::from_str(plan.envelopes_json()).unwrap();
    let envelope = envelopes.as_object().unwrap().values().next().unwrap();
    assert_eq!(envelope["coordinator_relations"], relations);
    assert_eq!(
        envelope["relation_fingerprint"],
        request["data_identities"][0]["relation_fingerprint"]
    );
    // Held-out masks stay outside the complete-target portable training plan.
    let mut heldout_masked = record.clone();
    heldout_masked["dataset"]["target_mask"]["values"][5][0] = json!(false);
    let masked = prepare(&heldout_masked).unwrap();
    assert_eq!(masked.sample_ids(), plan.sample_ids());
    assert_eq!(masked.x(), plan.x());
    assert_eq!(masked.y(), plan.y());
    let mut train_masked = record;
    train_masked["dataset"]["target_mask"]["values"][0][0] = json!(false);
    assert!(prepare(&train_masked)
        .err()
        .unwrap()
        .to_string()
        .contains("partial target masks"));
}
