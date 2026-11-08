use dag_ml_core::public_robustness::FrozenRobustnessScenario;
use dag_ml_core::{ConformalSmallSamplePolicy, RunId};
use nirs4all::conformal::{
    calibrate_archive, resolve_uncertainty_cohort, CalibrateArchiveRequest, UncertaintyCohortInput,
};
use nirs4all::n4a_native_package::load_native_predictor_package as load_archive_v2;
use nirs4all::robustness::{robustness_archive, FrozenRobustnessRequest};
use nirs4all::{
    predict_methods_archive_v2_matrix_conformal_presentation_v2, run_dense_regression_workflow,
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
    // A calibration update must preserve unrelated opaque bytes, identity,
    // links and entrypoints, including unknown features on an unrelated entry.
    let bytes = std::fs::read(&model).unwrap();
    let base = nirs4all::n4a::open_n4a_bytes(&bytes, Default::default()).unwrap();
    let mut manifest = base.manifest().clone();
    let foreign: nirs4all::n4a::N4aArtifact = serde_json::from_value(json!({
        "id":"foreign", "kind":"attachment", "owner":"foreign-owner",
        "content_type":{"id":"foreign.bytes","version":1},
        "identity":{"owner":"foreign-owner","schema":"foreign.bytes","schema_version":1,
            "fingerprint_profile":"foreign.identity","fingerprint":"keep:identity"},
        "storage":{"kind":"embedded","root_member":"foreign/data.bin","member_paths":["foreign/data.bin"]},
        "requirements":null
    })).unwrap();
    let foreign_entry: nirs4all::n4a::N4aEntrypoint = serde_json::from_value(json!({
        "id":"foreign-view", "owner":"foreign-owner", "operation":"foreign-op",
        "root_artifact":"foreign", "input_artifacts":[], "required_features":["future.foreign@4"], "requirements":null
    })).unwrap();
    let foreign_link = nirs4all::n4a::N4aLink {
        id: "foreign-link".into(),
        owner: "foreign-owner".into(),
        relation: "opaque_relation".into(),
        source_artifact: "foreign".into(),
        target_artifact: "foreign".into(),
    };
    manifest.artifacts.push(foreign.clone());
    manifest.entrypoints.push(foreign_entry.clone());
    manifest.links.push(foreign_link.clone());
    manifest.members.push(nirs4all::n4a::N4aMember {
        path: "foreign/data.bin".into(),
        byte_length: 4,
        sha256: format!("{:x}", Sha256::digest(b"\xffraw")),
    });
    let (_, mixed_bytes) = nirs4all::n4a::write_n4a_update(
        &base,
        manifest,
        vec![nirs4all::n4a::N4aMemberInput {
            path: "foreign/data.bin",
            bytes: b"\xffraw",
        }],
        Default::default(),
    )
    .unwrap();
    std::fs::write(&model, &mixed_bytes).unwrap();
    let original = load_archive_v2(&model).unwrap();
    let corrupted = directory.path().join("rehashed-corrupted-model.n4a");
    let mut members = original.members().clone();
    let state = &original.closure().state_members()[0];
    *members
        .get_mut(&state.member_path)
        .unwrap()
        .last_mut()
        .unwrap() ^= 1;
    // Generic storage accepts freshly derived hashes. The selected scientific
    // reader must still reject state differing from the package's owner proof.
    let mixed = nirs4all::n4a::open_n4a_bytes(&mixed_bytes, Default::default()).unwrap();
    let (_, bytes) = nirs4all::n4a::write_n4a_update(
        &mixed,
        original.manifest().clone(),
        members
            .iter()
            .map(|(path, bytes)| nirs4all::n4a::N4aMemberInput { path, bytes })
            .collect(),
        Default::default(),
    )
    .unwrap();
    std::fs::write(&corrupted, bytes).unwrap();
    assert!(load_archive_v2(&corrupted).is_err());
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
    assert!(refused.contains("closure"), "{refused}");
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
    assert!(refused_audit.contains("closure"), "{refused_audit}");
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
    let calibrated_file =
        nirs4all::n4a_file::open_n4a_file(&calibrated, Default::default()).unwrap();
    assert_eq!(
        calibrated_file
            .read_member("foreign/data.bin", Default::default())
            .unwrap(),
        b"\xffraw"
    );
    assert!(archive.manifest().artifacts.contains(&foreign));
    assert!(archive.manifest().links.contains(&foreign_link));
    assert!(archive.manifest().entrypoints.contains(&foreign_entry));
    // A retained foreign reference to a replaced root must refuse instead of
    // silently changing the identity its fixed ID resolves to.
    let base = nirs4all::n4a::open_n4a_bytes(&mixed_bytes, Default::default()).unwrap();
    let mut manifest = base.manifest().clone();
    manifest.links.push(nirs4all::n4a::N4aLink {
        id: "retained-model-reference".into(),
        owner: "foreign-owner".into(),
        relation: "opaque_reference".into(),
        source_artifact: "foreign".into(),
        target_artifact: "model".into(),
    });
    let (_, pinned_bytes) =
        nirs4all::n4a::write_n4a_update(&base, manifest, vec![], Default::default()).unwrap();
    let pinned = nirs4all::n4a::open_n4a_bytes(&pinned_bytes, Default::default()).unwrap();
    let before = nirs4all::n4a_native_package::read_native_package_for_operation(
        &pinned,
        None,
        "calibrate-external",
        Default::default(),
    )
    .unwrap();
    let error = nirs4all::n4a_native_package::calibration_update(
        &pinned,
        &before,
        archive.training_outcome_bytes().unwrap(),
        archive.portable_predictor_package().unwrap(),
        "refused:rebind",
    )
    .unwrap_err();
    assert!(error.to_string().contains("retained link"), "{error}");
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
