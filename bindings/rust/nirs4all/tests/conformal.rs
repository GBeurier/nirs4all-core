use dag_ml_core::public_robustness::FrozenRobustnessScenario;
use dag_ml_core::{ConformalSmallSamplePolicy, RunId};
use nirs4all::conformal::{
    calibrate_archive, resolve_uncertainty_cohort, CalibrateArchiveRequest, UncertaintyCohortInput,
};
use nirs4all::robustness::{robustness_archive, FrozenRobustnessRequest};
use nirs4all::{
    load_archive_v2, predict_methods_archive_v2_matrix_conformal_presentation_v2,
    run_dense_regression_workflow, write_archive_v2, ArchivePayload, ArchiveV2WriteRequest,
    DenseRegressionPreprocessing, DenseRegressionWorkflowRequest,
    MethodsArchiveMatrixPredictRequest,
};
use nirs4all_io_crate::core::public_dataset::dense_dataset_package;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixture(prefix: &str) -> Value {
    let x = (0..12)
        .map(|row| {
            (0..7)
                .map(|col| {
                    (((row + 1) * (col + 1)) as f64 * 0.13).sin()
                        + row as f64 * 0.11
                        + col as f64 * 0.2
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let y = x
        .iter()
        .enumerate()
        .map(|(i, row)| 2.0 * row[0] - row[2] + i as f64 * 0.13)
        .collect::<Vec<_>>();
    let ids = (0..12)
        .map(|i| format!("{prefix}{i:02}"))
        .collect::<Vec<_>>();
    json!({"schema":"nirs4all.dataset.v1","schema_version":1,"origin_ids":ids,"fold_ids":vec![Value::Null;12],
        "dataset":{"schema":"nirs4all.multimodal-dataset","schema_version":1,"name":"conformal-test","sample_ids":ids,
            "sources":[{"name":"spectra","sample_ids":ids,"representation_id":"signal_1d","axes":["sample","wavelength"],
                "feature_names":null,"axis_units":{},"axis_coordinates":{},"array":{"dtype":"float64","shape":[12,7],"values":x}}],
            "y":{"dtype":"float64","shape":[12],"values":y},"groups":null,
            "partitions":{"dtype":"<U5","shape":[12],"values":vec!["train";12]}}})
}

#[test]
fn native_frozen_archive_calibrates_replays_intervals_and_gaussian_audits() {
    let library =
        PathBuf::from(std::env::var_os("N4M_LIBRARY_PATH").expect("exact Methods library"))
            .canonicalize()
            .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let train = dense_dataset_package(&fixture("train."), "spectra").unwrap();
    let heldout = fixture("cal.");
    let model = directory.path().join("model.n4a");
    let workflow = run_dense_regression_workflow(DenseRegressionWorkflowRequest {
        dataset: &train,
        source_id: "spectra",
        components: &[1, 2],
        preprocessing: DenseRegressionPreprocessing::SnvSavitzkyGolay,
        methods_library_path: &library,
        archive_path: &model,
        run_id: "run:rust:conformal:train",
    })
    .unwrap();
    let original = load_archive_v2(&model).unwrap();
    let corrupted = directory.path().join("rehashed-corrupted-model.n4a");
    let payloads = original
        .members()
        .iter()
        .map(|(path, bytes)| {
            let mut bytes = bytes.clone();
            if path.ends_with(".n4mm") {
                *bytes.last_mut().unwrap() ^= 1;
            }
            ArchivePayload {
                path: path.clone(),
                bytes,
            }
        })
        .collect();
    write_archive_v2(
        &corrupted,
        ArchiveV2WriteRequest {
            manifest: original.manifest().clone(),
            payloads,
        },
    )
    .unwrap();
    // The container has valid newly derived raw hashes; semantic closure must
    // still refuse model bytes that differ from the signed native package.
    load_archive_v2(&corrupted).unwrap();
    let refused = calibrate_archive(CalibrateArchiveRequest {
        model_archive: &corrupted,
        calibration_record: &heldout,
        source_id: Some("spectra"),
        methods_library_path: &library,
        archive_path: &directory.path().join("refused-calibration.n4a"),
        run_id: "run:rust:conformal:refused",
        coverages: vec![0.8],
        small_sample_policy: ConformalSmallSamplePolicy::Error,
    })
    .unwrap_err();
    assert!(refused.contains("semantic closure"), "{refused}");
    let refused_audit = robustness_archive(FrozenRobustnessRequest {
        archive_path: &corrupted,
        methods_library_path: &library,
        methods_library_sha256: &format!("{:x}", Sha256::digest(std::fs::read(&library).unwrap())),
        sample_ids: vec!["test:corruption".into()],
        x: vec![vec![1.0; 7]],
        dataset: None,
        source_id: None,
        truth: vec![vec![1.0]],
        run_id: "run:rust:robustness:refused",
        scenarios: vec![
            FrozenRobustnessScenario {
                id: "observed".into(),
                kind: "observed".into(),
                severity: 0.0,
                seed: 0,
            },
            FrozenRobustnessScenario {
                id: "gaussian".into(),
                kind: "spectral_noise".into(),
                severity: 0.01,
                seed: 1,
            },
        ],
    })
    .unwrap_err();
    assert!(
        refused_audit.contains("semantic closure"),
        "{refused_audit}"
    );
    let calibrated = directory.path().join("calibrated.n4a");
    let mut aliases = heldout.clone();
    aliases["origin_ids"] = json!(vec!["train.00"; 12]);
    aliases["dataset"]["groups"] =
        json!({"dtype":"<U64", "shape":[12], "values": aliases["dataset"]["sample_ids"]});
    let alias_path = directory.path().join("refused-aliases.n4a");
    let alias_error = calibrate_archive(CalibrateArchiveRequest {
        model_archive: &model,
        calibration_record: &aliases,
        source_id: None,
        methods_library_path: &library,
        archive_path: &alias_path,
        run_id: "run:rust:conformal:aliases",
        coverages: vec![0.8],
        small_sample_policy: ConformalSmallSamplePolicy::Error,
    })
    .unwrap_err();
    assert!(
        alias_error.contains("independent physical"),
        "{alias_error}"
    );
    assert!(!alias_path.exists());
    let result = calibrate_archive(CalibrateArchiveRequest {
        model_archive: &model,
        calibration_record: &heldout,
        source_id: Some("spectra"),
        methods_library_path: &library,
        archive_path: &calibrated,
        run_id: "run:rust:conformal:calibrate",
        coverages: vec![0.8],
        small_sample_policy: ConformalSmallSamplePolicy::Error,
    })
    .unwrap();
    assert_eq!(result["calibration_replay"]["phase"], "PREDICT");
    assert_eq!(
        result["training_outcome"]["execution_bundle"]["refit_artifacts"],
        serde_json::to_value(workflow.training.execution_bundle.refit_artifacts).unwrap()
    );
    std::fs::remove_file(model).unwrap();
    let archive = load_archive_v2(&calibrated).unwrap();
    let independent = fixture("test.");
    let resolved = resolve_uncertainty_cohort(
        &archive,
        UncertaintyCohortInput::Dataset {
            record: &independent,
            source_id: None,
        },
    )
    .unwrap();
    assert_eq!(
        resolved.diagnostics["uncertainty_schema_validation"],
        "io_source_schema"
    );
    for field in [
        "groups",
        "independent_unit_ids",
        "repetition_ids",
        "origin_ids",
        "axis_units",
    ] {
        let mut invalid = independent.clone();
        match field {
            "groups" => {
                invalid["dataset"]["groups"] = json!({"dtype":"<U64", "shape":[12], "values": invalid["dataset"]["sample_ids"]})
            }
            "independent_unit_ids" | "repetition_ids" => {
                invalid["dataset"][field] = invalid["dataset"]["sample_ids"].clone()
            }
            "origin_ids" => invalid["origin_ids"] = json!(vec!["train.00"; 12]),
            "axis_units" => {
                invalid["dataset"]["sources"][0]["axis_units"] = json!({"wavelength":"nm"})
            }
            _ => unreachable!(),
        }
        assert!(
            resolve_uncertainty_cohort(
                &archive,
                UncertaintyCohortInput::Dataset {
                    record: &invalid,
                    source_id: None,
                }
            )
            .is_err(),
            "guard missed {field}"
        );
    }
    let ids: Vec<String> =
        serde_json::from_value(independent["dataset"]["sample_ids"].clone()).unwrap();
    let x: Vec<Vec<f64>> =
        serde_json::from_value(independent["dataset"]["sources"][0]["array"]["values"].clone())
            .unwrap();
    let y: Vec<f64> =
        serde_json::from_value(independent["dataset"]["y"]["values"].clone()).unwrap();
    let hash = format!("{:x}", Sha256::digest(std::fs::read(&library).unwrap()));
    let prediction = predict_methods_archive_v2_matrix_conformal_presentation_v2(
        &archive,
        MethodsArchiveMatrixPredictRequest {
            sample_ids: ids.clone(),
            x: x.clone(),
            expected_target_names: vec!["y".into()],
            methods_library_path: library.clone(),
            methods_library_sha256: hash.clone(),
            request_id: "rust:independent".into(),
            outcome_id: "rust:independent:outcome".into(),
            run_id: RunId::new("run:rust:independent").unwrap(),
            warnings: vec![],
            diagnostics: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(prediction.interval_block.intervals.len(), 1);
    let report = robustness_archive(FrozenRobustnessRequest {
        archive_path: &calibrated,
        methods_library_path: &library,
        methods_library_sha256: &hash,
        sample_ids: ids,
        x,
        dataset: None,
        source_id: None,
        truth: y.iter().map(|v| vec![*v]).collect(),
        run_id: "run:rust:robustness",
        scenarios: vec![
            FrozenRobustnessScenario {
                id: "observed".into(),
                kind: "observed".into(),
                severity: 0.0,
                seed: 0,
            },
            FrozenRobustnessScenario {
                id: "noise".into(),
                kind: "spectral_noise".into(),
                severity: 0.02,
                seed: 7,
            },
        ],
    })
    .unwrap();
    assert_eq!(report["mode"], "clean_frozen");
    assert_ne!(
        report["scenarios"][0]["point_predictions"],
        report["scenarios"][1]["point_predictions"]
    );
    let structured = robustness_archive(FrozenRobustnessRequest {
        archive_path: &calibrated,
        methods_library_path: &library,
        methods_library_sha256: &hash,
        sample_ids: vec![],
        x: vec![],
        dataset: Some(&independent),
        source_id: None,
        truth: y.iter().map(|v| vec![*v]).collect(),
        run_id: "run:rust:robustness:structured",
        scenarios: vec![FrozenRobustnessScenario {
            id: "observed".into(),
            kind: "observed".into(),
            severity: 0.0,
            seed: 0,
        }],
    })
    .unwrap();
    assert_eq!(
        structured["scenarios"][0]["point_predictions"],
        report["scenarios"][0]["point_predictions"]
    );
}
