//! Canonical n4a V1 transport. Scientific payloads remain opaque owner bytes.
//!
//! Opening proves storage integrity only. An operation requires a fresh caller
//! assertion from its owner port; this is neither authentication nor execution.
//!
//! Nothing here caps content. Only a declared [`N4aMemoryBudget`], allocator
//! failure, real ZIP/host representation limits and integrity checks refuse.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::{Deref, Range};

use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::archive_storage::ArchiveStoreError;
use crate::n4a_physical::{host_len, scan_json_charged, Ledger};
pub use crate::n4a_physical::{
    N4aMemberDigest, N4aMemberHasher, N4aMemoryBudget, N4aPlannedMember, N4aReadRequest,
    N4aScanned, N4aScanner, N4aWritePlan, N4aWriteStep, ValidatedIndex,
};

pub const N4A_SCHEMA: &str = "nirs4all.n4a.container.v1";
pub const N4A_PHYSICAL_PROFILE: &str = "zip-stored.v1";

#[derive(Debug, PartialEq, Eq)]
pub enum N4aError {
    /// A declared budget, the allocator or the host address space refused.
    Memory(String),
    Physical(String),
    Manifest(String),
    Integrity(String),
    UnsupportedFormat(String),
}
impl fmt::Display for N4aError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory(s) => write!(f, "n4a memory refusal: {s}"),
            Self::Physical(s) => write!(f, "n4a physical refusal: {s}"),
            Self::Manifest(s) => write!(f, "n4a manifest refusal: {s}"),
            Self::Integrity(s) => write!(f, "n4a integrity refusal: {s}"),
            Self::UnsupportedFormat(s) => write!(f, "n4a unsupported format: {s}"),
        }
    }
}
impl std::error::Error for N4aError {}
impl From<ArchiveStoreError> for N4aError {
    fn from(error: ArchiveStoreError) -> Self {
        match error {
            ArchiveStoreError::Format(s) => Self::Physical(
                s.strip_prefix("archive V2 format refusal: ")
                    .unwrap_or(&s)
                    .into(),
            ),
            ArchiveStoreError::Integrity(s) => Self::Integrity(s),
            ArchiveStoreError::Io(e) => Self::Physical(e.to_string()),
            other => Self::Physical(other.to_string()),
        }
    }
}
/// Recovers the refusal a verifying reader wrapped in its I/O error.
impl From<std::io::Error> for N4aError {
    fn from(error: std::io::Error) -> Self {
        match error.downcast::<Self>() {
            Ok(refusal) => refusal,
            Err(error) => Self::Physical(error.to_string()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct N4aManifest {
    pub schema: String,
    pub schema_version: u32,
    pub minimum_reader_version: u32,
    pub container_id: String,
    pub required_features: Vec<String>,
    pub physical_profile: String,
    pub members: Vec<N4aMember>,
    pub artifacts: Vec<N4aArtifact>,
    pub links: Vec<N4aLink>,
    pub entrypoints: Vec<N4aEntrypoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct N4aMember {
    pub path: String,
    pub byte_length: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Recipe,
    Dataset,
    Model,
    Run,
    Result,
    Score,
    Prediction,
    Residual,
    TrainingEvidence,
    FullRefit,
    Provenance,
    Attachment,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContentType {
    pub id: String,
    pub version: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnerIdentity {
    pub owner: String,
    pub schema: String,
    pub schema_version: u32,
    pub fingerprint_profile: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum N4aStorage {
    Embedded {
        root_member: String,
        member_paths: Vec<String>,
    },
    Reference {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "present_sha"
        )]
        root_sha256: Option<String>,
    },
}
fn present_sha<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RequirementRef {
    pub artifact_id: String,
}
fn required_nullable<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<RequirementRef>, D::Error> {
    Option::deserialize(deserializer)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct N4aArtifact {
    pub id: String,
    pub kind: ArtifactKind,
    pub owner: String,
    pub content_type: ContentType,
    pub identity: OwnerIdentity,
    pub storage: N4aStorage,
    // deserialize_with without default makes this nullable field REQUIRED.
    #[serde(deserialize_with = "required_nullable")]
    pub requirements: Option<RequirementRef>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct N4aLink {
    pub id: String,
    pub owner: String,
    pub relation: String,
    pub source_artifact: String,
    pub target_artifact: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct N4aEntrypoint {
    pub id: String,
    pub owner: String,
    pub operation: String,
    pub root_artifact: String,
    pub input_artifacts: Vec<String>,
    pub required_features: Vec<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub requirements: Option<RequirementRef>,
}

pub(crate) fn invalid(message: &str) -> N4aError {
    N4aError::Manifest(message.into())
}
fn id(value: &str) -> Result<(), N4aError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(invalid(
            "IDs must be nonempty strings without control characters",
        ));
    }
    Ok(())
}
fn raw_sha(value: &str) -> Result<(), N4aError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid("raw SHA-256 must be lowercase 64-character hex"));
    }
    Ok(())
}
fn distinct_ids(values: &[String]) -> Result<(), N4aError> {
    let mut seen = BTreeSet::new();
    for value in values {
        id(value)?;
        if !seen.insert(value) {
            return Err(invalid("duplicate ID"));
        }
    }
    Ok(())
}
/// One container header field. Any other JSON shape is consumed without building
/// a tree, so a deep unknown document can neither overflow the stack nor mask a
/// format refusal.
enum HeaderField {
    Text(String),
    Number(u64),
    Other,
}
struct HeaderFieldVisitor;
impl<'de> Visitor<'de> for HeaderFieldVisitor {
    type Value = HeaderField;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_str<E>(self, value: &str) -> Result<HeaderField, E> {
        Ok(HeaderField::Text(value.to_owned()))
    }
    fn visit_u64<E>(self, value: u64) -> Result<HeaderField, E> {
        Ok(HeaderField::Number(value))
    }
    fn visit_i64<E>(self, _: i64) -> Result<HeaderField, E> {
        Ok(HeaderField::Other)
    }
    fn visit_f64<E>(self, _: f64) -> Result<HeaderField, E> {
        Ok(HeaderField::Other)
    }
    fn visit_bool<E>(self, _: bool) -> Result<HeaderField, E> {
        Ok(HeaderField::Other)
    }
    fn visit_unit<E>(self) -> Result<HeaderField, E> {
        Ok(HeaderField::Other)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<HeaderField, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(HeaderField::Other)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<HeaderField, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(HeaderField::Other)
    }
}
impl<'de> Deserialize<'de> for HeaderField {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(HeaderFieldVisitor)
    }
}

/// The four fields that discriminate a container, whatever else is present.
#[derive(Default)]
struct FormatHeader {
    schema: Option<HeaderField>,
    schema_version: Option<HeaderField>,
    minimum_reader_version: Option<HeaderField>,
    physical_profile: Option<HeaderField>,
}
impl FormatHeader {
    fn is_supported(&self) -> bool {
        matches!(&self.schema, Some(HeaderField::Text(s)) if s == N4A_SCHEMA)
            && matches!(self.schema_version, Some(HeaderField::Number(1)))
            && matches!(self.minimum_reader_version, Some(HeaderField::Number(1)))
            && matches!(&self.physical_profile, Some(HeaderField::Text(s)) if s == N4A_PHYSICAL_PROFILE)
    }
}
struct FormatHeaderVisitor;
impl<'de> Visitor<'de> for FormatHeaderVisitor {
    type Value = FormatHeader;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<FormatHeader, A::Error> {
        let mut header = FormatHeader::default();
        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            let slot = match &*key {
                "schema" => &mut header.schema,
                "schema_version" => &mut header.schema_version,
                "minimum_reader_version" => &mut header.minimum_reader_version,
                "physical_profile" => &mut header.physical_profile,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                    continue;
                }
            };
            *slot = Some(map.next_value()?);
        }
        Ok(header)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<FormatHeader, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(FormatHeader::default())
    }
    fn visit_str<E>(self, _: &str) -> Result<FormatHeader, E> {
        Ok(FormatHeader::default())
    }
    fn visit_u64<E>(self, _: u64) -> Result<FormatHeader, E> {
        Ok(FormatHeader::default())
    }
    fn visit_i64<E>(self, _: i64) -> Result<FormatHeader, E> {
        Ok(FormatHeader::default())
    }
    fn visit_f64<E>(self, _: f64) -> Result<FormatHeader, E> {
        Ok(FormatHeader::default())
    }
    fn visit_bool<E>(self, _: bool) -> Result<FormatHeader, E> {
        Ok(FormatHeader::default())
    }
    fn visit_unit<E>(self) -> Result<FormatHeader, E> {
        Ok(FormatHeader::default())
    }
}
impl<'de> Deserialize<'de> for FormatHeader {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(FormatHeaderVisitor)
    }
}

fn unsupported_format() -> N4aError {
    N4aError::UnsupportedFormat(
        "only nirs4all.n4a.container.v1 / zip-stored.v1 reader 1 is supported".into(),
    )
}

/// Refuse duplicate keys, then discriminate only the container header, without
/// legacy dispatch or owner payload interpretation. Unknown future fields cannot
/// mask a refusal. The scan charge stands for the decoded manifest.
pub(crate) fn probe_manifest(bytes: &[u8], ledger: &mut Ledger) -> Result<(), N4aError> {
    scan_json_charged(bytes, ledger)?;
    let header: FormatHeader = serde_json::from_slice(bytes)
        .map_err(|_| N4aError::Physical("manifest is not JSON".into()))?;
    if !header.is_supported() {
        return Err(unsupported_format());
    }
    Ok(())
}

/// Strict typed decoding and structural validation of probed manifest bytes.
/// The schema is closed, so an unknown field refuses before its value is decoded.
pub(crate) fn decode_manifest(bytes: &[u8]) -> Result<N4aManifest, N4aError> {
    let manifest: N4aManifest =
        serde_json::from_slice(bytes).map_err(|e| invalid(&e.to_string()))?;
    validate(&manifest)?;
    Ok(manifest)
}

pub(crate) fn validate(manifest: &N4aManifest) -> Result<(), N4aError> {
    if manifest.schema != N4A_SCHEMA
        || manifest.schema_version != 1
        || manifest.minimum_reader_version != 1
        || manifest.physical_profile != N4A_PHYSICAL_PROFILE
    {
        return Err(unsupported_format());
    }
    id(&manifest.container_id)?;
    distinct_ids(&manifest.required_features)?;
    if manifest.members.is_empty() {
        return Err(invalid("nonempty inventory is required"));
    }
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    for member in &manifest.members {
        crate::n4a_physical::canonical_path_ok(&member.path)?;
        if !paths.insert(member.path.as_str()) {
            return Err(invalid("duplicate inventory path"));
        }
        total = total
            .checked_add(member.byte_length)
            .ok_or_else(|| invalid("aggregate length overflow"))?;
        raw_sha(&member.sha256)?;
    }
    let mut artifacts = BTreeSet::new();
    for artifact in &manifest.artifacts {
        id(&artifact.id)?;
        if !artifacts.insert(artifact.id.as_str()) {
            return Err(invalid("duplicate artifact ID"));
        }
    }
    let exists = |value: &str| -> Result<(), N4aError> {
        if !artifacts.contains(value) {
            return Err(invalid("artifact reference is not declared"));
        }
        Ok(())
    };
    let requirement = |value: &Option<RequirementRef>| -> Result<(), N4aError> {
        if let Some(reference) = value {
            exists(&reference.artifact_id)?;
        }
        Ok(())
    };
    let mut owned = BTreeSet::new();
    let mut embedded = 0usize;
    for artifact in &manifest.artifacts {
        id(&artifact.owner)?;
        id(&artifact.content_type.id)?;
        let identity = &artifact.identity;
        for value in [
            &identity.owner,
            &identity.schema,
            &identity.fingerprint_profile,
            &identity.fingerprint,
        ] {
            id(value)?;
        }
        if artifact.content_type.version == 0
            || identity.schema_version == 0
            || identity.owner != artifact.owner
        {
            return Err(invalid(
                "owner identity must be complete and have the artifact owner",
            ));
        }
        requirement(&artifact.requirements)?;
        match &artifact.storage {
            N4aStorage::Embedded {
                root_member,
                member_paths,
            } => {
                embedded += 1;
                if !member_paths.contains(root_member) {
                    return Err(invalid("embedded root must belong to its member paths"));
                }
                for path in member_paths {
                    if !paths.contains(path.as_str()) || !owned.insert(path.as_str()) {
                        return Err(invalid(
                            "each physical payload must have exactly one embedded artifact owner",
                        ));
                    }
                }
            }
            N4aStorage::Reference { root_sha256 } => {
                if let Some(sha) = root_sha256 {
                    raw_sha(sha)?;
                }
            }
        }
    }
    if embedded == 0 || owned != paths {
        return Err(invalid(
            "embedded ownership must exactly cover the physical inventory",
        ));
    }
    let mut links = BTreeSet::new();
    for link in &manifest.links {
        for value in [&link.id, &link.owner, &link.relation] {
            id(value)?;
        }
        if !links.insert(&link.id) {
            return Err(invalid("duplicate link ID"));
        }
        exists(&link.source_artifact)?;
        exists(&link.target_artifact)?;
    }
    let mut entrypoints = BTreeSet::new();
    for entrypoint in &manifest.entrypoints {
        for value in [&entrypoint.id, &entrypoint.owner, &entrypoint.operation] {
            id(value)?;
        }
        if !entrypoints.insert(&entrypoint.id) {
            return Err(invalid("duplicate entrypoint ID"));
        }
        exists(&entrypoint.root_artifact)?;
        distinct_ids(&entrypoint.input_artifacts)?;
        distinct_ids(&entrypoint.required_features)?;
        for input in &entrypoint.input_artifacts {
            exists(input)?;
        }
        requirement(&entrypoint.requirements)?;
    }
    Ok(())
}

/// Borrow original owner bytes. No scientific serialization occurs in Core.
#[derive(Clone, Copy, Debug)]
pub struct N4aMemberInput<'a> {
    pub path: &'a str,
    pub bytes: &'a [u8],
}
#[derive(Debug)]
pub struct N4aWriteRequest<'a> {
    pub manifest: N4aManifest,
    pub members: Vec<N4aMemberInput<'a>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct N4aReceipt {
    pub container_sha256: String,
    pub manifest_sha256: String,
    pub byte_length: u64,
    pub members: Vec<N4aMember>,
}

/// Parse and validate caller manifest bytes without any member bytes.
///
/// This runs the same duplicate-key scanner, format probe, strict typed decoding
/// and structural validation as opening a container. Declared `byte_length` and
/// `sha256` values are only checked for shape, so a producer may supply
/// placeholders that [`write_n4a_bytes`] overwrites from the real bytes.
pub fn parse_n4a_manifest(bytes: &[u8], budget: N4aMemoryBudget) -> Result<N4aManifest, N4aError> {
    probe_manifest(bytes, &mut Ledger::new(budget))?;
    decode_manifest(bytes)
}

/// One member of a byte write. A known digest avoids hashing validated bytes.
struct WritePart<'a> {
    path: &'a str,
    bytes: &'a [u8],
    digest: Option<N4aMemberDigest>,
}

/// Drive the write plan over a pre-reserved buffer. The bytes need no re-read:
/// the plan hashed them as they were emitted.
fn write_parts(
    manifest: N4aManifest,
    parts: Vec<WritePart<'_>>,
    budget: N4aMemoryBudget,
) -> Result<(N4aMetadata, Vec<u8>), N4aError> {
    let mut planned = Vec::with_capacity(parts.len());
    let mut sources = BTreeMap::new();
    for part in parts {
        let digest = part.digest.unwrap_or_else(|| {
            let mut hasher = N4aMemberHasher::new();
            hasher.update(part.bytes);
            hasher.finish()
        });
        planned.push(N4aPlannedMember {
            path: part.path.to_owned(),
            digest,
        });
        sources.insert(part.path, part.bytes);
    }
    let mut plan = N4aWritePlan::new(manifest, planned, budget)?;
    let mut out = Vec::new();
    plan.reserve_output(&mut out)?;
    while let Some(step) = plan.next_step()? {
        match step {
            N4aWriteStep::Frame(frame) => out.extend_from_slice(&frame),
            N4aWriteStep::Member { path, .. } => {
                let bytes = sources
                    .get(path.as_str())
                    .copied()
                    .ok_or_else(|| invalid("declared member bytes are absent"))?;
                // Immutable bytes, hashed just above or validated at open.
                plan.trust_current_member();
                plan.update_member(bytes)?;
                out.extend_from_slice(bytes);
            }
        }
    }
    Ok((plan.finish_metadata()?, out))
}

fn request_parts(request: N4aWriteRequest<'_>) -> (N4aManifest, Vec<WritePart<'_>>) {
    let parts = request
        .members
        .into_iter()
        .map(|member| WritePart {
            path: member.path,
            bytes: member.bytes,
            digest: None,
        })
        .collect();
    (request.manifest, parts)
}

/// Derive inventory sizes and raw hashes while retaining exact declared closure.
pub fn write_n4a_bytes(
    request: N4aWriteRequest<'_>,
    budget: N4aMemoryBudget,
) -> Result<(N4aReceipt, Vec<u8>), N4aError> {
    let (manifest, parts) = request_parts(request);
    let (metadata, bytes) = write_parts(manifest, parts, budget)?;
    Ok((metadata.receipt, bytes))
}

/// The same writer, returning the handle directly: nothing reparses its output.
pub fn write_n4a_owned(
    request: N4aWriteRequest<'_>,
    budget: N4aMemoryBudget,
) -> Result<OwnedN4a, N4aError> {
    let (manifest, parts) = request_parts(request);
    let (metadata, bytes) = write_parts(manifest, parts, budget)?;
    Ok(N4aContainer {
        bytes: bytes.into_boxed_slice(),
        metadata,
    })
}

/// Write a new container from an immutable validated `base` and a new manifest.
///
/// Each declared path takes its bytes from `replacements`, otherwise from the
/// identical member of `base`. A declared path found in neither, an undeclared
/// replacement or a duplicate replacement is refused; omitting a member from
/// `manifest.members` deletes it. The base is never modified. Retained members
/// reuse their validated digests instead of being hashed again.
pub fn write_n4a_update<S: AsRef<[u8]>>(
    base: &N4aContainer<S>,
    manifest: N4aManifest,
    replacements: Vec<N4aMemberInput<'_>>,
    budget: N4aMemoryBudget,
) -> Result<(N4aReceipt, Vec<u8>), N4aError> {
    let mut replaced = BTreeSet::new();
    for replacement in &replacements {
        if !replaced.insert(replacement.path) {
            return Err(invalid("duplicate input path"));
        }
    }
    let declared = manifest
        .members
        .iter()
        .map(|member| member.path.as_str())
        .collect::<BTreeSet<_>>();
    if !replaced.is_subset(&declared) {
        return Err(invalid("undeclared input members"));
    }
    let mut parts = replacements
        .into_iter()
        .map(|replacement| WritePart {
            path: replacement.path,
            bytes: replacement.bytes,
            digest: None,
        })
        .collect::<Vec<_>>();
    for path in declared.difference(&replaced) {
        let view = base
            .member(path)
            .ok_or_else(|| invalid("declared member bytes are absent"))?;
        parts.push(WritePart {
            path: view.path,
            bytes: view.bytes,
            digest: base.metadata.member_digest(path),
        });
    }
    drop((declared, replaced));
    let (metadata, bytes) = write_parts(manifest, parts, budget)?;
    Ok((metadata.receipt, bytes))
}

/// Physical location of one validated payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct N4aMemberLocation {
    pub data_offset: u64,
    pub byte_length: u64,
}

/// The validated description of one container, with no backing storage.
///
/// Every handle shares this: byte handles and [`N4aContainer`] add the bytes, a
/// file handle adds its file, and an external host keeps its own source beside
/// it. Scanned once; no handle ever reparses the archive to answer a query.
#[derive(Clone, Debug)]
pub struct N4aMetadata {
    pub(crate) manifest: N4aManifest,
    pub(crate) receipt: N4aReceipt,
    pub(crate) index: ValidatedIndex,
    pub(crate) budget: N4aMemoryBudget,
    pub(crate) retained_bytes: u64,
}

impl N4aMetadata {
    pub fn manifest(&self) -> &N4aManifest {
        &self.manifest
    }
    pub fn receipt(&self) -> &N4aReceipt {
        &self.receipt
    }
    pub fn index(&self) -> &ValidatedIndex {
        &self.index
    }
    /// The budget this handle was opened with.
    pub fn memory_budget(&self) -> N4aMemoryBudget {
        self.budget
    }
    /// Working bytes this metadata keeps allocated, by the opening ledger.
    pub(crate) fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }
    fn indexed(&self, path: &str) -> Option<(&N4aMember, u64, u32)> {
        let found = self
            .index
            .members
            .binary_search_by(|entry| self.receipt.members[entry.member].path.as_str().cmp(path))
            .ok()?;
        let entry = &self.index.members[found];
        Some((
            &self.receipt.members[entry.member],
            entry.data_start,
            entry.crc32,
        ))
    }
    /// Where a host reads a member from its retained source.
    pub fn member_location(&self, path: &str) -> Option<N4aMemberLocation> {
        self.indexed(path)
            .map(|(member, data_offset, _)| N4aMemberLocation {
                data_offset,
                byte_length: member.byte_length,
            })
    }
    /// The validated digest of a member, reusable when planning a retained copy.
    pub fn member_digest(&self, path: &str) -> Option<N4aMemberDigest> {
        self.indexed(path)
            .map(|(member, _, crc32)| N4aMemberDigest {
                byte_length: member.byte_length,
                sha256: member.sha256.clone(),
                crc32,
            })
    }
    /// Declared paths of an artifact: empty for a reference, `None` if absent.
    pub fn artifact_member_paths(&self, artifact_id: &str) -> Option<&[String]> {
        match &self
            .manifest
            .artifacts
            .iter()
            .find(|a| a.id == artifact_id)?
            .storage
        {
            N4aStorage::Embedded { member_paths, .. } => Some(member_paths),
            N4aStorage::Reference { .. } => Some(&[]),
        }
    }
    /// Compare a digest of bytes actually read to the validated inventory.
    pub fn verify_member(&self, path: &str, actual: &N4aMemberDigest) -> Result<(), N4aError> {
        let expected = self
            .member_digest(path)
            .ok_or_else(|| invalid("member is absent from the validated inventory"))?;
        if *actual != expected {
            return Err(N4aError::Integrity(format!(
                "bytes read for `{path}` differ from the validated inventory"
            )));
        }
        Ok(())
    }
    /// Compare a digest of the whole archive to the validated container identity.
    pub fn verify_archive(&self, actual: &N4aMemberDigest) -> Result<(), N4aError> {
        if actual.byte_length != self.receipt.byte_length
            || actual.sha256 != self.receipt.container_sha256
        {
            return Err(N4aError::Integrity(
                "archive bytes read differ from the validated container".into(),
            ));
        }
        Ok(())
    }
    pub fn inspect(&self) -> N4aInspection<'_> {
        let required_features = self
            .manifest
            .required_features
            .iter()
            .chain(
                self.manifest
                    .entrypoints
                    .iter()
                    .flat_map(|e| &e.required_features),
            )
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        N4aInspection {
            manifest: &self.manifest,
            receipt: &self.receipt,
            storage: StorageStatus::Verified,
            semantics: SemanticStatus::Unverified,
            required_features,
        }
    }
    /// Build the exact request for a separate live owner port. This grants nothing.
    pub fn operation_binding(
        &self,
        entrypoint_id: &str,
    ) -> Result<OperationBinding, OperationRefusal> {
        let entry = self
            .manifest
            .entrypoints
            .iter()
            .find(|e| e.id == entrypoint_id)
            .ok_or(OperationRefusal::UnknownEntrypoint)?;
        let artifact = |id: &str| {
            let a = self
                .manifest
                .artifacts
                .iter()
                .find(|a| a.id == id)
                .expect("validated artifact closure");
            ArtifactBinding {
                id: a.id.clone(),
                owner: a.owner.clone(),
                content_type: a.content_type.clone(),
            }
        };
        Ok(OperationBinding {
            container_sha256: self.receipt.container_sha256.clone(),
            entrypoint_id: entry.id.clone(),
            owner: entry.owner.clone(),
            operation: entry.operation.clone(),
            root: artifact(&entry.root_artifact),
            inputs: entry
                .input_artifacts
                .iter()
                .map(|id| artifact(id))
                .collect(),
        })
    }
    /// Check a live caller assertion before the caller invokes any owner action.
    /// Core has no model hydration, callbacks or execution in this module.
    pub fn require_operation(
        &self,
        entrypoint_id: &str,
        admission: Option<&OwnerAdmission>,
    ) -> Result<AdmittedOperation, OperationRefusal> {
        let binding = self.operation_binding(entrypoint_id)?;
        let admission = admission.ok_or(OperationRefusal::AbsentAdmission)?;
        match admission.status {
            OwnerAdmissionStatus::Refused => return Err(OperationRefusal::OwnerRefused),
            OwnerAdmissionStatus::Unverified => return Err(OperationRefusal::OwnerUnverified),
            OwnerAdmissionStatus::Verified => (),
        }
        if admission.binding != binding {
            return Err(OperationRefusal::BindingMismatch);
        }
        let entry = self
            .manifest
            .entrypoints
            .iter()
            .find(|entry| entry.id == entrypoint_id)
            .ok_or(OperationRefusal::UnknownEntrypoint)?;
        let missing = self
            .manifest
            .required_features
            .iter()
            .chain(&entry.required_features)
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|f| !admission.supported_features.iter().any(|s| s == f))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(OperationRefusal::UnknownRequiredFeatures(missing));
        }
        Ok(AdmittedOperation { binding })
    }
}

/// Validated immutable storage with one backing buffer and one range index.
///
/// Construct through [`open_n4a_bytes`] or [`open_owned_n4a_bytes`]. Private
/// fields prevent replacing storage after validation. Views never reparse ZIP
/// or load scientific runtimes; owner semantics remain unverified. Metadata
/// queries come from [`N4aMetadata`] through `Deref`.
#[derive(Debug)]
pub struct N4aContainer<S> {
    bytes: S,
    metadata: N4aMetadata,
}

impl<S> Deref for N4aContainer<S> {
    type Target = N4aMetadata;
    fn deref(&self) -> &N4aMetadata {
        &self.metadata
    }
}

/// Validated storage borrowing the caller's immutable byte slice.
pub type ValidatedN4a<'a> = N4aContainer<&'a [u8]>;

/// Validated storage owning the caller's `Box<[u8]>` allocation without copying.
///
/// The handle retains only bytes and metadata, with no borrowed self-reference or
/// live owner proof. Its views borrow the handle; [`copy_n4a_bytes`] explicitly
/// allocates an independent archive copy.
///
/// ```
/// use nirs4all::n4a::{open_owned_n4a_bytes, N4aError, N4aMemoryBudget, OwnedN4a};
/// fn take(bytes: Box<[u8]>) -> Result<OwnedN4a, N4aError> {
///     open_owned_n4a_bytes(bytes, N4aMemoryBudget::default())
/// }
/// ```
///
/// Member views cannot modify validated storage:
///
/// ```compile_fail
/// use nirs4all::n4a::OwnedN4a;
/// fn mutate(archive: &OwnedN4a, path: &str) {
///     archive.member(path).unwrap().bytes[0] = 0;
/// }
/// ```
///
/// Views cannot outlive their owned backing buffer:
///
/// ```compile_fail
/// use nirs4all::n4a::OwnedN4a;
/// fn escape(archive: OwnedN4a) -> &'static [u8] {
///     archive.manifest_bytes()
/// }
/// ```
///
/// Storage cannot be substituted after validation:
///
/// ```compile_fail
/// use nirs4all::n4a::OwnedN4a;
/// fn replace(archive: &mut OwnedN4a, bytes: Box<[u8]>) {
///     archive.bytes = bytes;
/// }
/// ```
pub type OwnedN4a = N4aContainer<Box<[u8]>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageStatus {
    Verified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticStatus {
    Unverified,
}
#[derive(Debug)]
pub struct N4aInspection<'a> {
    pub manifest: &'a N4aManifest,
    pub receipt: &'a N4aReceipt,
    pub storage: StorageStatus,
    pub semantics: SemanticStatus,
    /// All declared required features, including those on other entrypoints.
    pub required_features: Vec<&'a str>,
}
#[derive(Clone, Debug)]
pub struct N4aMemberView<'a> {
    pub path: &'a str,
    pub bytes: &'a [u8],
    pub range: Range<usize>,
}

/// Validate metadata/CRC/SHA directly over one borrowed buffer; no payload map.
/// The caller's bytes are not charged to `budget`; retained metadata is.
pub fn open_n4a_bytes(bytes: &[u8], budget: N4aMemoryBudget) -> Result<ValidatedN4a<'_>, N4aError> {
    let mut scanner = N4aScanner::new(bytes.len() as u64, budget)?;
    while let Some(request) = scanner.next_read() {
        let start = host_len(request.offset)?;
        let end = start + host_len(request.length)?;
        scanner.feed(&bytes[start..end])?;
    }
    let N4aScanned { metadata, .. } = scanner.finish()?;
    Ok(N4aContainer { bytes, metadata })
}
/// Validate once through [`open_n4a_bytes`], then move its metadata and the
/// original allocation into immutable owned storage. No archive clone, second
/// parse or second hash pass is retained or performed.
pub fn open_owned_n4a_bytes(
    bytes: Box<[u8]>,
    budget: N4aMemoryBudget,
) -> Result<OwnedN4a, N4aError> {
    let ValidatedN4a { metadata, .. } = open_n4a_bytes(&bytes, budget)?;
    Ok(N4aContainer { bytes, metadata })
}

fn manifest_range(metadata: &N4aMetadata) -> Range<usize> {
    let range = &metadata.index.manifest;
    range.start as usize..range.end as usize
}

impl<'a> ValidatedN4a<'a> {
    /// Exact manifest bytes with the lifetime of the caller's borrowed slice.
    pub fn manifest_bytes(&self) -> &'a [u8] {
        &self.bytes[manifest_range(&self.metadata)]
    }
}

impl OwnedN4a {
    /// Exact manifest bytes borrowing this owned handle.
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.bytes[manifest_range(&self.metadata)]
    }
    /// Move out the exact validated archive allocation, without copying it.
    pub fn into_archive_bytes(self) -> Box<[u8]> {
        self.bytes
    }
    /// Bytes already proved equal to `metadata`'s receipt by the caller's read.
    pub(crate) fn from_verified(bytes: Box<[u8]>, metadata: N4aMetadata) -> Self {
        Self { bytes, metadata }
    }
}

impl<S: AsRef<[u8]>> N4aContainer<S> {
    /// The exact original container bytes, as validated once at open time.
    pub fn archive_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }
    /// `O(log n)` in the member count. In-memory offsets fit `usize` by construction.
    pub fn member(&self, path: &str) -> Option<N4aMemberView<'_>> {
        let (member, start, _) = self.metadata.indexed(path)?;
        let range = start as usize..(start + member.byte_length) as usize;
        Some(N4aMemberView {
            path: &member.path,
            bytes: &self.bytes.as_ref()[range.clone()],
            range,
        })
    }
    pub fn artifact_members(&self, artifact_id: &str) -> Option<Vec<N4aMemberView<'_>>> {
        Some(
            self.metadata
                .artifact_member_paths(artifact_id)?
                .iter()
                .filter_map(|path| self.member(path))
                .collect(),
        )
    }
}

/// Member bytes for owner inspection, from any handle.
///
/// Byte handles borrow. A file handle reads and verifies each member on demand.
pub trait N4aMemberBytes {
    fn metadata(&self) -> &N4aMetadata;
    fn member_bytes(&self, path: &str) -> Result<Cow<'_, [u8]>, N4aError>;
    /// Archive bytes this handle already keeps in memory.
    fn resident_bytes(&self) -> u64;
}

impl<S: AsRef<[u8]>> N4aMemberBytes for N4aContainer<S> {
    fn metadata(&self) -> &N4aMetadata {
        &self.metadata
    }
    fn member_bytes(&self, path: &str) -> Result<Cow<'_, [u8]>, N4aError> {
        self.member(path)
            .map(|view| Cow::Borrowed(view.bytes))
            .ok_or_else(|| invalid("member is absent from the validated inventory"))
    }
    fn resident_bytes(&self) -> u64 {
        self.bytes.as_ref().len() as u64
    }
}

/// Explicitly allocates one exact output buffer, retaining all unknown facets.
pub fn copy_n4a_bytes<S: AsRef<[u8]>>(container: &N4aContainer<S>) -> Vec<u8> {
    container.bytes.as_ref().to_vec()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactBinding {
    pub id: String,
    pub owner: String,
    pub content_type: ContentType,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationBinding {
    pub container_sha256: String,
    pub entrypoint_id: String,
    pub owner: String,
    pub operation: String,
    pub root: ArtifactBinding,
    pub inputs: Vec<ArtifactBinding>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerAdmissionStatus {
    Verified,
    Refused,
    Unverified,
}
/// Caller-owned assertion. Deliberately has no serde/archive representation.
/// A producer owner string or user-supplied proof does not authenticate code.
#[derive(Clone, Debug)]
pub struct OwnerAdmission {
    pub binding: OperationBinding,
    pub status: OwnerAdmissionStatus,
    pub supported_features: Vec<String>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum OperationRefusal {
    UnknownEntrypoint,
    AbsentAdmission,
    OwnerRefused,
    OwnerUnverified,
    BindingMismatch,
    UnknownRequiredFeatures(Vec<String>),
}
impl fmt::Display for OperationRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "n4a operation refusal: {self:?}")
    }
}
impl std::error::Error for OperationRefusal {}
#[derive(Debug)]
pub struct AdmittedOperation {
    binding: OperationBinding,
}
impl AdmittedOperation {
    pub fn binding(&self) -> &OperationBinding {
        &self.binding
    }
}
