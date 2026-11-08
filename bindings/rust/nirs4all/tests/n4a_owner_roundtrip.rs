//! Exact committed owner bytes only. No live DAG API or old archive dependency.
//!
//! Qualified DAG/IO/Methods fixtures and provenance are packaged with this crate.
//! These default tests check transport only: no fitting, owner regeneration or
//! scientific JSON/native-state decoding. Fresh-process Rust replay is recorded
//! in the external qualification receipt. Synthetic tests live in n4a_container.rs.
use nirs4all::n4a::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn fixture_files(root: &Path, directory: &Path, files: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink(), "fixture members must be regular files");
        if kind.is_dir() {
            fixture_files(root, &entry.path(), files);
        } else {
            assert!(kind.is_file());
            files.insert(
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/"),
            );
        }
    }
}

fn assert_provenance_file(provenance: &serde_json::Value, path: &str, bytes: &[u8]) {
    let record = &provenance["files"][path];
    assert_eq!(record["bytes"].as_u64(), Some(bytes.len() as u64), "{path}");
    assert_eq!(
        record["sha256"].as_str().unwrap(),
        format!("{:x}", Sha256::digest(bytes)),
        "{path}"
    );
}

fn committed_owner_roundtrip(shape: &str) {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/fixtures/n4a_v1");
    let root = fixtures.join(shape);
    let provenance: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixtures.join("PROVENANCE.json")).unwrap()).unwrap();
    assert_eq!(
        provenance["schema"],
        "nirs4all.n4a.owner-fixture-provenance.v1"
    );
    // PID values are retained provenance metadata, not live runtime evidence.
    let producer = provenance["qualification"]["producer_pid"]
        .as_u64()
        .unwrap();
    let replay = provenance["qualification"]["fresh_replay_pid"]
        .as_u64()
        .unwrap();
    assert!(producer > 0 && replay > 0);
    assert_ne!(producer, replay);
    let raw_manifest =
        std::fs::read(root.join("manifest.json")).expect("committed owner manifest is required");
    assert_provenance_file(
        &provenance,
        &format!("{shape}/manifest.json"),
        &raw_manifest,
    );
    let manifest: N4aManifest = serde_json::from_slice(&raw_manifest).unwrap();
    let mut actual_files = BTreeSet::new();
    fixture_files(&root, &root, &mut actual_files);
    let expected_files = manifest
        .members
        .iter()
        .map(|m| m.path.clone())
        .chain(std::iter::once("manifest.json".into()))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_files, expected_files,
        "fixture inventory must be exact"
    );
    let provenance_files = provenance["files"]
        .as_object()
        .unwrap()
        .keys()
        .filter_map(|path| path.strip_prefix(&format!("{shape}/")).map(str::to_owned))
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_files, provenance_files);
    let mut owners = BTreeMap::new();
    for artifact in &manifest.artifacts {
        if let N4aStorage::Embedded { member_paths, .. } = &artifact.storage {
            for path in member_paths {
                assert!(owners.insert(path.as_str(), artifact.id.as_str()).is_none());
            }
        }
    }
    assert_eq!(
        owners.keys().copied().collect::<BTreeSet<_>>(),
        manifest.members.iter().map(|m| m.path.as_str()).collect()
    );
    if shape == "model" || shape == "combined" {
        let model = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == ArtifactKind::Model)
            .unwrap();
        let N4aStorage::Embedded {
            root_member,
            member_paths,
        } = &model.storage
        else {
            panic!("the model fixture must embed its owner-declared package and raw closure")
        };
        assert!(member_paths.contains(root_member));
        assert!(
            member_paths.iter().any(|path| path != root_member),
            "the qualified package has a raw child"
        );
        for path in member_paths {
            assert_eq!(owners.get(path.as_str()), Some(&model.id.as_str()));
            assert!(manifest.members.iter().any(|member| &member.path == path));
        }
        assert_eq!(
            manifest.members.len(),
            if shape == "combined" {
                9
            } else {
                member_paths.len()
            }
        );
    } else {
        assert_eq!(manifest.members.len(), 1);
    }

    // Preserve the exact owner-declared identities and links in partial bundles.
    // This compares transport metadata; it validates no scientific fingerprint.
    let combined_bytes = std::fs::read(fixtures.join("combined/manifest.json")).unwrap();
    assert_provenance_file(&provenance, "combined/manifest.json", &combined_bytes);
    let combined: N4aManifest = serde_json::from_slice(&combined_bytes).unwrap();
    assert_eq!(manifest.artifacts.len(), combined.artifacts.len());
    assert_eq!(manifest.links, combined.links);
    for artifact in &manifest.artifacts {
        let original = combined
            .artifacts
            .iter()
            .find(|a| a.id == artifact.id)
            .unwrap();
        assert_eq!(artifact.kind, original.kind);
        assert_eq!(artifact.owner, original.owner);
        assert_eq!(artifact.content_type, original.content_type);
        assert_eq!(artifact.identity, original.identity);
        assert_eq!(artifact.requirements, original.requirements);
        if shape != "combined" && artifact.id != shape {
            let N4aStorage::Reference { root_sha256 } = &artifact.storage else {
                panic!("omitted context must retain a full owner reference")
            };
            match &original.storage {
                N4aStorage::Embedded { root_member, .. } => {
                    let raw_sha = &combined
                        .members
                        .iter()
                        .find(|m| &m.path == root_member)
                        .unwrap()
                        .sha256;
                    assert_eq!(root_sha256.as_ref(), Some(raw_sha));
                }
                N4aStorage::Reference { .. } => assert_eq!(artifact.storage, original.storage),
            }
        } else {
            assert_eq!(artifact.storage, original.storage);
        }
    }
    let payloads = manifest
        .members
        .iter()
        .map(|m| {
            let bytes = std::fs::read(root.join(&m.path))
                .expect("committed exact owner bytes are required");
            assert_provenance_file(&provenance, &format!("{shape}/{}", m.path), &bytes);
            assert_eq!(m.byte_length, bytes.len() as u64);
            assert_eq!(m.sha256, format!("{:x}", Sha256::digest(&bytes)));
            (m.path.clone(), bytes)
        })
        .collect::<Vec<_>>();
    let request = N4aWriteRequest {
        manifest: manifest.clone(),
        members: payloads
            .iter()
            .map(|(path, bytes)| N4aMemberInput { path, bytes })
            .collect(),
    };
    let (receipt, bytes) = write_n4a_bytes(request, N4aMemoryBudget::default()).unwrap();
    let producer_container = &provenance["containers"][shape];
    assert_eq!(
        producer_container["bytes"].as_u64(),
        Some(bytes.len() as u64),
        "{shape}"
    );
    assert_eq!(receipt.byte_length, bytes.len() as u64);
    assert_eq!(
        producer_container["sha256"].as_str().unwrap(),
        receipt.container_sha256,
        "{shape}"
    );
    let opened = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    assert_eq!(opened.manifest_bytes(), raw_manifest.as_slice());
    assert_eq!(opened.manifest(), &manifest);
    assert_eq!(opened.receipt(), &receipt);
    for (path, payload) in &payloads {
        assert_eq!(opened.member(path).unwrap().bytes, payload);
    }
    assert_eq!(copy_n4a_bytes(&opened), bytes);
    assert_eq!(opened.inspect().semantics, SemanticStatus::Unverified);
    for entry in &manifest.entrypoints {
        assert_eq!(
            opened.require_operation(&entry.id, None).unwrap_err(),
            OperationRefusal::AbsentAdmission
        );
    }
}

#[test]
fn recipe_owner_bytes_roundtrip() {
    committed_owner_roundtrip("recipe");
}
#[test]
fn dataset_owner_bytes_roundtrip() {
    committed_owner_roundtrip("dataset");
}
#[test]
fn model_owner_bytes_roundtrip() {
    committed_owner_roundtrip("model");
}
#[test]
fn result_owner_bytes_roundtrip() {
    committed_owner_roundtrip("result");
}
#[test]
fn score_owner_bytes_roundtrip() {
    committed_owner_roundtrip("score");
}
#[test]
fn combined_owner_bytes_roundtrip() {
    committed_owner_roundtrip("combined");
}
