//! Production byte-facing actions and static refusals. No test admission port.
use dag_ml_core::artifact_facets::FacetRefusal;
use nirs4all::n4a::*;
use nirs4all::n4a_owner_native::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
#[path = "fixtures/n4a_native_actions_v1/support.rs"]
mod support;
use support::*;

#[test]
fn request_budget_refuses_snapshots_before_any_methods_runtime() {
    // Load the genuine corpus directly: source() trains through Methods and
    // would make this static admission regression depend on a native runtime.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/fixtures/n4a_v1/combined");
    let manifest: N4aManifest =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let payloads: BTreeMap<_, _> = manifest
        .members
        .iter()
        .map(|member| {
            (
                member.path.clone(),
                std::fs::read(root.join(&member.path)).unwrap(),
            )
        })
        .collect();
    let source = reseal(manifest, payloads);
    assert_eq!(source.memory_budget(), N4aMemoryBudget::default());
    let library = Path::new("/missing/budget-regression/libn4m.so");
    let sha = "0".repeat(64);
    let mut request = action(b"{}", "predict:stored.features", library, &sha);
    request.input = NativeInput::Stored;
    request.memory_budget = N4aMemoryBudget::bytes(1);
    assert!(matches!(
        predict_n4a_native(&source, request),
        Err(NativeActionError::Container(N4aError::Memory(_)))
    ));
    assert!(matches!(
        retrain_n4a_native(
            &source,
            NativeRetrainRequest {
                record: b"{}",
                runtime: NativeRuntime {
                    library,
                    expected_sha256: &sha
                },
                run_id: "budget-retrain",
                container_id: "budget-retrain",
                embed_training_record: false,
                memory_budget: N4aMemoryBudget::bytes(1),
            }
        ),
        Err(NativeActionError::Container(N4aError::Memory(_)))
    ));
}

#[test]
fn inference_is_target_free_and_sample_keyed_including_target_index_one() {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    for selected in [false, true] {
        let source = source(selected, false, if selected { "selected" } else { "wide" });
        let record = record("inference", 5, false, false);
        let bytes = serde_json::to_vec(&record).unwrap();
        let first = predict_n4a_native(&source, action(&bytes, "predict-external", &library, &sha))
            .unwrap();
        let claims = first.receipt.envelope.predict_cohort.as_ref().unwrap();
        assert_eq!(claims.role, dag_ml_core::PredictCohortRole::Inference);
        assert!(first.receipt.envelope.target_content_fingerprint.is_none());
        assert_eq!(
            first.receipt.envelope.relation_fingerprint.as_ref(),
            Some(&claims.relation_fingerprint)
        );
        assert!(first.receipt.observation_id_disjointness.is_none());
        assert!(first.receipt.relation_authority.is_none());
        assert_eq!(
            first.replay.outputs[0].predictions[0].target_names,
            expected_names(selected)
        );
        // Fresh call over the same supplied immutable bytes is deterministic.
        let second =
            predict_n4a_native(&source, action(&bytes, "predict-external", &library, &sha))
                .unwrap();
        assert_eq!(
            json!(first.replay.outputs[0].predictions),
            json!(second.replay.outputs[0].predictions)
        );
    }
}

#[test]
fn stored_input_cannot_be_overridden_and_training_prediction_is_in_sample() {
    let source = source(false, true, "stored");
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let bytes = serde_json::to_vec(&record("external", 5, false, false)).unwrap();
    assert!(predict_n4a_native(&source, action(&bytes, "predict", &library, &sha)).is_err());
    let mut request = action(&bytes, "predict", &library, &sha);
    request.input = NativeInput::Stored;
    request.partition = "train";
    let result = predict_n4a_native(&source, request).unwrap();
    assert_eq!(result.replay.outputs[0].predictions[0].sample_ids.len(), 18);
    assert!(result.receipt.observation_id_disjointness.is_none());
}

#[test]
fn strict_structure_aliases_and_independent_record_budget() {
    let source = source(false, false, "scanner");
    let library = Path::new("/missing/stage-d/libn4m.so");
    let sha = "0".repeat(64);
    let record = serde_json::to_string(&record("external", 5, false, false)).unwrap();
    for bad in [
        record.replacen('{', "{\"schema\":\"first\",", 1),
        record.replacen('{', "{\"sch\\u0065ma\":\"first\",", 1),
        "{\"x\":1,}".into(),
        "[1,]".into(),
        "{\"x\":01}".into(),
        "{\"x\":+1}".into(),
        "{\"x\":1e}".into(),
        "{\"x\":NaN}".into(),
        "{\"x\":\"\\x00\"}".into(),
        "{} {}".into(),
        "\u{b}{}".into(),
    ] {
        assert!(matches!(
            predict_n4a_native(
                &source,
                action(bad.as_bytes(), "predict-external", library, &sha)
            ),
            Err(NativeActionError::Record(_))
        ));
    }
    let mut request = action(record.as_bytes(), "predict-external", library, &sha);
    request.memory_budget = N4aMemoryBudget::bytes(1);
    assert!(matches!(
        predict_n4a_native(&source, request),
        Err(NativeActionError::Container(N4aError::Memory(_)))
    ));
    // Core has no nesting policy; IO's parser still refuses, without a stack risk.
    let deep = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    assert!(matches!(
        predict_n4a_native(
            &source,
            action(deep.as_bytes(), "predict-external", library, &sha)
        ),
        Err(NativeActionError::Record(_))
    ));
}

#[test]
fn schema_axis_units_and_column_order_refuse_on_both_actions() {
    let source = source(false, false, "schema");
    for kind in 0..3 {
        let mut raw = record("external", 12, false, true);
        match kind {
            0 => {
                raw["dataset"]["sources"][0]["axis_coordinates"] =
                    json!({"wavelength":[7,6,5,4,3,2,1]})
            }
            1 => raw["dataset"]["sources"][0]["axis_units"] = json!({"wavelength":"nm"}),
            _ => {
                raw["dataset"]["sources"][0]["feature_names"] =
                    json!(["b", "a", "c", "d", "e", "f", "g"])
            }
        }
        let bytes = serde_json::to_vec(&raw).unwrap();
        for operation in ["predict-external", "calibrate-external"] {
            let error = if operation == "calibrate-external" {
                calibrate_error(&source, &bytes)
            } else {
                match predict_n4a_native(
                    &source,
                    action(
                        &bytes,
                        operation,
                        Path::new("/missing/stage-d/libn4m.so"),
                        &"0".repeat(64),
                    ),
                ) {
                    Err(e) => e,
                    Ok(_) => panic!("schema must refuse"),
                }
            };
            assert!(
                matches!(
                    error,
                    NativeActionError::Schema(_) | NativeActionError::Record(_)
                ),
                "{error}"
            );
        }
    }
}

#[test]
fn complete_members_include_unproven_foreign_model_and_no_attachment_drop() {
    let source = source(false, false, "foreign");
    let mut manifest = source.manifest().clone();
    let mut payloads = members(&source);
    let mut foreign = manifest
        .artifacts
        .iter()
        .find(|a| a.id == "model")
        .unwrap()
        .clone();
    foreign.id = "foreign-model".into();
    foreign.owner = "foreign".into();
    foreign.identity.owner = "foreign".into();
    foreign.content_type.id = "foreign.model.v1".into();
    foreign.identity.schema = "foreign.model.v1".into();
    foreign.identity.fingerprint_profile = "sha256.raw.v1".into();
    foreign.identity.fingerprint = digest(b"foreign");
    foreign.storage = N4aStorage::Embedded {
        root_member: "foreign.bin".into(),
        member_paths: vec!["foreign.bin".into()],
    };
    foreign.requirements = None;
    manifest.artifacts.push(foreign);
    payloads.insert("foreign.bin".into(), b"foreign".to_vec());
    let forged = reseal(manifest.clone(), payloads.clone());
    let bytes = serde_json::to_vec(&record("external", 12, false, true)).unwrap();
    assert!(matches!(
        owner_error(calibrate_error(&forged, &bytes)),
        FacetRefusal::UnresolvedOwnerContent { .. }
    ));
    assert!(matches!(
        read_n4a_training_view(&forged),
        Err(NativeActionError::Owner(
            FacetRefusal::UnresolvedOwnerContent { .. }
        ))
    ));
    manifest.artifacts.last_mut().unwrap().kind = ArtifactKind::Attachment;
    let attached = reseal(manifest, payloads);
    assert!(matches!(
        owner_error(calibrate_error(&attached, &bytes)),
        FacetRefusal::UnsupportedOperation { .. }
    ));
    // Generic immutable copy still preserves the foreign payload exactly.
    assert_eq!(copy_n4a_bytes(&attached), copy_n4a_bytes(&attached));
}

#[test]
fn selected_unknown_features_and_wrong_roles_refuse_statically() {
    let source = source(false, false, "features");
    let mut manifest = source.manifest().clone();
    manifest.entrypoints[1]
        .required_features
        .push("unknown@999".into());
    let changed = reseal(manifest, members(&source));
    let raw = serde_json::to_vec(&record("external", 12, false, true)).unwrap();
    assert!(matches!(
        calibrate_error(&changed, &raw),
        NativeActionError::Operation(OperationRefusal::UnknownRequiredFeatures(_))
    ));
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    // Features of another entrypoint do not poison this exact selection.
    predict_n4a_native(&changed, action(&raw, "predict-external", &library, &sha)).unwrap();
    let mut manifest = source.manifest().clone();
    manifest.entrypoints[1].root_artifact = "result".into();
    let changed = reseal(manifest, members(&source));
    assert!(matches!(
        calibrate_error(&changed, &raw),
        NativeActionError::Owner(_)
    ));
}

#[test]
fn false_x_mask_and_declared_experimental_units_refuse_before_runtime() {
    let source = source(false, false, "masks");
    for kind in 0..5 {
        let mut raw = record("external", 12, false, true);
        match kind {
            0 => {
                raw["dataset"]["sources"][0]["presence_mask"] = json!({"dtype":"bool","shape":[12],"values":[false,true,true,true,true,true,true,true,true,true,true,true]});
            }
            1 => {
                raw["dataset"]["groups"] =
                    json!({"dtype":"<U2","shape":[12],"values":vec!["g";12]});
            }
            2 => {
                raw["origin_ids"][0] = json!("train-18");
            }
            3 => {
                raw["dataset"]["independent_unit_ids"] =
                    json!((0..12).map(|i| format!("u{i}")).collect::<Vec<_>>());
            }
            _ => {
                raw["dataset"]["independent_unit_ids"] =
                    json!((0..12).map(|i| format!("u{i}")).collect::<Vec<_>>());
                raw["dataset"]["repetition_ids"] = json!(vec!["r1"; 12]);
            }
        }
        let error = calibrate_error(&source, &serde_json::to_vec(&raw).unwrap());
        assert!(
            !matches!(
                error,
                NativeActionError::Owner(FacetRefusal::RuntimeUnavailable { .. })
            ),
            "{error}"
        );
    }
}

#[test]
fn supported_missing_runtime_is_typed_and_classifier_calibration_is_static() {
    let source = source(false, false, "runtime");
    let raw = serde_json::to_vec(&record("external", 12, false, true)).unwrap();
    assert!(matches!(
        owner_error(calibrate_error(&source, &raw)),
        FacetRefusal::RuntimeUnavailable { .. }
    ));
    let mut training = record("class-train", 18, true, true);
    training["dataset"]["task_type"] = json!("classification");
    training["dataset"]["target_names"] = json!(["class"]);
    training["dataset"]["y"] =
        json!({"dtype":"int64","shape":[18],"values":(0..18).map(|i|(i/2)%2).collect::<Vec<_>>()});
    let recipe = json!({"steps":[{"method_id":"models.classification.pls_lda","role":"classifier","params":{"n_components":2}}],"candidates":[{}]});
    let classifier = source_recipe(recipe, training, false, "classifier");
    assert!(matches!(
        owner_error(calibrate_error(&classifier, &raw)),
        FacetRefusal::UnsupportedOperation { .. }
    ));
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let inference = serde_json::to_vec(&record("future", 5, false, false)).unwrap();
    predict_n4a_native(
        &classifier,
        action(&inference, "predict-external", &library, &sha),
    )
    .unwrap();
}

#[test]
fn raw_pls_and_target_index_zero_are_genuine_native_families() {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let raw = serde_json::to_vec(&record("future", 5, false, false)).unwrap();
    for selected in [false, true] {
        let mut recipe = json!({"steps":[{"method_id":"models.pls.pls_regression","role":"regressor","params":{"n_components":2}}],"candidates":[{}]});
        if selected {
            recipe["steps"][0]["params"]["native_target_index"] = json!(0);
        }
        let source = source_recipe(
            recipe,
            record("train", 18, true, true),
            false,
            if selected { "raw:index0" } else { "raw:wide" },
        );
        let result =
            predict_n4a_native(&source, action(&raw, "predict-external", &library, &sha)).unwrap();
        assert_eq!(
            result.replay.outputs[0].predictions[0].target_names,
            if selected {
                vec!["protein".to_string()]
            } else {
                expected_names(false)
            }
        );
    }
}

#[test]
fn strict_owner_member_aliases_and_mixed_outcomes_refuse() {
    let source = source(false, false, "mixed:left");
    let other = support::source(true, false, "mixed:right");
    let bytes = serde_json::to_vec(&record("external", 12, false, true)).unwrap();
    let model_path = match &source
        .manifest()
        .artifacts
        .iter()
        .find(|a| a.id == "model")
        .unwrap()
        .storage
    {
        N4aStorage::Embedded { root_member, .. } => root_member.clone(),
        _ => panic!("embedded model"),
    };
    for alias in ["schema_version", "sch\\u0065ma_version", "e\u{301}"] {
        let mut payloads = members(&source);
        let text = std::str::from_utf8(&payloads[&model_path]).unwrap();
        let injected = if alias == "e\u{301}" {
            "{\"é\":1,\"e\\u0301\":2,".to_string()
        } else {
            format!("{{\"{alias}\":2,")
        };
        payloads.insert(
            model_path.clone(),
            text.replacen('{', &injected, 1).into_bytes(),
        );
        let altered = reseal(source.manifest().clone(), payloads);
        let error = calibrate_error(&altered, &bytes);
        assert!(matches!(error, NativeActionError::Owner(_)), "{error}");
        assert!(!matches!(
            error,
            NativeActionError::Owner(FacetRefusal::RuntimeUnavailable { .. })
        ));
    }
    let mut manifest = source.manifest().clone();
    let mut payloads = members(&source);
    let other_result = other
        .manifest()
        .artifacts
        .iter()
        .find(|a| a.id == "result")
        .unwrap();
    let result_index = manifest
        .artifacts
        .iter()
        .position(|a| a.id == "result")
        .unwrap();
    manifest.artifacts[result_index] = other_result.clone();
    if let N4aStorage::Embedded { root_member, .. } = &other_result.storage {
        payloads.insert(root_member.clone(), root(&other, "result").to_vec());
    }
    let mixed = reseal(manifest, payloads);
    assert!(matches!(
        calibrate_error(&mixed, &bytes),
        NativeActionError::Owner(_)
    ));
}

#[test]
fn large_universe_has_no_member_cap_and_only_a_declared_budget_refuses() {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let recipe = serde_json::from_value(recipe_value(false)).unwrap();
    let bytes = serde_json::to_vec(&record("large-train", 1024, true, true)).unwrap();
    let produce = |memory_budget| {
        produce_n4a_native(NativeProduceRequest {
            record: &bytes,
            source_id: "spectra",
            recipe: &recipe,
            runtime: NativeRuntime {
                library: &library,
                expected_sha256: &sha,
            },
            run_id: "large-universe",
            container_id: "large-universe",
            embed_training_record: false,
            memory_budget,
        })
    };
    assert!(produce(N4aMemoryBudget::default()).is_ok());
    assert!(matches!(
        produce(N4aMemoryBudget::bytes(64 * 1024)),
        Err(NativeActionError::Container(N4aError::Memory(_)))
    ));
}

#[test]
fn static_cold_child_has_no_methods_library_mapping() {
    if let Some(directory) = std::env::var_os("CORE_STATIC_CASE_DIRECTORY") {
        #[cfg(target_os = "linux")]
        assert!(!std::fs::read_to_string("/proc/self/maps")
            .unwrap()
            .contains("libn4m"));
        let bytes = std::fs::read(Path::new(&directory).join("source.n4a")).unwrap();
        let source =
            open_owned_n4a_bytes(bytes.into_boxed_slice(), N4aMemoryBudget::default()).unwrap();
        let bytes = serde_json::to_vec(&record("external", 12, false, true)).unwrap();
        let error = calibrate_error(&source, &bytes);
        assert!(matches!(
            error,
            NativeActionError::Owner(FacetRefusal::RuntimeUnavailable { .. })
        ));
        let mut overlap = record("external", 12, false, true);
        overlap["dataset"]["sample_ids"][0] = json!("train-18");
        overlap["dataset"]["sources"][0]["sample_ids"][0] = json!("train-18");
        overlap["origin_ids"][0] = json!("train-18");
        let error = calibrate_error(&source, &serde_json::to_vec(&overlap).unwrap());
        assert!(!matches!(
            error,
            NativeActionError::Owner(FacetRefusal::RuntimeUnavailable { .. })
        ));
        #[cfg(target_os = "linux")]
        assert!(!std::fs::read_to_string("/proc/self/maps")
            .unwrap()
            .contains("libn4m"));
        return;
    }
    let directory = std::path::PathBuf::from(
        std::env::var_os("CORE_F10_EXECUTION_SCRATCH").expect("controller scratch required"),
    )
    .join(format!("static-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(
        directory.join("source.n4a"),
        copy_n4a_bytes(&source(false, false, "static-cold")),
    )
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "static_cold_child_has_no_methods_library_mapping",
            "--nocapture",
        ])
        .env("CORE_STATIC_CASE_DIRECTORY", &directory)
        .output()
        .unwrap();
    std::fs::write(directory.join("child.stdout"), &output.stdout).unwrap();
    std::fs::write(directory.join("child.stderr"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn selector_chain_uses_a_real_manifest_role_and_preserves_state() {
    // Initialize the actual attested runtime through the production producer.
    let _bootstrap = source(false, false, "selector:bootstrap");
    let mut selectors: Vec<_> = n4m::roles::methods()
        .unwrap()
        .into_iter()
        .filter(|method| {
            method.kind == n4m::roles::MethodKind::Estimator
                && method.roles & n4m::roles::ROLE_SELECTOR != 0
                && method.capabilities & n4m::roles::CAP_SERIALIZABLE != 0
                && method.params.iter().all(|param| !param.required)
                && method
                    .inputs
                    .iter()
                    .all(|input| *input != n4m::roles::InputRequirement::Required)
        })
        .collect();
    selectors.sort_by(|a, b| a.method_id.cmp(&b.method_id));
    let selector = selectors
        .first()
        .expect("qualified Methods cohort must supply a defaultable independent selector fixture");
    eprintln!(
        "Stage D selector method={} state={}",
        selector.method_id, selector.state_format
    );
    let recipe = json!({"steps":[
        {"method_id":selector.method_id,"role":"selector","params":{}},
        {"method_id":"models.regularized.ridge","role":"regressor","params":{"scale_x":false}}
    ],"candidates":[{"alpha":0.1},{"alpha":1.0}]});
    let source = source_recipe(
        recipe,
        record("train", 18, true, true),
        false,
        "selector:chain",
    );
    let before = states(&source);
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let bytes = serde_json::to_vec(&record("external", 12, false, true)).unwrap();
    let calibrated = calibrate_n4a_native(
        &source,
        action(&bytes, "calibrate-external", &library, &sha),
        NativeCalibrationOptions {
            coverages: vec![0.8],
            ..Default::default()
        },
        "selector:calibrated",
    )
    .unwrap();
    assert_eq!(before, states(&calibrated.container));
    let future = serde_json::to_vec(&record("future", 5, false, false)).unwrap();
    predict_n4a_native(
        &calibrated.container,
        action(&future, "predict-external", &library, &sha),
    )
    .unwrap();
}

#[test]
fn training_view_of_a_sealed_producer_export_matches_its_model_evidence() {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let recipe = serde_json::from_value(recipe_value(false)).unwrap();
    let bytes = serde_json::to_vec(&record("train", 18, true, true)).unwrap();
    // One fit returns the sealed export and the owner view of that same fit.
    let produced = produce_n4a_native_with_view(NativeProduceRequest {
        record: &bytes,
        source_id: "spectra",
        recipe: &recipe,
        runtime: NativeRuntime {
            library: &library,
            expected_sha256: &sha,
        },
        run_id: "training-view",
        container_id: "training-view",
        embed_training_record: true,
        memory_budget: N4aMemoryBudget::default(),
    })
    .unwrap();
    // The read-only view over the sealed container agrees with the producer's.
    let read = read_n4a_training_view(&produced.container).unwrap();
    assert_eq!(read.source_id, "spectra");
    assert_eq!(read.source_id, produced.training.source_id);
    assert_eq!(read.outcome, produced.training.outcome);
    // The view outcome is the one the sealed `EvidenceFor` link declares for
    // the model, and the model package references exactly that fingerprint.
    let evidence: Vec<_> = produced
        .container
        .manifest()
        .links
        .iter()
        .filter(|link| {
            link.owner == "dag-ml"
                && link.relation == "evidence_for"
                && link.target_artifact == "model"
        })
        .collect();
    assert_eq!(evidence.len(), 1);
    // The owner stores the outcome once. The `EvidenceFor` source is a
    // same-identity reference to that embedded member, so resolve it as the
    // owner does: same owner, content type, identity and, when declared, the
    // raw root hash. Exactly one embedded artifact may match.
    let manifest = produced.container.manifest();
    let declared = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == evidence[0].source_artifact)
        .unwrap();
    let resolved = match &declared.storage {
        N4aStorage::Embedded { .. } => declared,
        N4aStorage::Reference { root_sha256 } => {
            let mut matches = manifest.artifacts.iter().filter(|candidate| {
                let N4aStorage::Embedded { root_member, .. } = &candidate.storage else {
                    return false;
                };
                candidate.owner == declared.owner
                    && candidate.content_type == declared.content_type
                    && candidate.identity == declared.identity
                    && root_sha256.as_ref().is_none_or(|sha| {
                        manifest
                            .members
                            .iter()
                            .any(|member| &member.path == root_member && &member.sha256 == sha)
                    })
            });
            let (Some(resolved), None) = (matches.next(), matches.next()) else {
                panic!("evidence reference must resolve to exactly one embedded owner artifact");
            };
            resolved
        }
    };
    let sealed = dag_ml_core::TrainingOutcome::from_json(
        std::str::from_utf8(root(&produced.container, &resolved.id)).unwrap(),
    )
    .unwrap();
    let fingerprint = read.outcome.to_reference().unwrap().outcome_fingerprint;
    assert_eq!(
        fingerprint,
        sealed.to_reference().unwrap().outcome_fingerprint
    );
    assert_eq!(
        fingerprint,
        package(&produced.container)
            .training_outcome
            .outcome_fingerprint
    );
}

#[test]
fn file_inspection_training_view_does_not_materialize_unrelated_attachment() {
    use nirs4all::n4a_file::*;
    use nirs4all::n4a_owner_portable::n4a_container_view;
    use std::borrow::Cow;
    use std::io::{self, Read};

    // Only the transport attachment is synthetic. The model and its evidence
    // come from the same attested native producer as the action regressions.
    let source = source(false, true, "training-view:file");
    let expected = read_n4a_training_view(&source).unwrap();
    let expected_report = n4a_container_view(&source);
    const ATTACHMENT: &str = "attachments/unrelated.bin";
    const ATTACHMENT_BYTES: u64 = 16 << 20;
    const BUDGET_BYTES: u64 = 8 << 20;
    struct GeneratedAttachment;
    impl N4aMemberSource for GeneratedAttachment {
        fn open(&self) -> io::Result<Box<dyn Read + '_>> {
            Ok(Box::new(io::repeat(0xa5).take(ATTACHMENT_BYTES)))
        }
    }
    let mut manifest = source.manifest().clone();
    manifest.members.push(N4aMember {
        path: ATTACHMENT.into(),
        byte_length: 0,
        sha256: "0".repeat(64),
    });
    manifest.artifacts.push(
        serde_json::from_value(json!({
            "id":"unrelated-attachment", "kind":"attachment", "owner":"test-host",
            "content_type":{"id":"test.opaque-bytes.v1", "version":1},
            "identity":{"owner":"test-host", "schema":"test.opaque-bytes.v1", "schema_version":1,
                "fingerprint_profile":"opaque.test-only", "fingerprint":"no-scientific-claim"},
            "storage":{"kind":"embedded", "root_member":ATTACHMENT, "member_paths":[ATTACHMENT]},
            "requirements":null,
        }))
        .unwrap(),
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("native-with-attachment.n4a");
    let receipt = write_n4a_file(
        &path,
        N4aStreamRequest {
            manifest,
            members: vec![N4aStreamInput {
                path: ATTACHMENT,
                source: &GeneratedAttachment,
            }],
            base: Some(N4aStreamBase::Owned(&source)),
        },
        N4aMemoryBudget::bytes(BUDGET_BYTES),
    )
    .unwrap();
    let file = open_n4a_file(&path, N4aMemoryBudget::bytes(BUDGET_BYTES)).unwrap();
    assert_eq!(file.receipt(), &receipt);
    assert_eq!(
        file.receipt().members.len(),
        source.manifest().members.len() + 1
    );
    assert_eq!(
        file.member_location(ATTACHMENT).unwrap().byte_length,
        ATTACHMENT_BYTES
    );
    // This opening budget admits metadata and required owner proofs, but cannot
    // admit the irrelevant member. Inspection must still expose readable training.
    assert!(matches!(
        file.read_member(ATTACHMENT, file.memory_budget()),
        Err(N4aError::Memory(_))
    ));
    let actual = read_n4a_training_view(&file).unwrap();
    assert_eq!(actual.source_id, expected.source_id);
    assert_eq!(actual.outcome, expected.outcome);
    // Exercise the actual n4a-inspect report path and its serialized JSON shape.
    let report: serde_json::Value =
        serde_json::from_str(&n4a_container_view(&file).to_string()).unwrap();
    let readability = &report["readability"];
    assert_eq!(readability["manifest"], json!(file.manifest()));
    assert_eq!(readability["training_view"]["status"], "readable");
    assert_eq!(
        readability["training_view"],
        expected_report["readability"]["training_view"]
    );
    assert_eq!(readability["semantics"]["status"], "report_only");
    assert!(readability["semantics"]["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|artifact| {
            artifact["artifact_id"] == "unrelated-attachment" && artifact["content"] == "opaque"
        }));
    assert_eq!(report["executability"], "not_evaluated");

    // Also catch an unwanted read when the host leaves RAM admission uncapped.
    // Every permitted read still goes through the real file's verifying reader.
    struct TrackedFile(N4aFile);
    impl N4aMemberBytes for TrackedFile {
        fn metadata(&self) -> &N4aMetadata {
            self.0.metadata()
        }
        fn member_bytes(&self, path: &str) -> Result<Cow<'_, [u8]>, N4aError> {
            assert_ne!(
                path, ATTACHMENT,
                "inspection requested an unrelated payload"
            );
            self.0.member_bytes(path)
        }
        fn resident_bytes(&self) -> u64 {
            self.0.resident_bytes()
        }
    }
    let tracked = TrackedFile(open_n4a_file(&path, N4aMemoryBudget::host_allocator()).unwrap());
    assert_eq!(n4a_container_view(&tracked), report);

    // Opaque link IDs must still prevent a training view even though their
    // payloads are not needed for report-only semantic inspection.
    let mut linked = source.manifest().clone();
    linked.links.push(N4aLink {
        id: "foreign-link".into(),
        owner: "test-host".into(),
        relation: "evidence_for".into(),
        source_artifact: "model".into(),
        target_artifact: "model".into(),
    });
    assert!(matches!(
        read_n4a_training_view(&reseal(linked, members(&source))),
        Err(NativeActionError::Owner(
            FacetRefusal::UnsupportedOperation { .. }
        ))
    ));
}

fn record_ids(record: &serde_json::Value) -> Vec<String> {
    serde_json::from_value(record["dataset"]["sample_ids"].clone()).unwrap()
}

/// Reorder every row-aligned field of a real fixture record. Optionally
/// replace the per-row partition labels (already in the new row order).
fn permute_rows(
    record: &serde_json::Value,
    order: &[usize],
    partitions: Option<&[&str]>,
) -> serde_json::Value {
    let pick = |value: &serde_json::Value| {
        serde_json::Value::Array(order.iter().map(|&row| value[row].clone()).collect())
    };
    let mut out = record.clone();
    out["origin_ids"] = pick(&record["origin_ids"]);
    out["fold_ids"] = pick(&record["fold_ids"]);
    out["dataset"]["sample_ids"] = pick(&record["dataset"]["sample_ids"]);
    let source = &record["dataset"]["sources"][0];
    out["dataset"]["sources"][0]["sample_ids"] = pick(&source["sample_ids"]);
    out["dataset"]["sources"][0]["array"]["values"] = pick(&source["array"]["values"]);
    if !record["dataset"]["y"].is_null() {
        out["dataset"]["y"]["values"] = pick(&record["dataset"]["y"]["values"]);
    }
    out["dataset"]["partitions"]["values"] = match partitions {
        Some(labels) => {
            assert_eq!(labels.len(), order.len());
            json!(labels)
        }
        None => pick(&record["dataset"]["partitions"]["values"]),
    };
    out
}

fn predict_partition(source: &OwnedN4a, bytes: &[u8], partition: &str) -> NativePrediction {
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let mut request = action(bytes, "predict-external", &library, &sha);
    request.partition = partition;
    predict_n4a_native(source, request).unwrap()
}

/// The signed replay block as an ID-keyed map, in whatever order it carries.
fn replay_rows(prediction: &NativePrediction) -> (Vec<String>, BTreeMap<String, Vec<f64>>) {
    let block = json!(prediction.replay.outputs[0].predictions[0]);
    let ids: Vec<String> = serde_json::from_value(block["sample_ids"].clone()).unwrap();
    let values: Vec<Vec<f64>> = serde_json::from_value(block["values"].clone()).unwrap();
    assert_eq!(ids.len(), values.len());
    let rows: BTreeMap<_, _> = ids.iter().cloned().zip(values).collect();
    assert_eq!(rows.len(), ids.len(), "replay IDs must be unique");
    (ids, rows)
}

#[test]
fn prediction_sample_ids_follow_permuted_io_record_order() {
    let source = source(false, false, "f1:order");
    let natural = record("future", 5, false, false);
    // Neither sorted, reverse sorted, nor the fixture's own order.
    let permuted = permute_rows(&natural, &[2, 0, 4, 1, 3], None);
    let expected = record_ids(&permuted);
    let mut sorted = expected.clone();
    sorted.sort();
    let mut reversed = sorted.clone();
    reversed.reverse();
    assert_ne!(expected, sorted);
    assert_ne!(expected, reversed);
    assert_ne!(expected, record_ids(&natural));

    let natural_bytes = serde_json::to_vec(&natural).unwrap();
    let permuted_bytes = serde_json::to_vec(&permuted).unwrap();
    let first = predict_partition(&source, &natural_bytes, "predict");
    let second = predict_partition(&source, &permuted_bytes, "predict");

    // Exact IO physical order of each record, not the replay's order.
    assert_eq!(first.sample_ids, record_ids(&natural));
    assert_eq!(second.sample_ids, expected);
    // The signed replay is untouched: it covers exactly these IDs, and each
    // ID keeps its prediction whatever the physical row position was.
    let (second_ids, second_rows) = replay_rows(&second);
    let (_, first_rows) = replay_rows(&first);
    assert_eq!(
        second_ids.iter().collect::<BTreeSet<_>>(),
        second.sample_ids.iter().collect::<BTreeSet<_>>()
    );
    assert_eq!(first_rows.len(), second_rows.len());
    for (id, values) in &first_rows {
        let other = &second_rows[id];
        assert_eq!(values.len(), other.len());
        for (a, b) in values.iter().zip(other) {
            assert!((a - b).abs() <= 1e-12, "{id}: {a} vs {b}");
        }
    }
}

#[test]
fn prediction_sample_ids_cover_only_the_selected_partition() {
    let source = source(false, false, "f1:partitions");
    let natural = record("multi", 9, false, false);
    // Valid non-training partitions interleaved over permuted physical rows.
    let labels = [
        "predict", "test", "predict", "predict", "test", "predict", "test", "predict", "predict",
    ];
    let record = permute_rows(&natural, &[4, 1, 7, 0, 8, 2, 6, 3, 5], Some(&labels));
    let all = record_ids(&record);
    let bytes = serde_json::to_vec(&record).unwrap();
    let mut seen = BTreeSet::new();
    for partition in ["predict", "test"] {
        // The expected subset comes from this fixture's own rows, in order.
        let expected: Vec<String> = all
            .iter()
            .zip(labels)
            .filter(|(_, label)| *label == partition)
            .map(|(id, _)| id.clone())
            .collect();
        assert!(!expected.is_empty() && expected.len() < all.len());
        let prediction = predict_partition(&source, &bytes, partition);
        assert_eq!(prediction.sample_ids, expected, "{partition}");
        // Exactly the replay block's ID set, and no other record ID.
        let (ids, rows) = replay_rows(&prediction);
        assert_eq!(rows.len(), expected.len());
        assert_eq!(
            ids.iter().collect::<BTreeSet<_>>(),
            expected.iter().collect::<BTreeSet<_>>()
        );
        for id in &prediction.sample_ids {
            assert!(seen.insert(id.clone()), "{id} selected by both partitions");
        }
    }
    assert_eq!(seen, all.into_iter().collect::<BTreeSet<_>>());
}

/// Per-ID values of the selected output's unweighted OOF average.
fn cohort_oof_rows(outcome: &dag_ml_core::TrainingOutcome) -> BTreeMap<String, Vec<f64>> {
    let outputs: Vec<_> = outcome
        .outputs
        .iter()
        .filter(|output| output.binding.binding_id == outcome.selection_output_id)
        .collect();
    let [output] = outputs.as_slice() else {
        panic!("expected exactly one selected output");
    };
    // The owner stores selected-variant averages here, including both avg and
    // w_avg. Bind the exact output and selected-variant score report first.
    let averages: Vec<_> = outcome
        .oof_averages
        .iter()
        .filter(|average| {
            let block = &average.predictions;
            block.producer_node == output.binding.node_id
                && block.producer_port.as_deref() == Some(output.binding.port_name.as_str())
                && block.target_names == output.binding.target_names
                && block.fold_id.as_ref().map(|id| id.as_str()) == Some("avg")
                && outcome.score_set.reports.iter().any(|report| {
                    report.variant_id.as_ref() == Some(&outcome.selected_variant_id)
                        && report.producer_node == block.producer_node
                        && report.producer_port == block.producer_port
                        && report.partition == block.partition
                        && report.fold_id == block.fold_id
                        && report.level == block.level
                })
        })
        .collect();
    let [average] = averages.as_slice() else {
        panic!(
            "expected exactly one selected-output avg block with selected-variant score evidence"
        );
    };
    let mut rows = BTreeMap::new();
    let block = json!(average.predictions);
    let ids = block["unit_ids"].as_array().unwrap();
    let values: Vec<Vec<f64>> = serde_json::from_value(block["values"].clone()).unwrap();
    assert_eq!(ids.len(), values.len());
    for (unit, row) in ids.iter().zip(values) {
        assert_eq!(unit["level"], "sample");
        assert!(rows
            .insert(unit["id"].as_str().unwrap().into(), row)
            .is_none());
    }
    rows
}

fn assert_cohort_rows_equal(a: &BTreeMap<String, Vec<f64>>, b: &BTreeMap<String, Vec<f64>>) {
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (id, values) in a {
        assert_eq!(values.len(), b[id].len());
        for (a, b) in values.iter().zip(&b[id]) {
            // Existing same-engine tolerance in this target.
            assert!((a - b).abs() <= 1e-12, "{id}: {a} vs {b}");
        }
    }
}

fn mixed_cohort_cold_replay(directory: &Path) {
    let archive = std::fs::read(directory.join("mixed.n4a")).unwrap();
    let source =
        open_owned_n4a_bytes(archive.into_boxed_slice(), N4aMemoryBudget::default()).unwrap();
    let bytes = std::fs::read(directory.join("record.json")).unwrap();
    assert_eq!(root(&source, "training-data"), bytes);
    let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let expected: Vec<String> = record_ids(&record)
        .into_iter()
        .zip(
            record["dataset"]["partitions"]["values"]
                .as_array()
                .unwrap(),
        )
        .filter(|(_, partition)| *partition == "test")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(expected.len(), 8);
    let reference: BTreeMap<String, Vec<f64>> =
        serde_json::from_slice(&std::fs::read(directory.join("reference.json")).unwrap()).unwrap();
    let before = states(&source);
    let archive_before = copy_n4a_bytes(&source);
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    // This child has never fitted a model. Both calls must load the frozen fit.
    for stored in [false, true] {
        let mut request = action(
            &bytes,
            if stored {
                "predict"
            } else {
                "predict-external"
            },
            &library,
            &sha,
        );
        request.source_id = "nir";
        request.partition = "test";
        if stored {
            request.input = NativeInput::Stored;
        }
        let predicted = predict_n4a_native(&source, request).unwrap();
        assert_eq!(predicted.sample_ids, expected);
        let (ids, rows) = replay_rows(&predicted);
        assert_eq!(
            ids.into_iter().collect::<BTreeSet<_>>(),
            expected.iter().cloned().collect()
        );
        assert_cohort_rows_equal(&rows, &reference);
        assert!(!predicted.replay.lineage.is_empty());
        assert!(predicted
            .replay
            .lineage
            .iter()
            .all(|entry| entry.phase == dag_ml_core::Phase::Predict));
        assert_eq!(states(&source), before);
        assert_eq!(copy_n4a_bytes(&source), archive_before);
    }
}

#[test]
fn mixed_cohort_matches_train_only_and_cold_test_replay() {
    const CHILD_DIRECTORY: &str = "N4A_MIXED_COHORT_COLD_DIRECTORY";
    if let Some(directory) = std::env::var_os(CHILD_DIRECTORY) {
        mixed_cohort_cold_replay(Path::new(&directory));
        return;
    }
    let record = interleaved_record();
    let train = training_cohort(&record);
    // Preserve the exact supplied bytes, including formatting, in the anchor.
    let bytes = format!("\n{}\n", serde_json::to_string(&record).unwrap()).into_bytes();
    let train_bytes = serde_json::to_vec(&train).unwrap();
    let library = library();
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap();
    let recipe = serde_json::from_value(recipe_value(false)).unwrap();
    let produce = |record, label| {
        produce_n4a_native_with_view(NativeProduceRequest {
            record,
            source_id: "nir",
            recipe: &recipe,
            runtime: NativeRuntime {
                library: &library,
                expected_sha256: &sha,
            },
            run_id: label,
            container_id: label,
            embed_training_record: true,
            memory_budget: N4aMemoryBudget::default(),
        })
        .unwrap()
    };
    let mixed = produce(&bytes, "cohort:mixed");
    let only_train = produce(&train_bytes, "cohort:train");
    assert_eq!(root(&mixed.container, "training-data"), bytes);
    let a = read_n4a_training_view(&mixed.container).unwrap();
    let b = read_n4a_training_view(&only_train.container).unwrap();
    assert_eq!(a.outcome, mixed.training.outcome);
    assert_eq!(b.outcome, only_train.training.outcome);
    let a: dag_ml_core::TrainingOutcome = dag_ml_core::TrainingOutcome::from_json(
        std::str::from_utf8(root(&mixed.container, "result")).unwrap(),
    )
    .unwrap();
    let b: dag_ml_core::TrainingOutcome = dag_ml_core::TrainingOutcome::from_json(
        std::str::from_utf8(root(&only_train.container, "result")).unwrap(),
    )
    .unwrap();
    assert_eq!(a.effective_plan.fold_set, b.effective_plan.fold_set);
    assert_eq!(a.selected_variant_id, b.selected_variant_id);
    assert_eq!(json!(a.refit)["status"], "completed");
    assert_eq!(json!(b.refit)["status"], "completed");
    let selections = json!(a.execution_bundle.selections);
    let oracle = json!(b.execution_bundle.selections);
    assert!(!selections.as_object().unwrap().is_empty());
    for (id, selection) in selections.as_object().unwrap() {
        assert_eq!(selection["evaluation_scope"], "oof");
        assert_eq!(
            selection["selected_candidate_id"],
            oracle[id]["selected_candidate_id"]
        );
        assert!(
            (selection["selected_score"].as_f64().unwrap()
                - oracle[id]["selected_score"].as_f64().unwrap())
            .abs()
                <= 1e-12
        );
    }
    let a_rows = cohort_oof_rows(&a);
    let b_rows = cohort_oof_rows(&b);
    assert_eq!(a_rows.len(), 32);
    assert_cohort_rows_equal(&a_rows, &b_rows);
    assert_eq!(
        a_rows.keys().cloned().collect::<BTreeSet<_>>(),
        record_ids(&train).into_iter().collect()
    );
    let predict = |source| {
        let mut request = action(&bytes, "predict-external", &library, &sha);
        request.source_id = "nir";
        request.partition = "test";
        predict_n4a_native(source, request).unwrap()
    };
    let mixed_test = predict(&mixed.container);
    let train_test = predict(&only_train.container);
    assert_eq!(mixed_test.sample_ids.len(), 8);
    let (_, reference) = replay_rows(&mixed_test);
    assert_cohort_rows_equal(&reference, &replay_rows(&train_test).1);

    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("mixed.n4a"),
        copy_n4a_bytes(&mixed.container),
    )
    .unwrap();
    std::fs::write(directory.path().join("record.json"), &bytes).unwrap();
    std::fs::write(
        directory.path().join("reference.json"),
        serde_json::to_vec(&reference).unwrap(),
    )
    .unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mixed_cohort_matches_train_only_and_cold_test_replay",
            "--nocapture",
        ])
        .env(CHILD_DIRECTORY, directory.path())
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "cold replay: {}\n{}\n{}",
        child.status,
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );

    // Mixed anchors keep the same descriptor and schema tamper refusals.
    let mut payloads = members(&mixed.container);
    let mut altered = record.clone();
    altered["dataset"]["y"]["values"][0][0] = json!(-12345.0);
    payloads.insert(
        "io/training.json".into(),
        serde_json::to_vec(&altered).unwrap(),
    );
    let forged = reseal(mixed.container.manifest().clone(), payloads);
    let mut request = action(&bytes, "predict-external", &library, &sha);
    request.source_id = "nir";
    request.partition = "test";
    assert!(predict_n4a_native(&forged, request).is_err());
    altered = record;
    altered["dataset"]["sources"][0]["axis_units"] = json!({"wavelength":"cm-1"});
    let altered_bytes = serde_json::to_vec(&altered).unwrap();
    let mut request = action(&altered_bytes, "predict-external", &library, &sha);
    request.source_id = "nir";
    request.partition = "test";
    assert!(predict_n4a_native(&mixed.container, request).is_err());
}
