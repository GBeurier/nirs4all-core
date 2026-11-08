//! Owner-neutral `.n4a` exchange: write, read once, byte-exact members, immutable
//! update and refusals over the shared opaque host fixture. No Methods runtime,
//! no fitting, no deserialization of host payloads. A host model is only bytes.
//!
//! Fixture: `tests/parity/fixtures/n4a_host_opaque_v1`. Its container digest is
//! recorded from the first real CLI run; the golden assertions are mandatory and
//! fail while `expected.json` still holds `null`.
use std::path::{Path, PathBuf};

use nirs4all::archive_command::execute_archive_command;
use nirs4all::n4a::{
    open_owned_n4a_bytes, parse_n4a_manifest, write_n4a_bytes, write_n4a_update, N4aError,
    N4aMemberInput, N4aMemoryBudget, N4aWriteRequest, OperationRefusal, OwnedN4a,
};
use nirs4all::n4a_owner_portable::n4a_container_view;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/fixtures/n4a_host_opaque_v1")
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
/// `(archive path, bytes)` for a fixture `members.json`, resolved like the CLI.
fn members(list: &Path) -> Vec<(String, Vec<u8>)> {
    let directory = list.parent().unwrap();
    json_file(list)
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["path"].as_str().unwrap().to_owned(),
                std::fs::read(directory.join(entry["file"].as_str().unwrap())).unwrap(),
            )
        })
        .collect()
}
fn inputs(members: &[(String, Vec<u8>)]) -> Vec<N4aMemberInput<'_>> {
    members
        .iter()
        .map(|(path, bytes)| N4aMemberInput { path, bytes })
        .collect()
}
fn write_fixture() -> (Vec<u8>, Vec<(String, Vec<u8>)>) {
    let dir = fixture();
    let manifest = parse_n4a_manifest(
        &std::fs::read(dir.join("manifest.json")).unwrap(),
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let files = members(&dir.join("members.json"));
    let (receipt, bytes) = write_n4a_bytes(
        N4aWriteRequest {
            manifest,
            members: inputs(&files),
        },
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert_eq!(receipt.byte_length as usize, bytes.len());
    (bytes, files)
}
/// Mandatory golden comparison: a `null` (pending) expectation is a failure.
fn assert_golden(label: &str, section: &Value, container: &OwnedN4a) {
    let text = |key: &str| {
        section[key].as_str().unwrap_or_else(|| {
            panic!("expected.json {label}.{key} is pending: Root must record the real CLI output first")
        })
    };
    let receipt = container.receipt();
    assert_eq!(
        receipt.container_sha256,
        text("container_sha256"),
        "{label}"
    );
    assert_eq!(receipt.manifest_sha256, text("manifest_sha256"), "{label}");
    let length = section["byte_length"].as_u64().unwrap_or_else(|| {
        panic!("expected.json {label}.byte_length is pending: Root must record the real CLI output first")
    });
    assert_eq!(receipt.byte_length, length, "{label}");
    assert_eq!(container.archive_bytes().len() as u64, length, "{label}");
}
fn open(bytes: &[u8]) -> OwnedN4a {
    open_owned_n4a_bytes(
        bytes.to_vec().into_boxed_slice(),
        N4aMemoryBudget::default(),
    )
    .unwrap()
}
fn update_manifest() -> nirs4all::n4a::N4aManifest {
    parse_n4a_manifest(
        &std::fs::read(fixture().join("update/manifest.json")).unwrap(),
        N4aMemoryBudget::default(),
    )
    .unwrap()
}

#[test]
fn write_open_and_read_every_member_byte_exact() {
    let (bytes, files) = write_fixture();
    let expected = json_file(&fixture().join("expected.json"));
    let container = open(&bytes);
    assert_golden("container", &expected["container"], &container);
    assert_eq!(container.archive_bytes(), &bytes[..]);
    for (path, original) in &files {
        assert_eq!(container.member(path).unwrap().bytes, &original[..]);
    }
    for entry in expected["members"].as_array().unwrap() {
        let view = container.member(entry["path"].as_str().unwrap()).unwrap();
        assert_eq!(
            view.bytes.len() as u64,
            entry["byte_length"].as_u64().unwrap()
        );
        assert_eq!(sha(view.bytes), entry["sha256"].as_str().unwrap());
    }
    let model = container.member("host/model.bin").unwrap().bytes;
    assert_eq!(&model[..2], b"\x80\x04", "inert pickle prefix is preserved");
    assert!(model.contains(&0) && model.contains(&0xff) && model.windows(2).any(|w| w == b"\r\n"));
    assert_eq!(
        container
            .artifact_members("model")
            .unwrap()
            .iter()
            .map(|view| view.path)
            .collect::<Vec<_>>(),
        ["host/model.bin"]
    );
    // A reference artifact owns no physical member; an unknown artifact is absent.
    assert!(container
        .artifact_members("dataset-ref")
        .unwrap()
        .is_empty());
    assert!(container.artifact_members("absent").is_none());
    assert!(container.member("host/absent.bin").is_none());
    // Same inputs always produce the same container.
    let (again, _) = write_fixture();
    assert_eq!(again, bytes);
}

#[test]
fn view_is_readable_storage_only_and_unknown_owners_stay_opaque() {
    let (bytes, _) = write_fixture();
    let view = n4a_container_view(&open(&bytes));
    assert_eq!(view["executability"], "not_evaluated");
    assert_eq!(view["readability"]["storage"], "verified");
    assert_eq!(
        view["readability"]["container_sha256"],
        sha(&bytes).as_str()
    );
    assert_eq!(
        view["readability"]["training_view"]["status"],
        "not_available"
    );
    let semantics = &view["readability"]["semantics"];
    assert_eq!(semantics["status"], "report_only");
    for artifact in semantics["artifacts"].as_array().unwrap() {
        assert_eq!(artifact["content"], "opaque", "{artifact}");
    }
}

#[test]
fn opaque_entrypoint_grants_no_operation() {
    let (bytes, _) = write_fixture();
    let container = open(&bytes);
    let binding = container.operation_binding("host-predict").unwrap();
    assert_eq!(binding.owner, "host.example");
    assert_eq!(
        container.require_operation("host-predict", None).err(),
        Some(OperationRefusal::AbsentAdmission)
    );
    assert_eq!(
        container.operation_binding("absent").err(),
        Some(OperationRefusal::UnknownEntrypoint)
    );
}

/// The public CLI refuses prediction on the opaque host entrypoint with the OWNER
/// refusal. The Methods library is a deliberately missing sentinel, so reaching
/// the runtime would surface a different error and fail this test.
#[test]
fn cli_predict_on_the_opaque_entrypoint_is_an_owner_refusal_not_a_runtime_error() {
    let work = tempfile::tempdir().unwrap();
    let (bytes, _) = write_fixture();
    let archive = work.path().join("host.n4a");
    std::fs::write(&archive, &bytes).unwrap();
    let sentinel = "/missing/n4a-exchange/libn4m.so";
    let message = run(&[
        "n4a-predict",
        "--archive",
        s(&archive),
        "--expected-archive-sha256",
        &sha(&bytes),
        "--entrypoint",
        "host-predict",
        "--source-id",
        "source",
        "--partition",
        "test",
        "--request-id",
        "opaque-request",
        "--run-id",
        "opaque-run",
        "--methods-library",
        sentinel,
        "--methods-library-sha256",
        &"0".repeat(64),
        "--output",
        s(&work.path().join("predict.json")),
    ])
    .expect_err("an opaque host entrypoint must be refused");
    assert!(
        message.contains("owner content for `model` is unresolved"),
        "expected the owner refusal, got: {message}"
    );
    assert!(
        !message.contains("Methods owner runtime is unavailable") && !message.contains(sentinel),
        "the refusal must precede the Methods runtime, got: {message}"
    );
    assert!(!work.path().join("predict.json").exists());
}

#[test]
fn update_creates_a_new_container_and_never_touches_the_base() {
    let (base_bytes, files) = write_fixture();
    let base = open(&base_bytes);
    let before = base.archive_bytes().to_vec();
    let replacement = members(&fixture().join("update/members.json"));
    assert_eq!(replacement.len(), 1);
    let (receipt, updated) = write_n4a_update(
        &base,
        update_manifest(),
        inputs(&replacement),
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert_eq!(base.archive_bytes(), &before[..]);
    assert_ne!(receipt.container_sha256, base.receipt().container_sha256);
    let updated = open(&updated);
    let expected = json_file(&fixture().join("expected.json"));
    assert_golden(
        "update.container",
        &expected["update"]["container"],
        &updated,
    );
    assert_eq!(
        updated.member("host/model.bin").unwrap().bytes,
        &replacement[0].1[..]
    );
    for path in [
        "host/recipe.json",
        "host/data.csv",
        "host/scores.json",
        "host/results.json",
    ] {
        let original = &files.iter().find(|(p, _)| p == path).unwrap().1;
        assert_eq!(updated.member(path).unwrap().bytes, &original[..], "{path}");
    }
    assert!(updated.member("host/foreign-model.bin").is_none());
    assert!(updated.artifact_members("foreign-model").is_none());
}

#[test]
fn update_and_write_refuse_ambiguous_inputs() {
    let (base_bytes, _) = write_fixture();
    let base = open(&base_bytes);
    let replacement = members(&fixture().join("update/members.json"));
    let budget = N4aMemoryBudget::default();
    // Undeclared replacement.
    let extra = [(String::from("host/undeclared.bin"), vec![1u8])];
    let mut undeclared = inputs(&replacement);
    undeclared.extend(inputs(&extra));
    assert!(write_n4a_update(&base, update_manifest(), undeclared, budget).is_err());
    // Duplicate replacement is refused, never collapsed.
    let mut duplicated = inputs(&replacement);
    duplicated.extend(inputs(&replacement));
    assert!(write_n4a_update(&base, update_manifest(), duplicated, budget).is_err());
    // A declared member that is neither replaced nor in the base.
    let mut manifest = update_manifest();
    let mut ghost = manifest.members[0].clone();
    ghost.path = "host/ghost.bin".into();
    manifest.members.push(ghost.clone());
    manifest.artifacts[0].storage = nirs4all::n4a::N4aStorage::Embedded {
        root_member: "host/recipe.json".into(),
        member_paths: vec!["host/recipe.json".into(), ghost.path],
    };
    assert!(write_n4a_update(&base, manifest, inputs(&replacement), budget).is_err());
    // Plain write refuses a duplicate input and an undeclared input.
    let (_, files) = write_fixture();
    let mut doubled = inputs(&files);
    doubled.push(doubled[0]);
    let request = N4aWriteRequest {
        manifest: parse_n4a_manifest(
            &std::fs::read(fixture().join("manifest.json")).unwrap(),
            budget,
        )
        .unwrap(),
        members: doubled,
    };
    assert!(write_n4a_bytes(request, budget).is_err());
}

#[test]
fn tampered_bytes_future_schema_and_hostile_manifests_are_refused() {
    let (mut bytes, _) = write_fixture();
    let model = open(&bytes).member("host/model.bin").unwrap().range;
    bytes[model.start] ^= 0xff;
    assert!(open_owned_n4a_bytes(bytes.into_boxed_slice(), N4aMemoryBudget::default()).is_err());
    let mut manifest = std::fs::read_to_string(fixture().join("manifest.json")).unwrap();
    manifest = manifest.replace("\"schema_version\": 1,", "\"schema_version\": 2,");
    assert!(matches!(
        parse_n4a_manifest(manifest.as_bytes(), N4aMemoryBudget::default()),
        Err(N4aError::UnsupportedFormat(_))
    ));
    let duplicate = br#"{"schema":"a","schema":"b"}"#;
    assert!(parse_n4a_manifest(duplicate, N4aMemoryBudget::default()).is_err());
}

fn args(items: &[&str]) -> Vec<std::ffi::OsString> {
    items.iter().map(Into::into).collect()
}
fn run(items: &[&str]) -> Result<(), String> {
    execute_archive_command(args(items)).map_err(|error| error.to_string())
}
fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn cli_write_read_update_roundtrip_with_spaces_and_numbered_files() {
    let work = tempfile::tempdir().unwrap();
    let dir = work.path().join("with space");
    std::fs::create_dir(&dir).unwrap();
    let (library_bytes, files) = write_fixture();
    let manifest = fixture().join("manifest.json");
    let list = fixture().join("members.json");
    let base = dir.join("base container.n4a");
    let report = dir.join("write report.json");
    run(&[
        "n4a-write",
        "--manifest",
        s(&manifest),
        "--members",
        s(&list),
        "--destination",
        s(&base),
        "--output",
        s(&report),
    ])
    .unwrap();
    // The CLI and the library API produce the same bytes.
    assert_eq!(std::fs::read(&base).unwrap(), library_bytes);
    let written = json_file(&report);
    assert_eq!(written["operation"], "n4a-write");
    assert_eq!(written["archive_sha256"], sha(&library_bytes).as_str());
    assert_eq!(written["base_archive_sha256"], Value::Null);
    assert_eq!(written["members"].as_array().unwrap().len(), files.len());
    // No replacement of an existing destination or report.
    assert!(run(&[
        "n4a-write",
        "--manifest",
        s(&manifest),
        "--members",
        s(&list),
        "--destination",
        s(&base),
        "--output",
        s(&dir.join("other.json")),
    ])
    .is_err());
    assert_eq!(std::fs::read(&base).unwrap(), library_bytes);

    // Read everything: fixed names, report maps archive path -> numbered file.
    let out = dir.join("all members");
    let read_report = dir.join("read report.json");
    run(&[
        "n4a-read",
        "--archive",
        s(&base),
        "--expected-archive-sha256",
        &sha(&library_bytes),
        "--directory",
        s(&out),
        "--output",
        s(&read_report),
    ])
    .unwrap();
    let read = json_file(&read_report);
    assert_eq!(read["selection"]["kind"], "all");
    assert_eq!(read["manifest_file"], "manifest.json");
    assert_eq!(
        std::fs::read(out.join("manifest.json")).unwrap(),
        open(&library_bytes).manifest_bytes()
    );
    for (index, entry) in read["members"].as_array().unwrap().iter().enumerate() {
        assert_eq!(entry["file"], format!("member-{index:06}.bin").as_str());
        let expected = &files
            .iter()
            .find(|(p, _)| p == entry["path"].as_str().unwrap())
            .unwrap()
            .1;
        assert_eq!(
            &std::fs::read(out.join(entry["file"].as_str().unwrap())).unwrap(),
            expected
        );
    }
    // Archive paths never became directories.
    assert!(!out.join("host").exists());
    let names = std::fs::read_dir(&out).unwrap().count();
    assert_eq!(names, 1 + files.len());

    // Single member and artifact selections.
    let one = dir.join("one");
    run(&[
        "n4a-read",
        "--archive",
        s(&base),
        "--member",
        "host/model.bin",
        "--directory",
        s(&one),
        "--output",
        s(&dir.join("one.json")),
    ])
    .unwrap();
    assert_eq!(
        std::fs::read(one.join("member-000000.bin")).unwrap(),
        files.iter().find(|(p, _)| p == "host/model.bin").unwrap().1
    );
    let artifact = dir.join("artifact");
    run(&[
        "n4a-read",
        "--archive",
        s(&base),
        "--artifact",
        "dataset-ref",
        "--directory",
        s(&artifact),
        "--output",
        s(&dir.join("artifact.json")),
    ])
    .unwrap();
    assert_eq!(
        json_file(&dir.join("artifact.json"))["members"],
        Value::Array(Vec::new())
    );
    // Refusals leave nothing published.
    let refused = dir.join("refused");
    for extra in [["--member", "host/absent.bin"], ["--artifact", "absent"]] {
        assert!(run(&[
            "n4a-read",
            "--archive",
            s(&base),
            extra[0],
            extra[1],
            "--directory",
            s(&refused),
            "--output",
            s(&dir.join("refused.json")),
        ])
        .is_err());
        assert!(!refused.exists() && !dir.join("refused.json").exists());
    }
    assert!(run(&[
        "n4a-read",
        "--archive",
        s(&base),
        "--member",
        "host/model.bin",
        "--artifact",
        "model",
        "--directory",
        s(&refused),
        "--output",
        s(&dir.join("refused.json")),
    ])
    .is_err());
    assert!(run(&[
        "n4a-read",
        "--archive",
        s(&base),
        "--expected-archive-sha256",
        &"0".repeat(64),
        "--directory",
        s(&refused),
        "--output",
        s(&dir.join("refused.json")),
    ])
    .is_err());
    assert!(!refused.exists());

    // Update from the immutable base.
    let updated = dir.join("updated.n4a");
    let update_manifest = fixture().join("update/manifest.json");
    let update_list = fixture().join("update/members.json");
    run(&[
        "n4a-write",
        "--manifest",
        s(&update_manifest),
        "--members",
        s(&update_list),
        "--archive",
        s(&base),
        "--expected-archive-sha256",
        &sha(&library_bytes),
        "--destination",
        s(&updated),
        "--output",
        s(&dir.join("update.json")),
    ])
    .unwrap();
    assert_eq!(std::fs::read(&base).unwrap(), library_bytes);
    let update_report = json_file(&dir.join("update.json"));
    assert_eq!(
        update_report["base_archive_sha256"],
        sha(&library_bytes).as_str()
    );
    let after = open(&std::fs::read(&updated).unwrap());
    assert!(after.member("host/foreign-model.bin").is_none());
    // An undeclared replacement and a wrong base digest are refused.
    assert!(run(&[
        "n4a-write",
        "--manifest",
        s(&update_manifest),
        "--members",
        s(&list),
        "--archive",
        s(&base),
        "--destination",
        s(&dir.join("bad.n4a")),
        "--output",
        s(&dir.join("bad.json")),
    ])
    .is_err());
    assert!(run(&[
        "n4a-write",
        "--manifest",
        s(&update_manifest),
        "--members",
        s(&update_list),
        "--archive",
        s(&base),
        "--expected-archive-sha256",
        &"0".repeat(64),
        "--destination",
        s(&dir.join("bad2.n4a")),
        "--output",
        s(&dir.join("bad2.json")),
    ])
    .is_err());
    assert!(!dir.join("bad.n4a").exists() && !dir.join("bad2.n4a").exists());

    // Inspect uses the shared view and stays storage-only.
    run(&[
        "n4a-inspect",
        "--archive",
        s(&base),
        "--output",
        s(&dir.join("inspect.json")),
    ])
    .unwrap();
    let inspect = json_file(&dir.join("inspect.json"));
    assert_eq!(inspect["operation"], "n4a-inspect");
    assert_eq!(inspect["executability"], "not_evaluated");
}
