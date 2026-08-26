//! Archive V3 storage for target-bound native full-refit children.
//!
//! Core owns only the bounded ZIP container and exact raw-member inventory.
//! DAG-ML remains the sole owner of every document carried in the archive and
//! is the only layer allowed to parse or replay those bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};

use crate::archive_v2::{
    atomic_create, closed, fmt_err, id_ok, object, open_v2_preflight, path_ok,
    preflight_member_sizes, read_manifest_member, read_payload_members, refuse, required_str,
    sha256, sha256_file, stored_zip,
};
use crate::{ArchivePayload, ArchiveStoreError};

const PROFILE: &str = "nirs4all.archive_workspace.v3";
const WRITER_ID: &str = "nirs4all-core.archive_workspace_writer.v3";
const MANIFEST: &str = "manifest.json";
const PACKAGE: &str = "dagml/portable_refit_package.json";
const GRAPH: &str = "dagml/graph.json";
const BUNDLE: &str = "dagml/portable_refit_execution_bundle.json";
const OUTCOME: &str = "dagml/portable_refit_outcome.json";
const MAX_ENTRIES: usize = 256;
const MAX_MEMBER: usize = 134_217_728;
const MAX_TOTAL: usize = 536_870_912;

const PACKAGE_SCHEMA: &str =
    "https://github.com/GBeurier/dag-ml/schemas/portable_refit_package.v3.schema.json";
const GRAPH_SCHEMA: &str = "https://github.com/GBeurier/dag-ml/schemas/graph_spec.v1.schema.json";
const BUNDLE_SCHEMA: &str =
    "https://github.com/GBeurier/dag-ml/schemas/portable_refit_execution_bundle.v3.schema.json";
const OUTCOME_SCHEMA: &str =
    "https://github.com/GBeurier/dag-ml/schemas/portable_refit_outcome.v3.schema.json";

/// Typed input for the V3 writer. The caller supplies DAG-ML-owned semantic
/// references; Core derives raw hashes and byte sizes at the write boundary.
#[derive(Clone, Debug)]
pub struct ArchiveV3WriteRequest {
    pub manifest: Value,
    pub payloads: Vec<ArchivePayload>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveV3Reference {
    archive_id: String,
    archive_sha256: String,
}

impl ArchiveV3Reference {
    pub fn archive_id(&self) -> &str {
        &self.archive_id
    }
    pub fn schema_version(&self) -> u32 {
        3
    }
    pub fn profile(&self) -> &str {
        PROFILE
    }
    pub fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
    pub fn portable_refit_member(&self) -> &'static str {
        PACKAGE
    }
}

/// A V3 archive that passed container, closed-manifest, and raw-integrity
/// validation. Its members are opaque exact bytes for DAG-ML.
#[derive(Clone, Debug)]
pub struct LoadedArchiveV3 {
    reference: ArchiveV3Reference,
    manifest: Value,
    members: BTreeMap<String, Vec<u8>>,
}

impl LoadedArchiveV3 {
    pub fn reference(&self) -> &ArchiveV3Reference {
        &self.reference
    }
    pub fn manifest(&self) -> &Value {
        &self.manifest
    }
    pub fn member(&self, path: &str) -> Result<&[u8], ArchiveStoreError> {
        self.members.get(path).map(Vec::as_slice).ok_or_else(|| {
            ArchiveStoreError::Integrity(format!("V3 member `{path}` disappeared after validation"))
        })
    }
    pub fn portable_refit_package(&self) -> Result<&[u8], ArchiveStoreError> {
        self.member(PACKAGE)
    }
}

pub fn write_archive_v3(
    path: &Path,
    request: ArchiveV3WriteRequest,
) -> Result<ArchiveV3Reference, ArchiveStoreError> {
    let (manifest, members, archive_id) = prepare(request.manifest, request.payloads, true)?;
    let bytes = stored_zip(&manifest, &members)?;
    let reference = ArchiveV3Reference {
        archive_id,
        archive_sha256: sha256(&bytes),
    };
    atomic_create(path, &bytes)?;
    Ok(reference)
}

/// The V3 reader validates the entire bounded manifest and central-directory
/// declaration closure before it reads, allocates, or CRC-checks a payload.
pub fn load_archive_v3(path: &Path) -> Result<LoadedArchiveV3, ArchiveStoreError> {
    let (mut file, preflight) = open_v2_preflight(path)?;
    let manifest = read_manifest_member(&mut file, &preflight)?;
    let physical = preflight_member_sizes(&preflight);
    validate_manifest_declarations(&manifest, &physical)?;
    let members = read_payload_members(&mut file, &preflight)?;
    let (manifest, members, archive_id) = prepare(
        manifest,
        members
            .into_iter()
            .map(|(path, bytes)| ArchivePayload { path, bytes })
            .collect(),
        false,
    )?;
    let archive_sha256 = sha256_file(&mut file, preflight.archive_len)?;
    Ok(LoadedArchiveV3 {
        reference: ArchiveV3Reference {
            archive_id,
            archive_sha256,
        },
        manifest,
        members,
    })
}

type Prepared = (Value, BTreeMap<String, Vec<u8>>, String);
type Inventory = BTreeMap<String, (String, String, String)>;

#[derive(Clone, Copy)]
struct DagSpec {
    schema: &'static str,
    version: u64,
    producer_port: bool,
    profile: &'static str,
    fixed_path: &'static str,
}

fn prepare(
    mut manifest: Value,
    payloads: Vec<ArchivePayload>,
    derive_raw: bool,
) -> Result<Prepared, ArchiveStoreError> {
    let mut members = BTreeMap::new();
    let mut total = 0usize;
    for payload in payloads {
        path_ok(&payload.path)?;
        if payload.path == MANIFEST {
            return refuse("manifest.json cannot be supplied as a payload");
        }
        if payload.bytes.len() > MAX_MEMBER {
            return refuse("payload exceeds V3 member budget");
        }
        total = total
            .checked_add(payload.bytes.len())
            .ok_or_else(|| fmt_err("payload total overflow"))?;
        if total > MAX_TOTAL {
            return refuse("payload total exceeds V3 budget");
        }
        if members
            .insert(payload.path.clone(), payload.bytes)
            .is_some()
        {
            return refuse("duplicate payload path");
        }
    }
    if members.len() + 1 > MAX_ENTRIES {
        return refuse("payload entry count exceeds V3 budget");
    }
    if derive_raw {
        derive_inventory(&mut manifest, &members)?;
    }
    let physical = members
        .iter()
        .map(|(path, bytes)| (path.clone(), bytes.len()))
        .collect();
    validate_manifest_declarations(&manifest, &physical)?;
    validate_raw_closure(&manifest, &members)?;
    let archive_id = required_str(object(&manifest, "manifest")?, "archive_id")?.to_owned();
    Ok((manifest, members, archive_id))
}

fn derive_inventory(
    manifest: &mut Value,
    members: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ArchiveStoreError> {
    let root = manifest
        .as_object_mut()
        .ok_or_else(|| fmt_err("manifest must be an object"))?;
    let inventory = root
        .get_mut("member_inventory")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| fmt_err("member_inventory must be an array"))?;
    for entry in inventory {
        let o = entry
            .as_object_mut()
            .ok_or_else(|| fmt_err("member_inventory entry must be object"))?;
        let path = o
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| fmt_err("member_inventory path is missing"))?
            .to_owned();
        let bytes = members
            .get(&path)
            .ok_or_else(|| fmt_err("member_inventory path is missing from payloads"))?;
        o.insert("raw_sha256".into(), Value::String(sha256(bytes)));
        o.insert("uncompressed_size_bytes".into(), Value::from(bytes.len()));
        if path.ends_with(".n4mm") {
            o.insert("semantic_fingerprint".into(), Value::String(sha256(bytes)));
            o.insert(
                "semantic_profile".into(),
                Value::String("n4mm_raw_sha256".into()),
            );
        }
    }
    fn sync(
        value: &mut Value,
        members: &BTreeMap<String, Vec<u8>>,
    ) -> Result<(), ArchiveStoreError> {
        let o = value
            .as_object_mut()
            .ok_or_else(|| fmt_err("member reference must be object"))?;
        let path = o
            .get("member_path")
            .and_then(Value::as_str)
            .ok_or_else(|| fmt_err("member reference path is missing"))?
            .to_owned();
        let bytes = members
            .get(&path)
            .ok_or_else(|| fmt_err("member reference path is missing from payloads"))?;
        o.insert("raw_sha256".into(), Value::String(sha256(bytes)));
        if path.ends_with(".n4mm") {
            o.insert("semantic_fingerprint".into(), Value::String(sha256(bytes)));
            o.insert(
                "semantic_profile".into(),
                Value::String("n4mm_raw_sha256".into()),
            );
        }
        Ok(())
    }
    let replay = root
        .get_mut("replay")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| fmt_err("replay must be object"))?;
    sync(
        replay
            .get_mut("portable_refit_package")
            .ok_or_else(|| fmt_err("portable_refit_package missing"))?,
        members,
    )?;
    let artifacts = replay
        .get_mut("refit_artifacts")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| fmt_err("refit_artifacts must be object"))?;
    for key in ["graph", "execution_bundle", "refit_outcome"] {
        sync(
            artifacts
                .get_mut(key)
                .ok_or_else(|| fmt_err("refit artifact missing"))?,
            members,
        )?;
    }
    let methods = root
        .get_mut("payloads")
        .and_then(Value::as_object_mut)
        .and_then(|payloads| payloads.get_mut("methods"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| fmt_err("payloads.methods must be object"))?;
    for item in methods
        .get_mut("n4mm")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| fmt_err("payloads.methods.n4mm must be array"))?
    {
        sync(item, members)?;
    }
    Ok(())
}

fn validate_manifest_declarations(
    manifest: &Value,
    physical: &BTreeMap<String, usize>,
) -> Result<(), ArchiveStoreError> {
    let root = object(manifest, "manifest")?;
    closed(
        root,
        &[
            "schema_version",
            "profile",
            "archive_id",
            "persistence_kind",
            "writer",
            "reader_dispatch",
            "physical_profile",
            "replay",
            "payloads",
            "member_inventory",
            "migration_provenance",
            "security",
            "workspace",
        ],
        "V3 manifest",
    )?;
    if root.get("schema_version").and_then(Value::as_u64) != Some(3)
        || root.get("profile").and_then(Value::as_str) != Some(PROFILE)
        || root.get("persistence_kind").and_then(Value::as_str) != Some("n4a_archive")
    {
        return refuse("not an exact Archive V3 manifest");
    }
    id_ok(required_str(root, "archive_id")?, "archive_id")?;
    validate_writer(required_object(root, "writer")?)?;
    validate_dispatch(required_object(root, "reader_dispatch")?)?;
    validate_physical(required_object(root, "physical_profile")?)?;
    if root.get("migration_provenance") != Some(&Value::Null)
        || root.get("workspace") != Some(&Value::Null)
    {
        return refuse("V3 P0 refuses migration provenance and workspace snapshots");
    }
    let security = required_object(root, "security")?;
    closed(security, &["integrity_profile", "signature"], "security")?;
    if security.get("integrity_profile").and_then(Value::as_str)
        != Some("sha256_raw_member_inventory_v3")
        || security.get("signature") != Some(&Value::Null)
    {
        return refuse("V3 security profile is not exact");
    }
    validate_replay(required_object(root, "replay")?)?;
    validate_payloads(required_object(root, "payloads")?)?;
    let inventory = validate_inventory(root, physical)?;
    validate_reference_declarations(root, &inventory)
}

fn validate_writer(writer: &Map<String, Value>) -> Result<(), ArchiveStoreError> {
    closed(
        writer,
        &["product_aggregate_owner", "canonical_writer_id"],
        "writer",
    )?;
    if writer
        .get("product_aggregate_owner")
        .and_then(Value::as_str)
        != Some("nirs4all-core")
        || writer.get("canonical_writer_id").and_then(Value::as_str) != Some(WRITER_ID)
    {
        return refuse("V3 writer identity is not exact");
    }
    Ok(())
}

fn validate_dispatch(dispatch: &Map<String, Value>) -> Result<(), ArchiveStoreError> {
    closed(
        dispatch,
        &["archive_v3", "archive_v2", "archive_v1"],
        "reader_dispatch",
    )?;
    let v3 = required_object(dispatch, "archive_v3")?;
    closed(
        v3,
        &[
            "accepted_versions",
            "future_versions",
            "dispatch_before_extraction",
        ],
        "archive_v3",
    )?;
    if v3.get("accepted_versions") != Some(&serde_json::json!([3]))
        || v3.get("future_versions").and_then(Value::as_str) != Some("refuse")
        || v3.get("dispatch_before_extraction") != Some(&Value::Bool(true))
    {
        return refuse("archive_v3 dispatch is not exact");
    }
    for key in ["archive_v2", "archive_v1"] {
        let prior = required_object(dispatch, key)?;
        closed(prior, &["accepted_versions", "read_mode", "mutation"], key)?;
        let version = if key == "archive_v2" { 2 } else { 1 };
        if prior.get("accepted_versions") != Some(&serde_json::json!([version]))
            || prior.get("read_mode").and_then(Value::as_str) != Some("immutable_dual_read")
            || prior.get("mutation").and_then(Value::as_str) != Some("never_in_place")
        {
            return refuse("prior archive dispatch is not immutable dual-read");
        }
    }
    Ok(())
}

fn validate_physical(physical: &Map<String, Value>) -> Result<(), ArchiveStoreError> {
    closed(
        physical,
        &[
            "container",
            "manifest_member",
            "regular_files_only",
            "limits",
        ],
        "physical_profile",
    )?;
    if physical.get("container").and_then(Value::as_str) != Some("zip")
        || physical.get("manifest_member").and_then(Value::as_str) != Some(MANIFEST)
        || physical.get("regular_files_only") != Some(&Value::Bool(true))
    {
        return refuse("V3 physical profile is not exact");
    }
    let limits = required_object(physical, "limits")?;
    closed(
        limits,
        &[
            "max_entries",
            "max_total_uncompressed_bytes",
            "max_member_uncompressed_bytes",
            "max_compression_ratio",
        ],
        "physical limits",
    )?;
    if limits.get("max_entries").and_then(Value::as_u64) != Some(MAX_ENTRIES as u64)
        || limits
            .get("max_total_uncompressed_bytes")
            .and_then(Value::as_u64)
            != Some(MAX_TOTAL as u64)
        || limits
            .get("max_member_uncompressed_bytes")
            .and_then(Value::as_u64)
            != Some(MAX_MEMBER as u64)
        || limits.get("max_compression_ratio").and_then(Value::as_u64) != Some(100)
    {
        return refuse("V3 physical limits are not exact");
    }
    Ok(())
}

fn validate_replay(replay: &Map<String, Value>) -> Result<(), ArchiveStoreError> {
    closed(
        replay,
        &[
            "portable_refit_package",
            "refit_artifacts",
            "future_artifacts",
        ],
        "replay",
    )?;
    let future = replay
        .get("future_artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| fmt_err("future_artifacts must be array"))?;
    if !future.is_empty() {
        return refuse("V3 P0 refuses future replay artifacts");
    }
    let artifacts = required_object(replay, "refit_artifacts")?;
    closed(
        artifacts,
        &["graph", "execution_bundle", "refit_outcome"],
        "refit_artifacts",
    )
}

fn validate_payloads(payloads: &Map<String, Value>) -> Result<(), ArchiveStoreError> {
    closed(
        payloads,
        &[
            "methods",
            "n4d_aggregate_reference",
            "conformal",
            "robustness",
            "host_artifacts",
        ],
        "payloads",
    )?;
    if payloads.get("n4d_aggregate_reference") != Some(&Value::Null)
        || payloads.get("conformal") != Some(&Value::Null)
        || payloads.get("robustness") != Some(&Value::Null)
        || payloads
            .get("host_artifacts")
            .and_then(Value::as_array)
            .filter(|items| items.is_empty())
            .is_none()
    {
        return refuse("V3 P0 refuses sidecars and optional payload families");
    }
    let methods = required_object(payloads, "methods")?;
    closed(methods, &["n4mm", "n4mopt"], "payloads.methods")?;
    if methods
        .get("n4mm")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .is_none()
        || methods
            .get("n4mopt")
            .and_then(Value::as_array)
            .filter(|items| items.is_empty())
            .is_none()
    {
        return refuse("V3 requires N4MM and refuses N4MOPT resume state");
    }
    Ok(())
}

fn validate_inventory(
    root: &Map<String, Value>,
    physical: &BTreeMap<String, usize>,
) -> Result<Inventory, ArchiveStoreError> {
    let entries = root
        .get("member_inventory")
        .and_then(Value::as_array)
        .ok_or_else(|| fmt_err("member_inventory must be an array"))?;
    if entries.len() < 5 || entries.len() != physical.len() || entries.len() + 1 > MAX_ENTRIES {
        return refuse("V3 inventory does not exactly cover payloads");
    }
    let mut inventory = BTreeMap::new();
    for value in entries {
        let entry = object(value, "member inventory entry")?;
        closed(
            entry,
            &[
                "path",
                "regular_file",
                "raw_sha256",
                "uncompressed_size_bytes",
                "semantic_fingerprint",
                "semantic_profile",
            ],
            "member inventory entry",
        )?;
        let path = required_str(entry, "path")?;
        path_ok(path)?;
        let raw = required_sha256(entry, "raw_sha256")?;
        let semantic = required_sha256(entry, "semantic_fingerprint")?;
        let profile = entry
            .get("semantic_profile")
            .and_then(Value::as_str)
            .ok_or_else(|| fmt_err("inventory semantic_profile missing"))?;
        if !matches!(
            profile,
            "dagml_tcv1" | "dagml_historical_serde_json_v1" | "n4mm_raw_sha256"
        ) || entry.get("regular_file") != Some(&Value::Bool(true))
            || entry.get("uncompressed_size_bytes").and_then(Value::as_u64)
                != physical.get(path).copied().map(|v| v as u64)
            || inventory
                .insert(
                    path.to_owned(),
                    (raw.to_owned(), semantic.to_owned(), profile.to_owned()),
                )
                .is_some()
        {
            return refuse("V3 inventory member is malformed or duplicated");
        }
    }
    Ok(inventory)
}

fn validate_reference_declarations(
    root: &Map<String, Value>,
    inventory: &Inventory,
) -> Result<(), ArchiveStoreError> {
    let replay = required_object(root, "replay")?;
    let mut paths = BTreeSet::new();
    validate_dag_ref(
        required_value(replay, "portable_refit_package")?,
        inventory,
        &mut paths,
        DagSpec {
            schema: PACKAGE_SCHEMA,
            version: 3,
            producer_port: true,
            profile: "dagml_tcv1",
            fixed_path: PACKAGE,
        },
    )?;
    let artifacts = required_object(replay, "refit_artifacts")?;
    validate_dag_ref(
        required_value(artifacts, "graph")?,
        inventory,
        &mut paths,
        DagSpec {
            schema: GRAPH_SCHEMA,
            version: 1,
            producer_port: false,
            profile: "dagml_historical_serde_json_v1",
            fixed_path: GRAPH,
        },
    )?;
    validate_dag_ref(
        required_value(artifacts, "execution_bundle")?,
        inventory,
        &mut paths,
        DagSpec {
            schema: BUNDLE_SCHEMA,
            version: 3,
            producer_port: true,
            profile: "dagml_tcv1",
            fixed_path: BUNDLE,
        },
    )?;
    validate_dag_ref(
        required_value(artifacts, "refit_outcome")?,
        inventory,
        &mut paths,
        DagSpec {
            schema: OUTCOME_SCHEMA,
            version: 3,
            producer_port: true,
            profile: "dagml_tcv1",
            fixed_path: OUTCOME,
        },
    )?;
    let methods = required_object(required_object(root, "payloads")?, "methods")?;
    let mut ids = BTreeSet::new();
    for value in methods
        .get("n4mm")
        .and_then(Value::as_array)
        .ok_or_else(|| fmt_err("n4mm must be array"))?
    {
        let n4mm = object(value, "N4MM reference")?;
        closed(
            n4mm,
            &[
                "artifact_id",
                "kind",
                "owner",
                "format_version",
                "abi_major",
                "member_path",
                "raw_sha256",
                "semantic_fingerprint",
                "semantic_profile",
            ],
            "N4MM reference",
        )?;
        let id = required_str(n4mm, "artifact_id")?;
        let path = required_str(n4mm, "member_path")?;
        if !is_id(id)
            || !ids.insert(id)
            || !n4mm_path_ok(path)
            || n4mm.get("kind").and_then(Value::as_str) != Some("N4MM")
            || n4mm.get("owner").and_then(Value::as_str) != Some("nirs4all-methods")
            || n4mm.get("format_version").and_then(Value::as_u64) != Some(1)
            || n4mm.get("abi_major").and_then(Value::as_u64) != Some(2)
            || n4mm.get("semantic_profile").and_then(Value::as_str) != Some("n4mm_raw_sha256")
            || n4mm.get("semantic_fingerprint") != n4mm.get("raw_sha256")
        {
            return refuse("V3 N4MM reference is not exact native portable data");
        }
        validate_inventory_binding(n4mm, inventory, &mut paths)?;
    }
    if paths.len() != inventory.len() || paths.iter().any(|path| !inventory.contains_key(path)) {
        return refuse("V3 member inventory is not closed over declared references");
    }
    Ok(())
}

fn validate_dag_ref(
    value: &Value,
    inventory: &Inventory,
    paths: &mut BTreeSet<String>,
    spec: DagSpec,
) -> Result<(), ArchiveStoreError> {
    let reference = object(value, "DAG-ML reference")?;
    let mut keys = vec![
        "owner",
        "schema_id",
        "schema_version",
        "member_path",
        "raw_sha256",
        "semantic_fingerprint",
        "semantic_profile",
    ];
    if spec.producer_port {
        keys.push("producer_port_required");
    }
    closed(reference, &keys, "DAG-ML reference")?;
    if reference.get("owner").and_then(Value::as_str) != Some("dag-ml")
        || reference.get("schema_id").and_then(Value::as_str) != Some(spec.schema)
        || reference.get("schema_version").and_then(Value::as_u64) != Some(spec.version)
        || reference.get("member_path").and_then(Value::as_str) != Some(spec.fixed_path)
        || reference.get("semantic_profile").and_then(Value::as_str) != Some(spec.profile)
        || (spec.producer_port
            && reference.get("producer_port_required") != Some(&Value::Bool(true)))
    {
        return refuse("V3 DAG-ML reference is outside its exact schema family");
    }
    validate_inventory_binding(reference, inventory, paths)
}

fn validate_inventory_binding(
    reference: &Map<String, Value>,
    inventory: &Inventory,
    paths: &mut BTreeSet<String>,
) -> Result<(), ArchiveStoreError> {
    let path = required_str(reference, "member_path")?;
    path_ok(path)?;
    let raw = required_sha256(reference, "raw_sha256")?;
    let semantic = required_sha256(reference, "semantic_fingerprint")?;
    let profile = reference
        .get("semantic_profile")
        .and_then(Value::as_str)
        .ok_or_else(|| fmt_err("member semantic_profile missing"))?;
    if !paths.insert(path.to_owned())
        || inventory.get(path).map(|item| item.0.as_str()) != Some(raw)
        || inventory.get(path).map(|item| item.1.as_str()) != Some(semantic)
        || inventory.get(path).map(|item| item.2.as_str()) != Some(profile)
    {
        return refuse("reference does not exactly bind one V3 inventory member");
    }
    Ok(())
}

fn validate_raw_closure(
    manifest: &Value,
    members: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ArchiveStoreError> {
    let root = object(manifest, "manifest")?;
    let inventory = validate_inventory(
        root,
        &members.iter().map(|(p, b)| (p.clone(), b.len())).collect(),
    )?;
    for (path, (raw, _, _)) in &inventory {
        let bytes = members
            .get(path)
            .ok_or_else(|| fmt_err("inventory member missing after load"))?;
        if sha256(bytes) != *raw {
            return Err(ArchiveStoreError::Integrity(format!(
                "V3 inventory raw SHA mismatch for `{path}`"
            )));
        }
    }
    validate_reference_declarations(root, &inventory)
}

fn required_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, ArchiveStoreError> {
    object
        .get(key)
        .ok_or_else(|| fmt_err(&format!("missing `{key}`")))
        .and_then(|value| crate::archive_v2::object(value, key))
}

fn required_value<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Value, ArchiveStoreError> {
    object
        .get(key)
        .ok_or_else(|| fmt_err(&format!("missing `{key}`")))
}

fn required_sha256<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, ArchiveStoreError> {
    let value = required_str(object, key)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(fmt_err(&format!("`{key}` must be lowercase SHA-256")));
    }
    Ok(value)
}

fn is_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-'))
}

fn n4mm_path_ok(path: &str) -> bool {
    let Some(name) = path.strip_prefix("methods/") else {
        return false;
    };
    name.ends_with(".n4mm")
        && name.len() > ".n4mm".len()
        && name[..name.len() - ".n4mm".len()]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("n4a-v3-{name}-{}", std::process::id()))
    }

    fn dag_ref(
        schema_id: &str,
        version: u64,
        member_path: &str,
        profile: &str,
        port: bool,
    ) -> Value {
        let mut value = serde_json::json!({
            "owner": "dag-ml", "schema_id": schema_id, "schema_version": version,
            "member_path": member_path, "raw_sha256": "0".repeat(64),
            "semantic_fingerprint": "1".repeat(64), "semantic_profile": profile,
        });
        if port {
            value["producer_port_required"] = Value::Bool(true);
        }
        value
    }

    fn request() -> ArchiveV3WriteRequest {
        let n4mm = b"N4MM-v3-test".to_vec();
        let payloads = vec![
            ArchivePayload {
                path: PACKAGE.into(),
                bytes: b"package".to_vec(),
            },
            ArchivePayload {
                path: GRAPH.into(),
                bytes: b"graph".to_vec(),
            },
            ArchivePayload {
                path: BUNDLE.into(),
                bytes: b"bundle".to_vec(),
            },
            ArchivePayload {
                path: OUTCOME.into(),
                bytes: b"outcome".to_vec(),
            },
            ArchivePayload {
                path: "methods/model.n4mm".into(),
                bytes: n4mm,
            },
        ];
        let inventory = payloads.iter().map(|payload| serde_json::json!({
            "path": payload.path, "regular_file": true, "raw_sha256": "0".repeat(64),
            "uncompressed_size_bytes": 0, "semantic_fingerprint": if payload.path.ends_with(".n4mm") { "0".repeat(64) } else { "1".repeat(64) },
            "semantic_profile": if payload.path.ends_with(".n4mm") { "n4mm_raw_sha256" } else if payload.path == GRAPH { "dagml_historical_serde_json_v1" } else { "dagml_tcv1" },
        })).collect::<Vec<_>>();
        ArchiveV3WriteRequest {
            payloads,
            manifest: serde_json::json!({
                "schema_version": 3, "profile": PROFILE, "archive_id": "v3-test", "persistence_kind": "n4a_archive",
                "writer": {"product_aggregate_owner": "nirs4all-core", "canonical_writer_id": WRITER_ID},
                "reader_dispatch": {
                    "archive_v3": {"accepted_versions": [3], "future_versions": "refuse", "dispatch_before_extraction": true},
                    "archive_v2": {"accepted_versions": [2], "read_mode": "immutable_dual_read", "mutation": "never_in_place"},
                    "archive_v1": {"accepted_versions": [1], "read_mode": "immutable_dual_read", "mutation": "never_in_place"}
                },
                "physical_profile": {"container": "zip", "manifest_member": MANIFEST, "regular_files_only": true,
                    "limits": {"max_entries": MAX_ENTRIES, "max_total_uncompressed_bytes": MAX_TOTAL, "max_member_uncompressed_bytes": MAX_MEMBER, "max_compression_ratio": 100}},
                "replay": {"portable_refit_package": dag_ref(PACKAGE_SCHEMA, 3, PACKAGE, "dagml_tcv1", true),
                    "refit_artifacts": {"graph": dag_ref(GRAPH_SCHEMA, 1, GRAPH, "dagml_historical_serde_json_v1", false),
                        "execution_bundle": dag_ref(BUNDLE_SCHEMA, 3, BUNDLE, "dagml_tcv1", true),
                        "refit_outcome": dag_ref(OUTCOME_SCHEMA, 3, OUTCOME, "dagml_tcv1", true)}, "future_artifacts": []},
                "payloads": {"methods": {"n4mm": [{"artifact_id":"model", "kind":"N4MM", "owner":"nirs4all-methods", "format_version":1, "abi_major":2,
                    "member_path":"methods/model.n4mm", "raw_sha256":"0".repeat(64), "semantic_fingerprint":"0".repeat(64), "semantic_profile":"n4mm_raw_sha256"}], "n4mopt": []},
                    "n4d_aggregate_reference": null, "conformal": null, "robustness": null, "host_artifacts": []},
                "member_inventory": inventory, "migration_provenance": null,
                "security": {"integrity_profile": "sha256_raw_member_inventory_v3", "signature": null}, "workspace": null
            }),
        }
    }

    #[test]
    fn round_trips_opaque_v3_bytes_and_dispatches() {
        let path = temp("roundtrip.n4a");
        let _ = fs::remove_file(&path);
        let reference = write_archive_v3(&path, request()).unwrap();
        let loaded = load_archive_v3(&path).unwrap();
        assert_eq!(loaded.reference(), &reference);
        assert_eq!(loaded.portable_refit_package().unwrap(), b"package");
        assert!(matches!(
            crate::load_archive(&path).unwrap(),
            crate::LoadedArchive::V3(_)
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_sidecars_before_write() {
        let path = temp("sidecar.n4a");
        let _ = fs::remove_file(&path);
        let mut request = request();
        request.manifest["payloads"]["conformal"] = serde_json::json!({"forged":true});
        assert!(matches!(
            write_archive_v3(&path, request),
            Err(ArchiveStoreError::Format(_))
        ));
        assert!(!path.exists());
    }

    #[test]
    fn refuses_unknown_manifest_members_and_never_replaces() {
        let path = temp("closed-and-exclusive.n4a");
        let _ = fs::remove_file(&path);
        let mut invalid = request();
        invalid.manifest["unexpected"] = Value::Bool(true);
        assert!(matches!(
            write_archive_v3(&path, invalid),
            Err(ArchiveStoreError::Format(_))
        ));
        let reference = write_archive_v3(&path, request()).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(matches!(
            write_archive_v3(&path, request()),
            Err(ArchiveStoreError::AlreadyExists(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(load_archive_v3(&path).unwrap().reference(), &reference);
        fs::remove_file(path).unwrap();
    }
}
