//! Adversarial owner artifacts are trained with real IO and owner execution.
//! This private unit module exports no test bridge or caller admission API.
use super::*;
use crate::native_action_test_data as data;
use crate::pipeline_workflow::native_pipeline_export_training_request;
use dag_ml_core::{BundleId, FittedArtifactMode, TrainingOutcome, TrainingRequest};
const RELATIONS: &str = "native_pipeline_training_relations";

fn library() -> (std::path::PathBuf, String) {
    let path = std::path::PathBuf::from(
        std::env::var_os("N4M_LIBRARY_PATH").expect("controller runtime required"),
    );
    assert_eq!(
        std::env::var_os("N4M_LIB_PATH").as_deref(),
        Some(path.as_os_str())
    );
    let sha = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").expect("controller digest required");
    assert_eq!(sha256(&std::fs::read(&path).unwrap()), sha);
    (path, sha)
}
fn trained(edit: impl FnOnce(&mut TrainingRequest)) -> TrainingOutcome {
    let (library, sha) = library();
    configure_attested_methods_runtime(&library, &sha).unwrap();
    let recipe = serde_json::from_value(data::recipe_value(true)).unwrap();
    let (dataset, mut request) = native_pipeline_export_training_request(
        &data::record("train", 18, true, true),
        "spectra",
        &recipe,
        &library,
    )
    .unwrap();
    edit(&mut request);
    request.request_fingerprint = request.compute_fingerprint().unwrap();
    request.validate().unwrap();
    let provider = DatasetPackageMethodsProvider::new(&dataset, "spectra").unwrap();
    crate::io_training::execute_dataset_package_methods_training(
        &provider,
        &request,
        "outcome:authority",
        RunId::new("run:authority").unwrap(),
        BundleId::new("bundle:authority").unwrap(),
        &library,
        &mut dag_ml_core::InMemoryArtifactStore::new(),
    )
    .unwrap()
}
fn seal(outcome: &TrainingOutcome) -> OwnedN4a {
    let package = outcome
        .to_portable_predictor_package(
            "package:authority",
            FittedArtifactMode::PortableRequired,
            ArtifactLoadMode::NativePortable,
        )
        .unwrap();
    let facets = build_training_artifact_facets(outcome, &package).unwrap();
    let empty = ArtifactFacetExport {
        graph: ArtifactFacetGraph::default(),
        members: BTreeMap::new(),
        context: FacetValidationContext::default(),
    };
    compose(
        None,
        &empty,
        facets,
        "authority",
        N4aMemoryBudget::default(),
    )
    .unwrap()
}
fn request(bytes: &[u8]) -> NativeActionRequest<'_> {
    NativeActionRequest {
        entrypoint: "calibrate-external",
        input: NativeInput::External(bytes),
        source_id: "spectra",
        partition: "predict",
        schema_policy: NativeSchemaPolicy::RequireTrainingSchema,
        request_id: "authority",
        run_id: "authority",
        memory_budget: N4aMemoryBudget::default(),
        runtime: NativeRuntime {
            library: Path::new("/missing/stage-d/libn4m.so"),
            expected_sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        },
    }
}
fn static_error(source: &OwnedN4a) -> NativeActionError {
    let bytes = serde_json::to_vec(&data::record("external", 12, false, true)).unwrap();
    match prepare(source, &request(&bytes), true) {
        Err(error) => error,
        Ok(_) => panic!("static refusal required"),
    }
}

#[test]
fn absent_audit_inference_does_not_recover_but_calibration_requires_capability() {
    let outcome = trained(|r| {
        r.campaign.metadata.remove(RELATIONS);
    });
    let source = seal(&outcome);
    assert!(matches!(
        static_error(&source),
        NativeActionError::Owner(FacetRefusal::MissingCapability { .. })
    ));
    let bytes = serde_json::to_vec(&data::record("train", 5, false, false)).unwrap();
    let mut req = request(&bytes);
    req.entrypoint = "predict-external";
    let prepared = prepare(&source, &req, false).unwrap();
    assert!(prepared.receipt.relation_authority.is_none());
    assert!(prepared.receipt.observation_id_disjointness.is_none());
    // Genuine native prediction still succeeds on this current-owner artifact.
    let (library, sha) = library();
    req.runtime = NativeRuntime {
        library: &library,
        expected_sha256: &sha,
    };
    predict_n4a_native(&source, req).unwrap();
}

#[test]
fn audit_version_is_checked_before_unknown_fields_and_wrong_types() {
    let future = seal(&trained(|r| {
        let audit = r.campaign.metadata.get_mut(RELATIONS).unwrap();
        audit["schema_version"] = json!(2);
        audit["future_field"] = json!(true);
    }));
    let error = static_error(&future);
    assert!(
        matches!(
            error,
            NativeActionError::Owner(FacetRefusal::MissingCapability { .. })
        ),
        "future audit with unknown field: unexpected refusal: {error:?}"
    );
    // Serde accepts a sequence for this struct, defaulting its missing records.
    let empty: dag_ml_core::SampleRelationSet = serde_json::from_value(json!([])).unwrap();
    assert!(empty.records.is_empty());
    empty.validate().unwrap();
    for kind in 0..4 {
        let source = seal(&trained(|r| {
            let audit = r.campaign.metadata.get_mut(RELATIONS).unwrap();
            match kind {
                0 => audit["unknown"] = json!(true),
                1 => audit["schema_version"] = json!("1"),
                2 => audit["relations"] = json!([]),
                _ => audit["relations"] = json!(false),
            }
        }));
        let error = static_error(&source);
        let expected = match kind {
            0 | 3 => matches!(&error, NativeActionError::Owner(FacetRefusal::Json { .. })),
            1 => matches!(
                &error,
                NativeActionError::Owner(FacetRefusal::InvalidPayload { .. })
            ),
            2 => matches!(
                &error,
                NativeActionError::Owner(FacetRefusal::Mismatch { detail })
                    if detail == "retained IO relations differ from owner identity/influence/binding/source"
            ),
            _ => unreachable!(),
        };
        assert!(
            expected,
            "audit kind {kind} (0: unknown field, 1: string version, 2: empty relations sequence, 3: boolean relations): unexpected refusal: {error:?}"
        );
    }
}

#[test]
fn old_relation_hash_and_rewritten_metadata_never_supply_authority() {
    for partial in [false, true] {
        let source = seal(&trained(|r| {
            let audit = r.campaign.metadata.get_mut(RELATIONS).unwrap();
            audit["relations"]["records"][0]["observation_id"] = json!("edited-observation");
            if partial {
                audit["source_id"] = json!("other-source");
            }
        }));
        assert!(matches!(
            static_error(&source),
            NativeActionError::Owner(FacetRefusal::Mismatch { .. })
        ));
    }
}

#[test]
fn stale_influence_entry_is_checked_by_owner_closure_after_rehash() {
    let mut outcome = trained(|_| {});
    let entry = outcome.training_influence.entries.first_mut().unwrap();
    entry
        .origin_sample_ids
        .push(SampleId::new("stale-origin").unwrap());
    entry.origin_sample_ids.sort();
    entry.origin_sample_ids.dedup();
    outcome.training_influence.manifest_fingerprint =
        outcome.training_influence.compute_fingerprint().unwrap();
    outcome.outcome_fingerprint = outcome.compute_fingerprint().unwrap();
    // If the owner's general validation refuses earlier, that is also a
    // closed owner refusal; no partial forgery is relaxed for the action.
    if outcome.validate().is_err() {
        return;
    }
    let source = seal(&outcome);
    assert!(matches!(
        static_error(&source),
        NativeActionError::Owner(FacetRefusal::OwnerValidation { .. })
    ));
}

#[test]
fn c1_static_owner_authority_rejects_unrelated_coordinator_and_circular_string() {
    let outcome = trained(|_| {});
    let source = seal(&outcome);
    let bytes = serde_json::to_vec(&data::record("external", 12, false, true)).unwrap();
    let prepared = prepare(&source, &request(&bytes), true).unwrap();
    let package = prepared.snapshot.context.owner_proofs[&prepared.root_id]
        .predictor_package()
        .unwrap();
    let requirement = &package.execution_bundle.data_requirements[0];
    let training = recover_native_pipeline_training_relations(&outcome, package).unwrap();
    let canonical_training = training.fingerprint().unwrap();
    assert_eq!(
        prepared.receipt.envelope.relation_fingerprint,
        requirement.relation_fingerprint
    );
    assert_eq!(
        prepared.receipt.envelope.coordinator_relations.as_ref(),
        Some(&training)
    );
    assert_ne!(
        canonical_training,
        prepared.relations.fingerprint().unwrap()
    );
    prepared
        .receipt
        .envelope
        .validate_external_test_training_authority(
            requirement.relation_fingerprint.as_deref(),
            Some(canonical_training.as_str()),
            package.effective_plan.fold_set.as_ref(),
        )
        .unwrap();

    // Keep S and all sample/fold/group/origin identities; only the retained
    // record weight changes. Independent C must still reject the rewritten set.
    let mut changed = prepared.receipt.envelope.clone();
    let records = &mut changed.coordinator_relations.as_mut().unwrap().records;
    let old_weight = records[0].sample_influence_weight;
    records[0].sample_influence_weight = if old_weight == Some(0.5) {
        Some(0.25)
    } else {
        Some(0.5)
    };
    changed.validate().unwrap();
    assert_ne!(
        changed
            .coordinator_relations
            .as_ref()
            .unwrap()
            .fingerprint()
            .unwrap(),
        canonical_training
    );
    let error = changed
        .validate_external_test_training_authority(
            requirement.relation_fingerprint.as_deref(),
            Some(canonical_training.as_str()),
            package.effective_plan.fold_set.as_ref(),
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("training relations do not match owner authority"));

    for own_string in [false, true] {
        let mut envelope = prepared.receipt.envelope.clone();
        envelope.coordinator_relations = Some(prepared.relations.clone());
        if own_string {
            envelope.relation_fingerprint = Some(prepared.relations.fingerprint().unwrap());
        }
        assert!(envelope
            .validate_external_test_training_authority(
                requirement.relation_fingerprint.as_deref(),
                Some(canonical_training.as_str()),
                package.effective_plan.fold_set.as_ref()
            )
            .is_err());
    }
}

#[test]
fn relation_universe_coverage_both_policies_and_observation_alias() {
    let outcome = trained(|_| {});
    let package = outcome
        .to_portable_predictor_package(
            "package:authority",
            FittedArtifactMode::PortableRequired,
            ArtifactLoadMode::NativePortable,
        )
        .unwrap();
    let relations = recover_native_pipeline_training_relations(&outcome, &package).unwrap();
    let folds = package.effective_plan.fold_set.as_ref().unwrap();
    assert!(folds
        .sample_ids
        .iter()
        .all(|id| relations.records.iter().any(|r| &r.sample_id == id)));
    let mut incomplete = relations.clone();
    incomplete.records.pop();
    assert!(package
        .training_influence
        .validate_identity_closure(&incomplete)
        .is_err());
    let mut external = relations.clone();
    for (i, row) in external.records.iter_mut().enumerate() {
        row.sample_id = SampleId::new(format!("external-{i}")).unwrap();
        row.origin_sample_id = None;
    }
    // Different sample IDs still overlap the recorded observation universe.
    assert!(validate_disjoint_calibration_relations(&relations, &external).is_err());
    let mut changed = package.clone();
    changed
        .effective_plan
        .campaign
        .split_invocation
        .as_mut()
        .unwrap()
        .leakage_policy
        .require_group_ids = true;
    // Plan mutation cannot evade owner validation, even if the campaign policy
    // itself would allow the ungrouped relation universe.
    assert!(recover_native_pipeline_training_relations(&outcome, &changed).is_err());
}

#[test]
fn float_relation_weight_keeps_fingerprint_over_cold_owner_json_roundtrip() {
    let outcome = trained(|_| {});
    let package = outcome
        .to_portable_predictor_package(
            "package:authority",
            FittedArtifactMode::PortableRequired,
            ArtifactLoadMode::NativePortable,
        )
        .unwrap();
    let mut relations = recover_native_pipeline_training_relations(&outcome, &package).unwrap();
    relations.records[0].sample_influence_weight = Some(0.12345678901234566);
    let before = relations.fingerprint().unwrap();
    let bytes = serde_json::to_vec(&relations).unwrap();
    let after: SampleRelationSet = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(before, after.fingerprint().unwrap());
}

#[test]
fn missing_schema_anchor_predicts_with_explicit_unavailable_and_calibration_refuses() {
    let source = seal(&trained(|r| {
        r.campaign.metadata.remove("raw_source_schema");
    }));
    assert!(matches!(
        static_error(&source),
        NativeActionError::Owner(FacetRefusal::MissingCapability { .. })
    ));
    let bytes = serde_json::to_vec(&data::record("external", 12, false, false)).unwrap();
    let mut request = request(&bytes);
    request.entrypoint = "predict-external";
    assert!(prepare(&source, &request, false).is_err());
    request.schema_policy = NativeSchemaPolicy::AllowUnavailable;
    let prepared = prepare(&source, &request, false).unwrap();
    assert_eq!(prepared.receipt.schema_validation, "unavailable");
}

#[test]
fn origin_overlap_in_both_directions_and_augmented_rows_refuse_owner_independence() {
    let outcome = trained(|_| {});
    let package = outcome
        .to_portable_predictor_package(
            "package:authority",
            FittedArtifactMode::PortableRequired,
            ArtifactLoadMode::NativePortable,
        )
        .unwrap();
    let relations = recover_native_pipeline_training_relations(&outcome, &package).unwrap();
    for kind in 0..4 {
        let mut external = relations.clone();
        for (i, r) in external.records.iter_mut().enumerate() {
            r.sample_id = SampleId::new(format!("external-{i}")).unwrap();
            r.observation_id = dag_ml_core::ObservationId::new(format!("observation-{i}")).unwrap();
            r.origin_sample_id = None;
        }
        let mut training = relations.clone();
        match kind {
            0 => external.records[0].origin_sample_id = Some(training.records[0].sample_id.clone()),
            1 => training.records[0].origin_sample_id = Some(external.records[0].sample_id.clone()),
            2 => external.records[0].is_augmented = true,
            _ => {
                external.records[0].group_id =
                    Some(dag_ml_core::GroupId::new("declared-group").unwrap())
            }
        }
        if kind < 2 {
            assert!(validate_disjoint_calibration_relations(&training, &external).is_err());
        } else if kind == 2 {
            assert!(
                dag_ml_core::public_robustness::validate_independent_uncertainty_cohort(
                    &package, &external
                )
                .is_err()
            );
        } else {
            // The public record gate refuses all declared groups, even groups
            // absent from influence; it cannot turn them into a positive claim.
            let mut raw = data::record("external", 12, false, true);
            raw["dataset"]["groups"] = json!({"dtype":"<U2","shape":[12],"values":vec!["g";12]});
            assert!(validate_independent_record(&normalized_record(&raw).unwrap()).is_err());
        }
    }
}

#[test]
fn self_consistent_renaming_has_no_authenticity_or_content_disjointness_claim() {
    let (library, sha) = library();
    let recipe = serde_json::from_value(data::recipe_value(true)).unwrap();
    // Identical spectra/targets to `train`, with all training IDs renamed by a
    // real producer. Unkeyed fingerprints cannot authenticate those names.
    let bytes = serde_json::to_vec(&data::record("renamed", 18, true, true)).unwrap();
    let source = produce_n4a_native(NativeProduceRequest {
        record: &bytes,
        source_id: "spectra",
        recipe: &recipe,
        runtime: NativeRuntime {
            library: &library,
            expected_sha256: &sha,
        },
        run_id: "renamed",
        container_id: "renamed",
        embed_training_record: false,
        memory_budget: N4aMemoryBudget::default(),
    })
    .unwrap();
    let same_content_new_ids = serde_json::to_vec(&data::record("train", 18, false, true)).unwrap();
    let prepared = prepare(&source, &request(&same_content_new_ids), true).unwrap();
    assert_eq!(prepared.receipt.authenticity, "not_established");
    assert_eq!(
        prepared.receipt.observation_content_disjointness.as_deref(),
        Some("unavailable")
    );
    assert_eq!(
        prepared.receipt.observation_id_disjointness.as_deref(),
        Some("checked_against_recorded_universe")
    );
}

#[test]
fn foreign_reserved_metadata_is_opaque_and_cannot_supply_training_authority() {
    let source = seal(&trained(|r| {
        r.campaign.metadata.remove(RELATIONS);
    }));
    let mut manifest = source.manifest().clone();
    let bytes =
        serde_json::to_vec(&json!({RELATIONS:{"schema_version":1,"relations":{"records":[]}}}))
            .unwrap();
    manifest.artifacts.push(N4aArtifact {
        id: "foreign-audit".into(),
        kind: ArtifactKind::Attachment,
        owner: "foreign".into(),
        content_type: ContentType {
            id: "foreign.audit".into(),
            version: 1,
        },
        identity: crate::n4a::OwnerIdentity {
            owner: "foreign".into(),
            schema: "foreign.audit".into(),
            schema_version: 1,
            fingerprint_profile: "sha256.raw.v1".into(),
            fingerprint: sha256(&bytes),
        },
        storage: N4aStorage::Embedded {
            root_member: "foreign.json".into(),
            member_paths: vec!["foreign.json".into()],
        },
        requirements: None,
    });
    manifest.members.push(N4aMember {
        path: "foreign.json".into(),
        byte_length: bytes.len() as u64,
        sha256: sha256(&bytes),
    });
    let mut members: Vec<_> = source
        .manifest()
        .members
        .iter()
        .map(|m| N4aMemberInput {
            path: &m.path,
            bytes: source.member(&m.path).unwrap().bytes,
        })
        .collect();
    members.push(N4aMemberInput {
        path: "foreign.json",
        bytes: &bytes,
    });
    let (_, bytes) = write_n4a_bytes(
        N4aWriteRequest { manifest, members },
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let foreign =
        open_owned_n4a_bytes(bytes.into_boxed_slice(), N4aMemoryBudget::default()).unwrap();
    assert!(matches!(
        static_error(&foreign),
        NativeActionError::Owner(FacetRefusal::MissingCapability { .. })
    ));
}
