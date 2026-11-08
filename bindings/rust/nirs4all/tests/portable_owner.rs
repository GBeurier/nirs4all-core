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

/// Three classes whose ids follow an unsorted host level order (2, 0, 1);
/// the owner only ever sees these integral ids.
fn classification_dataset() -> Value {
    let rows = 30;
    let order = [2, 0, 1];
    let x = (0..rows)
        .map(|r| {
            let class = order[r % 3];
            (0..8)
                .map(|c| {
                    1.0 + 0.05 * c as f64
                        + if c == class * 2 + 1 { 0.6 } else { 0.0 }
                        + 0.01 * ((r * 7 + c * 3) % 11) as f64
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let y = (0..rows).map(|r| order[r % 3]).collect::<Vec<_>>();
    json!({"X":x,"y":y})
}

fn assert_owner_classification(result: &Value, data: &Value, scope: &str) {
    assert_eq!(result["profile"], "portable_linear_classification.v1");
    assert_eq!(result["evaluation"]["scope"], scope);
    assert_eq!(result["evaluation"]["independent_test"], false);
    let y = data["y"].as_array().unwrap();
    let pick = |key: &str| {
        result["split"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| y[i.as_u64().unwrap() as usize].as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    let (fit, evaluation) = (pick("trainIndices"), pick("testIndices"));
    assert_eq!(result["targets"], json!(evaluation));
    let fit_classes = fit.iter().collect::<std::collections::BTreeSet<_>>();
    let mut best: Option<(f64, &Value)> = None;
    for variant in result["variants"].as_array().unwrap() {
        let predictions = variant["predictions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().expect("class ids are JSON integers"))
            .collect::<Vec<_>>();
        assert_eq!(predictions.len(), evaluation.len());
        assert!(predictions.iter().all(|p| fit_classes.contains(p)));
        let correct = predictions
            .iter()
            .zip(&evaluation)
            .filter(|(p, t)| p == t)
            .count();
        let accuracy = variant["accuracy"].as_f64().unwrap();
        assert_eq!(accuracy, correct as f64 / evaluation.len() as f64);
        if best.is_none_or(|(score, _)| accuracy > score) {
            best = Some((accuracy, variant));
        }
    }
    // Fixed owner rule: maximum accuracy, first ordinal on ties.
    assert_eq!(&result["selected"], best.unwrap().1);
    // These recipes have no sample filter, so the fitted domain equals the
    // classes of the fit rows and the owner report is checkable from the split.
    let mut unseen = std::collections::BTreeMap::<i64, usize>::new();
    for class in evaluation
        .iter()
        .filter(|class| !fit_classes.contains(class))
    {
        *unseen.entry(*class).or_default() += 1;
    }
    let reported = result["unseen_validation_classes"].as_array().unwrap();
    assert_eq!(
        reported
            .iter()
            .map(|c| (
                c["class_id"].as_i64().unwrap(),
                c["count"].as_u64().unwrap() as usize
            ))
            .collect::<Vec<_>>(),
        unseen.into_iter().collect::<Vec<_>>()
    );
}

#[test]
fn native_sparse_pls_da_banks_select_accuracy_over_owner_ids() {
    let library = library();
    let data = classification_dataset();
    let ks = json!({"name":"cls","pipeline":[{"class":"n4m.KennardStone","params":{"test_size":0.3}},
        {"class":"n4m.SNV"},{"model":{"class":"n4m.SparsePLSDA"},"param":"n_components","_range_":[1,3,1]}]});
    let result = nirs4all::portable_execution::run(
        &json!({"schema_version":1,"definition":ks,"dataset":data}),
        &library,
    )
    .unwrap();
    assert_eq!(result["mode"], "single_holdout");
    assert_eq!(
        result["owner"]["profile"],
        "single_resampled_holdout_bank.v2"
    );
    assert_eq!(
        result["owner"]["decision"]["policy_id"],
        "select:single_resampled_holdout_bank.v2:accuracy"
    );
    assert_eq!(result["owner"]["decision"]["objective"], "maximize");
    assert_eq!(
        result["unseen_validation_classes"],
        result["owner"]["evidence"]["unseen_validation_classes"]
    );
    assert_owner_classification(&result, &data, "selection_validation");

    let training = json!({"pipeline":[{"class":"n4m.SNV"},
        {"model":{"class":"n4m.SparsePLSDA","params":{"sparsity_lambda":0.1}},"param":"n_components","_range_":[1,2,1]}]});
    let result = nirs4all::portable_execution::run(
        &json!({"schema_version":1,"definition":training,"dataset":data}),
        &library,
    )
    .unwrap();
    assert_eq!(result["owner"]["profile"], "training_bank.v2");
    assert_eq!(result["split"]["kind"], "all");
    assert_eq!(result["unseen_validation_classes"], json!([]));
    assert_owner_classification(&result, &data, "training");
}

#[test]
fn native_sparse_pls_da_width_is_refused_by_methods_on_the_terminal_input() {
    // Requires the Methods artifact carrying the SparsePlsDaClassifier
    // fit-time bound (separate Methods author); the owner adds no width check.
    let library = library();
    let refused = |definition: Value| {
        nirs4all::portable_execution::run(
            &json!({"schema_version":1,"definition":definition,"dataset":classification_dataset()}),
            &library,
        )
        .unwrap_err()
    };
    // Raw width 8: the ninth candidate exceeds min(n_fit - 1, 8).
    let error = refused(
        json!({"pipeline":[{"class":"n4m.KennardStone","params":{"test_size":0.3}},
        {"model":{"class":"n4m.SparsePLSDA"},"param":"n_components","_range_":[1,9,1]}]}),
    );
    assert!(
        error.contains("portable candidate chain refused"),
        "{error}"
    );
    assert!(error.contains("n_components k=9"), "{error}");
    assert!(error.contains("p_transformed=8"), "{error}");
    // A selector keeps 4 of 8 columns: 5 components fit the raw width but not
    // the transformed terminal input, and are refused at the classifier fit.
    let selected = |n: i64| {
        json!({"pipeline":[{"class":"n4m.Selector","params":{"method":"spa_select","n_components":4,"method_params":{"top_k":4}}},
            {"model":{"class":"n4m.SparsePLSDA","params":{"n_components":n}}}]})
    };
    let error = refused(selected(5));
    assert!(error.contains("n_components k=5"), "{error}");
    assert!(error.contains("p_transformed=4"), "{error}");
    let result = nirs4all::portable_execution::run(
        &json!({"schema_version":1,"definition":selected(4),"dataset":classification_dataset()}),
        &library,
    )
    .unwrap();
    assert_eq!(result["owner"]["profile"], "training_bank.v2");
}

#[test]
fn nofit_plan_keeps_non_alphabetical_object_branch_insertion_order() {
    let request: Value = serde_json::from_str(r#"{"schema_version":1,"definition":[
        {"branch":{"z_msc":[{"class":"n4m.MSC"}],"a_snv":[{"class":"n4m.SNV","params":{"with_std":false}}]}},
        {"merge":"features"},{"model":{"class":"n4m.PLS","params":{"scale":false}}}]}"#).unwrap();
    let plan = nirs4all::portable_execution::plan(&request).unwrap();
    assert_eq!(plan["profile"], "portable_feature_regression.v1");
    assert_eq!(plan["owner_profile"], "feature_graph.v1");
    assert_eq!(plan["recipe"][0]["branch"][0]["id"], "z_msc");
    assert_eq!(plan["recipe"][0]["branch"][1]["id"], "a_snv");
    assert_eq!(
        plan["recipe"][0]["branch"][1]["steps"][0]["params"]["with_std"],
        false
    );
    assert_eq!(plan["candidates"][0]["scale_x"], false);
    assert_eq!(
        plan["evaluation"],
        json!({"scope":"training","independent_test":false})
    );
    assert!(plan.get("feature_blocks").is_none());
    assert!(
        parse_execution_plan_str(&request["definition"].to_string()).is_err(),
        "fitted compact/export/session surface must refuse this graph profile"
    );
}

/// Row-major rows of the flat oracle matrix, in the given order.
fn pick_rows(data: &Value, indexes: &[usize]) -> Vec<f64> {
    let cols = data["cols"].as_u64().unwrap() as usize;
    let x = data["X"].as_array().unwrap();
    indexes
        .iter()
        .flat_map(|row| (0..cols).map(move |col| x[row * cols + col].as_f64().unwrap()))
        .collect()
}

fn indexes(value: &Value) -> Vec<usize> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_u64().unwrap() as usize)
        .collect()
}

fn params(
    context: &n4m::Context,
    method: &str,
    named: &[(&str, n4m::roles::ParamValue)],
) -> n4m::roles::Params {
    let mut params = n4m::roles::Params::new(context, method).unwrap();
    for (name, value) in named {
        params.set(name, value).unwrap();
    }
    params
}

/// One direct Methods augmenter procedure run on row-major fit rows.
fn augment(
    context: &n4m::Context,
    method: &str,
    named: &[(&str, n4m::roles::ParamValue)],
    x: &[f64],
    cols: usize,
) -> Vec<f64> {
    let params = params(context, method, named);
    let x = n4m::MatrixRef::row_major(x, x.len() / cols, cols).unwrap();
    n4m::roles::run_procedure(
        context,
        method,
        Some(&params),
        &n4m::roles::FitInputs::new(x),
    )
    .unwrap()
    .double_matrix("X")
    .unwrap()
    .data
}

/// Fit SNV then the model on `fit_x`, predict `eval_x` through the same fitted
/// SNV state. `blocks` / `groups` attach per-column fit inputs to the model
/// fit only.
#[allow(clippy::too_many_arguments)]
fn snv_then_model(
    context: &n4m::Context,
    fit_x: &[f64],
    fit_y: &[f64],
    eval_x: &[f64],
    cols: usize,
    model: &str,
    named: &[(&str, n4m::roles::ParamValue)],
    blocks: Option<&[i64]>,
    groups: Option<&[i64]>,
) -> Vec<f64> {
    use n4m::roles::{Estimator, FitInputs};
    fn view(x: &[f64], cols: usize) -> n4m::MatrixRef<'_> {
        n4m::MatrixRef::row_major(x, x.len() / cols, cols).unwrap()
    }
    let mut snv = Estimator::new(context, "preprocessing.scatter.snv", None).unwrap();
    snv.fit(context, &FitInputs::new(view(fit_x, cols)))
        .unwrap();
    let fit = snv.transform(context, view(fit_x, cols)).unwrap();
    let eval = snv.transform(context, view(eval_x, cols)).unwrap();
    let params = params(context, model, named);
    let mut estimator = Estimator::new(context, model, Some(&params)).unwrap();
    let y = n4m::MatrixRef::row_major(fit_y, fit_y.len(), 1).unwrap();
    let mut inputs = FitInputs::new(fit.as_ref()).y(y);
    if let Some(blocks) = blocks {
        inputs = inputs.blocks(blocks);
    }
    if let Some(groups) = groups {
        inputs = inputs.feature_groups(groups);
    }
    estimator.fit(context, &inputs).unwrap();
    estimator.predict(context, eval.as_ref()).unwrap().data
}

fn assert_close(actual: &Value, expected: &[f64], label: &str) {
    let actual = actual
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), expected.len(), "{label}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() <= 1e-10, "{label}: {a} vs {e}");
    }
}

fn pls(k: i64) -> Vec<(&'static str, n4m::roles::ParamValue)> {
    use n4m::roles::ParamValue::{Bool, Enum, Int};
    vec![
        ("n_components", Int(k)),
        ("solver", Enum("simpls".into())),
        ("center_x", Bool(true)),
        ("scale_x", Bool(false)),
        ("center_y", Bool(true)),
        ("scale_y", Bool(false)),
    ]
}

#[test]
fn native_train_augmentation_fits_fit_rows_only_and_scores_raw_rows_in_both_modes() {
    use n4m::roles::ParamValue::{Double, Int};
    let library = library();
    let data = oracle()["dataset"].clone();
    let cols = data["cols"].as_u64().unwrap() as usize;
    let y = data["y"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    let declared = |kind: &str, values: Value, seed: Value| {
        json!({"train_augmentation":{"class":"n4m.NativeXAugmentation",
            "params":{"kind":kind,"values":values,"seed":seed}}})
    };
    // R wire doubles; band_perturb mixes int and double positions.
    let noise = declared("gaussian_noise", json!([0.02]), json!(7.0));
    let band = declared(
        "band_perturb",
        json!([2.0, 3, 6, 0.9, 1.1, -0.01, 0.01]),
        json!(42.0),
    );
    for holdout in [true, false] {
        let mut pipeline = Vec::new();
        if holdout {
            pipeline.push(json!({"class":"n4m.KennardStone","params":{"test_size":0.3}}));
        }
        pipeline.extend([
            noise.clone(),
            band.clone(),
            json!({"class":"n4m.SNV"}),
            json!({"model":{"class":"n4m.PLS","params":{"scale":false}},"param":"n_components","_range_":[1,3,1]}),
        ]);
        let result = nirs4all::portable_execution::run(
            &json!({"schema_version":1,"definition":{"pipeline":pipeline},"dataset":data}),
            &library,
        )
        .unwrap();
        assert_eq!(result["profile"], "portable_feature_regression.v1");
        assert_eq!(
            result["owner"]["profile"],
            if holdout {
                "single_resampled_holdout_graph_bank.v1"
            } else {
                "training_graph_bank.v1"
            }
        );
        // The canonical identity keeps the declared, unresolved entries.
        let recipe = &result["owner"]["evidence"]["variants"][0]["identity"]["recipe"];
        assert_eq!(
            recipe[0],
            declared("gaussian_noise", json!([0.02]), json!(7))
        );
        assert_eq!(
            recipe[1],
            declared(
                "band_perturb",
                json!([2.0, 3, 6, 0.9, 1.1, -0.01, 0.01]),
                json!(42)
            )
        );
        assert_eq!(result["preprocessing"][1], recipe[1]);
        // Direct reference on the fit rows in their split order: procedures in
        // declared order with their own recipe seeds, then SNV and PLS fitted
        // on the augmented rows and evaluated on raw rows.
        let train = indexes(&result["split"]["trainIndices"]);
        let test = indexes(&result["split"]["testIndices"]);
        let fit_x = pick_rows(&data, &train);
        let fit_y = train.iter().map(|i| y[*i]).collect::<Vec<_>>();
        let eval_x = pick_rows(&data, &test);
        let context = n4m::Context::new().unwrap();
        let noisy = augment(
            &context,
            "augmentation.noise.gaussian_noise",
            &[("sigma", Double(0.02)), ("seed", Int(7))],
            &fit_x,
            cols,
        );
        let augmented = augment(
            &context,
            "augmentation.spectral.band_perturb",
            &[
                ("n_bands", Int(2)),
                ("bw_lo", Int(3)),
                ("bw_hi", Int(6)),
                ("gain_lo", Double(0.9)),
                ("gain_hi", Double(1.1)),
                ("offset_lo", Double(-0.01)),
                ("offset_hi", Double(0.01)),
                ("seed", Int(42)),
            ],
            &noisy,
            cols,
        );
        assert_ne!(augmented, fit_x, "augmentation must act on the fit rows");
        for (variant, k) in result["variants"].as_array().unwrap().iter().zip(1..) {
            assert_eq!(variant["n_components"], k);
            let expected = snv_then_model(
                &context,
                &augmented,
                &fit_y,
                &eval_x,
                cols,
                "models.pls.pls_regression",
                &pls(k),
                None,
                None,
            );
            assert_close(
                &variant["predictions"],
                &expected,
                &format!("holdout={holdout} k={k}: fit on augmented rows, evaluate raw rows"),
            );
        }
    }
}

#[test]
fn native_terminal_fit_inputs_reach_mbpls_and_group_sparse_and_methods_owns_width() {
    use n4m::roles::ParamValue::{Double, Int};
    let library = library();
    let data = oracle()["dataset"].clone();
    let cols = data["cols"].as_u64().unwrap() as usize;
    assert_eq!(cols, 28);
    let y = data["y"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    let run = |pipeline: Value| {
        nirs4all::portable_execution::run(
            &json!({"schema_version":1,"definition":{"pipeline":pipeline},"dataset":data}),
            &library,
        )
    };
    // Training mode MBPLS with two declared blocks over the SNV output.
    let blocks = [10_i64, 18];
    let result = run(json!([{"class":"n4m.SNV"},
        {"model":{"class":"n4m.MBPLS","params":{"block_sizes":[10.0, 18],"n_components":2}}}]))
    .unwrap();
    assert_eq!(result["owner"]["profile"], "training_graph_bank.v1");
    assert_eq!(
        result["owner"]["evidence"]["variants"][0]["identity"]["recipe"][1]["params"]
            ["fit_inputs_v1"],
        json!({"blocks":[10, 18]})
    );
    let all = (0..data["rows"].as_u64().unwrap() as usize).collect::<Vec<_>>();
    let x = pick_rows(&data, &all);
    let context = n4m::Context::new().unwrap();
    let expected = snv_then_model(
        &context,
        &x,
        &y,
        &x,
        cols,
        "models.multiblock.mb_pls",
        &[("n_components", Int(2))],
        Some(&blocks),
        None,
    );
    assert_close(
        &result["selected"]["predictions"],
        &expected,
        "MBPLS blocks",
    );
    // Kennard-Stone GroupSparsePLS with four feature groups of seven columns.
    let groups = (0..28).map(|c| c / 7).collect::<Vec<i64>>();
    let result = run(json!([{"class":"n4m.KennardStone","params":{"test_size":0.3}},{"class":"n4m.SNV"},
        {"model":{"class":"n4m.GroupSparsePLS","params":{"group_assignment":groups,"group_lambda":0.2,"n_components":2}}}]))
    .unwrap();
    assert_eq!(
        result["owner"]["profile"],
        "single_resampled_holdout_graph_bank.v1"
    );
    let train = indexes(&result["split"]["trainIndices"]);
    let test = indexes(&result["split"]["testIndices"]);
    let expected = snv_then_model(
        &context,
        &pick_rows(&data, &train),
        &train.iter().map(|i| y[*i]).collect::<Vec<_>>(),
        &pick_rows(&data, &test),
        cols,
        "models.sparse.group_sparse_pls",
        &[("n_components", Int(2)), ("group_lambda", Double(0.2))],
        None,
        Some(&groups),
    );
    assert_close(
        &result["selected"]["predictions"],
        &expected,
        "GroupSparsePLS feature groups",
    );
    // Methods alone checks the actual terminal width: SPA keeps 6 columns.
    let selected = |blocks: Value| {
        json!([{"class":"n4m.KennardStone","params":{"test_size":0.3}},
            {"class":"n4m.Selector","params":{"method":"spa_select","n_components":6,"method_params":{"top_k":6}}},
            {"model":{"class":"n4m.MBPLS","params":{"block_sizes":blocks,"n_components":2}}}])
    };
    assert!(run(selected(json!([3, 3]))).is_ok());
    for (pipeline, name) in [
        (selected(json!([10, 18])), "blocks"),
        (
            json!([{"class":"n4m.KennardStone","params":{"test_size":0.3}},
                {"model":{"class":"n4m.GroupSparsePLS","params":{"group_assignment":[0, 1]}}}]),
            "feature_groups",
        ),
    ] {
        let error = run(pipeline).unwrap_err();
        assert!(
            error.contains("portable candidate chain refused"),
            "{error}"
        );
        assert!(error.contains("incompatible length"), "{error}");
        assert!(error.contains(name), "{error}");
    }
}
