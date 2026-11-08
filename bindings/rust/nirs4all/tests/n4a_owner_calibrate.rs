//! F10 production cold-process test. Historical red bytes remain external.
use dag_ml_core::{ConformalMultiTargetPolicy, ConformalRadius, ConformalSmallSamplePolicy};
use nirs4all::n4a::*;
use nirs4all::n4a_owner_native::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
#[path = "fixtures/n4a_native_actions_v1/support.rs"]
mod support;
use support::*;

fn calibration_child(directory: &Path, selected: bool) {
    let bytes = std::fs::read(directory.join("source.n4a")).unwrap();
    let before_hash = digest(&bytes);
    let source =
        open_owned_n4a_bytes(bytes.into_boxed_slice(), N4aMemoryBudget::default()).unwrap();
    let source_states = states(&source);
    let record_bytes = std::fs::read(directory.join("calibration.json")).unwrap();
    let raw: Value = serde_json::from_slice(&record_bytes).unwrap();
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let result = calibrate_n4a_native(
        &source,
        action(&record_bytes, "calibrate-external", &library, &sha),
        NativeCalibrationOptions {
            coverages: vec![0.8],
            ..Default::default()
        },
        "f10:calibrated",
    )
    .unwrap();
    assert_mapped_n4m_library();
    assert_eq!(states(&result.container), source_states);
    assert_eq!(digest(&copy_n4a_bytes(&source)), before_hash);
    assert_eq!(
        result.container.manifest().artifacts.len(),
        source.manifest().artifacts.len()
    );
    assert_eq!(
        result.container.manifest().entrypoints,
        source.manifest().entrypoints
    );
    assert_eq!(
        result.receipt.relation_authority.as_deref(),
        Some("fingerprint_bound_self_consistent")
    );
    assert_eq!(result.receipt.authenticity, "not_established");
    assert_eq!(
        result.receipt.observation_id_disjointness.as_deref(),
        Some("checked_against_recorded_universe")
    );
    assert_eq!(
        result.receipt.observation_content_disjointness.as_deref(),
        Some("unavailable")
    );
    assert_eq!(result.container.manifest().links, source.manifest().links);
    let expected = dag_ml_core::build_training_artifact_facets_from_bytes(
        root(&result.container, "result"),
        root(&result.container, "model"),
    )
    .unwrap();
    expected.validate().unwrap();
    assert_eq!(members(&result.container), expected.members);
    assert_eq!(
        json!(result.container.manifest().artifacts),
        json!(expected.graph.artifacts)
    );
    assert_eq!(result.replay.prediction_block_count, 1);
    assert_eq!(
        package(&result.container)
            .conformal_calibration_replay
            .unwrap()
            .outcome_fingerprint,
        result.replay.outcome_fingerprint
    );
    let claims = result.receipt.envelope.predict_cohort.as_ref().unwrap();
    assert_eq!(claims.role, dag_ml_core::PredictCohortRole::ExternalTest);
    assert_ne!(
        result.receipt.envelope.relation_fingerprint.as_ref(),
        Some(&claims.relation_fingerprint)
    );
    let pkg = package(&result.container);
    let calibration = pkg.conformal_calibration.as_ref().unwrap();
    assert_eq!(calibration.target_names, expected_names(selected));
    assert!(!pkg
        .effective_plan
        .fold_set
        .as_ref()
        .unwrap()
        .folds
        .is_empty());
    // Independent test-only rank/order-statistic reference, joined by sample ID
    // and target name against the literal raw record, never record position.
    let point = &result.replay.outputs[0].predictions[0];
    assert!(point.sample_ids.windows(2).all(|w| w[0] < w[1]));
    let ids: Vec<String> = serde_json::from_value(raw["dataset"]["sample_ids"].clone()).unwrap();
    let truth: Vec<Vec<f64>> =
        serde_json::from_value(raw["dataset"]["y"]["values"].clone()).unwrap();
    let truth: BTreeMap<_, _> = ids.into_iter().zip(truth).collect();
    let predictions = serde_json::to_value(point).unwrap();
    // PredictionBlock's values are dense rows in the owner contract.
    let values: Vec<Vec<f64>> = serde_json::from_value(predictions["values"].clone()).unwrap();
    for (column, radius) in calibration.quantiles[0].radii.iter().enumerate() {
        let raw_column = if selected { 1 } else { column };
        let mut residuals: Vec<f64> = point
            .sample_ids
            .iter()
            .zip(&values)
            .map(|(id, row)| (truth[id.as_str()][raw_column] - row[column]).abs())
            .collect();
        residuals.sort_by(f64::total_cmp);
        let rank = ((residuals.len() + 1) as f64 * 0.8).ceil() as usize;
        assert_eq!(*radius, ConformalRadius::Finite(residuals[rank - 1]));
    }
    std::fs::write(
        directory.join("calibrated.n4a"),
        copy_n4a_bytes(&result.container),
    )
    .unwrap();
    let inference = std::fs::read(directory.join("inference.json")).unwrap();
    let actual = predict_n4a_native(
        &result.container,
        action(&inference, "predict-external", &library, &sha),
    )
    .unwrap();
    assert_eq!(actual.replay.conformal_intervals.len(), 1);
    assert_eq!(
        serde_json::to_value(
            calibration
                .apply(&actual.replay.outputs[0].predictions[0])
                .unwrap()
        )
        .unwrap(),
        json!(actual.replay.conformal_intervals[0])
    );
    assert!(actual.receipt.observation_id_disjointness.is_none());
    assert!(actual.receipt.relation_authority.is_none());
    std::fs::write(directory.join("reference.json"), serde_json::to_vec(&json!({
        "predictions":actual.replay.outputs[0].predictions, "intervals":actual.replay.conformal_intervals,
        "states":source_states})).unwrap()).unwrap();
}
fn reload_child(directory: &Path, selected: bool) {
    let source = open_owned_n4a_bytes(
        std::fs::read(directory.join("calibrated.n4a"))
            .unwrap()
            .into_boxed_slice(),
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert_eq!(
        package(&source).output_bindings[0].target_names,
        expected_names(selected)
    );
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let bytes = std::fs::read(directory.join("inference.json")).unwrap();
    let actual =
        predict_n4a_native(&source, action(&bytes, "predict-external", &library, &sha)).unwrap();
    assert_mapped_n4m_library();
    assert!(actual
        .replay
        .lineage
        .iter()
        .all(|l| l.phase == dag_ml_core::Phase::Predict));
    let reference: Value =
        serde_json::from_slice(&std::fs::read(directory.join("reference.json")).unwrap()).unwrap();
    assert_eq!(
        json!(actual.replay.outputs[0].predictions),
        reference["predictions"]
    );
    assert_eq!(
        json!(actual.replay.conformal_intervals),
        reference["intervals"]
    );
    assert_eq!(json!(states(&source)), reference["states"]);
}
#[test]
fn admitted_replay_to_calibration() {
    if let Ok(stage) = std::env::var("CORE_F10_CHILD_STAGE") {
        let directory = PathBuf::from(std::env::var_os("CORE_F10_CASE_DIRECTORY").unwrap());
        let selected = std::env::var("CORE_F10_SELECTED").unwrap() == "1";
        match stage.as_str() {
            "calibrate" => calibration_child(&directory, selected),
            "reload" => reload_child(&directory, selected),
            _ => panic!("unknown child stage"),
        }
        return;
    }
    let scratch = PathBuf::from(
        std::env::var_os("CORE_F10_EXECUTION_SCRATCH")
            .expect("controller scratch admission required"),
    );
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = scratch.join(format!("f10-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let mut failures = vec![];
    for selected in [false, true] {
        let label = if selected { "selected1" } else { "wide" };
        let case = directory.join(label);
        std::fs::create_dir(&case).unwrap();
        let produced = source(selected, false, label);
        std::fs::write(case.join("source.n4a"), copy_n4a_bytes(&produced)).unwrap();
        std::fs::write(
            case.join("calibration.json"),
            serde_json::to_vec(&record("calibration", 12, false, true)).unwrap(),
        )
        .unwrap();
        std::fs::write(
            case.join("inference.json"),
            serde_json::to_vec(&record("inference", 5, false, false)).unwrap(),
        )
        .unwrap();
        drop(produced);
        for stage in ["calibrate", "reload"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "admitted_replay_to_calibration", "--nocapture"])
                .env("CORE_F10_CHILD_STAGE", stage)
                .env("CORE_F10_CASE_DIRECTORY", &case)
                .env("CORE_F10_SELECTED", if selected { "1" } else { "0" })
                .output()
                .unwrap();
            std::fs::write(case.join(format!("{stage}.stdout")), &output.stdout).unwrap();
            std::fs::write(case.join(format!("{stage}.stderr")), &output.stderr).unwrap();
            std::fs::write(
                case.join(format!("{stage}.status")),
                output.status.to_string(),
            )
            .unwrap();
            eprint!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if !output.status.success() {
                failures.push(format!("{label}:{stage}"));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "F10 production failures: {failures:?}; evidence {}",
        directory.display()
    );
}

#[test]
fn whole_record_and_observation_identity_refuse_before_runtime() {
    let source = source(false, false, "identity");
    let mut row = record("external", 12, false, true);
    row["dataset"]["sample_ids"][0] = json!("train-18");
    row["dataset"]["sources"][0]["sample_ids"][0] = json!("train-18");
    row["origin_ids"][0] = json!("train-18");
    assert!(!matches!(
        calibrate_error(&source, &serde_json::to_vec(&row).unwrap()),
        NativeActionError::Owner(dag_ml_core::FacetRefusal::RuntimeUnavailable { .. })
    ));
    let mut partitioned = record("external", 12, false, true);
    partitioned["dataset"]["partitions"]["values"][0] = json!("test");
    assert!(matches!(
        calibrate_error(&source, &serde_json::to_vec(&partitioned).unwrap()),
        NativeActionError::Owner(dag_ml_core::FacetRefusal::UnsupportedOperation { .. })
    ));
}

#[test]
fn joint_max_and_small_sample_policy_use_owner() {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let source = source(false, true, "joint-max");
    let bytes = serde_json::to_vec(&record("external", 12, false, true)).unwrap();
    for policy in [
        ConformalMultiTargetPolicy::Marginal,
        ConformalMultiTargetPolicy::JointMax,
    ] {
        let result = calibrate_n4a_native(
            &source,
            action(&bytes, "calibrate-external", &library, &sha),
            NativeCalibrationOptions {
                coverages: vec![0.8],
                multi_target_policy: policy,
                small_sample_policy: ConformalSmallSamplePolicy::Error,
            },
            "policy",
        )
        .unwrap();
        let package = package(&result.container);
        let calibration = package.conformal_calibration.unwrap();
        assert_eq!(calibration.multi_target_policy, policy);
        assert_eq!(
            calibration.quantiles[0].radii.len(),
            if policy == ConformalMultiTargetPolicy::JointMax {
                1
            } else {
                2
            }
        );
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let ids: Vec<String> =
            serde_json::from_value(raw["dataset"]["sample_ids"].clone()).unwrap();
        let truth: Vec<Vec<f64>> =
            serde_json::from_value(raw["dataset"]["y"]["values"].clone()).unwrap();
        let truth: BTreeMap<_, _> = ids.into_iter().zip(truth).collect();
        let point = &result.replay.outputs[0].predictions[0];
        let residuals: Vec<Vec<f64>> = point
            .sample_ids
            .iter()
            .zip(&point.values)
            .map(|(id, row)| {
                row.iter()
                    .zip(&truth[id.as_str()])
                    .map(|(prediction, actual)| (actual - prediction).abs())
                    .collect()
            })
            .collect();
        for (column, radius) in calibration.quantiles[0].radii.iter().enumerate() {
            let mut scores: Vec<f64> = residuals
                .iter()
                .map(|row| {
                    if policy == ConformalMultiTargetPolicy::JointMax {
                        row.iter().copied().fold(0.0, f64::max)
                    } else {
                        row[column]
                    }
                })
                .collect();
            scores.sort_by(f64::total_cmp);
            let rank = ((scores.len() + 1) as f64 * 0.8).ceil() as usize;
            assert_eq!(*radius, ConformalRadius::Finite(scores[rank - 1]));
        }
        assert_eq!(
            root(&source, "training-data"),
            root(&result.container, "training-data")
        );
        assert_eq!(states(&source), states(&result.container));
        assert!(calibrate_n4a_native(
            &result.container,
            action(&bytes, "calibrate-external", &library, &sha),
            NativeCalibrationOptions::default(),
            "again",
        )
        .is_err());
    }
    let small = serde_json::to_vec(&record("small", 2, false, true)).unwrap();
    assert!(calibrate_n4a_native(
        &source,
        action(&small, "calibrate-external", &library, &sha),
        NativeCalibrationOptions::default(),
        "small:error",
    )
    .is_err());
    let unbounded = calibrate_n4a_native(
        &source,
        action(&small, "calibrate-external", &library, &sha),
        NativeCalibrationOptions {
            small_sample_policy: ConformalSmallSamplePolicy::Unbounded,
            ..Default::default()
        },
        "small:unbounded",
    )
    .unwrap();
    assert!(package(&unbounded.container)
        .conformal_calibration
        .unwrap()
        .quantiles[0]
        .radii
        .iter()
        .all(|r| *r == ConformalRadius::Unbounded));
}
