//! Targeted native-owner gates for Root's pooled cohort. Never skip absent native artifacts.
use std::path::PathBuf;

use nirs4all::{
    parse_execution_plan_str, predict_exported_portable_model_with_library,
    run_portable_pipeline_with_exported_model, PortableDataset, PortableSession,
};
use serde_json::{json, Value};

fn library() -> PathBuf {
    PathBuf::from(std::env::var_os("N4M_LIBRARY_PATH").expect("Root pooled exact Methods artifact"))
        .canonicalize()
        .unwrap()
}

fn oracle() -> Value {
    serde_json::from_str(include_str!("parity/expected/portable_python_oracle.json")).unwrap()
}

#[test]
fn public_no_fit_plan_preserves_flags_and_refuses_controls_without_a_library() {
    let recipe = json!({"pipeline":[{"class":"n4m.SNV","params":{"with_std":false}},
        {"model":{"class":"n4m.PLS","params":{"scale":false,"n_components":5}},"param":"n_components","_range_":[1,3,1]}]});
    let plan = nirs4all::portable_execution::plan(&json!({"schema_version":1,"definition":recipe}))
        .unwrap();
    assert_eq!(plan["mode"], "training");
    assert_eq!(plan["nComponents"], json!([1, 2, 3]));
    assert_eq!(plan["recipe"][0]["params"]["with_std"], false);
    assert_eq!(plan["candidates"][0]["scale_x"], false);
    let compact = parse_execution_plan_str(&recipe.to_string()).unwrap();
    assert_eq!(
        compact.owner_admission.recipe[0]["params"]["with_std"],
        false
    );
    let invalid = json!({"schema_version":1,"definition":{"cv":5,"pipeline":recipe["pipeline"]},"dataset":{}});
    let error = nirs4all::portable_execution::run(
        &invalid,
        std::path::Path::new("/missing/native/library"),
    )
    .unwrap_err();
    assert!(
        error.contains("$.cv"),
        "unsupported intent must precede any native load: {error}"
    );
    let invalid_id = json!({"schema_version":1,"definition":[{"model":{"class":"n4m.PLS","params":{"n_components":1}}}],
        "dataset":{"X":[[1.0],[2.0]],"y":[1.0,2.0],"sample_ids":["external:040","external_039"]}});
    let error = nirs4all::portable_execution::run(
        &invalid_id,
        std::path::Path::new("/missing/native/library"),
    )
    .unwrap_err();
    assert!(
        error.contains("sample id 'external:040' is not a dag-ml identifier"),
        "IO must refuse the verbatim unsupported declared ID before native load: {error}"
    );
}

#[test]
fn native_bank_keeps_frozen_ks_order_and_declared_identities() {
    let library = library();
    let oracle = oracle();
    let recipe: Value = serde_json::from_str(include_str!(
        "parity/fixtures/portable_kennard_stone_snv_pls.json"
    ))
    .unwrap();
    let expected = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == recipe["name"])
        .unwrap();
    let mut data = oracle["dataset"].clone();
    let rows = data["rows"].as_u64().unwrap() as usize;
    let ids = (0..rows)
        .map(|i| format!("external_{:03}", rows - i))
        .collect::<Vec<_>>();
    data["sample_ids"] = json!(ids);
    data["axis"] = json!({"kind":"wavelength","unit":"nm","provenance":"declared test source"});
    data["metadata"] = json!((0..rows)
        .map(|row| {
            if row % 2 == 0 {
                json!({"row":row,"groups":["opaque"]})
            } else {
                json!([])
            }
        })
        .collect::<Vec<_>>());
    data["provenance"] = json!((0..rows)
        .map(|row| {
            if row % 2 == 0 {
                json!({})
            } else {
                json!([{"row":row},["source",null]])
            }
        })
        .collect::<Vec<_>>());
    let result = nirs4all::portable_execution::run(
        &json!({"schema_version":1,"definition":recipe,"dataset":data}),
        &library,
    )
    .unwrap();
    assert_eq!(
        result["owner"]["profile"],
        "single_resampled_holdout_bank.v1"
    );
    assert_eq!(result["mode"], "single_holdout");
    assert_eq!(result["split"], expected["split"]);
    assert_eq!(result["identities"]["kind"], "declared");
    assert_eq!(result["identities"]["sample_ids"], json!(ids));
    assert_eq!(result["identities"]["metadata"], data["metadata"]);
    assert_eq!(result["identities"]["provenance"], data["provenance"]);
    assert_eq!(
        result["evaluation"],
        json!({"scope":"selection_validation","independent_test":false})
    );
    let train = result["split"]["trainIndices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| json!(ids[i.as_u64().unwrap() as usize]))
        .collect::<Vec<_>>();
    assert_eq!(result["identities"]["training_sample_ids"], json!(train));
    assert_eq!(
        result["selected"]["n_components"],
        expected["selected"]["n_components"]
    );
    let tolerance = oracle["metadata"]["tolerances"]["rmse_abs"]
        .as_f64()
        .unwrap();
    for (actual, expected) in result["variants"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected["variants"].as_array().unwrap())
    {
        assert!(
            (actual["rmse"].as_f64().unwrap() - expected["rmse"].as_f64().unwrap()).abs()
                <= tolerance
        );
    }
}

#[test]
fn training_scope_and_explicit_export_population_survive_session_replay() {
    let library = library();
    let oracle = oracle();
    let dataset = PortableDataset::from_json_value(&oracle["dataset"]).unwrap();
    let recipe = json!({"pipeline":[{"class":"n4m.SNV","params":{"with_std":false}},
        {"model":{"class":"n4m.PLS","params":{"n_components":2,"scale":false}}}]});
    let evaluated = nirs4all::portable_execution::run(
        &json!({"schema_version":1,"definition":recipe,"dataset":oracle["dataset"]}),
        &library,
    )
    .unwrap();
    assert_eq!(evaluated["owner"]["profile"], "training_bank.v1");
    assert_eq!(
        evaluated["evaluation"],
        json!({"scope":"training","independent_test":false})
    );
    assert!(
        evaluated["owner"]["evidence"]["selected_package"]["effective_plan"]["fold_set"].is_null()
    );
    assert_eq!(evaluated["identities"]["kind"], "positional");
    assert_eq!(
        evaluated["identities"]["sample_ids"],
        json!((0..dataset.rows)
            .map(|i| format!("position_{i}"))
            .collect::<Vec<_>>())
    );
    assert!(evaluated["identities"].get("metadata").is_none());
    assert!(evaluated["identities"].get("provenance").is_none());
    let (result, bytes) =
        run_portable_pipeline_with_exported_model(&recipe.to_string(), &dataset, &library).unwrap();
    let predicted = predict_exported_portable_model_with_library(
        &recipe.to_string(),
        &bytes,
        &dataset.x,
        dataset.rows,
        dataset.cols,
        &library,
    )
    .unwrap();
    for (a, b) in predicted.iter().zip(&result.selected.predictions) {
        assert!((a - b).abs() < 1e-10);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let session =
        PortableSession::run_with_library(&recipe.to_string(), &dataset, &library).unwrap();
    session.save(&path).unwrap();
    let loaded = PortableSession::load_with_library(&path, &library).unwrap();
    assert_eq!(
        loaded
            .predict(&dataset.x, dataset.rows, dataset.cols)
            .unwrap(),
        predicted
    );

    let ks = include_str!("parity/fixtures/portable_kennard_stone_snv_pls.json");
    let (result, bytes) =
        run_portable_pipeline_with_exported_model(ks, &dataset, &library).unwrap();
    let descriptor = nirs4all::dag_ml::inspect_methods_native_predictor_descriptor_v1(
        &nirs4all::dag_ml::ControllerId::new("controller:methods.pls").unwrap(),
        &bytes,
    )
    .unwrap();
    assert_eq!(
        descriptor.dimensions.training_samples as usize,
        result.split.train_indices.len()
    );
}

#[test]
fn native_generic_regressors_keep_training_scope_and_component_identity() {
    let library = library();
    let data = oracle()["dataset"].clone();
    for (model, count, method) in [
        (
            json!({"class":"n4m.Ridge","params":{"alpha":0.5}}),
            Value::Null,
            "models.regularized.ridge",
        ),
        (
            json!({"class":"n4m.PLS","params":{"algo":"pcr_svd","n_components":2,"center_x":true,"scale_x":false,"center_y":true,"scale_y":false}}),
            json!(2),
            "models.pls.pcr",
        ),
        (
            json!({"class":"n4m.PLS","params":{"algo":"opls_nipals","n_components":2,"center_x":true,"scale_x":false,"center_y":true,"scale_y":false}}),
            json!(2),
            "models.pls.opls",
        ),
    ] {
        let result = nirs4all::portable_execution::run(
            &json!({"schema_version":1,"definition":{"pipeline":[{"class":"n4m.MSC"},{"model":model}]},"dataset":data}),
            &library,
        ).unwrap();
        assert_eq!(result["owner"]["profile"], "training_bank.v1");
        assert_eq!(result["evaluation"]["scope"], "training");
        assert_eq!(result["selected"]["n_components"], count);
        assert_eq!(
            result["owner"]["evidence"]["selected"]["recipe"][1]["methodId"],
            method
        );
        assert_eq!(
            result["selected"]["predictions"].as_array().unwrap().len(),
            data["rows"].as_u64().unwrap() as usize
        );
        assert!(
            !result["owner"]["evidence"]["selected_package"]["raw_artifact_payloads"]
                .as_object()
                .unwrap()
                .is_empty()
        );
    }
}
