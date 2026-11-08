//! Storage-only checks over the packaged genuine owner corpus. No native runtime
//! or sibling source tree is required; scientific payloads are never decoded.
use nirs4all::n4a::*;
use serde_json::Value;
use std::path::Path;

fn fixture(shape: &str) -> (Vec<u8>, Vec<u8>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/fixtures/n4a_v1");
    let raw_manifest = std::fs::read(root.join(shape).join("manifest.json")).unwrap();
    let manifest: N4aManifest = serde_json::from_slice(&raw_manifest).unwrap();
    let payloads = manifest
        .members
        .iter()
        .map(|m| std::fs::read(root.join(shape).join(&m.path)).unwrap())
        .collect::<Vec<_>>();
    let members = manifest
        .members
        .iter()
        .zip(&payloads)
        .map(|(m, bytes)| N4aMemberInput {
            path: &m.path,
            bytes,
        })
        .collect();
    let (receipt, bytes) = write_n4a_bytes(
        N4aWriteRequest {
            manifest: manifest.clone(),
            members,
        },
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let provenance: Value =
        serde_json::from_slice(&std::fs::read(root.join("PROVENANCE.json")).unwrap()).unwrap();
    assert_eq!(
        provenance["containers"][shape]["sha256"].as_str().unwrap(),
        receipt.container_sha256,
        "reconstructed container must match its genuine producer"
    );
    assert_eq!(
        provenance["containers"][shape]["bytes"].as_u64(),
        Some(bytes.len() as u64)
    );
    (bytes, raw_manifest)
}

fn assert_same_views(bytes: Vec<u8>, raw_manifest: &[u8]) {
    let borrowed = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    // The allocation and its local constructor input both leave this scope.
    let (owned, backing) = {
        let input = bytes.clone().into_boxed_slice();
        let backing = input.as_ptr();
        let handle = open_owned_n4a_bytes(input, N4aMemoryBudget::default()).unwrap();
        (handle, backing)
    };
    assert_eq!(owned.receipt(), borrowed.receipt());
    assert_eq!(owned.manifest(), borrowed.manifest());
    assert_eq!(owned.manifest_bytes(), raw_manifest);
    assert_eq!(owned.manifest_bytes(), borrowed.manifest_bytes());
    let manifest_offset = borrowed.manifest_bytes().as_ptr() as usize - bytes.as_ptr() as usize;
    assert_eq!(
        owned.manifest_bytes().as_ptr(),
        backing.wrapping_add(manifest_offset)
    );
    let index = owned.index() as *const ValidatedIndex;
    let manifest = owned.manifest() as *const N4aManifest;
    let receipt = owned.receipt() as *const N4aReceipt;
    for _ in 0..3 {
        assert_eq!(index, owned.index());
        assert_eq!(manifest, owned.manifest());
        assert_eq!(receipt, owned.receipt());
        for declaration in &borrowed.manifest().members {
            let left = borrowed.member(&declaration.path).unwrap();
            let right = owned.member(&declaration.path).unwrap();
            assert_eq!(right.path, left.path);
            assert_eq!(right.range, left.range);
            assert_eq!(right.bytes, left.bytes);
            assert_eq!(right.bytes, &bytes[right.range.clone()]);
            assert_eq!(
                right.bytes.as_ptr(),
                backing.wrapping_add(right.range.start)
            );
            let again = owned.member(&declaration.path).unwrap();
            assert_eq!(again.bytes.as_ptr(), right.bytes.as_ptr());
            assert_eq!(again.path.as_ptr(), right.path.as_ptr());
        }
        for artifact in &borrowed.manifest().artifacts {
            let left = borrowed.artifact_members(&artifact.id).unwrap();
            let right = owned.artifact_members(&artifact.id).unwrap();
            assert_eq!(left.len(), right.len());
            for (left, right) in left.iter().zip(&right) {
                assert_eq!(right.path, left.path);
                assert_eq!(right.range, left.range);
                assert_eq!(right.bytes, left.bytes);
                assert_eq!(
                    right.bytes.as_ptr(),
                    backing.wrapping_add(right.range.start)
                );
            }
        }
        let left = borrowed.inspect();
        let right = owned.inspect();
        assert_eq!(right.manifest, left.manifest);
        assert_eq!(right.receipt, left.receipt);
        assert_eq!(right.storage, StorageStatus::Verified);
        assert_eq!(right.semantics, SemanticStatus::Unverified);
        assert_eq!(right.required_features, left.required_features);
        for entry in &borrowed.manifest().entrypoints {
            assert_eq!(
                owned.operation_binding(&entry.id).unwrap(),
                borrowed.operation_binding(&entry.id).unwrap()
            );
            assert_eq!(
                owned.require_operation(&entry.id, None).unwrap_err(),
                OperationRefusal::AbsentAdmission
            );
        }
        let mut copy = copy_n4a_bytes(&owned);
        assert_eq!(copy, bytes);
        assert_eq!(copy_n4a_bytes(&borrowed), bytes);
        assert_ne!(copy.as_ptr(), backing);
        // Explicit copies are independently mutable; the validated handle is not.
        copy.fill(0);
        assert_eq!(owned.manifest_bytes(), raw_manifest);
    }
    assert!(owned.member("absent").is_none());
    assert!(owned.artifact_members("absent").is_none());
}

macro_rules! corpus_test {
    ($name:ident, $shape:literal) => {
        #[test]
        fn $name() {
            let (bytes, manifest) = fixture($shape);
            assert_same_views(bytes, &manifest);
        }
    };
}
corpus_test!(recipe_borrowed_owned_equality, "recipe");
corpus_test!(dataset_borrowed_owned_equality, "dataset");
corpus_test!(model_borrowed_owned_equality, "model");
corpus_test!(result_borrowed_owned_equality, "result");
corpus_test!(score_borrowed_owned_equality, "score");
corpus_test!(combined_borrowed_owned_equality, "combined");

#[test]
fn owned_archive_outlives_every_constructor_input_and_borrowed_manifest_keeps_its_lifetime() {
    let owned = {
        let (local, _) = fixture("model");
        open_owned_n4a_bytes(local.into_boxed_slice(), N4aMemoryBudget::default()).unwrap()
    };
    let (expected, raw_manifest) = fixture("model");
    assert_eq!(copy_n4a_bytes(&owned), expected);
    assert_eq!(owned.manifest_bytes(), raw_manifest);
    let raw = {
        let borrowed = open_n4a_bytes(&expected, N4aMemoryBudget::default()).unwrap();
        borrowed.manifest_bytes()
    };
    assert_eq!(raw, raw_manifest);
}

#[test]
fn unknown_optional_attachment_and_features_remain_exact_and_inert() {
    let (bytes, _) = fixture("recipe");
    let original = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let mut manifest = original.manifest().clone();
    let path = "opaque/café.bin";
    // Synthetic attachment only; the genuine scientific members stay untouched.
    let opaque = b"\0\xff\x80\n18446744073709551615\r\n";
    let mut artifact = manifest.artifacts[0].clone();
    artifact.id = "opaque-optional".into();
    artifact.kind = ArtifactKind::Attachment;
    artifact.owner = "unknown-test-owner".into();
    artifact.content_type = ContentType {
        id: "unknown.opaque.v99".into(),
        version: 99,
    };
    artifact.identity = OwnerIdentity {
        owner: artifact.owner.clone(),
        schema: "unknown.identity.v99".into(),
        schema_version: 99,
        fingerprint_profile: "opaque-test-only".into(),
        fingerprint: "opaque-test-token".into(),
    };
    artifact.storage = N4aStorage::Embedded {
        root_member: path.into(),
        member_paths: vec![path.into()],
    };
    artifact.requirements = None;
    manifest.artifacts.push(artifact);
    manifest.members.push(N4aMember {
        path: path.into(),
        byte_length: 0,
        sha256: String::new(),
    });
    // Inspection reports unknown requirements without loading an owner runtime.
    manifest.required_features.push("unknown.global.v99".into());
    manifest.entrypoints.push(N4aEntrypoint {
        id: "opaque-inspect".into(),
        owner: "unknown-test-owner".into(),
        operation: "inspect".into(),
        root_artifact: "opaque-optional".into(),
        input_artifacts: Vec::new(),
        required_features: vec!["unknown.entry.v99".into()],
        requirements: None,
    });
    let mut members = original
        .manifest()
        .members
        .iter()
        .map(|m| N4aMemberInput {
            path: &m.path,
            bytes: original.member(&m.path).unwrap().bytes,
        })
        .collect::<Vec<_>>();
    members.push(N4aMemberInput {
        path,
        bytes: opaque,
    });
    let (_, bytes) = write_n4a_bytes(
        N4aWriteRequest { manifest, members },
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    assert_eq!(view.member(path).unwrap().bytes, opaque);
    assert!(view
        .inspect()
        .required_features
        .contains(&"unknown.global.v99"));
    assert!(view
        .inspect()
        .required_features
        .contains(&"unknown.entry.v99"));
    let manifest = view.manifest_bytes().to_vec();
    assert_same_views(bytes, &manifest);
}

fn same_refusal(bytes: &[u8], budget: N4aMemoryBudget) -> N4aError {
    let borrowed = open_n4a_bytes(bytes, budget).unwrap_err();
    let owned = open_owned_n4a_bytes(bytes.to_vec().into_boxed_slice(), budget).unwrap_err();
    assert_eq!(borrowed, owned);
    borrowed
}

fn u16at(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}
fn u32at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

// Same test-only ZIP header/CRC surgery as n4a_container.rs, restricted to
// equal-size edits of valid writer output. This is not a second archive reader.
fn repair_crc(bytes: &mut [u8], path: &str, range: std::ops::Range<usize>) {
    let mut crc = !0u32;
    for byte in &bytes[range] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    let crc = (!crc).to_le_bytes();
    let end = bytes.len() - 22;
    let mut at = u32at(bytes, end + 16);
    for _ in 0..u16at(bytes, end + 10) {
        let len = u16at(bytes, at + 28);
        if &bytes[at + 46..at + 46 + len] == path.as_bytes() {
            let local = u32at(bytes, at + 42);
            bytes[local + 14..local + 18].copy_from_slice(&crc);
            bytes[at + 16..at + 20].copy_from_slice(&crc);
            return;
        }
        at += 46 + len;
    }
    panic!("test member must exist");
}

#[test]
fn valid_size_corruption_refuses_at_crc_then_at_raw_sha_with_repaired_crc() {
    let (bytes, _) = fixture("model");
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let path = &view.manifest().members[0].path;
    let range = view.member(path).unwrap().range;
    let mut bad = bytes.clone();
    bad[range.start] ^= 1;
    assert_eq!(bad.len(), bytes.len());
    let crc_error = same_refusal(&bad, N4aMemoryBudget::default());
    assert!(matches!(crc_error, N4aError::Integrity(_)));
    assert!(crc_error.to_string().contains("CRC"));
    repair_crc(&mut bad, path, range);
    let sha_error = same_refusal(&bad, N4aMemoryBudget::default());
    assert!(matches!(sha_error, N4aError::Integrity(_)));
    assert!(sha_error.to_string().contains("raw size/SHA mismatch"));
}

#[test]
fn unsupported_container_schema_profile_and_reader_refuse_after_valid_physical_checks() {
    let (bytes, _) = fixture("combined");
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let start = view.manifest_bytes().as_ptr() as usize - bytes.as_ptr() as usize;
    let range = start..start + view.manifest_bytes().len();
    for (from, to) in [
        ("nirs4all.n4a.container.v1", "nirs4all.n4a.container.v2"),
        ("\"schema_version\":1", "\"schema_version\":2"),
        ("zip-stored.v1", "zip-stored.v2"),
        (
            "\"minimum_reader_version\":1",
            "\"minimum_reader_version\":2",
        ),
    ] {
        assert_eq!(from.len(), to.len());
        let mut bad = bytes.clone();
        let offset = view
            .manifest_bytes()
            .windows(from.len())
            .position(|w| w == from.as_bytes())
            .unwrap();
        bad[start + offset..start + offset + from.len()].copy_from_slice(to.as_bytes());
        repair_crc(&mut bad, "manifest.json", range.clone());
        assert_eq!(bad.len(), bytes.len());
        assert!(matches!(
            same_refusal(&bad, N4aMemoryBudget::default()),
            N4aError::UnsupportedFormat(_)
        ));
    }
}

#[test]
fn declared_memory_budget_refuses_identically_on_both_byte_routes() {
    let (bytes, _) = fixture("combined");
    assert!(open_n4a_bytes(&bytes, N4aMemoryBudget::bytes(1 << 20)).is_ok());
    let error = same_refusal(&bytes, N4aMemoryBudget::bytes(1024));
    assert!(matches!(error, N4aError::Memory(_)), "{error}");
}
