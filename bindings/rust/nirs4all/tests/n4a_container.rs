//! Synthetic transport-only qualification. These bytes are not executable models.
use nirs4all::n4a::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const PATH: &str = "opaque/café.bin";
const PAYLOAD: &[u8] = b"\0\xff\x80\n18446744073709551615\r\n";
fn manifest(path: &str) -> Value {
    json!({"schema":N4A_SCHEMA,"schema_version":1,"minimum_reader_version":1,
        "container_id":"storage-only:synthetic","required_features":[],"physical_profile":N4A_PHYSICAL_PROFILE,
        "members":[{"path":path,"byte_length":0,"sha256":"writer-derives-this"}],
        "artifacts":[{"id":"opaque","kind":"attachment","owner":"synthetic-test",
            "content_type":{"id":"synthetic.opaque.v9","version":9},
            "identity":{"owner":"synthetic-test","schema":"synthetic.identity.v17","schema_version":17,
                "fingerprint_profile":"synthetic-only.no-scientific-validation","fingerprint":"opaque-owner-test-token"},
            "storage":{"kind":"embedded","root_member":path,"member_paths":[path]},"requirements":null}],
        "links":[],"entrypoints":[{"id":"inspect-owner","owner":"synthetic-test","operation":"owner-action",
            "root_artifact":"opaque","input_artifacts":["opaque"],"required_features":[],"requirements":null}]})
}
fn write(
    value: Value,
    parts: &[(&str, &[u8])],
    budget: N4aMemoryBudget,
) -> Result<(N4aReceipt, Vec<u8>), N4aError> {
    write_n4a_bytes(
        N4aWriteRequest {
            manifest: serde_json::from_value(value).unwrap(),
            members: parts
                .iter()
                .map(|(path, bytes)| N4aMemberInput { path, bytes })
                .collect(),
        },
        budget,
    )
}
fn sample() -> Vec<u8> {
    write(
        manifest(PATH),
        &[(PATH, PAYLOAD)],
        N4aMemoryBudget::default(),
    )
    .unwrap()
    .1
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn u16at(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}
fn u32at(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn put16(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 2].copy_from_slice(&(value as u16).to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: usize) {
    bytes[at..at + 4].copy_from_slice(&(value as u32).to_le_bytes());
}
fn crc(bytes: &[u8]) -> usize {
    let mut value = !0u32;
    for byte in bytes {
        value ^= u32::from(*byte);
        for _ in 0..8 {
            value = (value >> 1) ^ if value & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    (!value) as usize
}
// Test-only mutators for a known valid production ZIP. No alternate reader is used.
#[derive(Debug)]
struct Record {
    central: usize,
    local: usize,
    name: String,
    data: std::ops::Range<usize>,
}
fn records(bytes: &[u8]) -> Vec<Record> {
    let eocd = bytes.len() - 22;
    let mut at = u32at(bytes, eocd + 16);
    (0..u16at(bytes, eocd + 10))
        .map(|_| {
            let local = u32at(bytes, at + 42);
            let name_len = u16at(bytes, at + 28);
            let name = String::from_utf8(bytes[at + 46..at + 46 + name_len].to_vec()).unwrap();
            let start = local + 30 + name_len;
            let record = Record {
                central: at,
                local,
                name,
                data: start..start + u32at(bytes, at + 24),
            };
            at += 46 + name_len;
            record
        })
        .collect()
}
fn replace_member(bytes: &[u8], name: &str, replacement: &[u8]) -> Vec<u8> {
    let entries = records(bytes);
    let target = entries.iter().find(|e| e.name == name).unwrap();
    let delta = replacement.len() as isize - target.data.len() as isize;
    let shift = |value: usize| value.checked_add_signed(delta).unwrap();
    let mut out = bytes.to_vec();
    out.splice(target.data.clone(), replacement.iter().copied());
    for entry in &entries {
        let central = shift(entry.central);
        let local = if entry.local >= target.data.end {
            shift(entry.local)
        } else {
            entry.local
        };
        put32(&mut out, central + 42, local);
        if entry.name == name {
            for pos in [local + 14, central + 16] {
                put32(&mut out, pos, crc(replacement));
            }
            for pos in [local + 18, local + 22, central + 20, central + 24] {
                put32(&mut out, pos, replacement.len());
            }
        }
    }
    let end = out.len() - 22;
    put32(&mut out, end + 16, shift(u32at(bytes, bytes.len() - 6)));
    out
}
fn reject_value(mutator: impl FnOnce(&mut Value)) {
    let bytes = sample();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let mut value = serde_json::to_value(view.manifest()).unwrap();
    mutator(&mut value);
    let bad = replace_member(
        &bytes,
        "manifest.json",
        &serde_json::to_vec(&value).unwrap(),
    );
    assert!(
        open_n4a_bytes(&bad, N4aMemoryBudget::default()).is_err(),
        "accepted {value}"
    );
}

#[test]
fn production_roundtrip_exact_bytes_hashes_and_borrowed_ranges() {
    let raw_json = "{\"u64\":18446744073709551615,\"decimal\":0.12345678901234567890123456789,\"text\":\"é é 雪\"}".as_bytes();
    let mut value = manifest(PATH);
    value["members"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"opaque/numbers.json","byte_length":0,"sha256":"derived"}));
    value["artifacts"][0]["storage"]["member_paths"]
        .as_array_mut()
        .unwrap()
        .push(json!("opaque/numbers.json"));
    let (receipt, bytes) = write(
        value,
        &[(PATH, PAYLOAD), ("opaque/numbers.json", raw_json)],
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert_eq!(receipt.container_sha256, sha(&bytes));
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    assert_eq!(view.receipt(), &receipt);
    assert_eq!(view.receipt().manifest_sha256, sha(view.manifest_bytes()));
    for (path, original) in [(PATH, PAYLOAD), ("opaque/numbers.json", raw_json)] {
        let member = view.member(path).unwrap();
        assert_eq!(member.bytes, original);
        assert_eq!(member.bytes.as_ptr(), bytes[member.range.clone()].as_ptr());
        assert!(member.bytes.as_ptr() >= bytes.as_ptr());
        assert_eq!(
            view.receipt()
                .members
                .iter()
                .find(|m| m.path == path)
                .unwrap()
                .sha256,
            sha(original)
        );
    }
    let manifest_entry = records(&bytes)
        .into_iter()
        .find(|e| e.name == "manifest.json")
        .unwrap();
    assert_eq!(
        view.manifest_bytes().as_ptr(),
        bytes[manifest_entry.data].as_ptr()
    );
    assert_eq!(view.artifact_members("opaque").unwrap().len(), 2);
    assert_eq!(view.inspect().storage, StorageStatus::Verified);
    assert_eq!(view.inspect().semantics, SemanticStatus::Unverified);
    assert_eq!(copy_n4a_bytes(&view), bytes);
    // No owner callback or serializer is supplied, even for invalid owner JSON/binary.
    assert_eq!(view.member(PATH).unwrap().bytes, PAYLOAD);
}

#[test]
fn exact_declared_input_closure_and_derived_inventory() {
    let value = manifest(PATH);
    assert!(write(value.clone(), &[], N4aMemoryBudget::default()).is_err());
    assert!(write(
        value.clone(),
        &[(PATH, PAYLOAD), ("extra", b"")],
        N4aMemoryBudget::default()
    )
    .is_err());
    assert!(write(
        value.clone(),
        &[(PATH, PAYLOAD), (PATH, PAYLOAD)],
        N4aMemoryBudget::default()
    )
    .is_err());
    let (receipt, _) = write(value, &[(PATH, PAYLOAD)], N4aMemoryBudget::default()).unwrap();
    assert_eq!(receipt.members[0].sha256, sha(PAYLOAD));
    assert_eq!(receipt.members[0].byte_length, PAYLOAD.len() as u64);
    reject_value(|v| {
        v["members"][0]["sha256"] = json!("0".repeat(64));
    });
    reject_value(|v| {
        v["members"][0]["byte_length"] = json!(1);
    });
}

#[test]
fn exact_manifest_shape_required_nullables_unknown_fields_and_versions() {
    for field in [
        "schema",
        "schema_version",
        "minimum_reader_version",
        "container_id",
        "required_features",
        "physical_profile",
        "members",
        "artifacts",
        "links",
        "entrypoints",
    ] {
        reject_value(|v| {
            v.as_object_mut().unwrap().remove(field);
        });
    }
    for field in ["schema_version", "minimum_reader_version"] {
        for bad in [
            json!(true),
            json!("1"),
            json!(1.0),
            json!(0),
            json!(2),
            Value::Null,
            json!(-1),
        ] {
            reject_value(|v| v[field] = bad);
        }
    }
    reject_value(|v| v["schema"] = json!("nirs4all.archive_workspace.v3"));
    reject_value(|v| v["physical_profile"] = json!("zip64"));
    reject_value(|v| v["owner_admission"] = json!({"verified":true}));
    for pointer in [
        "/artifacts/0",
        "/artifacts/0/identity",
        "/artifacts/0/content_type",
        "/artifacts/0/storage",
        "/members/0",
        "/entrypoints/0",
    ] {
        reject_value(|v| {
            v.pointer_mut(pointer).unwrap()["unknown"] = json!(true);
        });
    }
    for pointer in ["/artifacts/0", "/entrypoints/0"] {
        reject_value(|v| {
            v.pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove("requirements");
        });
        reject_value(|v| {
            v.pointer_mut(pointer).unwrap()["requirements"] = json!({});
        });
        reject_value(|v| {
            v.pointer_mut(pointer).unwrap()["requirements"] =
                json!({"artifact_id":"opaque","extra":true});
        });
        reject_value(|v| {
            v.pointer_mut(pointer).unwrap()["requirements"] = json!({"artifact_id":"absent"});
        });
        reject_value(|v| {
            v.pointer_mut(pointer).unwrap()["requirements"] = json!("opaque");
        });
    }
    for pointer in [
        "/artifacts/0/identity/owner",
        "/artifacts/0/identity/schema",
        "/artifacts/0/identity/fingerprint_profile",
        "/artifacts/0/identity/fingerprint",
        "/artifacts/0/content_type/id",
    ] {
        reject_value(|v| *v.pointer_mut(pointer).unwrap() = json!(""));
    }
    for pointer in [
        "/artifacts/0/identity/schema_version",
        "/artifacts/0/content_type/version",
    ] {
        reject_value(|v| *v.pointer_mut(pointer).unwrap() = json!(0));
        reject_value(|v| *v.pointer_mut(pointer).unwrap() = json!(true));
    }
    reject_value(|v| v["artifacts"][0]["identity"]["owner"] = json!("different"));
    reject_value(|v| v["artifacts"][0]["kind"] = json!("calibration"));
    reject_value(|v| v["members"][0]["sha256"] = json!("A".repeat(64)));
    reject_value(|v| v["members"][0]["byte_length"] = json!(true));
    reject_value(|v| v["members"][0]["byte_length"] = json!(1.5));
    reject_value(|v| v["members"][0]["byte_length"] = json!(u64::MAX));
    reject_value(|v| v["members"] = json!([]));
}

#[test]
fn duplicate_keys_including_escaped_aliases_are_refused() {
    let bytes = sample();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    for prefix in [
        br#"{"schema_version":1,"#.as_slice(),
        br#"{"\u0073chema_version":1,"#.as_slice(),
    ] {
        let mut raw = prefix.to_vec();
        raw.extend_from_slice(&view.manifest_bytes()[1..]);
        let bad = replace_member(&bytes, "manifest.json", &raw);
        assert!(open_n4a_bytes(&bad, N4aMemoryBudget::default()).is_err());
    }
}

#[test]
fn ownership_reference_link_and_entrypoint_closure_are_structural() {
    reject_value(|v| v["artifacts"] = json!([]));
    reject_value(|v| v["artifacts"][0]["storage"]["root_member"] = json!("absent"));
    reject_value(|v| v["artifacts"][0]["storage"]["member_paths"] = json!([]));
    reject_value(|v| v["artifacts"][0]["storage"]["member_paths"] = json!([PATH, PATH]));
    reject_value(|v| v["artifacts"][0]["storage"]["member_paths"] = json!([PATH, "absent"]));
    reject_value(|v| {
        let a = v["artifacts"][0].clone();
        v["artifacts"].as_array_mut().unwrap().push(a);
    });
    reject_value(|v| {
        let mut a = v["artifacts"][0].clone();
        a["id"] = json!("second-owner");
        v["artifacts"].as_array_mut().unwrap().push(a);
    });
    reject_value(|v| v["entrypoints"][0]["root_artifact"] = json!("absent"));
    reject_value(|v| v["entrypoints"][0]["input_artifacts"] = json!(["absent"]));
    reject_value(|v| v["entrypoints"][0]["input_artifacts"] = json!(["opaque", "opaque"]));
    reject_value(|v| {
        let e = v["entrypoints"][0].clone();
        v["entrypoints"].as_array_mut().unwrap().push(e);
    });
    reject_value(|v| v["required_features"] = json!(["x", "x"]));
    reject_value(|v| v["entrypoints"][0]["required_features"] = json!([""]));
    reject_value(
        |v| v["links"] = json!([{"id":"link","owner":"owner","relation":"any","source_artifact":"opaque","target_artifact":"absent"}]),
    );
    let mut v = manifest(PATH);
    let mut reference = v["artifacts"][0].clone();
    reference["id"] = json!("external");
    reference["storage"] = json!({"kind":"reference"});
    v["artifacts"].as_array_mut().unwrap().push(reference);
    v["artifacts"][0]["requirements"] = json!({"artifact_id":"external"});
    v["entrypoints"][0]["root_artifact"] = json!("external");
    v["entrypoints"][0]["requirements"] = json!({"artifact_id":"external"});
    v["links"] = json!([{"id":"custom-link","owner":"other-owner","relation":"outside-DAG-vocabulary","source_artifact":"opaque","target_artifact":"external"}]);
    let (_, bytes) = write(v.clone(), &[(PATH, PAYLOAD)], N4aMemoryBudget::default()).unwrap();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    assert!(view.artifact_members("external").unwrap().is_empty());
    // Core does not equate identity and content schemas/versions or infer root kinds.
    assert_eq!(view.manifest().artifacts[0].identity.schema_version, 17);
    assert_eq!(view.manifest().artifacts[0].content_type.version, 9);
    assert_eq!(
        view.require_operation("inspect-owner", None).unwrap_err(),
        OperationRefusal::AbsentAdmission
    );
    for storage in [
        json!({"kind":"reference","root_sha256":null}),
        json!({"kind":"reference","root_sha256":"bad"}),
        json!({"kind":"reference","member_paths":[]}),
    ] {
        let bad = replace_member(
            &bytes,
            "manifest.json",
            &serde_json::to_vec(&{
                let mut m = v.clone();
                m["artifacts"][1]["storage"] = storage;
                m
            })
            .unwrap(),
        );
        assert!(open_n4a_bytes(&bad, N4aMemoryBudget::default()).is_err());
    }
    let mut duplicate = v.clone();
    duplicate["links"]
        .as_array_mut()
        .unwrap()
        .push(v["links"][0].clone());
    assert!(write(duplicate, &[(PATH, PAYLOAD)], N4aMemoryBudget::default()).is_err());
    let mut complete = v;
    complete["artifacts"][1]["storage"]["root_sha256"] = json!(sha(PAYLOAD));
    let (_, bytes) = write(complete, &[(PATH, PAYLOAD)], N4aMemoryBudget::default()).unwrap();
    open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
}

#[test]
fn live_admission_binds_every_operation_context_field_and_status() {
    let bytes = sample();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let binding = view.operation_binding("inspect-owner").unwrap();
    let good = OwnerAdmission {
        binding: binding.clone(),
        status: OwnerAdmissionStatus::Verified,
        supported_features: vec![],
    };
    assert_eq!(
        view.require_operation("inspect-owner", None).unwrap_err(),
        OperationRefusal::AbsentAdmission
    );
    assert_eq!(
        view.require_operation("absent", Some(&good)).unwrap_err(),
        OperationRefusal::UnknownEntrypoint
    );
    assert_eq!(
        view.require_operation("inspect-owner", Some(&good))
            .unwrap()
            .binding(),
        &binding
    );
    for (status, expected) in [
        (
            OwnerAdmissionStatus::Refused,
            OperationRefusal::OwnerRefused,
        ),
        (
            OwnerAdmissionStatus::Unverified,
            OperationRefusal::OwnerUnverified,
        ),
    ] {
        let mut bad = good.clone();
        bad.status = status;
        assert_eq!(
            view.require_operation("inspect-owner", Some(&bad))
                .unwrap_err(),
            expected
        );
    }
    let mut variants = Vec::new();
    for field in 0..11 {
        let mut bad = good.clone();
        match field {
            0 => bad.binding.container_sha256 = "0".repeat(64),
            1 => bad.binding.entrypoint_id = "other".into(),
            2 => bad.binding.owner = "other".into(),
            3 => bad.binding.operation = "other".into(),
            4 => bad.binding.root.id = "other".into(),
            5 => bad.binding.root.owner = "other".into(),
            6 => bad.binding.root.content_type.id = "other".into(),
            7 => bad.binding.root.content_type.version += 1,
            8 => bad.binding.inputs[0].id = "other".into(),
            9 => bad.binding.inputs[0].content_type.version += 1,
            _ => bad.binding.inputs.clear(),
        }
        variants.push(bad);
    }
    for bad in variants {
        assert_eq!(
            view.require_operation("inspect-owner", Some(&bad))
                .unwrap_err(),
            OperationRefusal::BindingMismatch
        );
    }
    let (_, changed) = write(
        manifest(PATH),
        &[(PATH, b"changed opaque bytes")],
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let stale = open_n4a_bytes(&changed, N4aMemoryBudget::default()).unwrap();
    assert_eq!(
        stale
            .require_operation("inspect-owner", Some(&good))
            .unwrap_err(),
        OperationRefusal::BindingMismatch
    );
}

#[test]
fn admission_checks_global_and_selected_features_while_inspection_shows_the_union() {
    let mut v = manifest(PATH);
    v["required_features"] = json!(["global-required"]);
    v["entrypoints"][0]["required_features"] = json!(["feature-a"]);
    let mut other = v["entrypoints"][0].clone();
    other["id"] = json!("other");
    other["required_features"] = json!(["unknown-b"]);
    v["entrypoints"].as_array_mut().unwrap().push(other);
    let (_, bytes) = write(v, &[(PATH, PAYLOAD)], N4aMemoryBudget::default()).unwrap();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    assert_eq!(
        view.inspect().required_features,
        vec!["feature-a", "global-required", "unknown-b"]
    );
    let admitted_a = OwnerAdmission {
        binding: view.operation_binding("inspect-owner").unwrap(),
        status: OwnerAdmissionStatus::Verified,
        supported_features: vec!["global-required".into(), "feature-a".into()],
    };
    // Unknown B cannot disable independently owner-admitted A.
    view.require_operation("inspect-owner", Some(&admitted_a))
        .unwrap();
    let mut admitted_b = OwnerAdmission {
        binding: view.operation_binding("other").unwrap(),
        ..admitted_a.clone()
    };
    assert_eq!(
        view.require_operation("other", Some(&admitted_b))
            .unwrap_err(),
        OperationRefusal::UnknownRequiredFeatures(vec!["unknown-b".into()])
    );
    // Feature support remains a live caller assertion, never producer trust.
    admitted_b.supported_features.push("unknown-b".into());
    view.require_operation("other", Some(&admitted_b)).unwrap();

    let mut missing_a = admitted_a.clone();
    missing_a.supported_features = vec!["global-required".into(), "unknown-b".into()];
    assert_eq!(
        view.require_operation("inspect-owner", Some(&missing_a))
            .unwrap_err(),
        OperationRefusal::UnknownRequiredFeatures(vec!["feature-a".into()])
    );
    for (entry, mut admission) in [("inspect-owner", admitted_a.clone()), ("other", admitted_b)] {
        admission
            .supported_features
            .retain(|feature| feature != "global-required");
        assert_eq!(
            view.require_operation(entry, Some(&admission)).unwrap_err(),
            OperationRefusal::UnknownRequiredFeatures(vec!["global-required".into()])
        );
    }
    assert_eq!(
        view.require_operation("inspect-owner", None).unwrap_err(),
        OperationRefusal::AbsentAdmission
    );
    let mut unverified_a = admitted_a.clone();
    unverified_a.status = OwnerAdmissionStatus::Unverified;
    assert_eq!(
        view.require_operation("inspect-owner", Some(&unverified_a))
            .unwrap_err(),
        OperationRefusal::OwnerUnverified
    );
    // Even with enough features, another entrypoint's admission never suffices.
    let mut wrong_entry = admitted_a;
    wrong_entry.supported_features.push("unknown-b".into());
    assert_eq!(
        view.require_operation("other", Some(&wrong_entry))
            .unwrap_err(),
        OperationRefusal::BindingMismatch
    );
    assert_eq!(view.member(PATH).unwrap().bytes, PAYLOAD);
    assert_eq!(copy_n4a_bytes(&view), bytes);
}

#[test]
fn storage_capacity_450_members_and_explicit_budget_refusal() {
    let paths = (0..450)
        .map(|i| format!("storage/{i:03}.bin"))
        .collect::<Vec<_>>();
    let mut v = manifest(&paths[0]);
    v["members"] = json!(paths
        .iter()
        .map(|p| json!({"path":p,"byte_length":0,"sha256":"derived"}))
        .collect::<Vec<_>>());
    v["artifacts"][0]["storage"]["member_paths"] = json!(paths);
    let parts = paths
        .iter()
        .map(|p| (p.as_str(), b"x".as_slice()))
        .collect::<Vec<_>>();
    let (_, bytes) = write(v, &parts, N4aMemoryBudget::default()).unwrap();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    assert_eq!(view.receipt().members.len(), 450); // Capacity only: no model or fit claim.
    open_n4a_bytes(&bytes, N4aMemoryBudget::bytes(1 << 20)).unwrap();
    // A declared budget is the only sizing refusal, for reader and writer alike.
    for budget in [N4aMemoryBudget::bytes(0), N4aMemoryBudget::bytes(1024)] {
        assert!(matches!(
            open_n4a_bytes(&bytes, budget),
            Err(N4aError::Memory(_))
        ));
        assert!(matches!(
            write(manifest(PATH), &[(PATH, PAYLOAD)], budget),
            Err(N4aError::Memory(_))
        ));
    }
}

#[test]
fn canonical_path_guard_utf8_flags_nfc_and_byte_boundaries() {
    for bad in [
        "e\u{301}.bin",
        "/absolute",
        "C:drive",
        "nested/d:drive",
        "back\\slash",
        "a//b",
        "a/./b",
        "a/../b",
        "a/",
        "\0name",
        "a\nfile",
        "a\u{0085}file",
        "manifest.json",
        "",
    ] {
        assert!(
            write(manifest(bad), &[(bad, b"")], N4aMemoryBudget::default()).is_err(),
            "{bad:?}"
        );
    }
    for good in ["data/雪/é.bin", "case/A", "case/a"] {
        let (_, bytes) = write(
            manifest(good),
            &[(good, PAYLOAD)],
            N4aMemoryBudget::default(),
        )
        .unwrap();
        open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    }
    // The only path length bound is the ZIP name field.
    for length in [513, 65_535] {
        let path = "x".repeat(length);
        let (_, bytes) =
            write(manifest(&path), &[(&path, b"")], N4aMemoryBudget::default()).unwrap();
        open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    }
    let too_long = "x".repeat(65_536);
    assert!(write(
        manifest(&too_long),
        &[(&too_long, b"")],
        N4aMemoryBudget::default()
    )
    .is_err());
    let mut bytes = sample();
    for entry in records(&bytes) {
        assert_eq!(u16at(&bytes, entry.local + 6), 0x800);
        assert_eq!(u16at(&bytes, entry.central + 8), 0x800);
    }
    let unicode = records(&bytes)
        .into_iter()
        .find(|e| e.name == PATH)
        .unwrap();
    put16(&mut bytes, unicode.local + 6, 0);
    put16(&mut bytes, unicode.central + 8, 0);
    assert!(open_n4a_bytes(&bytes, N4aMemoryBudget::default()).is_err());
    let (_, mut ascii) = write(
        manifest("ascii"),
        &[("ascii", PAYLOAD)],
        N4aMemoryBudget::default(),
    )
    .unwrap();
    for entry in records(&ascii) {
        put16(&mut ascii, entry.local + 6, 0);
        put16(&mut ascii, entry.central + 8, 0);
    }
    open_n4a_bytes(&ascii, N4aMemoryBudget::default()).unwrap();
}

#[test]
fn crc_headers_modes_compression_zip64_and_local_central_mismatches_refuse() {
    let bytes = sample();
    let entry = records(&bytes)
        .into_iter()
        .find(|e| e.name == PATH)
        .unwrap();
    let mut bad = bytes.clone();
    bad[entry.data.start] ^= 1;
    assert!(matches!(
        open_n4a_bytes(&bad, N4aMemoryBudget::default()),
        Err(N4aError::Integrity(_))
    ));
    for (at, size, value) in [
        (entry.central + 8, 2, 1),
        (entry.central + 8, 2, 0x808),
        (entry.local + 6, 2, 0),
        (entry.central + 10, 2, 8),
        (entry.central + 6, 2, 45),
        (entry.central + 30, 2, 1),
        (entry.central + 34, 2, 1),
        (entry.central + 38, 4, 0o120777 << 16),
        (entry.central + 38, 4, 0o100755 << 16),
        (entry.central + 38, 4, 0x10),
        (entry.central + 20, 4, 1),
        (entry.central + 24, 4, 1),
        (entry.local + 10, 2, 1),
        (entry.local + 12, 2, 0),
        (entry.central + 12, 2, 1),
        (entry.central + 14, 2, 0),
        (entry.local + 14, 4, 1),
        (entry.local + 18, 4, 1),
        (entry.local + 22, 4, 1),
        (entry.local + 28, 2, 1),
        (entry.central + 24, 4, u32::MAX as usize),
    ] {
        let mut bad = bytes.clone();
        if size == 2 {
            put16(&mut bad, at, value)
        } else {
            put32(&mut bad, at, value)
        };
        assert!(
            open_n4a_bytes(&bad, N4aMemoryBudget::default()).is_err(),
            "offset {at} value {value}"
        );
    }
    let mut bad = bytes.clone();
    bad[entry.local + 30] ^= 1;
    assert!(open_n4a_bytes(&bad, N4aMemoryBudget::default()).is_err());
    let mut bad = bytes.clone();
    bad[entry.central + 46] ^= 1;
    assert!(open_n4a_bytes(&bad, N4aMemoryBudget::default()).is_err());
    for length in [0, 1, 21, bytes.len() - 1] {
        assert!(open_n4a_bytes(&bytes[..length], N4aMemoryBudget::default()).is_err());
    }
}

// Insert an invisible physical byte while fixing all affected offsets. This
// ensures refusal is caused by canonical contiguity, not merely bad offsets.
fn insert_gap(bytes: &[u8], at: usize) -> Vec<u8> {
    let entries = records(bytes);
    let mut out = bytes.to_vec();
    out.insert(at, 0);
    for e in entries {
        put32(
            &mut out,
            e.central + 1 + 42,
            e.local + usize::from(e.local >= at),
        );
    }
    let end = out.len() - 22;
    put32(&mut out, end + 16, u32at(bytes, bytes.len() - 6) + 1);
    out
}
#[test]
fn prefixes_internal_gaps_central_gaps_trailers_comments_and_legacy_bytes_refuse() {
    let bytes = sample();
    let entries = records(&bytes);
    for at in [0, entries[1].local, u32at(&bytes, bytes.len() - 6)] {
        assert!(open_n4a_bytes(&insert_gap(&bytes, at), N4aMemoryBudget::default()).is_err());
    }
    let mut trailer = bytes.clone();
    trailer.extend_from_slice(b"hidden");
    assert!(open_n4a_bytes(&trailer, N4aMemoryBudget::default()).is_err());
    let mut comment = bytes.clone();
    let end = comment.len() - 22;
    put16(&mut comment, end + 20, 1);
    comment.push(b'x');
    assert!(open_n4a_bytes(&comment, N4aMemoryBudget::default()).is_err());
    for old in [
        b"{}".as_slice(),
        br#"{"schema":"nirs4all.archive_workspace.v2"}"#.as_slice(),
        b"PK\x03\x04old",
    ] {
        assert!(open_n4a_bytes(old, N4aMemoryBudget::default()).is_err());
    }
    let oldzip = replace_member(
        &bytes,
        "manifest.json",
        br#"{"schema":"nirs4all.archive_workspace.v2"}"#,
    );
    assert!(open_n4a_bytes(&oldzip, N4aMemoryBudget::default()).is_err());
}

#[test]
fn reader_rejects_unsafe_names_duplicate_physical_members_and_repaired_crc_tampering() {
    for (good, bad) in [
        ("abc", "e\u{301}"),
        ("abc", "C:x"),
        ("abc", "a\nb"),
        ("abc", "a\\b"),
        ("abc", "a//"),
        ("abc", "a/."),
    ] {
        assert_eq!(good.len(), bad.len());
        let (_, bytes) = write(
            manifest(good),
            &[(good, PAYLOAD)],
            N4aMemoryBudget::default(),
        )
        .unwrap();
        let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
        let mut m = serde_json::to_value(view.manifest()).unwrap();
        m["members"][0]["path"] = json!(bad);
        m["artifacts"][0]["storage"]["root_member"] = json!(bad);
        m["artifacts"][0]["storage"]["member_paths"] = json!([bad]);
        let mut changed = replace_member(&bytes, "manifest.json", &serde_json::to_vec(&m).unwrap());
        let e = records(&changed)
            .into_iter()
            .find(|e| e.name == good)
            .unwrap();
        for at in [e.local + 30, e.central + 46] {
            changed[at..at + bad.len()].copy_from_slice(bad.as_bytes());
        }
        assert!(
            open_n4a_bytes(&changed, N4aMemoryBudget::default()).is_err(),
            "reader accepted {bad:?}"
        );
    }
    let mut m = manifest("x");
    m["members"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"y","byte_length":0,"sha256":"derived"}));
    m["artifacts"][0]["storage"]["member_paths"] = json!(["x", "y"]);
    let (_, mut bytes) = write(
        m,
        &[("x", PAYLOAD), ("y", PAYLOAD)],
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let e = records(&bytes).into_iter().find(|e| e.name == "y").unwrap();
    bytes[e.local + 30] = b'x';
    bytes[e.central + 46] = b'x';
    assert!(open_n4a_bytes(&bytes, N4aMemoryBudget::default()).is_err());
    let bytes = sample();
    let mut replacement = PAYLOAD.to_vec();
    replacement[0] ^= 1;
    let repaired = replace_member(&bytes, PATH, &replacement);
    assert!(matches!(
        open_n4a_bytes(&repaired, N4aMemoryBudget::default()),
        Err(N4aError::Integrity(_))
    ));
}

#[test]
fn actual_core_unicode_zip_cross_reads_with_python_stdlib() {
    let bytes = sample();
    for entry in records(&bytes) {
        assert_eq!(
            u16at(&bytes, entry.local + 10),
            0,
            "local time: {}",
            entry.name
        );
        assert_eq!(
            u16at(&bytes, entry.local + 12),
            0x21,
            "local date: {}",
            entry.name
        );
        assert_eq!(
            u16at(&bytes, entry.central + 12),
            0,
            "central time: {}",
            entry.name
        );
        assert_eq!(
            u16at(&bytes, entry.central + 14),
            0x21,
            "central date: {}",
            entry.name
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unicode.n4a");
    std::fs::write(&path, &bytes).unwrap();
    let result = std::process::Command::new("python3")
        .arg("-c")
        .arg(
            r#"
import datetime, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
    assert z.namelist() == ['manifest.json', 'opaque/café.bin'], z.namelist()
    assert z.read('opaque/café.bin') == b'\0\xff\x80\n18446744073709551615\r\n'
    for info in z.infolist():
        assert info.flag_bits == 0x800
        assert info.date_time == (1980, 1, 1, 0, 0, 0), (info.filename, info.date_time)
        assert datetime.datetime(*info.date_time) == datetime.datetime(1980, 1, 1)
    assert z.testzip() is None
print('Python standard zipfile: exact NFC names, raw bytes and valid deterministic datetimes PASS')
"#,
        )
        .arg(path)
        .output()
        .expect("Python 3 is required for this qualification test");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}

#[test]
fn review_canonical_timestamp_admission_checks_every_member() {
    let bytes = sample();
    for entry in records(&bytes) {
        for (time, date) in [(0, 0), (1, 0x21)] {
            let mut bad = bytes.clone();
            put16(&mut bad, entry.local + 10, time);
            put16(&mut bad, entry.local + 12, date);
            put16(&mut bad, entry.central + 12, time);
            put16(&mut bad, entry.central + 14, date);
            assert!(
                matches!(
                    open_n4a_bytes(&bad, N4aMemoryBudget::default()),
                    Err(N4aError::Physical(_))
                ),
                "{} time={time} date={date}",
                entry.name
            );
        }
    }
    let mut all_zero = bytes.clone();
    for entry in records(&bytes) {
        put16(&mut all_zero, entry.local + 12, 0);
        put16(&mut all_zero, entry.central + 14, 0);
    }
    assert!(matches!(
        open_n4a_bytes(&all_zero, N4aMemoryBudget::default()),
        Err(N4aError::Physical(_))
    ));
}

#[test]
fn review_format_discriminator_precedes_strict_shape_and_timestamp_admission() {
    let bytes = sample();
    let view = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    for minimum in [false, true] {
        let mut future = serde_json::to_value(view.manifest()).unwrap();
        if minimum {
            future["minimum_reader_version"] = json!(2);
        } else {
            future["schema_version"] = json!(2);
            future["future_extension"] = json!({"unknown": true});
        }
        let future = replace_member(
            &bytes,
            "manifest.json",
            &serde_json::to_vec(&future).unwrap(),
        );
        assert!(matches!(
            open_n4a_bytes(&future, N4aMemoryBudget::default()),
            Err(N4aError::UnsupportedFormat(_))
        ));
        let mut zero_date = future.clone();
        for entry in records(&future) {
            put16(&mut zero_date, entry.local + 12, 0);
            put16(&mut zero_date, entry.central + 14, 0);
        }
        assert!(matches!(
            open_n4a_bytes(&zero_date, N4aMemoryBudget::default()),
            Err(N4aError::UnsupportedFormat(_))
        ));
        let mut corrupt = future.clone();
        let payload = records(&future)
            .into_iter()
            .find(|entry| entry.name == PATH)
            .unwrap();
        corrupt[payload.data.start] ^= 1;
        // Foreign versions refuse at the manifest, before scanning bulk data.
        // Supported V1 payload integrity remains checked by the reader cases.
        assert!(matches!(
            open_n4a_bytes(&corrupt, N4aMemoryBudget::default()),
            Err(N4aError::UnsupportedFormat(_))
        ));
        let mut flags = future.clone();
        put16(&mut flags, payload.local + 6, 1);
        put16(&mut flags, payload.central + 8, 1);
        assert!(matches!(
            open_n4a_bytes(&flags, N4aMemoryBudget::default()),
            Err(N4aError::Physical(_))
        ));
    }
    let mut current = serde_json::to_value(view.manifest()).unwrap();
    current["illegal_extension"] = json!(true);
    let current = replace_member(
        &bytes,
        "manifest.json",
        &serde_json::to_vec(&current).unwrap(),
    );
    assert!(matches!(
        open_n4a_bytes(&current, N4aMemoryBudget::default()),
        Err(N4aError::Manifest(_))
    ));
}

#[test]
fn review_real_historical_writers_are_unsupported_before_canonical_dates() {
    // Required one-time captures from the actual old builders. Missing fixtures
    // are a hard preparation dependency, never replaced with synthetic ZIP bytes.
    let v2: &[u8] = include_bytes!("fixtures/historical_physical/v2.n4a");
    let v3: &[u8] = include_bytes!("fixtures/historical_physical/v3.n4a");
    let provenance: Value =
        serde_json::from_str(include_str!("fixtures/historical_physical/PROVENANCE.json")).unwrap();
    for (path, bytes) in [("v2.n4a", v2), ("v3.n4a", v3)] {
        let capture = provenance["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["path"] == path)
            .expect("captured fixture provenance");
        assert_eq!(capture["sha256"].as_str().unwrap(), sha(bytes));
        assert_eq!(capture["byte_length"].as_u64().unwrap(), bytes.len() as u64);
        for entry in records(bytes) {
            assert_eq!(u16at(bytes, entry.local + 6), 0);
            assert_eq!(u16at(bytes, entry.local + 12), 0);
            assert_eq!(u16at(bytes, entry.central + 14), 0);
        }
        assert!(matches!(
            open_n4a_bytes(bytes, N4aMemoryBudget::default()),
            Err(N4aError::UnsupportedFormat(_))
        ));
    }
}

#[test]
fn review_tolerated_headers_and_member_order_retain_raw_container_identity() {
    let (_, bytes) = write(
        manifest("ascii"),
        &[("ascii", PAYLOAD)],
        N4aMemoryBudget::default(),
    )
    .unwrap();
    let mut tolerant = bytes.clone();
    for entry in records(&bytes) {
        put16(&mut tolerant, entry.local + 4, 10);
        put16(&mut tolerant, entry.central + 6, 10);
        put16(&mut tolerant, entry.local + 6, 0);
        put16(&mut tolerant, entry.central + 8, 0);
        put16(&mut tolerant, entry.central + 4, 0x0314);
        put16(&mut tolerant, entry.central + 36, 1);
        put32(&mut tolerant, entry.central + 38, (0o100644 << 16) | 1);
    }
    let entries = records(&tolerant);
    let payload_start = entries[1].local;
    let payload_end = entries[1].data.end;
    let mut reordered = tolerant[payload_start..payload_end].to_vec();
    reordered.extend_from_slice(&tolerant[..payload_start]);
    reordered.extend_from_slice(&tolerant[payload_end..]);
    put32(
        &mut reordered,
        entries[0].central + 42,
        payload_end - payload_start,
    );
    put32(&mut reordered, entries[1].central + 42, 0);
    for alternate in [&tolerant, &reordered] {
        let opened = open_n4a_bytes(alternate, N4aMemoryBudget::default()).unwrap();
        assert_eq!(opened.member("ascii").unwrap().bytes, PAYLOAD);
        assert_eq!(opened.receipt().container_sha256, sha(alternate));
        assert_ne!(opened.receipt().container_sha256, sha(&bytes));
        assert_eq!(copy_n4a_bytes(&opened), *alternate);
    }
}

#[test]
fn adversarial_unsigned_integer_lexemes_refuse_integral_float_manifest_fields() {
    assert_eq!(PAYLOAD.len(), 26);
    let bytes = sample();
    let opened = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let raw = std::str::from_utf8(opened.manifest_bytes()).unwrap();
    // Keep the valid integer fixture's exact payload length/hash and all other
    // fields unchanged. Alter raw tokens, never parse/stringify the float cases.
    for (field, integer) in [("byte_length", 26), ("version", 9), ("schema_version", 17)] {
        let original = format!("\"{field}\":{integer}");
        assert_eq!(raw.matches(&original).count(), 1);
        for token in [format!("{integer}.0"), format!("{integer}e0")] {
            let altered = raw.replacen(&original, &format!("\"{field}\":{token}"), 1);
            let bad = replace_member(&bytes, "manifest.json", altered.as_bytes());
            let error = open_n4a_bytes(&bad, N4aMemoryBudget::default()).unwrap_err();
            match error {
                N4aError::Manifest(detail) => assert!(
                    detail.contains("floating point"),
                    "{field}={token}: {detail}"
                ),
                other => panic!(
                    "{field}={token} must be a floating-point Manifest refusal, got {other:?}"
                ),
            }
        }
    }
}

#[test]
fn adversarial_python_legacy_shaped_compression_and_json_error_categories() {
    let directory = tempfile::tempdir().unwrap();
    // These are actual Python zipfile productions mimicking a legacy shape,
    // not an SDK exporter execution or scientific payload qualification.
    let result = std::process::Command::new("python3").arg("-c").arg(r#"
import pathlib, sys, zipfile
root = pathlib.Path(sys.argv[1])
legacy = b'{"bundle_format_version":"1.0","pipeline":[]}'
(root / 'legacy.json').write_bytes(legacy)
for name, method in [('stored', zipfile.ZIP_STORED), ('deflated', zipfile.ZIP_DEFLATED)]:
    with zipfile.ZipFile(root / (name + '.zip'), 'w') as archive:
        for path, payload in [('manifest.json', legacy), ('opaque.bin', b'opaque legacy-shaped bytes')]:
            info = zipfile.ZipInfo(path, date_time=(1980,1,1,0,0,0))
            info.compress_type = method
            info.external_attr = 0o100644 << 16
            archive.writestr(info, payload)
print('Python ZIP_STORED and ZIP_DEFLATED legacy-shaped inputs generated; no SDK exporter invoked')
"#).arg(directory.path()).output().expect("Python 3 is required for this interoperability qualification test");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
    let stored = std::fs::read(directory.path().join("stored.zip")).unwrap();
    let deflated = std::fs::read(directory.path().join("deflated.zip")).unwrap();
    let json = std::fs::read(directory.path().join("legacy.json")).unwrap();
    assert!(matches!(
        open_n4a_bytes(&stored, N4aMemoryBudget::default()),
        Err(N4aError::UnsupportedFormat(_))
    ));
    assert!(matches!(
        open_n4a_bytes(&deflated, N4aMemoryBudget::default()),
        Err(N4aError::Physical(_))
    ));
    assert!(matches!(
        open_n4a_bytes(&json, N4aMemoryBudget::default()),
        Err(N4aError::Physical(_))
    ));
}

#[test]
fn raw_top_level_version_exponents_are_unsupported_format() {
    let bytes = sample();
    let opened = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let raw = std::str::from_utf8(opened.manifest_bytes()).unwrap();
    for field in ["schema_version", "minimum_reader_version"] {
        let integer = format!("\"{field}\":1,");
        assert_eq!(raw.matches(&integer).count(), 1);
        let exponent = raw.replacen(&integer, &format!("\"{field}\":1e0,"), 1);
        let bad = replace_member(&bytes, "manifest.json", exponent.as_bytes());
        assert!(
            matches!(
                open_n4a_bytes(&bad, N4aMemoryBudget::default()),
                Err(N4aError::UnsupportedFormat(_))
            ),
            "{field}=1e0"
        );
    }
}

#[test]
fn n4a_file_publishes_a_relative_target_without_cleanup_error() {
    use nirs4all::n4a_file::{open_n4a_file, write_n4a_file, N4aStreamInput, N4aStreamRequest};
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let target =
        std::path::PathBuf::from(format!("n4a-relative-{}-{nonce}.n4a", std::process::id()));
    assert_eq!(target.parent(), Some(std::path::Path::new("")));
    let payload = PAYLOAD.to_vec();
    let receipt = write_n4a_file(
        &target,
        N4aStreamRequest {
            manifest: serde_json::from_value(manifest(PATH)).unwrap(),
            members: vec![N4aStreamInput {
                path: PATH,
                source: &payload,
            }],
            base: None,
        },
        N4aMemoryBudget::default(),
    )
    .expect("publish relative neutral V1 target");
    let loaded = open_n4a_file(&target, N4aMemoryBudget::default()).unwrap();
    assert_eq!(loaded.receipt().container_sha256, receipt.container_sha256);
    assert_eq!(
        loaded
            .read_member(PATH, N4aMemoryBudget::default())
            .unwrap(),
        PAYLOAD
    );
    drop(loaded);
    std::fs::remove_file(target).unwrap();
}
