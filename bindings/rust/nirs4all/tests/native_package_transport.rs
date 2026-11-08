//! Runtime-free transport regressions over unchanged DAG RolePipeline fixtures.
//! These do not qualify numerical execution or introduce another numerical oracle.
use nirs4all::n4a::*;
use nirs4all::n4a_file::open_n4a_file;
use nirs4all::n4a_native_package::*;
use sha2::{Digest, Sha256};

const OUTCOME: &[u8] = include_bytes!("fixtures/native_package_transport_v1/training_outcome.json");
const PACKAGE: &[u8] =
    include_bytes!("fixtures/native_package_transport_v1/predictor_package.json");
const CHILD: &[u8] = include_bytes!("fixtures/native_package_transport_v1/child_refit_v3.json");

fn source() -> OwnedN4a {
    native_package_container(
        Some(OUTCOME),
        PACKAGE,
        "transport:role",
        N4aMemoryBudget::default(),
    )
    .unwrap()
}

fn rewrite(base: &OwnedN4a, manifest: N4aManifest, replacements: &[(&str, &[u8])]) -> OwnedN4a {
    let (_, bytes) = write_n4a_update(
        base,
        manifest,
        replacements
            .iter()
            .map(|&(path, bytes)| N4aMemberInput { path, bytes })
            .collect(),
        N4aMemoryBudget::default(),
    )
    .unwrap();
    open_owned_n4a_bytes(bytes.into_boxed_slice(), N4aMemoryBudget::default()).unwrap()
}

fn mixed(base: &OwnedN4a) -> OwnedN4a {
    let mut manifest = base.manifest().clone();
    let path = "foreign/opaque.bin";
    let bytes: &[u8] = b"\xffnot JSON\x00";
    manifest.members.push(N4aMember {
        path: path.into(),
        byte_length: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    });
    manifest.artifacts.push(N4aArtifact {
        id: "foreign".into(),
        kind: ArtifactKind::Attachment,
        owner: "opaque-owner".into(),
        content_type: ContentType {
            id: "opaque.example".into(),
            version: 77,
        },
        identity: OwnerIdentity {
            owner: "opaque-owner".into(),
            schema: "opaque.example".into(),
            schema_version: 77,
            fingerprint_profile: "opaque-id".into(),
            fingerprint: "foreign:stable".into(),
        },
        storage: N4aStorage::Embedded {
            root_member: path.into(),
            member_paths: vec![path.into()],
        },
        requirements: None,
    });
    manifest.entrypoints.push(N4aEntrypoint {
        id: "foreign-operation".into(),
        owner: "opaque-owner".into(),
        operation: "unknown".into(),
        root_artifact: "foreign".into(),
        input_artifacts: vec![],
        required_features: vec!["unknown.foreign@999".into()],
        requirements: None,
    });
    rewrite(base, manifest, &[(path, bytes)])
}

#[test]
fn package_and_outcome_bytes_survive_without_legacy_zip_or_uri_aliases() {
    let base = source();
    assert_eq!(base.manifest().schema, N4A_SCHEMA);
    let loaded = read_native_package(&base, None).unwrap();
    assert_eq!(loaded.portable_predictor_package().unwrap(), PACKAGE);
    assert_eq!(loaded.training_outcome_bytes().unwrap(), OUTCOME);
    assert_eq!(
        loaded.reference().archive_id(),
        base.manifest().container_id
    );
    assert_eq!(
        loaded.reference().archive_sha256(),
        base.receipt().container_sha256
    );
    assert_eq!(loaded.view()["schema_version"], 1);
    assert_eq!(loaded.view()["package_schema_version"], 2);
    assert_eq!(loaded.methods_requirements()[0].abi_min_minor, 14);
    for record in &loaded
        .predictor_package()
        .unwrap()
        .execution_bundle
        .refit_artifacts
    {
        let raw = &loaded
            .predictor_package()
            .unwrap()
            .execution_bundle
            .raw_artifact_payloads[&record.artifact.id];
        assert_eq!(
            loaded.state_member(record.artifact.id.as_str()).unwrap(),
            raw
        );
        assert!(
            base.member(record.artifact.uri.as_deref().unwrap())
                .is_none(),
            "a logical package URI must not become a physical member alias"
        );
    }
}

#[test]
fn standalone_child_uses_full_refit_without_fabricated_parent_or_score() {
    let base = native_package_container(None, CHILD, "transport:child", N4aMemoryBudget::default())
        .unwrap();
    let loaded = read_native_package(&base, None).unwrap();
    assert_eq!(loaded.portable_refit_package().unwrap(), CHILD);
    assert!(loaded.training_outcome_bytes().is_err());
    assert_eq!(loaded.view()["root_artifact"], "full_refit");
    assert!(!base
        .manifest()
        .links
        .iter()
        .any(|link| link.relation == "refit_of"));
    assert!(!base
        .manifest()
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == ArtifactKind::Score));
    assert!(read_native_package_for_operation(
        &base,
        None,
        "calibrate-external",
        N4aMemoryBudget::default()
    )
    .is_err());
}

#[test]
fn foreign_content_and_unrelated_entry_features_do_not_enter_scientific_admission() {
    let base = mixed(&source());
    let loaded = read_native_package(&base, Some("predict-external")).unwrap();
    assert_eq!(loaded.portable_predictor_package().unwrap(), PACKAGE);
    assert!(!loaded.members().contains_key("foreign/opaque.bin"));
    assert_eq!(
        base.member("foreign/opaque.bin").unwrap().bytes,
        b"\xffnot JSON\x00"
    );
    assert!(read_native_package(&base, Some("foreign-operation")).is_err());
}

#[test]
fn multiple_native_entries_require_selection_and_apply_only_selected_features() {
    let source = source();
    let mut manifest = source.manifest().clone();
    let mut extra = manifest.entrypoints[0].clone();
    extra.id = "other-native-entry".into();
    extra.required_features.push("future.runtime@7".into());
    manifest.entrypoints.push(extra);
    let base = rewrite(&source, manifest, &[]);
    assert!(read_native_package(&base, None)
        .unwrap_err()
        .to_string()
        .contains("ambiguous"));
    read_native_package(&base, Some("predict-external")).unwrap();
    assert!(read_native_package(&base, Some("other-native-entry"))
        .unwrap_err()
        .to_string()
        .contains("UnknownRequiredFeatures"));
    let mut manifest = base.manifest().clone();
    manifest.required_features.push("future.global@2".into());
    let global = rewrite(&base, manifest, &[]);
    assert!(read_native_package(&global, Some("predict-external")).is_err());
}

#[test]
fn rehashed_companion_and_injected_dag_link_refuse_selected_reconstruction() {
    let base = source();
    let path = match &base
        .manifest()
        .artifacts
        .iter()
        .find(|artifact| artifact.id == "bundle")
        .unwrap()
        .storage
    {
        N4aStorage::Embedded { root_member, .. } => root_member.clone(),
        _ => unreachable!(),
    };
    let corrupt = rewrite(&base, base.manifest().clone(), &[(&path, b"{}")]);
    // Physical integrity passed above. DAG must reject the scientific companion.
    assert!(read_native_package(&corrupt, None).is_err());
    let mut manifest = base.manifest().clone();
    manifest.links.push(N4aLink {
        id: "injected".into(),
        owner: "dag-ml".into(),
        relation: "produced_by".into(),
        source_artifact: "model".into(),
        target_artifact: "result".into(),
    });
    let corrupt = rewrite(&base, manifest, &[]);
    assert!(read_native_package(&corrupt, None).is_err());
}

#[test]
fn file_handle_reuses_metadata_and_writer_never_replaces_destination() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("native.n4a");
    let reference = write_native_package_file(
        &path,
        Some(OUTCOME),
        PACKAGE,
        "transport:file",
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let file = open_n4a_file(&path, N4aMemoryBudget::default()).unwrap();
    let loaded =
        read_native_package_from_file(&file, None, "predict-external", file.memory_budget())
            .unwrap();
    assert_eq!(
        loaded.reference().archive_sha256(),
        reference.archive_sha256()
    );
    assert!(read_native_package_from_file(
        &file,
        None,
        "predict-external",
        N4aMemoryBudget::bytes(0)
    )
    .is_err());
    let original = std::fs::read(&path).unwrap();
    assert!(write_native_package_file(
        &path,
        Some(OUTCOME),
        PACKAGE,
        "replacement",
        N4aMemoryBudget::default()
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn fixed_native_root_collision_is_explicit_and_generic_storage_remains_open() {
    let base = source();
    let mut manifest = base.manifest().clone();
    let mut second = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.id == "model")
        .unwrap()
        .clone();
    second.id = "second-model".into();
    // A reference avoids double member ownership; generic storage permits it.
    second.storage = N4aStorage::Reference { root_sha256: None };
    manifest.artifacts.push(second);
    let base = rewrite(&base, manifest, &[]);
    assert!(read_native_package(&base, None)
        .unwrap_err()
        .to_string()
        .contains("collision"));
}

#[test]
fn opaque_package2_evidence_refuses_native_and_conformal_admission_before_methods_runtime() {
    let outcome = b"opaque:dagml/training_outcome.json";
    let package = b"opaque:dagml/portable_predictor_package.json";
    let error = native_package_container(
        Some(outcome),
        package,
        "native:storage-fixture",
        N4aMemoryBudget::host_allocator(),
    )
    .expect_err("opaque training evidence cannot authorize native or conformal replay");
    assert_eq!(
        error,
        N4aError::Manifest(
            concat!(
                "DAG native package closure refused: invalid facet payload: ",
                "invalid JSON at UTF-8 byte 0: expected a JSON value"
            )
            .into()
        )
    );
    assert!(!error
        .to_string()
        .contains("cannot configure the Methods runtime"));
}

#[test]
fn opaque_package3_refuses_native_admission_before_methods_runtime() {
    let package = b"opaque:dagml/portable_refit_package.json";
    let error = native_package_container(
        None,
        package,
        "native:v3-storage-fixture",
        N4aMemoryBudget::host_allocator(),
    )
    .expect_err("opaque storage fixture cannot admit a native Package3 proof");
    assert_eq!(
        error,
        N4aError::Manifest(
            concat!(
                "DAG native package closure refused: invalid facet payload: ",
                "invalid JSON at UTF-8 byte 0: expected a JSON value"
            )
            .into()
        )
    );
    assert!(!error
        .to_string()
        .contains("cannot configure the Methods runtime"));
}
