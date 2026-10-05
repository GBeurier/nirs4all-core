use nirs4all::results::{open_experiment_results, save_experiment_results};

#[path = "../src/test_fixture.rs"]
mod test_fixture;

fn result_fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let root = tempfile::tempdir().unwrap();
    let library = std::path::PathBuf::from(
        std::env::var_os("N4M_LIBRARY_PATH").expect("exact Methods library"),
    )
    .canonicalize()
    .unwrap();
    let dataset = nirs4all_io_crate::core::public_dataset::dense_dataset_package(
        &test_fixture::dense_training_record(),
        "spectra",
    )
    .unwrap();
    let archive = root.path().join("model.n4a");
    let outcome =
        nirs4all::run_dense_regression_workflow(nirs4all::DenseRegressionWorkflowRequest {
            dataset: &dataset,
            source_id: "spectra",
            components: &[1, 2],
            preprocessing: nirs4all::DenseRegressionPreprocessing::SnvSavitzkyGolay,
            methods_library_path: &library,
            archive_path: &archive,
            run_id: "run:results:qualification",
        })
        .unwrap();
    let native = root.path().join("native");
    nirs4all::results::write_training_results(&outcome.training, &dataset, "spectra", &native)
        .unwrap();
    let experiment = root.path().join("experiment");
    save_experiment_results(&native, &experiment, Some(&archive)).unwrap();
    (root, native, archive, experiment)
}

#[test]
fn native_experiment_cold_queries_and_copy_preserve_result() {
    let (_fixture_root, _native, _archive, fixture) = result_fixture();
    eprintln!("native results fixture={}", fixture.display());
    let original = open_experiment_results(&fixture).expect("native experiment opens");
    let scores = original.compare(None, None).expect("score reports");
    assert!(!scores.is_empty());
    let rows = original.predictions(None, None, None).expect("predictions");
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| row
        .sample_ids
        .as_ref()
        .is_some_and(|ids| ids.len() == row.sample_indices.len())));
    assert!(original.compare(Some("absent"), None).is_err());
    assert!(original.predictions(Some("absent"), None, None).is_err());
    let root = tempfile::tempdir().unwrap();
    let copied = save_experiment_results(
        std::path::Path::new(&fixture).join("results"),
        root.path().join("copy"),
        original.model_archive.as_deref(),
    )
    .expect("native save");
    assert_eq!(original.native, copied.native);
    assert_eq!(original.run_id, copied.run_id);
    assert_eq!(original.winner_variant_id, copied.winner_variant_id);
    std::fs::write(copied.path.join("results/score_set.json"), b"{}").unwrap();
    assert!(open_experiment_results(copied.path).is_err());
}

#[test]
fn native_model_closure_rejects_another_training_fingerprint() {
    let (_fixture_root, native, archive, _experiment) = result_fixture();
    let root = tempfile::tempdir().unwrap();
    let saved = save_experiment_results(
        &native,
        root.path().join("model-experiment"),
        Some(std::path::Path::new(&archive)),
    )
    .expect("native model closes over native scores");
    assert!(saved.model_archive.is_some());
    let altered = root.path().join("altered");
    std::fs::create_dir(&altered).unwrap();
    for name in ["manifest.json", "score_set.json", "predictions.parquet"] {
        std::fs::copy(std::path::Path::new(&native).join(name), altered.join(name)).unwrap();
    }
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(altered.join("manifest.json")).unwrap()).unwrap();
    manifest["training_outcome_fingerprint"] = serde_json::Value::String("f".repeat(64));
    std::fs::write(
        altered.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let rejected = root.path().join("rejected");
    let error =
        match save_experiment_results(altered, &rejected, Some(std::path::Path::new(&archive))) {
            Err(error) => error,
            Ok(_) => panic!("mismatched model closure accepted"),
        };
    assert!(error.contains("does not close"), "{error}");
    assert!(
        !rejected.exists(),
        "failed model closure must not publish experiment"
    );
}

#[test]
fn model_closure_rejects_rewritten_native_prediction_arrays_and_identities() {
    use dag_ml_results::{read_native_results, write_native_results, NativePredictionRow};
    use serde_json::Value;
    let (_fixture_root, source, archive, _experiment) = result_fixture();
    let original = read_native_results(&source).unwrap();
    let root = tempfile::tempdir().unwrap();
    for field in [
        "y_pred",
        "y_true",
        "sample_ids",
        "sample_indices",
        "target_names",
        "refit_context",
    ] {
        let mut rows: Value = serde_json::to_value(&original.predictions).unwrap();
        let row = &mut rows.as_array_mut().unwrap()[0];
        match field {
            "y_pred" | "y_true" => {
                row[field][0] = serde_json::json!(row[field][0].as_f64().unwrap() + 1000.0)
            }
            "sample_ids" | "sample_indices" => row[field].as_array_mut().unwrap().swap(0, 1),
            "target_names" => row[field][0] = "forged-target".into(),
            _ => row[field] = "forged-context".into(),
        }
        let rows: Vec<NativePredictionRow> = serde_json::from_value(rows).unwrap();
        let forged = root.path().join(field);
        std::fs::create_dir(&forged).unwrap();
        write_native_results(
            &forged,
            original.manifest.clone(),
            original.score_set.clone(),
            rows,
        )
        .unwrap();
        let rejected = root.path().join(format!("{field}-experiment"));
        let error =
            match save_experiment_results(&forged, &rejected, Some(std::path::Path::new(&archive)))
            {
                Err(error) => error,
                Ok(_) => panic!("forged {field} accepted by model closure"),
            };
        assert!(error.contains("predictions differ"), "{field}: {error}");
        assert!(!rejected.exists());
        eprintln!("native signed prediction closure rejects forged {field}");
    }
}
