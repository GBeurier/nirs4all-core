use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use nirs4all::dag_ml::RunId;
use nirs4all::{
    dense_regression_training_request, dense_regression_workflow_config, load_archive_v2,
    load_spectrum_dataset_package, predict_methods_archive_v2_matrix,
    preflight_workflow_archive_path, run_dense_regression_workflow, DenseRegressionPreprocessing,
    DenseRegressionWorkflowRequest, MethodsArchiveMatrixPredictRequest,
};
use sha2::{Digest, Sha256};

#[test]
fn native_candidates_select_oof_refit_and_relocated_archive_replays() {
    let library =
        PathBuf::from(std::env::var_os("N4M_LIBRARY_PATH").expect("exact Methods library"))
            .canonicalize()
            .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data.csv");
    let mut csv = "sample_id,y,1100,1150,1200,1250,1300,1350,1400\n".to_owned();
    for row in 0..12 {
        let x: Vec<f64> = (0..7)
            .map(|col| {
                (((row + 1) * (col + 1)) as f64 * 0.13).sin() + row as f64 * 0.11 + col as f64 * 0.2
            })
            .collect();
        let y = 2.0 * x[0] - x[2] + row as f64 * 0.13;
        csv.push_str(&format!(
            "S{row},{y},{}\n",
            x.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    fs::write(&data, csv).unwrap();
    let loaded = load_spectrum_dataset_package(&data).unwrap();
    let source_id = loaded.source_id.clone();
    let request = dense_regression_training_request(
        &loaded.package,
        &loaded.source_id,
        &[1, 2],
        DenseRegressionPreprocessing::SnvSavitzkyGolay,
    )
    .unwrap();
    assert_eq!(request.campaign.generation.dimensions[0].choices.len(), 2);
    assert_eq!(
        request.campaign.metadata["input_sample_ids"],
        serde_json::json!((0..12).map(|row| format!("S{row}")).collect::<Vec<_>>())
    );
    assert_eq!(
        request
            .campaign
            .split_invocation
            .as_ref()
            .unwrap()
            .fold_set
            .as_ref()
            .unwrap()
            .folds
            .len(),
        2
    );
    let archive_path = directory.path().join("model.n4a");
    let outcome = run_dense_regression_workflow(DenseRegressionWorkflowRequest {
        dataset: &loaded.package,
        source_id: &loaded.source_id,
        components: &[1, 2],
        preprocessing: DenseRegressionPreprocessing::SnvSavitzkyGolay,
        methods_library_path: &library,
        archive_path: &archive_path,
        run_id: "run:workflow:rust",
    })
    .unwrap();
    assert!(
        run_dense_regression_workflow(DenseRegressionWorkflowRequest {
            dataset: &loaded.package,
            source_id: &loaded.source_id,
            components: &[1, 2],
            preprocessing: DenseRegressionPreprocessing::SnvSavitzkyGolay,
            methods_library_path: &directory.path().join("missing-native-library"),
            archive_path: &archive_path,
            run_id: "run:workflow:collision",
        })
        .err()
        .unwrap()
        .contains("archive already exists")
    );
    assert_eq!(outcome.training.variant_oof_averages.len(), 1);
    assert_eq!(outcome.training.effective_plan.variants.len(), 2);
    assert_eq!(
        outcome.training.oof_averages[0].predictions.unit_ids.len(),
        12
    );
    assert_eq!(outcome.training.execution_bundle.refit_artifacts.len(), 1);
    assert_eq!(outcome.training.execution_bundle.selections.len(), 1);
    assert!(outcome.training.execution_bundle.refit_artifacts[0]
        .artifact
        .native_predictor_descriptor
        .as_ref()
        .unwrap()
        .pipeline
        .is_some());
    let relocated = directory.path().join("relocated.n4a");
    fs::rename(&archive_path, &relocated).unwrap();
    fs::remove_file(data).unwrap();
    drop(loaded);
    let archive = load_archive_v2(&relocated).unwrap();
    let package: nirs4all::dag_ml::PortablePredictorPackage =
        serde_json::from_slice(archive.portable_predictor_package().unwrap()).unwrap();
    assert_eq!(
        dense_regression_workflow_config(&package).unwrap(),
        serde_json::json!({
            "source_id": source_id, "components": [1, 2], "preprocessing": "snv_savgol"
        })
    );
    // Isolate the product recipe guard from native fingerprint validation:
    // these mutations do not claim to be valid signed archives.
    let value = serde_json::to_value(&package).unwrap();
    for (pointer, replacement) in [
        ("/template/campaign/root_seed", serde_json::json!(92)),
        ("/execution_bundle/selections/selection:rmse/metric_name", serde_json::json!("mae")),
        ("/execution_bundle/selections/selection:rmse/evaluation_scope", serde_json::json!("train")),
        ("/template/campaign/aggregation_policy/method", serde_json::json!("weighted_mean")),
        ("/template/campaign/generation/strategy", serde_json::json!("none")),
        ("/template/campaign/generation/dimensions/0/choices/0/value", serde_json::json!("1")),
        ("/template/campaign/generation/dimensions/0/choices/1/value", serde_json::json!(1)),
        ("/template/campaign/generation/dimensions/0/choices/0/param_overrides/0/params/n_components", serde_json::json!(3)),
        ("/template/campaign/split_invocation/params/kind", serde_json::json!("random")),
        ("/template/campaign/split_invocation/fold_set/folds/0/train_sample_ids", serde_json::json!([])),
        ("/output_bindings/0/target_names", serde_json::json!(["y", "another"])),
    ] {
        let mut altered = value.clone();
        *altered.pointer_mut(pointer).unwrap() = replacement;
        let altered = serde_json::from_value(altered).unwrap();
        assert!(dense_regression_workflow_config(&altered).is_err(), "profile guard missed {pointer}");
    }
    for (field, replacement) in [
        ("requested_rank", serde_json::json!(1)),
        ("reduction_id", serde_json::json!("reduction:unsupported")),
        (
            "refit_slot_plan",
            serde_json::json!({
                "strategy":"refit_one", "selection_level":"sample", "member_count":1,
                "selection_metric":{"name":"rmse", "objective":"minimize"}
            }),
        ),
    ] {
        let mut altered = value.clone();
        altered["execution_bundle"]["selections"]["selection:rmse"][field] = replacement;
        let altered: nirs4all::dag_ml::PortablePredictorPackage =
            serde_json::from_value(altered).unwrap();
        // These are meaningful, natively valid SelectionDecision options, not
        // malformed JSON. The bounded winner-only product recipe refuses them.
        altered.execution_bundle.selections["selection:rmse"]
            .validate()
            .unwrap();
        assert!(
            dense_regression_workflow_config(&altered).is_err(),
            "profile guard missed selection {field}"
        );
    }
    let predicted = predict_methods_archive_v2_matrix(
        &archive,
        MethodsArchiveMatrixPredictRequest {
            sample_ids: vec!["fresh:1".into()],
            x: vec![vec![1.0, 1.8, 3.2, 4.1, 5.3, 6.0, 7.0]],
            expected_target_names: vec!["y".into()],
            methods_library_path: library.clone(),
            methods_library_sha256: format!("{:x}", Sha256::digest(fs::read(library).unwrap())),
            request_id: "request:workflow:fresh".into(),
            outcome_id: "outcome:workflow:fresh".into(),
            run_id: RunId::new("run:workflow:fresh").unwrap(),
            warnings: vec![],
            diagnostics: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(predicted.outputs[0].predictions[0].sample_ids.len(), 1);
    assert!(predicted.outputs[0].predictions[0].values[0][0].is_finite());
}

#[test]
fn archive_destination_guard_refuses_collisions_and_missing_parents() {
    let directory = tempfile::tempdir().unwrap();
    let new_archive = directory.path().join("new.n4a");
    preflight_workflow_archive_path(&new_archive).unwrap();
    fs::write(&new_archive, b"existing user file").unwrap();
    assert!(preflight_workflow_archive_path(&new_archive)
        .unwrap_err()
        .contains("already exists"));
    assert_eq!(fs::read(&new_archive).unwrap(), b"existing user file");
    assert!(
        preflight_workflow_archive_path(&directory.path().join("missing/model.n4a"))
            .unwrap_err()
            .contains("parent must be an existing directory")
    );
    assert!(preflight_workflow_archive_path(&new_archive.join("model.n4a")).is_err());
    #[cfg(unix)]
    {
        let dangling = directory.path().join("dangling.n4a");
        std::os::unix::fs::symlink("missing-target", &dangling).unwrap();
        assert!(preflight_workflow_archive_path(&dangling)
            .unwrap_err()
            .contains("already exists"));
    }
}
