//! Semantic boundary regressions over the unchanged genuine canonical corpus.
//! Rewrites below are synthetic negative/lexical controls, never new fixtures.
use std::collections::BTreeMap;
use std::path::Path;

use dag_ml_core::artifact_facets::{
    validate_owner_facet_bytes, FacetContentType, FacetRefusal, FacetSemanticStatus,
};
use nirs4all::n4a::*;
use nirs4all::n4a_owner::*;
use sha2::{Digest, Sha256};

const SHAPES: [&str; 6] = ["recipe", "dataset", "model", "score", "result", "combined"];

struct Fixture {
    manifest: N4aManifest,
    members: BTreeMap<String, Vec<u8>>,
}

impl Fixture {
    fn load(shape: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/parity/fixtures/n4a_v1")
            .join(shape);
        let manifest: N4aManifest =
            serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        let members = manifest
            .members
            .iter()
            .map(|member| {
                (
                    member.path.clone(),
                    std::fs::read(root.join(&member.path)).unwrap(),
                )
            })
            .collect();
        Self { manifest, members }
    }

    fn bytes(&mut self) -> Vec<u8> {
        // Recalculate only physical inventory: owner identities are NOT repaired.
        self.manifest.members = self
            .members
            .iter()
            .map(|(path, bytes)| N4aMember {
                path: path.clone(),
                byte_length: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
            })
            .collect();
        write_n4a_bytes(
            N4aWriteRequest {
                manifest: self.manifest.clone(),
                members: self
                    .members
                    .iter()
                    .map(|(path, bytes)| N4aMemberInput { path, bytes })
                    .collect(),
            },
            N4aMemoryBudget::default(),
        )
        .unwrap()
        .1
    }

    fn inspect(&mut self) -> Result<N4aSemanticInspection, FacetRefusal> {
        let bytes = self.bytes();
        // Every semantic counterexample first passes the real physical reader.
        let container = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
        inspect_n4a_semantics(&container)
    }

    fn artifact(&mut self, id: &str) -> &mut N4aArtifact {
        self.manifest
            .artifacts
            .iter_mut()
            .find(|a| a.id == id)
            .unwrap()
    }

    fn recipe_only() -> Self {
        let mut fixture = Self::load("recipe");
        fixture.manifest.artifacts.retain(|a| a.id == "recipe");
        fixture.manifest.links.clear();
        fixture.manifest.entrypoints.clear();
        fixture
    }

    fn add_opaque(&mut self, owner: &str, content: &str) {
        let mut artifact = self.manifest.artifacts[0].clone();
        artifact.id = "foreign".into();
        artifact.kind = ArtifactKind::Dataset;
        artifact.owner = owner.into();
        artifact.identity.owner = owner.into();
        artifact.content_type.id = content.into();
        artifact.storage = N4aStorage::Embedded {
            root_member: "foreign/opaque.bin".into(),
            member_paths: vec!["foreign/opaque.bin".into()],
        };
        artifact.requirements = None;
        self.manifest.artifacts.push(artifact);
        self.members
            .insert("foreign/opaque.bin".into(), b"\xffopaque\0".to_vec());
    }
}

fn content<'a>(report: &'a N4aSemanticInspection, id: &str) -> &'a OwnerContentInspection {
    &report
        .artifacts
        .iter()
        .find(|a| a.artifact_id == id)
        .unwrap()
        .content
}

#[test]
fn file_inspection_charges_the_live_root_and_member_map_together() {
    let mut fixture = Fixture::recipe_only();
    // Whitespace preserves the genuine owner's identity while making the two
    // simultaneously live root buffers larger than the declared budget.
    fixture
        .members
        .get_mut("dagml/recipe.json")
        .unwrap()
        .resize(128 * 1024, b' ');
    fixture.add_opaque("foreign-owner", "foreign.attachment");
    fixture
        .members
        .get_mut("foreign/opaque.bin")
        .unwrap()
        .resize(1024 * 1024, 0xff);
    let bytes = fixture.bytes();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owner-budget.n4a");
    std::fs::write(&path, &bytes).unwrap();
    let budget = N4aMemoryBudget::bytes(224 * 1024);
    let file = nirs4all::n4a_file::open_n4a_file(&path, budget).unwrap();
    assert_eq!(
        file.read_member("dagml/recipe.json", budget).unwrap().len(),
        128 * 1024
    );
    let error = inspect_n4a_semantics(&file).unwrap_err();
    assert!(
        matches!(
            error,
            FacetRefusal::InvalidStructure { ref detail }
                if detail.contains("working bytes exceed the authorized")
        ),
        "{error}"
    );
    // The opaque MiB is not materialized or charged by inspection.
    for budget in [
        N4aMemoryBudget::bytes(384 * 1024),
        N4aMemoryBudget::default(),
    ] {
        let file = nirs4all::n4a_file::open_n4a_file(&path, budget).unwrap();
        let report = inspect_n4a_semantics(&file).unwrap();
        assert!(matches!(
            content(&report, "recipe"),
            OwnerContentInspection::Validated { .. }
        ));
        assert!(matches!(
            content(&report, "foreign"),
            OwnerContentInspection::Opaque
        ));
    }
}

#[test]
fn genuine_six_shapes_use_exact_bytes_for_owned_and_borrowed_inspection() {
    let provenance: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/parity/fixtures/n4a_v1/PROVENANCE.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for shape in SHAPES {
        let mut fixture = Fixture::load(shape);
        let bytes = fixture.bytes();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            provenance["containers"][shape]["sha256"]
        );
        let borrowed = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
        let owned =
            open_owned_n4a_bytes(bytes.clone().into_boxed_slice(), N4aMemoryBudget::default())
                .unwrap();
        let index = owned.index() as *const ValidatedIndex;
        let report = inspect_n4a_semantics(&borrowed).unwrap();
        let owned_report = inspect_n4a_semantics(&owned).unwrap();
        assert_eq!(format!("{report:?}"), format!("{owned_report:?}"));
        assert_eq!(report.artifacts.len(), fixture.manifest.artifacts.len());
        assert_eq!(report.links.len(), fixture.manifest.links.len());
        assert!(report.opaque_link_ids.is_empty());
        for artifact in &fixture.manifest.artifacts {
            match (&artifact.storage, artifact.owner.as_str()) {
                (N4aStorage::Embedded { .. }, "dag-ml") => assert!(matches!(
                    content(&report, &artifact.id),
                    OwnerContentInspection::Validated { .. }
                )),
                (_, "dag-ml") => assert!(matches!(
                    content(&report, &artifact.id),
                    OwnerContentInspection::Unresolved
                )),
                _ => assert!(matches!(
                    content(&report, &artifact.id),
                    OwnerContentInspection::Opaque
                )),
            }
        }
        for (path, original) in &fixture.members {
            let view = borrowed.member(path).unwrap();
            assert_eq!(view.bytes, original);
            assert_eq!(view.bytes.as_ptr(), bytes[view.range.clone()].as_ptr());
            assert_eq!(owned.member(path).unwrap().bytes, original);
        }
        assert_eq!(owned.index() as *const ValidatedIndex, index);
        assert_eq!(copy_n4a_bytes(&owned), bytes);
        assert_eq!(owned.inspect().semantics, SemanticStatus::Unverified);
        for entry in &fixture.manifest.entrypoints {
            assert_eq!(
                owned.require_operation(&entry.id, None).unwrap_err(),
                OperationRefusal::AbsentAdmission
            );
        }
        if shape == "combined" || shape == "model" {
            let OwnerContentInspection::Validated {
                state_members,
                model_states,
            } = content(&report, "model")
            else {
                panic!("validated model required")
            };
            assert_eq!(state_members.len(), 1);
            assert_eq!(model_states.len(), 1);
            assert!(model_states
                .iter()
                .all(|s| s.verdict.status == FacetSemanticStatus::Unverified
                    && s.native_descriptor.is_none()));
        }
        if shape == "combined" {
            for id in ["link:dataset", "link:10"] {
                assert_eq!(
                    report
                        .links
                        .iter()
                        .find(|l| l.link_id == id)
                        .unwrap()
                        .verdict
                        .status,
                    FacetSemanticStatus::Unverified
                );
            }
            assert_eq!(
                report
                    .links
                    .iter()
                    .find(|l| l.link_id == "link:4")
                    .unwrap()
                    .verdict
                    .status,
                FacetSemanticStatus::Validated
            );
            assert!(
                report.entrypoints[0].content.is_err(),
                "IO content is unverified"
            );
        }
    }
}

#[test]
fn foreign_payloads_and_dag_spelled_relations_remain_opaque() {
    let mut fixture = Fixture::load("combined");
    let baseline = fixture.inspect().unwrap();
    // Even a known DAG content spelling must not cause foreign bytes to parse.
    fixture.add_opaque("foreign-owner", "dagml.graph_spec");
    for (i, relation) in [
        "recipe_of",
        "dataset_of",
        "produced_by",
        "evaluates",
        "residual_of",
        "evidence_for",
        "refit_of",
        "future_relation",
    ]
    .iter()
    .enumerate()
    {
        fixture.manifest.links.push(N4aLink {
            id: format!("foreign:{i}"),
            owner: "foreign-owner".into(),
            relation: (*relation).into(),
            source_artifact: "foreign".into(),
            target_artifact: "foreign".into(),
        });
    }
    let report = fixture.inspect().unwrap();
    assert_eq!(report.links, baseline.links);
    assert_eq!(report.opaque_link_ids.len(), 8);
    assert!(matches!(
        content(&report, "foreign"),
        OwnerContentInspection::Opaque
    ));
    // A DAG-owned relation to that opaque endpoint remains in the graph.
    // Synthetic rewrite of an existing declaration, not an owner-authored export.
    fixture
        .manifest
        .links
        .iter_mut()
        .find(|l| l.id == "link:dataset")
        .unwrap()
        .source_artifact = "foreign".into();
    let report = fixture.inspect().unwrap();
    assert_eq!(report.links.len(), baseline.links.len());
    assert_eq!(
        report.links.last().unwrap().verdict.status,
        FacetSemanticStatus::Unverified
    );
}

#[test]
fn unknown_dag_relation_refuses_instead_of_becoming_opaque() {
    let mut fixture = Fixture::load("combined");
    fixture.manifest.links[0].relation = "future_relation".into();
    assert!(matches!(
        fixture.inspect(),
        Err(FacetRefusal::InvalidStructure { .. })
    ));
}

#[test]
fn changed_identity_kind_version_and_known_json_refuse_even_when_unselected() {
    for mutation in 0..4 {
        let mut fixture = Fixture::load("combined");
        match mutation {
            0 => fixture.artifact("plan").identity.fingerprint = "0".repeat(64),
            1 => fixture.artifact("score").kind = ArtifactKind::Model,
            2 => fixture.artifact("plan").content_type.version = 99,
            _ => {
                fixture
                    .members
                    .insert("dagml/plan.json".into(), b"{\"bad\":true}".to_vec());
            }
        }
        assert!(fixture.inspect().is_err(), "mutation {mutation}");
    }
}

#[test]
fn changed_link_direction_and_proof_identity_refuse() {
    let mut fixture = Fixture::load("combined");
    fixture.manifest.links[4].source_artifact = "model".into();
    assert!(matches!(
        fixture.inspect(),
        Err(FacetRefusal::InvalidStructure { .. })
    ));
    let mut fixture = Fixture::load("combined");
    fixture.artifact("model").identity.fingerprint = "0".repeat(64);
    assert!(matches!(
        fixture.inspect(),
        Err(FacetRefusal::Mismatch { .. })
    ));
}

#[test]
fn individually_valid_owner_content_with_incompatible_scientific_link_refuses() {
    let mut fixture = Fixture::load("combined");
    let mut graph: serde_json::Value =
        serde_json::from_slice(&fixture.members["dagml/recipe.json"]).unwrap();
    graph["metadata"]["changed-recipe"] = serde_json::json!(true);
    let bytes = serde_json::to_vec(&graph).unwrap();
    let proof = validate_owner_facet_bytes(&FacetContentType::new("dagml.graph_spec", 1), &bytes)
        .unwrap()
        .unwrap();
    fixture.artifact("recipe").identity =
        serde_json::from_value(serde_json::to_value(proof.identity()).unwrap()).unwrap();
    fixture.members.insert("dagml/recipe.json".into(), bytes);
    // Both endpoints have genuine owner validation, but their graph identities
    // differ. DAG must reject the declared recipe_of edge itself.
    assert!(matches!(
        fixture.inspect(),
        Err(FacetRefusal::Mismatch { .. })
    ));
}

#[test]
fn physically_valid_state_rewrites_and_missing_closure_refuse() {
    for mutation in 0..3 {
        let mut fixture = Fixture::load("combined");
        let raw = fixture
            .members
            .keys()
            .find(|path| path.ends_with(".bin"))
            .unwrap()
            .clone();
        match mutation {
            0 => fixture.members.get_mut(&raw).unwrap()[0] ^= 1,
            1 => {
                fixture.members.remove(&raw);
                let N4aStorage::Embedded { member_paths, .. } =
                    &mut fixture.artifact("model").storage
                else {
                    unreachable!()
                };
                member_paths.retain(|path| path != &raw);
            }
            _ => {
                fixture
                    .members
                    .insert("dagml/extra.bin".into(), b"extra".to_vec());
                let N4aStorage::Embedded { member_paths, .. } =
                    &mut fixture.artifact("model").storage
                else {
                    unreachable!()
                };
                member_paths.push("dagml/extra.bin".into());
            }
        }
        assert!(
            matches!(fixture.inspect(), Err(FacetRefusal::Mismatch { .. })),
            "mutation {mutation}"
        );
    }
}

#[test]
fn unresolved_roots_inputs_and_requirements_refuse_content_resolution() {
    for id in ["model", "run", "evidence", "dataset"] {
        let mut fixture = Fixture::load("recipe");
        fixture.manifest.entrypoints = Fixture::load("combined").manifest.entrypoints;
        let entry = &mut fixture.manifest.entrypoints[0];
        entry.root_artifact = id.into();
        entry.input_artifacts.clear();
        let report = fixture.inspect().unwrap();
        assert!(
            matches!(&report.entrypoints[0].content, Err(FacetRefusal::UnresolvedOwnerContent { artifact_id }) if artifact_id == id)
        );
    }
    for on_artifact in [false, true] {
        let mut fixture = Fixture::recipe_only();
        let reference = Fixture::load("recipe")
            .manifest
            .artifacts
            .into_iter()
            .find(|a| a.id == "evidence")
            .unwrap();
        fixture.manifest.artifacts.push(reference);
        let requirement = Some(RequirementRef {
            artifact_id: "evidence".into(),
        });
        fixture.manifest.entrypoints.push(N4aEntrypoint {
            id: "inspect:required".into(),
            owner: "dag-ml".into(),
            operation: "inspect".into(),
            root_artifact: "recipe".into(),
            input_artifacts: vec![],
            required_features: vec![],
            requirements: if on_artifact {
                None
            } else {
                requirement.clone()
            },
        });
        if on_artifact {
            fixture.artifact("recipe").requirements = requirement;
        }
        let report = fixture.inspect().unwrap();
        assert!(
            matches!(&report.entrypoints[0].content, Err(FacetRefusal::UnresolvedOwnerContent { artifact_id }) if artifact_id == "evidence")
        );
    }
    let mut fixture = Fixture::load("recipe");
    fixture.manifest.entrypoints = Fixture::load("combined").manifest.entrypoints;
    fixture.manifest.entrypoints[0].root_artifact = "recipe".into();
    fixture.manifest.entrypoints[0].input_artifacts = vec!["model".into()];
    assert!(
        matches!(&fixture.inspect().unwrap().entrypoints[0].content, Err(FacetRefusal::UnresolvedOwnerContent { artifact_id }) if artifact_id == "model")
    );
}

#[test]
fn unknown_required_dag_root_is_unsupported_while_storage_remains_available() {
    let mut fixture = Fixture::recipe_only();
    fixture.artifact("recipe").content_type.id = "dagml.future_content".into();
    fixture.manifest.entrypoints.push(N4aEntrypoint {
        id: "inspect:unknown".into(),
        owner: "dag-ml".into(),
        operation: "inspect".into(),
        root_artifact: "recipe".into(),
        input_artifacts: vec![],
        required_features: vec![],
        requirements: None,
    });
    let bytes = fixture.bytes();
    let container = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let report = inspect_n4a_semantics(&container).unwrap();
    assert!(matches!(
        content(&report, "recipe"),
        OwnerContentInspection::Opaque
    ));
    assert!(matches!(
        &report.entrypoints[0].content,
        Err(FacetRefusal::UnsupportedPayload { .. })
    ));
    assert_eq!(copy_n4a_bytes(&container), bytes);
    assert_eq!(
        container.member("dagml/recipe.json").unwrap().bytes,
        fixture.members["dagml/recipe.json"]
    );
}

#[test]
fn exact_uint64_and_integer_float_lexical_tokens_reach_owner_unchanged() {
    let mut fixture = Fixture::recipe_only();
    let mut value: serde_json::Value =
        serde_json::from_slice(&fixture.members["dagml/recipe.json"]).unwrap();
    value["metadata"]["large"] = serde_json::json!(u64::MAX);
    value["metadata"]["number"] = serde_json::json!(1);
    let text = format!(" \n\t{}\n ", serde_json::to_string(&value).unwrap());
    assert!(text.contains("18446744073709551615"));
    let proof = validate_owner_facet_bytes(
        &FacetContentType::new("dagml.graph_spec", 1),
        text.as_bytes(),
    )
    .unwrap()
    .unwrap();
    fixture.artifact("recipe").identity =
        serde_json::from_value(serde_json::to_value(proof.identity()).unwrap()).unwrap();
    fixture
        .members
        .insert("dagml/recipe.json".into(), text.as_bytes().to_vec());
    let bytes = fixture.bytes();
    let container = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    inspect_n4a_semantics(&container).unwrap();
    assert_eq!(
        container.member("dagml/recipe.json").unwrap().bytes,
        proof.original_bytes()
    );
    // Same mathematical value, different owner fingerprint: Core must not normalize.
    fixture.members.insert(
        "dagml/recipe.json".into(),
        text.replace("\"number\":1", "\"number\":1.0").into_bytes(),
    );
    assert!(matches!(
        fixture.inspect(),
        Err(FacetRefusal::Mismatch { .. })
    ));
    for bad in [
        text.replace("18446744073709551615", "18446744073709551616"),
        text.replacen('{', "{\"id\":\"duplicate\",", 1),
    ] {
        fixture
            .members
            .insert("dagml/recipe.json".into(), bad.into_bytes());
        assert!(fixture.inspect().is_err());
    }
}

#[test]
fn producer_feature_strings_never_grant_executable_authority() {
    let mut fixture = Fixture::load("combined");
    fixture.manifest.required_features = vec!["native-workflow@1".into()];
    fixture.manifest.entrypoints[0].required_features =
        vec!["caller-claims-all-controllers".into()];
    let bytes = fixture.bytes();
    let container = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    inspect_n4a_semantics(&container).unwrap();
    assert_eq!(
        container
            .require_operation("predict:stored.features", None)
            .unwrap_err(),
        OperationRefusal::AbsentAdmission
    );
}

#[test]
#[cfg(target_os = "linux")]
fn semantic_only_fresh_process_never_loads_native_library() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "semantic_only_child", "--ignored", "--nocapture"])
        .env("N4M_LIB_PATH", "/nonexistent/t1-semantic/libn4m.so")
        .env("N4M_LIBRARY_PATH", "/nonexistent/t1-semantic/libn4m.so")
        .env("OMP_NUM_THREADS", "1")
        .env("OPENBLAS_NUM_THREADS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout)
        .contains("semantic process: no libn4m mapping before or after"));
}

#[test]
#[ignore = "invoked in a fresh process by semantic_only_fresh_process_never_loads_native_library"]
#[cfg(target_os = "linux")]
fn semantic_only_child() {
    for name in ["N4M_LIB_PATH", "N4M_LIBRARY_PATH"] {
        assert_eq!(
            std::env::var(name).unwrap(),
            "/nonexistent/t1-semantic/libn4m.so"
        );
    }
    let assert_no_native = || {
        let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
        assert!(!maps.to_lowercase().contains("libn4m"), "{maps}");
    };
    assert_no_native();
    for shape in SHAPES {
        Fixture::load(shape).inspect().unwrap();
    }
    assert_no_native();
    println!("semantic process: no libn4m mapping before or after");
}

/// Preserve an already fitted, genuine DMD S != C package for the two WASM readers.
#[test]
#[ignore = "Root-only producer: requires NIRS4ALL_CORE_RELINK_INPUT and a fresh NIRS4ALL_CORE_RELINK_OUTPUT"]
fn produce_real_dmd_initial_refit_facets() {
    use dag_ml_core::artifact_facets::{build_initial_refit_artifact_facets, ArtifactFacetStorage};
    use dag_ml_core::InitialFullRefitPackage;

    let input = std::env::var_os("NIRS4ALL_CORE_RELINK_INPUT")
        .expect("NIRS4ALL_CORE_RELINK_INPUT must name the preserved run02 package");
    let output = std::env::var_os("NIRS4ALL_CORE_RELINK_OUTPUT")
        .expect("NIRS4ALL_CORE_RELINK_OUTPUT must name a fresh external directory");
    let output = Path::new(&output);
    assert!(output.is_absolute(), "producer output must be absolute");
    let parent = output.parent().unwrap().canonicalize().unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .canonicalize()
        .unwrap();
    assert!(
        !parent.starts_with(&repository),
        "no generated containers in Core"
    );
    assert!(!output.exists(), "preserve any prior producer output");

    let original = std::fs::read(&input).unwrap();
    let original_sha256 = format!("{:x}", Sha256::digest(&original));
    assert_eq!(
        original_sha256,
        "20552dc6779a742f8cddc02a13cf7c3c14056144054fc516a461a4c1def3a918"
    );
    let package = InitialFullRefitPackage::from_json(std::str::from_utf8(&original).unwrap())
        .unwrap_or_else(|error| panic!("STOP: unsupported/refused package: {error}"));
    let source_s = package
        .training_envelope
        .relation_fingerprint
        .as_deref()
        .unwrap();
    let canonical_c = package.training_relations.fingerprint().unwrap();
    assert_eq!(
        source_s,
        "2395c9d0ee44f21670136244ec9fcd4aac321755d83b67e2575d5176b234a257"
    );
    assert_eq!(
        canonical_c,
        "10404fec25008afabeff39ca5e7ba036bff93eecbcf0a4635bcbdf9ed93b8179"
    );
    assert_ne!(source_s, canonical_c);
    assert_eq!(package.training_sample_ids.len(), 28);
    let ordered_ids = serde_json::to_value(&package.training_sample_ids).unwrap();
    assert_eq!(
        ordered_ids,
        serde_json::json!([
            "p3", "p20", "p12", "p27", "p8", "p30", "p15", "p25", "p33", "p37", "p0", "p17", "p6",
            "p10", "p23", "p28", "p26", "p29", "p11", "p31", "p4", "p14", "p32", "p13", "p39",
            "p22", "p35", "p1"
        ])
    );

    let export = build_initial_refit_artifact_facets(&package)
        .unwrap_or_else(|error| panic!("STOP: unsupported/refused export: {error}"));
    // Transfer only neutral owner descriptors, never the process-local proof context.
    let mut fixture = Fixture {
        manifest: N4aManifest {
            schema: N4A_SCHEMA.into(),
            schema_version: 1,
            minimum_reader_version: 1,
            container_id: "container:core-relink-real-dmd-initial-refit".into(),
            required_features: vec![],
            physical_profile: N4A_PHYSICAL_PROFILE.into(),
            members: vec![],
            artifacts: serde_json::from_value(
                serde_json::to_value(&export.graph.artifacts).unwrap(),
            )
            .unwrap_or_else(|error| panic!("STOP: refused descriptor transfer: {error}")),
            links: serde_json::from_value(serde_json::to_value(&export.graph.links).unwrap())
                .unwrap_or_else(|error| panic!("STOP: refused link transfer: {error}")),
            entrypoints: vec![],
        },
        members: export.members.clone(),
    };
    assert_eq!(
        serde_json::to_value(&fixture.manifest.artifacts).unwrap(),
        serde_json::to_value(&export.graph.artifacts).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&fixture.manifest.links).unwrap(),
        serde_json::to_value(&export.graph.links).unwrap()
    );
    assert_eq!(fixture.members, export.members);
    let (root_path, mut raw_paths) = match &fixture
        .manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == "full_refit")
        .unwrap()
        .storage
    {
        N4aStorage::Embedded {
            root_member,
            member_paths,
        } => (
            root_member.clone(),
            member_paths
                .iter()
                .filter(|path| *path != root_member)
                .cloned()
                .collect::<Vec<_>>(),
        ),
        _ => panic!("STOP: owner full_refit is not embedded"),
    };
    assert_eq!(root_path, "dagml/full_refit.json");
    let root_bytes = fixture.members[&root_path].clone();
    let root_text = std::str::from_utf8(&root_bytes).unwrap();
    let root_sha256 = format!("{:x}", Sha256::digest(&root_bytes));
    let root_package = InitialFullRefitPackage::from_json(root_text)
        .unwrap_or_else(|error| panic!("STOP: owner root refused: {error}"));
    assert_eq!(
        root_package, package,
        "owner reserialization preserves the complete parsed package"
    );
    for id in ["model", "run"] {
        let artifact = export
            .graph
            .artifacts
            .iter()
            .find(|artifact| artifact.id == id)
            .unwrap();
        assert!(matches!(&artifact.storage,
            ArtifactFacetStorage::Reference { root_sha256: Some(value) } if value == &root_sha256));
    }

    let facets = fixture.bytes();
    let container = open_n4a_bytes(&facets, N4aMemoryBudget::default()).unwrap();
    assert_eq!(container.metadata().manifest(), &fixture.manifest);
    for (path, bytes) in &export.members {
        assert_eq!(container.member(path).unwrap().bytes, bytes.as_slice());
    }
    let inspection = inspect_n4a_semantics(&container)
        .unwrap_or_else(|error| panic!("STOP: native semantic inspection refused: {error}"));
    assert_eq!(inspection.artifacts.len(), 4);
    assert!(matches!(
        content(&inspection, "recipe"),
        OwnerContentInspection::Validated { .. }
    ));
    assert!(matches!(
        content(&inspection, "model"),
        OwnerContentInspection::Unresolved
    ));
    assert!(matches!(
        content(&inspection, "run"),
        OwnerContentInspection::Unresolved
    ));
    let state_members = match content(&inspection, "full_refit") {
        OwnerContentInspection::Validated {
            state_members,
            model_states,
        } => {
            assert!(
                !model_states.is_empty(),
                "initial-refit model records must be reported"
            );
            state_members.clone()
        }
        other => panic!("STOP: full_refit content is not validated: {other:?}"),
    };
    let mut actual_paths = state_members
        .iter()
        .map(|state| state.member_path.clone())
        .collect::<Vec<_>>();
    raw_paths.sort();
    actual_paths.sort();
    assert_eq!(actual_paths, raw_paths);
    assert_eq!(state_members.len(), package.raw_artifact_payloads.len());
    for state in &state_members {
        let raw = package
            .raw_artifact_payloads
            .get(&state.artifact_id)
            .unwrap();
        assert_eq!(
            container.member(&state.member_path).unwrap().bytes,
            raw.as_slice()
        );
        assert_eq!(state.sha256, format!("{:x}", Sha256::digest(raw)));
        assert_eq!(state.byte_length, raw.len() as u64);
    }
    let expected_links = [
        (
            "recipe_of",
            "recipe",
            "full_refit",
            FacetSemanticStatus::Validated,
        ),
        (
            "recipe_of",
            "recipe",
            "model",
            FacetSemanticStatus::Unverified,
        ),
        (
            "recipe_of",
            "recipe",
            "run",
            FacetSemanticStatus::Unverified,
        ),
        (
            "produced_by",
            "full_refit",
            "run",
            FacetSemanticStatus::Unverified,
        ),
        (
            "produced_by",
            "model",
            "run",
            FacetSemanticStatus::Unverified,
        ),
    ];
    assert_eq!(fixture.manifest.links.len(), expected_links.len());
    assert_eq!(inspection.links.len(), expected_links.len());
    for (relation, source, target, status) in &expected_links {
        let links = fixture
            .manifest
            .links
            .iter()
            .filter(|link| {
                link.relation == *relation
                    && link.source_artifact == *source
                    && link.target_artifact == *target
            })
            .collect::<Vec<_>>();
        assert_eq!(links.len(), 1);
        let reports = inspection
            .links
            .iter()
            .filter(|link| link.link_id == links[0].id)
            .collect::<Vec<_>>();
        assert_eq!(reports.len(), 1);
        assert_eq!(&reports[0].verdict.status, status);
        if *status == FacetSemanticStatus::Unverified {
            assert!(reports[0]
                .verdict
                .reason
                .contains("owner bytes/proof are unresolved"));
        }
    }
    assert!(inspection.opaque_link_ids.is_empty());
    assert!(inspection.entrypoints.is_empty());
    let manifest = fixture.manifest.clone();

    // One byte-local S tamper. Only physical inventory is regenerated by Fixture::bytes.
    let tamper_s = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    assert_ne!(tamper_s, source_s);
    assert_ne!(tamper_s, canonical_c);
    let envelope_start = root_text.find("\"training_envelope\":").unwrap();
    let key = format!("\"relation_fingerprint\":\"{source_s}\"");
    let offset = envelope_start
        + root_text[envelope_start..].find(&key).unwrap()
        + "\"relation_fingerprint\":\"".len();
    let mut tampered_root = root_text.to_owned();
    tampered_root.replace_range(offset..offset + source_s.len(), tamper_s);
    let original_value: serde_json::Value = serde_json::from_slice(&root_bytes).unwrap();
    let mut only_s_changed = original_value.clone();
    only_s_changed["training_envelope"]["relation_fingerprint"] = serde_json::json!(tamper_s);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&tampered_root).unwrap(),
        only_s_changed
    );
    assert_eq!(root_bytes.len(), tampered_root.len());
    fixture
        .members
        .insert(root_path.clone(), tampered_root.into_bytes());
    let tamper = fixture.bytes();
    let tampered_container = open_n4a_bytes(&tamper, N4aMemoryBudget::default()).unwrap();
    let refusal = inspect_n4a_semantics(&tampered_container)
        .expect_err("the physically valid S tamper must refuse semantically")
        .to_string();
    assert!(
        refusal.contains("package fingerprint"),
        "unexpected refusal; STOP: {refusal}"
    );
    let receipt = serde_json::json!({
        "source_path": Path::new(&input),
        "source_sha256": original_sha256,
        "owner_root_member": root_path,
        "owner_root_sha256": root_sha256,
        "encoding_note": "Owner reserialization; source and root encodings may differ. Complete parsed packages and exact fitted-state bytes were compared.",
        "source_and_root_bytes_equal": original == root_bytes,
        "package_id": package.package_id,
        "package_fingerprint": package.package_fingerprint,
        "ordered_training_ids": ordered_ids,
        "source_relation_key_s": source_s,
        "canonical_relation_key_c": canonical_c,
        "full_refit_state_members": state_members,
        "native_links": inspection.links,
        "manifest": manifest,
        "facets_sha256": format!("{:x}", Sha256::digest(&facets)),
        "tamper_sha256": format!("{:x}", Sha256::digest(&tamper)),
        "tamper_relation_key_s": tamper_s,
        "tamper_refusal_detail": refusal,
        "tamper_scope": "Single training_envelope S value changed; owner fingerprints unchanged; only physical inventory recalculated.",
        "semantic_scope": "Two embedded contents validated, two references unresolved, one link validated, four unverified; no executable admission or native-state validation.",
    });
    // All semantic checks precede publication of this isolated producer fixture.
    std::fs::create_dir(output).unwrap();
    std::fs::write(output.join("source-package.json"), &original).unwrap();
    std::fs::write(output.join("facets.n4a"), &facets).unwrap();
    std::fs::write(output.join("tamper-s.n4a"), &tamper).unwrap();
    std::fs::write(
        output.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    println!("real DMD facet producer: {}", output.display());
    println!("{}", serde_json::to_string(&receipt).unwrap());
}
