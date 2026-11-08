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
