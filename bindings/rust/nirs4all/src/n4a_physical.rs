//! Canonical `zip-stored.v1` physical codec shared by every host.
//!
//! This module performs no I/O. [`N4aScanner`] asks for byte ranges and
//! [`N4aWritePlan`] emits framing, so a synchronous `Read + Seek` driver and an
//! asynchronous JavaScript source run the same parser and the same writer.
//!
//! The profile is a stored ZIP that uses ZIP64 fields exactly where a value does
//! not fit: a value at or above the classic sentinel (`0xFFFF_FFFF`, or `0xFFFF`
//! for entry counts) uses ZIP64, anything below never does. Every content has one
//! encoding, and archives below every threshold keep their ZIP32 bytes.
//!
//! No content size is limited here. [`N4aMemoryBudget`] covers Core's working
//! allocations only, never payload bytes on disk.

use std::io::Write;
use std::mem::size_of;
use std::ops::Range;

use sha2::{Digest, Sha256};

use crate::archive_codec as codec;
use crate::n4a::{
    decode_manifest, invalid, probe_manifest, validate, N4aError, N4aManifest, N4aMetadata,
    N4aReceipt,
};

impl crate::n4a::OwnedN4a {
    /// Apply an operation's budget to already validated owned storage.
    ///
    /// File materialization preserves the opening metadata. Hosts overriding
    /// that budget must also pass the override to metadata-based owner reads.
    /// Only metadata is cloned; archive bytes move without parsing or hashing.
    pub fn with_memory_budget(self, budget: N4aMemoryBudget) -> Result<Self, N4aError> {
        let resident = self
            .retained_bytes
            .checked_add(self.archive_bytes().len() as u64)
            .ok_or_else(|| N4aError::Memory("working set size overflow".into()))?;
        budget.admit(resident)?;
        if budget == self.memory_budget() {
            return Ok(self);
        }
        let mut metadata: N4aMetadata = (*self).clone();
        metadata.budget = budget;
        Ok(Self::from_verified(self.into_archive_bytes(), metadata))
    }
}

const MANIFEST: &str = "manifest.json";
const MAX_CHUNK: u64 = 1 << 20;
const MIN_CHUNK: u64 = 4096;

const LOCAL_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_SIGNATURE: u32 = 0x0201_4b50;
const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const EOCD64_SIGNATURE: u32 = 0x0606_4b50;
const LOCATOR_SIGNATURE: u32 = 0x0706_4b50;

const LOCAL_HEADER_LEN: usize = 30;
const CENTRAL_HEADER_LEN: usize = 46;
const EOCD_LEN: usize = 22;
const EOCD64_LEN: usize = 56;
const LOCATOR_LEN: usize = 20;
const ZIP64_TAIL_LEN: usize = EOCD64_LEN + LOCATOR_LEN + EOCD_LEN;
/// Size of an EOCD64 record, excluding its signature and this size field.
const EOCD64_RECORD_SIZE: u64 = 44;

const SENTINEL_32: u64 = u32::MAX as u64;
const SENTINEL_16: u64 = u16::MAX as u64;
const ZIP64_EXTRA_ID: u16 = 0x0001;
const ZIP64_VERSION: u16 = 45;
const STORED_VERSION: u16 = 20;
const UTF8_FLAG: u16 = 0x800;
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = 0x0021;

/// Declared working memory for one operation.
///
/// `None` adds no Core ceiling: only what the allocator and host grant applies.
/// This makes no claim about machine RAM, and on an overcommitting system an
/// allocation can succeed and the process can be killed later, so operators who
/// need a hard guarantee declare bytes or use cgroups/ulimit. A budget covers
/// Core's own allocations, never on-disk payload size; allocations made inside
/// IO, DAG, Methods or serde are outside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct N4aMemoryBudget {
    working_bytes: Option<u64>,
}

impl N4aMemoryBudget {
    pub const fn new(working_bytes: Option<u64>) -> Self {
        Self { working_bytes }
    }
    pub const fn host_allocator() -> Self {
        Self::new(None)
    }
    pub const fn bytes(working_bytes: u64) -> Self {
        Self::new(Some(working_bytes))
    }
    pub const fn working_bytes(self) -> Option<u64> {
        self.working_bytes
    }
    /// Streaming buffer size. This sizes a buffer and never limits content.
    pub const fn chunk_bytes(self) -> u64 {
        match self.working_bytes {
            None => MAX_CHUNK,
            Some(bytes) => {
                let quarter = bytes / 4;
                if quarter < MIN_CHUNK {
                    MIN_CHUNK
                } else if quarter > MAX_CHUNK {
                    MAX_CHUNK
                } else {
                    quarter
                }
            }
        }
    }
    /// Refuse `bytes` if they exceed the declared budget. `None` admits them.
    ///
    /// Hosts call this before an allocation Core cannot make fallible. It does
    /// not replace fallible allocation.
    pub fn admit(self, bytes: u64) -> Result<(), N4aError> {
        Ledger::new(self).charge(bytes)
    }
}

/// Charges working allocations before they are made.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ledger {
    limit: Option<u64>,
    used: u64,
}

impl Ledger {
    pub(crate) fn new(budget: N4aMemoryBudget) -> Self {
        Self {
            limit: budget.working_bytes(),
            used: 0,
        }
    }
    pub(crate) fn used(&self) -> u64 {
        self.used
    }
    pub(crate) fn charge(&mut self, bytes: u64) -> Result<(), N4aError> {
        let next = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| N4aError::Memory("working set size overflow".into()))?;
        if let Some(limit) = self.limit {
            if next > limit {
                return Err(N4aError::Memory(format!(
                    "{next} working bytes exceed the authorized {limit}"
                )));
            }
        }
        self.used = next;
        Ok(())
    }
    pub(crate) fn release(&mut self, bytes: u64) {
        self.used = self.used.saturating_sub(bytes);
    }
    /// Charge and fallibly reserve room for `additional` more elements.
    pub(crate) fn reserve<T>(
        &mut self,
        vec: &mut Vec<T>,
        additional: usize,
    ) -> Result<(), N4aError> {
        let required = vec
            .len()
            .checked_add(additional)
            .ok_or_else(|| N4aError::Memory("collection size overflow".into()))?;
        if required > vec.capacity() {
            let growth = (required - vec.capacity()) as u64;
            let bytes = growth
                .checked_mul(size_of::<T>() as u64)
                .ok_or_else(|| N4aError::Memory("collection size overflow".into()))?;
            self.charge(bytes)?;
            vec.try_reserve_exact(additional)
                .map_err(allocation_refusal)?;
        }
        Ok(())
    }
}

fn allocation_refusal(error: std::collections::TryReserveError) -> N4aError {
    N4aError::Memory(format!("allocation refused by the host: {error}"))
}

/// A value kept in memory must be addressable on this host.
pub(crate) fn host_len(value: u64) -> Result<usize, N4aError> {
    usize::try_from(value)
        .map_err(|_| N4aError::Memory(format!("{value} bytes exceed this host's address space")))
}

/// Copy bytes through a fallible allocation.
pub(crate) fn try_copy(bytes: &[u8]) -> Result<Vec<u8>, N4aError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len())
        .map_err(allocation_refusal)?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

/// Run the duplicate-key scanner with no depth policy, charging its working set.
/// The charge stays on `ledger` as the estimate of the decoded document.
pub(crate) fn scan_json_charged(bytes: &[u8], ledger: &mut Ledger) -> Result<(), N4aError> {
    let mut refused = None;
    let mut charge = |cost: usize| match ledger.charge(cost as u64) {
        Ok(()) => true,
        Err(error) => {
            refused = Some(error);
            false
        }
    };
    let scanned = codec::scan_json_structure_charged(bytes, usize::MAX, &mut charge);
    match refused {
        Some(error) => Err(error),
        None => scanned.map_err(Into::into),
    }
}

fn refuse<T>(detail: &str) -> Result<T, N4aError> {
    Err(N4aError::Physical(detail.into()))
}

fn le16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}
fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
fn le64(bytes: &[u8], at: usize) -> u64 {
    let mut value = [0u8; 8];
    value.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(value)
}
fn push16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn push64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
/// Classic field value: the value itself, or the sentinel when ZIP64 carries it.
fn classic32(value: u64) -> u32 {
    value.min(SENTINEL_32) as u32
}
fn classic16(value: u64) -> u16 {
    value.min(SENTINEL_16) as u16
}

/// Header fields shared by a member's local and central records.
struct MemberFields<'a> {
    name: &'a [u8],
    version_needed: u16,
    flags: u16,
    time: u16,
    date: u16,
    crc: u32,
    size: u64,
}

fn local_extra_len(size: u64) -> usize {
    if size >= SENTINEL_32 {
        20
    } else {
        0
    }
}

fn local_header_len(name_len: usize, size: u64) -> u64 {
    (LOCAL_HEADER_LEN + name_len + local_extra_len(size)) as u64
}

fn local_header(fields: &MemberFields<'_>) -> Vec<u8> {
    let extra_len = local_extra_len(fields.size);
    let mut out = Vec::with_capacity(LOCAL_HEADER_LEN + fields.name.len() + extra_len);
    push32(&mut out, LOCAL_SIGNATURE);
    push16(&mut out, fields.version_needed);
    push16(&mut out, fields.flags);
    push16(&mut out, 0);
    push16(&mut out, fields.time);
    push16(&mut out, fields.date);
    push32(&mut out, fields.crc);
    push32(&mut out, classic32(fields.size));
    push32(&mut out, classic32(fields.size));
    push16(&mut out, fields.name.len() as u16);
    push16(&mut out, extra_len as u16);
    out.extend_from_slice(fields.name);
    if extra_len != 0 {
        push16(&mut out, ZIP64_EXTRA_ID);
        push16(&mut out, 16);
        push64(&mut out, fields.size);
        push64(&mut out, fields.size);
    }
    out
}

/// Central ZIP64 extra payload bytes: only the fields that overflow, APPNOTE order.
fn central_extra_payload(size: u64, offset: u64) -> u16 {
    let mut payload = 0;
    if size >= SENTINEL_32 {
        payload += 16;
    }
    if offset >= SENTINEL_32 {
        payload += 8;
    }
    payload
}

fn central_record_len(name_len: usize, size: u64, offset: u64) -> u64 {
    let payload = central_extra_payload(size, offset);
    let extra = if payload == 0 { 0 } else { 4 + payload };
    (CENTRAL_HEADER_LEN + name_len) as u64 + u64::from(extra)
}

fn writer_version(size: u64, offset: u64) -> u16 {
    if central_extra_payload(size, offset) == 0 {
        STORED_VERSION
    } else {
        ZIP64_VERSION
    }
}

fn central_record(out: &mut Vec<u8>, fields: &MemberFields<'_>, offset: u64) {
    let payload = central_extra_payload(fields.size, offset);
    let extra_len = if payload == 0 { 0 } else { 4 + payload };
    push32(out, CENTRAL_SIGNATURE);
    push16(out, fields.version_needed);
    push16(out, fields.version_needed);
    push16(out, fields.flags);
    push16(out, 0);
    push16(out, fields.time);
    push16(out, fields.date);
    push32(out, fields.crc);
    push32(out, classic32(fields.size));
    push32(out, classic32(fields.size));
    push16(out, fields.name.len() as u16);
    push16(out, extra_len);
    push16(out, 0);
    push16(out, 0);
    push16(out, 0);
    push32(out, 0);
    push32(out, classic32(offset));
    out.extend_from_slice(fields.name);
    if payload != 0 {
        push16(out, ZIP64_EXTRA_ID);
        push16(out, payload);
        if fields.size >= SENTINEL_32 {
            push64(out, fields.size);
            push64(out, fields.size);
        }
        if offset >= SENTINEL_32 {
            push64(out, offset);
        }
    }
}

fn tail_needs_zip64(count: u64, central_size: u64, central_offset: u64) -> bool {
    count >= SENTINEL_16 || central_size >= SENTINEL_32 || central_offset >= SENTINEL_32
}

fn tail_len(count: u64, central_size: u64, central_offset: u64) -> u64 {
    if tail_needs_zip64(count, central_size, central_offset) {
        ZIP64_TAIL_LEN as u64
    } else {
        EOCD_LEN as u64
    }
}

/// Everything after the central directory. `central_end` was checked by layout.
fn tail_bytes(count: u64, central_size: u64, central_offset: u64, central_end: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(ZIP64_TAIL_LEN);
    if tail_needs_zip64(count, central_size, central_offset) {
        push32(&mut out, EOCD64_SIGNATURE);
        push64(&mut out, EOCD64_RECORD_SIZE);
        push16(&mut out, ZIP64_VERSION);
        push16(&mut out, ZIP64_VERSION);
        push32(&mut out, 0);
        push32(&mut out, 0);
        push64(&mut out, count);
        push64(&mut out, count);
        push64(&mut out, central_size);
        push64(&mut out, central_offset);
        push32(&mut out, LOCATOR_SIGNATURE);
        push32(&mut out, 0);
        push64(&mut out, central_end);
        push32(&mut out, 1);
    }
    push32(&mut out, EOCD_SIGNATURE);
    push16(&mut out, 0);
    push16(&mut out, 0);
    push16(&mut out, classic16(count));
    push16(&mut out, classic16(count));
    push32(&mut out, classic32(central_size));
    push32(&mut out, classic32(central_offset));
    push16(&mut out, 0);
    out
}

/// The central directory's declared position. `end` is where its bytes stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CentralBounds {
    count: u64,
    size: u64,
    offset: u64,
    end: u64,
}

fn parse_tail(tail: &[u8], archive_len: u64) -> Result<CentralBounds, N4aError> {
    let eocd = tail.len() - EOCD_LEN;
    if le32(tail, eocd) != EOCD_SIGNATURE || le16(tail, eocd + 20) != 0 {
        return refuse("ZIP end record must end the file without a comment");
    }
    if le16(tail, eocd + 4) != 0
        || le16(tail, eocd + 6) != 0
        || le16(tail, eocd + 8) != le16(tail, eocd + 10)
    {
        return refuse("multi-disk ZIP is outside the canonical physical profile");
    }
    let count = u64::from(le16(tail, eocd + 10));
    let size = u64::from(le32(tail, eocd + 12));
    let offset = u64::from(le32(tail, eocd + 16));
    let bounds = if count == SENTINEL_16 || size == SENTINEL_32 || offset == SENTINEL_32 {
        parse_zip64_tail(tail, archive_len, count, size, offset)?
    } else {
        CentralBounds {
            count,
            size,
            offset,
            end: archive_len - EOCD_LEN as u64,
        }
    };
    // Structural, not a cap: a declared directory must fit its own bytes, so a
    // hostile count or size cannot reserve memory the file does not back.
    if bounds.count == 0
        || bounds.offset.checked_add(bounds.size) != Some(bounds.end)
        || bounds
            .count
            .checked_mul(CENTRAL_HEADER_LEN as u64)
            .is_none_or(|minimum| minimum > bounds.size)
    {
        return refuse("unsupported ZIP central directory");
    }
    Ok(bounds)
}

fn parse_zip64_tail(
    tail: &[u8],
    archive_len: u64,
    classic_count: u64,
    classic_size: u64,
    classic_offset: u64,
) -> Result<CentralBounds, N4aError> {
    if tail.len() < ZIP64_TAIL_LEN {
        return refuse("ZIP64 end records are missing");
    }
    let eocd64 = tail.len() - ZIP64_TAIL_LEN;
    let locator = eocd64 + EOCD64_LEN;
    let eocd64_offset = archive_len - ZIP64_TAIL_LEN as u64;
    if le32(tail, locator) != LOCATOR_SIGNATURE
        || le32(tail, locator + 4) != 0
        || le64(tail, locator + 8) != eocd64_offset
        || le32(tail, locator + 16) != 1
    {
        return refuse("ZIP64 locator is outside the canonical physical profile");
    }
    if le32(tail, eocd64) != EOCD64_SIGNATURE
        || le64(tail, eocd64 + 4) != EOCD64_RECORD_SIZE
        || le16(tail, eocd64 + 12) != ZIP64_VERSION
        || le16(tail, eocd64 + 14) != ZIP64_VERSION
        || le32(tail, eocd64 + 16) != 0
        || le32(tail, eocd64 + 20) != 0
        || le64(tail, eocd64 + 24) != le64(tail, eocd64 + 32)
    {
        return refuse("ZIP64 end record is outside the canonical physical profile");
    }
    let count = le64(tail, eocd64 + 32);
    let size = le64(tail, eocd64 + 40);
    let offset = le64(tail, eocd64 + 48);
    // Each classic field holds min(actual, sentinel). A sentinel over a value
    // that fits is a ZIP64 encoding the content did not need.
    if classic_count != count.min(SENTINEL_16)
        || classic_size != size.min(SENTINEL_32)
        || classic_offset != offset.min(SENTINEL_32)
    {
        return refuse("ZIP end record and ZIP64 end record disagree");
    }
    Ok(CentralBounds {
        count,
        size,
        offset,
        end: eocd64_offset,
    })
}

/// Decode the central extra field and return `(size, local_offset, uses_zip64)`.
fn parse_central_sizes(
    extra: &[u8],
    size32: u32,
    compressed32: u32,
    offset32: u32,
) -> Result<(u64, u64, bool), N4aError> {
    if size32 != compressed32 {
        return refuse("ZIP member is outside the regular stored profile");
    }
    let size_overflows = u64::from(size32) == SENTINEL_32;
    let offset_overflows = u64::from(offset32) == SENTINEL_32;
    if !size_overflows && !offset_overflows {
        if !extra.is_empty() {
            return refuse("ZIP extra field is present without a required ZIP64 value");
        }
        return Ok((u64::from(size32), u64::from(offset32), false));
    }
    let payload = 16 * usize::from(size_overflows) + 8 * usize::from(offset_overflows);
    if extra.len() != 4 + payload
        || le16(extra, 0) != ZIP64_EXTRA_ID
        || usize::from(le16(extra, 2)) != payload
    {
        return refuse("ZIP64 extra field is outside the canonical physical profile");
    }
    let mut at = 4;
    let mut size = u64::from(size32);
    if size_overflows {
        size = le64(extra, at);
        if le64(extra, at + 8) != size {
            return refuse("ZIP member is outside the regular stored profile");
        }
        at += 16;
    }
    let mut offset = u64::from(offset32);
    if offset_overflows {
        offset = le64(extra, at);
    }
    if (size_overflows && size < SENTINEL_32) || (offset_overflows && offset < SENTINEL_32) {
        return refuse("ZIP64 extra field carries a value that does not need ZIP64");
    }
    Ok((size, offset, true))
}

/// One canonical guard for reader and writer, without OS extraction. The only
/// length bound is the ZIP name field itself.
pub(crate) fn canonical_path_ok(path: &str) -> Result<(), N4aError> {
    codec::path_shape_ok(path)?;
    if path.len() > usize::from(u16::MAX) {
        return refuse("ZIP name overflow");
    }
    if path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|s| s.as_bytes().get(1) == Some(&b':') && s.as_bytes()[0].is_ascii_alphabetic())
    {
        return refuse("unsafe canonical member path");
    }
    Ok(())
}

#[derive(Debug)]
struct ScanEntry {
    name: String,
    version_needed: u16,
    flags: u16,
    time: u16,
    date: u16,
    crc: u32,
    size: u64,
    local_offset: u64,
    data_start: u64,
    actual_crc: u32,
    sha256: String,
}

impl ScanEntry {
    fn fields(&self) -> MemberFields<'_> {
        MemberFields {
            name: self.name.as_bytes(),
            version_needed: self.version_needed,
            flags: self.flags,
            time: self.time,
            date: self.date,
            crc: self.crc,
            size: self.size,
        }
    }
}

/// Parse one central record, returning its length, or `None` if incomplete.
fn parse_central_record(record: &[u8]) -> Result<Option<(ScanEntry, usize)>, N4aError> {
    if record.len() < CENTRAL_HEADER_LEN {
        return Ok(None);
    }
    if le32(record, 0) != CENTRAL_SIGNATURE {
        return refuse("invalid ZIP central entry");
    }
    let name_len = usize::from(le16(record, 28));
    let extra_len = usize::from(le16(record, 30));
    let comment_len = usize::from(le16(record, 32));
    let name_end = CENTRAL_HEADER_LEN + name_len;
    let extra_end = name_end + extra_len;
    let total = extra_end + comment_len;
    if record.len() < total {
        return Ok(None);
    }
    let version_needed = le16(record, 6);
    let flags = le16(record, 8);
    let method = le16(record, 10);
    let external = le32(record, 38);
    let mode = external >> 16;
    let file_type = mode & 0o170000;
    if !matches!(flags, 0 | UTF8_FLAG)
        || mode & 0o111 != 0
        || method != 0
        || comment_len != 0
        || le16(record, 34) != 0
        || external & 0x10 != 0
        || !matches!(file_type, 0 | 0o100000)
    {
        return refuse("ZIP member is outside the regular stored profile");
    }
    let (size, local_offset, zip64) = parse_central_sizes(
        &record[name_end..extra_end],
        le32(record, 24),
        le32(record, 20),
        le32(record, 42),
    )?;
    if (zip64 && version_needed != ZIP64_VERSION)
        || (!zip64 && !matches!(version_needed, 10 | STORED_VERSION))
    {
        return refuse("ZIP version-needed is outside the canonical physical profile");
    }
    let name = std::str::from_utf8(&record[CENTRAL_HEADER_LEN..name_end])
        .map_err(|_| N4aError::Physical("ZIP name is not UTF-8".into()))?;
    if !name.is_ascii() && flags != UTF8_FLAG {
        return refuse("non-ASCII ZIP names require the UTF-8 flag");
    }
    if name != MANIFEST {
        canonical_path_ok(name)?;
    }
    Ok(Some((
        ScanEntry {
            name: name.to_owned(),
            version_needed,
            flags,
            time: le16(record, 12),
            date: le16(record, 14),
            crc: le32(record, 16),
            size,
            local_offset,
            data_start: 0,
            actual_crc: 0,
            sha256: String::new(),
        },
        total,
    )))
}

/// One byte range the host must read next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct N4aReadRequest {
    pub offset: u64,
    pub length: u64,
}

/// A completed scan: validated metadata and the exact manifest bytes.
#[derive(Debug)]
pub struct N4aScanned {
    pub metadata: N4aMetadata,
    pub manifest_bytes: Box<[u8]>,
}

/// Validated size and hashes of one byte sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct N4aMemberDigest {
    pub byte_length: u64,
    pub sha256: String,
    pub crc32: u32,
}

/// Incremental size, SHA-256 and CRC-32 in one pass over the bytes.
#[derive(Clone)]
pub struct N4aMemberHasher {
    byte_length: u64,
    sha256: Sha256,
    crc32: crc32fast::Hasher,
}

impl N4aMemberHasher {
    pub fn new() -> Self {
        Self {
            byte_length: 0,
            sha256: Sha256::new(),
            crc32: crc32fast::Hasher::new(),
        }
    }
    pub fn update(&mut self, bytes: &[u8]) {
        self.byte_length += bytes.len() as u64;
        self.sha256.update(bytes);
        self.crc32.update(bytes);
    }
    pub fn finish(self) -> N4aMemberDigest {
        N4aMemberDigest {
            byte_length: self.byte_length,
            sha256: format!("{:x}", self.sha256.finalize()),
            crc32: self.crc32.finalize(),
        }
    }
}

impl Default for N4aMemberHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Validated location of one payload, ordered by path for `O(log n)` lookup.
#[derive(Clone, Debug)]
pub(crate) struct IndexedMember {
    /// Position in the receipt inventory, which holds the path and SHA-256.
    pub(crate) member: usize,
    pub(crate) data_start: u64,
    pub(crate) crc32: u32,
}

/// Metadata-only validated ranges. Fields are private and never mutable through views.
#[derive(Clone, Debug)]
pub struct ValidatedIndex {
    pub(crate) members: Vec<IndexedMember>,
    pub(crate) manifest: Range<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanPhase {
    Tail,
    Central,
    Stream,
    Complete,
}

enum StreamCursor {
    Header {
        expected: Vec<u8>,
        at: usize,
    },
    Payload {
        remaining: u64,
        hasher: N4aMemberHasher,
    },
    Trailing,
}

/// Sans-IO reader of a canonical container.
///
/// Drive it with [`next_read`](Self::next_read) and [`feed`](Self::feed), then
/// call [`finish`](Self::finish). It reads the tail, the central directory, and
/// then one contiguous pass that checks local headers and computes CRC, member
/// SHA and container SHA together. Physical refusals surface while feeding; CRC,
/// format probe, timestamps, typed decoding and inventory hashes surface in
/// `finish`, in that order.
pub struct N4aScanner {
    archive_len: u64,
    budget: N4aMemoryBudget,
    ledger: Ledger,
    chunk: u64,
    phase: ScanPhase,
    tail: Vec<u8>,
    bounds: CentralBounds,
    central_read: u64,
    pending: Vec<u8>,
    entries: Vec<ScanEntry>,
    first_pass: Sha256,
    first_digest: Vec<u8>,
    second_pass: Sha256,
    stream_read: u64,
    container: Sha256,
    cursor: StreamCursor,
    entry: usize,
    capturing_manifest: bool,
    manifest: Vec<u8>,
    manifest_probe_error: Option<N4aError>,
}

impl N4aScanner {
    pub fn new(archive_len: u64, budget: N4aMemoryBudget) -> Result<Self, N4aError> {
        if archive_len < EOCD_LEN as u64 {
            return refuse("archive is shorter than a ZIP end record");
        }
        let chunk = budget.chunk_bytes();
        let mut ledger = Ledger::new(budget);
        ledger.charge(chunk)?;
        Ok(Self {
            archive_len,
            budget,
            ledger,
            chunk,
            phase: ScanPhase::Tail,
            tail: Vec::new(),
            bounds: CentralBounds::default(),
            central_read: 0,
            pending: Vec::new(),
            entries: Vec::new(),
            first_pass: Sha256::new(),
            first_digest: Vec::new(),
            second_pass: Sha256::new(),
            stream_read: 0,
            container: Sha256::new(),
            cursor: StreamCursor::Trailing,
            entry: 0,
            capturing_manifest: false,
            manifest: Vec::new(),
            manifest_probe_error: None,
        })
    }

    /// The pending read, or `None` when ready for [`finish`](Self::finish).
    /// Repeats the same request until it is fed.
    pub fn next_read(&self) -> Option<N4aReadRequest> {
        match self.phase {
            ScanPhase::Tail => {
                let length = self.archive_len.min(ZIP64_TAIL_LEN as u64);
                Some(N4aReadRequest {
                    offset: self.archive_len - length,
                    length,
                })
            }
            ScanPhase::Central => Some(N4aReadRequest {
                offset: self.central_read,
                length: self.chunk.min(self.bounds.end - self.central_read),
            }),
            ScanPhase::Stream => Some(N4aReadRequest {
                offset: self.stream_read,
                length: self.chunk.min(self.archive_len - self.stream_read),
            }),
            ScanPhase::Complete => None,
        }
    }

    /// Supply exactly the bytes of the pending request.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), N4aError> {
        let Some(request) = self.next_read() else {
            return refuse("scan has no pending read");
        };
        if bytes.len() as u64 != request.length {
            return Err(N4aError::Integrity(
                "archive source returned a short read".into(),
            ));
        }
        match self.phase {
            ScanPhase::Tail => self.feed_tail(bytes),
            ScanPhase::Central => self.feed_central(bytes),
            ScanPhase::Stream => self.feed_stream(bytes),
            ScanPhase::Complete => Ok(()),
        }
    }

    fn feed_tail(&mut self, bytes: &[u8]) -> Result<(), N4aError> {
        self.bounds = parse_tail(bytes, self.archive_len)?;
        self.tail = bytes.to_vec();
        self.ledger
            .reserve(&mut self.entries, host_len(self.bounds.count)?)?;
        self.central_read = self.bounds.offset;
        self.phase = ScanPhase::Central;
        Ok(())
    }

    fn feed_central(&mut self, bytes: &[u8]) -> Result<(), N4aError> {
        self.first_pass.update(bytes);
        self.ledger.reserve(&mut self.pending, bytes.len())?;
        self.pending.extend_from_slice(bytes);
        self.central_read += bytes.len() as u64;
        let mut consumed = 0;
        while let Some((entry, used)) = parse_central_record(&self.pending[consumed..])? {
            if self.entries.len() as u64 == self.bounds.count {
                return refuse("ZIP central directory/member inventory is invalid");
            }
            // The retained name and the hex digest later stored beside it.
            self.ledger.charge(entry.name.len() as u64 + 64)?;
            self.entries.push(entry);
            consumed += used;
        }
        self.pending.drain(..consumed);
        if self.central_read == self.bounds.end {
            self.seal_central()?;
        }
        Ok(())
    }

    /// Whole-directory checks, all before any payload byte is read or reserved.
    fn seal_central(&mut self) -> Result<(), N4aError> {
        if !self.pending.is_empty() {
            return refuse("truncated central directory");
        }
        self.ledger.release(self.pending.capacity() as u64);
        self.pending = Vec::new();
        if self.entries.len() as u64 != self.bounds.count
            || self.entries.iter().filter(|e| e.name == MANIFEST).count() != 1
        {
            return refuse("ZIP central directory/member inventory is invalid");
        }
        self.entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        if self
            .entries
            .windows(2)
            .any(|pair| pair[0].name == pair[1].name)
        {
            return refuse("duplicate ZIP member");
        }
        self.entries
            .sort_unstable_by_key(|entry| entry.local_offset);
        let mut expected = 0u64;
        for entry in &mut self.entries {
            if entry.local_offset != expected {
                return refuse("canonical ZIP layout must be contiguous");
            }
            entry.data_start = entry
                .local_offset
                .checked_add(local_header_len(entry.name.len(), entry.size))
                .ok_or_else(|| N4aError::Physical("ZIP local offset overflow".into()))?;
            expected = entry
                .data_start
                .checked_add(entry.size)
                .ok_or_else(|| N4aError::Physical("ZIP local offset overflow".into()))?;
        }
        if expected != self.bounds.offset {
            return refuse("canonical ZIP layout must be contiguous");
        }
        // Remember the directory this index came from: the contiguous pass
        // re-reads it, and a source that changed between passes must refuse.
        let suffix = (self.archive_len - self.bounds.end) as usize;
        self.first_pass
            .update(&self.tail[self.tail.len() - suffix..]);
        self.first_digest = std::mem::take(&mut self.first_pass).finalize().to_vec();
        self.begin_entry(0);
        self.phase = ScanPhase::Stream;
        Ok(())
    }

    fn begin_entry(&mut self, index: usize) {
        self.entry = index;
        self.cursor = match self.entries.get(index) {
            Some(entry) => StreamCursor::Header {
                expected: local_header(&entry.fields()),
                at: 0,
            },
            None => StreamCursor::Trailing,
        };
    }

    fn begin_payload(&mut self) -> Result<(), N4aError> {
        let size = self.entries[self.entry].size;
        self.capturing_manifest = self.entries[self.entry].name == MANIFEST;
        if self.capturing_manifest {
            self.ledger.reserve(&mut self.manifest, host_len(size)?)?;
        }
        self.cursor = StreamCursor::Payload {
            remaining: size,
            hasher: N4aMemberHasher::new(),
        };
        if size == 0 {
            self.complete_payload()?;
        }
        Ok(())
    }

    fn complete_payload(&mut self) -> Result<(), N4aError> {
        let cursor = std::mem::replace(&mut self.cursor, StreamCursor::Trailing);
        if let StreamCursor::Payload { hasher, .. } = cursor {
            let digest = hasher.finish();
            let entry = &mut self.entries[self.entry];
            entry.actual_crc = digest.crc32;
            entry.sha256 = digest.sha256;
        }
        if self.capturing_manifest {
            // Discriminate once, before scanning the remaining payloads. Typed
            // V1 decoding and every CRC/SHA/layout check still run in finish.
            match probe_manifest(&self.manifest, &mut self.ledger) {
                Err(error @ N4aError::UnsupportedFormat(_)) => return Err(error),
                // Preserve CRC-before-JSON refusal order for malformed V1.
                Err(error) => self.manifest_probe_error = Some(error),
                Ok(()) => {}
            }
        }
        self.begin_entry(self.entry + 1);
        Ok(())
    }

    fn feed_stream(&mut self, bytes: &[u8]) -> Result<(), N4aError> {
        self.container.update(bytes);
        self.stream_read += bytes.len() as u64;
        let mut rest = bytes;
        while !rest.is_empty() {
            match &mut self.cursor {
                StreamCursor::Header { expected, at } => {
                    let take = rest.len().min(expected.len() - *at);
                    if rest[..take] != expected[*at..*at + take] {
                        return refuse("ZIP local header mismatch");
                    }
                    rest = &rest[take..];
                    *at += take;
                    if *at == expected.len() {
                        self.begin_payload()?;
                    }
                }
                StreamCursor::Payload { remaining, hasher } => {
                    let take = (rest.len() as u64).min(*remaining) as usize;
                    hasher.update(&rest[..take]);
                    *remaining -= take as u64;
                    let done = *remaining == 0;
                    if self.capturing_manifest {
                        self.manifest.extend_from_slice(&rest[..take]);
                    }
                    rest = &rest[take..];
                    if done {
                        self.complete_payload()?;
                    }
                }
                StreamCursor::Trailing => {
                    self.second_pass.update(rest);
                    rest = &[];
                }
            }
        }
        if self.stream_read == self.archive_len {
            let second = std::mem::take(&mut self.second_pass).finalize();
            if second.as_slice() != self.first_digest.as_slice() {
                return Err(N4aError::Integrity(
                    "archive changed while being read".into(),
                ));
            }
            self.phase = ScanPhase::Complete;
        }
        Ok(())
    }

    /// Validate the fully read archive and produce its metadata.
    pub fn finish(mut self) -> Result<N4aScanned, N4aError> {
        if self.phase != ScanPhase::Complete {
            return refuse("archive scan is incomplete");
        }
        for entry in &self.entries {
            if entry.actual_crc != entry.crc {
                return Err(N4aError::Integrity(format!(
                    "ZIP CRC mismatch for `{}`",
                    entry.name
                )));
            }
        }
        if let Some(error) = self.manifest_probe_error {
            return Err(error);
        }
        if self
            .entries
            .iter()
            .any(|entry| entry.time != DOS_TIME || entry.date != DOS_DATE)
        {
            return refuse("canonical ZIP requires DOS time 0 and date 0x0021 on every member");
        }
        let manifest = decode_manifest(&self.manifest)?;
        let index = build_index(&manifest, &self.entries, &mut self.ledger)?;
        let manifest_entry = self
            .entries
            .iter()
            .find(|entry| entry.name == MANIFEST)
            .ok_or_else(|| invalid("manifest is absent"))?;
        let receipt = N4aReceipt {
            container_sha256: format!("{:x}", self.container.finalize()),
            manifest_sha256: manifest_entry.sha256.clone(),
            byte_length: self.archive_len,
            members: manifest.members.clone(),
        };
        charge_inventory(&mut self.ledger, &manifest)?;
        // Scan-only state goes away with `self`; the rest is the retained set.
        self.ledger.release(self.chunk);
        self.ledger.release(scan_entries_cost(&self.entries));
        let metadata = N4aMetadata {
            manifest,
            receipt,
            index,
            budget: self.budget,
            retained_bytes: self.ledger.used(),
        };
        Ok(N4aScanned {
            metadata,
            manifest_bytes: self.manifest.into_boxed_slice(),
        })
    }
}

fn scan_entries_cost(entries: &[ScanEntry]) -> u64 {
    let names: u64 = entries.iter().map(|e| e.name.len() as u64 + 64).sum();
    names + std::mem::size_of_val(entries) as u64
}

/// The receipt holds a second copy of the inventory.
fn charge_inventory(ledger: &mut Ledger, manifest: &N4aManifest) -> Result<(), N4aError> {
    let mut bytes = 0u64;
    for member in &manifest.members {
        bytes += (size_of::<crate::n4a::N4aMember>() + member.path.len() + 64) as u64;
    }
    ledger.charge(bytes)
}

/// Compare the physical entries to the declared inventory, by path.
fn build_index(
    manifest: &N4aManifest,
    entries: &[ScanEntry],
    ledger: &mut Ledger,
) -> Result<ValidatedIndex, N4aError> {
    let payload_count = entries.len() - 1;
    if manifest.members.len() != payload_count {
        return Err(invalid("physical inventory differs from manifest"));
    }
    let mut declared: Vec<usize> = Vec::new();
    ledger.reserve(&mut declared, payload_count)?;
    declared.extend(0..payload_count);
    declared.sort_unstable_by(|a, b| manifest.members[*a].path.cmp(&manifest.members[*b].path));
    let mut physical: Vec<&ScanEntry> = Vec::new();
    ledger.reserve(&mut physical, entries.len())?;
    physical.extend(entries.iter());
    physical.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    let mut manifest_range = 0..0;
    let mut members = Vec::new();
    ledger.reserve(&mut members, payload_count)?;
    let mut declared = declared.into_iter();
    for entry in physical {
        if entry.name == MANIFEST {
            manifest_range = entry.data_start..entry.data_start + entry.size;
            continue;
        }
        let Some(position) = declared.next() else {
            return Err(invalid("unlisted physical member"));
        };
        let declaration = &manifest.members[position];
        if declaration.path != entry.name {
            return Err(invalid("unlisted physical member"));
        }
        if declaration.byte_length != entry.size || declaration.sha256 != entry.sha256 {
            return Err(N4aError::Integrity(format!(
                "raw size/SHA mismatch for `{}`",
                entry.name
            )));
        }
        members.push(IndexedMember {
            member: position,
            data_start: entry.data_start,
            crc32: entry.crc,
        });
    }
    ledger.release(((payload_count + entries.len()) * size_of::<usize>()) as u64);
    Ok(ValidatedIndex {
        members,
        manifest: manifest_range,
    })
}

/// One member of a write plan, with the digest of its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct N4aPlannedMember {
    pub path: String,
    pub digest: N4aMemberDigest,
}

/// The next action of a host driving an [`N4aWritePlan`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum N4aWriteStep {
    /// Framing or manifest bytes to append to the sink unchanged.
    Frame(Vec<u8>),
    /// Stream exactly `byte_length` payload bytes: append each chunk to the sink
    /// and pass it to [`N4aWritePlan::update_member`].
    Member { path: String, byte_length: u64 },
}

#[derive(Clone, Copy, Debug)]
enum WriteCursor {
    ManifestHeader,
    ManifestData(usize),
    MemberHeader(usize),
    MemberData(usize),
    Central(usize),
    Tail,
    Done,
}

struct ActiveMember {
    entry: usize,
    fed: u64,
    /// `None` when the caller already proved these exact bytes.
    hasher: Option<N4aMemberHasher>,
}

struct PlanEntry {
    path: String,
    digest: N4aMemberDigest,
    offset: u64,
}

impl PlanEntry {
    fn fields(&self) -> MemberFields<'_> {
        MemberFields {
            name: self.path.as_bytes(),
            version_needed: writer_version(self.digest.byte_length, self.offset),
            flags: UTF8_FLAG,
            time: DOS_TIME,
            date: DOS_DATE,
            crc: self.digest.crc32,
            size: self.digest.byte_length,
        }
    }
}

/// Fully serialized manifest length, without keeping the bytes.
struct CountingWriter(u64);
impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serialize_manifest(manifest: &N4aManifest, ledger: &mut Ledger) -> Result<Vec<u8>, N4aError> {
    let serialization = |error: serde_json::Error| {
        N4aError::Physical(format!("manifest serialization failed: {error}"))
    };
    let mut counter = CountingWriter(0);
    serde_json::to_writer(&mut counter, manifest).map_err(serialization)?;
    let mut bytes = Vec::new();
    ledger.reserve(&mut bytes, host_len(counter.0)?)?;
    serde_json::to_writer(&mut bytes, manifest).map_err(serialization)?;
    Ok(bytes)
}

/// Sans-IO writer of a canonical container.
///
/// The layout, the total length and the exact manifest are fixed from digests
/// before any byte is emitted, because the manifest is first and contains them.
/// Member order is the manifest followed by members sorted by path.
pub struct N4aWritePlan {
    manifest: N4aManifest,
    manifest_bytes: Vec<u8>,
    manifest_sha256: String,
    entries: Vec<PlanEntry>,
    central_offset: u64,
    central_size: u64,
    byte_length: u64,
    chunk: u64,
    budget: N4aMemoryBudget,
    ledger: Ledger,
    cursor: WriteCursor,
    active: Option<ActiveMember>,
    container: Sha256,
    written: u64,
}

impl N4aWritePlan {
    /// `members` is the complete physical set and must exactly match
    /// `manifest.members`. Declared sizes and hashes are replaced by `digest`.
    pub fn new(
        mut manifest: N4aManifest,
        mut members: Vec<N4aPlannedMember>,
        budget: N4aMemoryBudget,
    ) -> Result<Self, N4aError> {
        let chunk = budget.chunk_bytes();
        let mut ledger = Ledger::new(budget);
        ledger.charge(chunk)?;
        for member in &members {
            canonical_path_ok(&member.path)?;
        }
        members.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        if members.windows(2).any(|pair| pair[0].path == pair[1].path) {
            return Err(invalid("duplicate input path"));
        }
        let mut declared: Vec<usize> = Vec::new();
        ledger.reserve(&mut declared, manifest.members.len())?;
        declared.extend(0..manifest.members.len());
        declared.sort_unstable_by(|a, b| manifest.members[*a].path.cmp(&manifest.members[*b].path));
        if declared
            .windows(2)
            .any(|pair| manifest.members[pair[0]].path == manifest.members[pair[1]].path)
        {
            return Err(invalid("duplicate inventory path"));
        }
        for member in &mut manifest.members {
            let found = members
                .binary_search_by(|planned| planned.path.as_str().cmp(member.path.as_str()))
                .map_err(|_| invalid("declared member bytes are absent"))?;
            member.byte_length = members[found].digest.byte_length;
            member.sha256.clone_from(&members[found].digest.sha256);
        }
        if members.len() != declared.len() {
            return Err(invalid("undeclared input members"));
        }
        validate(&manifest)?;
        let manifest_bytes = serialize_manifest(&manifest, &mut ledger)?;
        charge_inventory(&mut ledger, &manifest)?;
        let mut manifest_digest = N4aMemberHasher::new();
        manifest_digest.update(&manifest_bytes);
        let manifest_digest = manifest_digest.finish();
        let manifest_sha256 = manifest_digest.sha256.clone();

        let mut entries = Vec::new();
        ledger.reserve(&mut entries, members.len() + 1)?;
        entries.push(PlanEntry {
            path: MANIFEST.to_owned(),
            digest: manifest_digest,
            offset: 0,
        });
        for member in members {
            ledger.charge((member.path.len() + 64) as u64)?;
            entries.push(PlanEntry {
                path: member.path,
                digest: member.digest,
                offset: 0,
            });
        }
        let (central_offset, central_size, byte_length) = lay_out(&mut entries)?;
        Ok(Self {
            manifest,
            manifest_bytes,
            manifest_sha256,
            entries,
            central_offset,
            central_size,
            byte_length,
            chunk,
            budget,
            ledger,
            cursor: WriteCursor::ManifestHeader,
            active: None,
            container: Sha256::new(),
            written: 0,
        })
    }

    /// Exact final archive size, known before writing.
    pub fn byte_length(&self) -> u64 {
        self.byte_length
    }
    pub fn chunk_bytes(&self) -> u64 {
        self.chunk
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }

    fn close_member(&mut self) -> Result<(), N4aError> {
        let Some(active) = self.active.take() else {
            return Ok(());
        };
        let entry = &self.entries[active.entry];
        let changed = || {
            N4aError::Integrity(format!(
                "member source `{}` changed between the hash and write passes",
                entry.path
            ))
        };
        if active.fed != entry.digest.byte_length {
            return Err(changed());
        }
        if let Some(hasher) = active.hasher {
            if hasher.finish() != entry.digest {
                return Err(changed());
            }
        }
        Ok(())
    }

    fn emit(&mut self, frame: Vec<u8>) -> Option<N4aWriteStep> {
        self.container.update(&frame);
        self.written += frame.len() as u64;
        Some(N4aWriteStep::Frame(frame))
    }

    /// `Ok(None)` when complete. Refuses a member that was under-fed or changed.
    pub fn next_step(&mut self) -> Result<Option<N4aWriteStep>, N4aError> {
        self.close_member()?;
        loop {
            match self.cursor {
                WriteCursor::ManifestHeader => {
                    self.cursor = WriteCursor::ManifestData(0);
                    return Ok(self.emit(local_header(&self.entries[0].fields())));
                }
                WriteCursor::ManifestData(at) => {
                    let end = self
                        .manifest_bytes
                        .len()
                        .min(at.saturating_add(self.chunk as usize));
                    if at == end {
                        self.cursor = WriteCursor::MemberHeader(1);
                        continue;
                    }
                    self.cursor = WriteCursor::ManifestData(end);
                    let frame = self.manifest_bytes[at..end].to_vec();
                    return Ok(self.emit(frame));
                }
                WriteCursor::MemberHeader(index) => {
                    if index == self.entries.len() {
                        self.cursor = WriteCursor::Central(0);
                        continue;
                    }
                    self.cursor = WriteCursor::MemberData(index);
                    return Ok(self.emit(local_header(&self.entries[index].fields())));
                }
                WriteCursor::MemberData(index) => {
                    self.cursor = WriteCursor::MemberHeader(index + 1);
                    self.active = Some(ActiveMember {
                        entry: index,
                        fed: 0,
                        hasher: Some(N4aMemberHasher::new()),
                    });
                    let entry = &self.entries[index];
                    return Ok(Some(N4aWriteStep::Member {
                        path: entry.path.clone(),
                        byte_length: entry.digest.byte_length,
                    }));
                }
                WriteCursor::Central(mut index) => {
                    if index == self.entries.len() {
                        self.cursor = WriteCursor::Tail;
                        continue;
                    }
                    let mut frame = Vec::new();
                    while index < self.entries.len() && (frame.len() as u64) < self.chunk {
                        let entry = &self.entries[index];
                        central_record(&mut frame, &entry.fields(), entry.offset);
                        index += 1;
                    }
                    self.cursor = WriteCursor::Central(index);
                    return Ok(self.emit(frame));
                }
                WriteCursor::Tail => {
                    self.cursor = WriteCursor::Done;
                    let frame = tail_bytes(
                        self.entries.len() as u64,
                        self.central_size,
                        self.central_offset,
                        self.central_offset + self.central_size,
                    );
                    return Ok(self.emit(frame));
                }
                WriteCursor::Done => return Ok(None),
            }
        }
    }

    /// Hash and account one payload chunk of the current member.
    pub fn update_member(&mut self, chunk: &[u8]) -> Result<(), N4aError> {
        let Some(active) = self.active.as_mut() else {
            return refuse("no member is being written");
        };
        let entry = &self.entries[active.entry];
        let fed = active.fed + chunk.len() as u64;
        if fed > entry.digest.byte_length {
            return Err(N4aError::Integrity(format!(
                "member source `{}` grew between the hash and write passes",
                entry.path
            )));
        }
        active.fed = fed;
        if let Some(hasher) = active.hasher.as_mut() {
            hasher.update(chunk);
        }
        self.container.update(chunk);
        self.written += chunk.len() as u64;
        Ok(())
    }

    /// The caller proves the current member's bytes itself: immutable validated
    /// storage, or a reader that verifies them. Avoids a redundant hash.
    pub(crate) fn trust_current_member(&mut self) {
        if let Some(active) = self.active.as_mut() {
            active.hasher = None;
        }
    }

    /// Charge and reserve the whole output of a byte writer.
    pub(crate) fn reserve_output(&mut self, out: &mut Vec<u8>) -> Result<(), N4aError> {
        self.ledger.reserve(out, host_len(self.byte_length)?)
    }

    fn close(&mut self) -> Result<String, N4aError> {
        self.close_member()?;
        if !matches!(self.cursor, WriteCursor::Done) || self.written != self.byte_length {
            return refuse("write plan is incomplete");
        }
        let container = std::mem::take(&mut self.container);
        Ok(format!("{:x}", container.finalize()))
    }

    /// The receipt of the exact bytes emitted. Refuses unless every step ran.
    pub fn finish(mut self) -> Result<N4aReceipt, N4aError> {
        let container_sha256 = self.close()?;
        Ok(N4aReceipt {
            container_sha256,
            manifest_sha256: self.manifest_sha256,
            byte_length: self.byte_length,
            members: self.manifest.members,
        })
    }

    /// Metadata of the written container, so a byte writer can return a handle
    /// without reparsing its own output.
    pub(crate) fn finish_metadata(mut self) -> Result<N4aMetadata, N4aError> {
        let container_sha256 = self.close()?;
        let mut positions: Vec<usize> = Vec::new();
        self.ledger
            .reserve(&mut positions, self.manifest.members.len())?;
        positions.extend(0..self.manifest.members.len());
        positions.sort_unstable_by(|a, b| {
            self.manifest.members[*a]
                .path
                .cmp(&self.manifest.members[*b].path)
        });
        let mut members = Vec::new();
        self.ledger
            .reserve(&mut members, self.manifest.members.len())?;
        // Entries after the manifest are sorted by path, exactly like `positions`.
        for (position, entry) in positions.into_iter().zip(self.entries.iter().skip(1)) {
            members.push(IndexedMember {
                member: position,
                data_start: entry.offset
                    + local_header_len(entry.path.len(), entry.digest.byte_length),
                crc32: entry.digest.crc32,
            });
        }
        let manifest_start = local_header_len(MANIFEST.len(), self.entries[0].digest.byte_length);
        let receipt = N4aReceipt {
            container_sha256,
            manifest_sha256: self.manifest_sha256,
            byte_length: self.byte_length,
            members: self.manifest.members.clone(),
        };
        self.ledger.release(self.chunk);
        Ok(N4aMetadata {
            manifest: self.manifest,
            receipt,
            index: ValidatedIndex {
                members,
                manifest: manifest_start..manifest_start + self.manifest_bytes.len() as u64,
            },
            budget: self.budget,
            retained_bytes: self.ledger.used(),
        })
    }
}

/// One forward pass: each entry depends only on the entries before it.
/// Returns the central offset, central size and total archive length.
fn lay_out(entries: &mut [PlanEntry]) -> Result<(u64, u64, u64), N4aError> {
    let overflow = || N4aError::Physical("ZIP offset overflow".into());
    let mut offset = 0u64;
    let mut central_size = 0u64;
    for entry in entries.iter_mut() {
        entry.offset = offset;
        let size = entry.digest.byte_length;
        let name_len = entry.path.len();
        offset = offset
            .checked_add(local_header_len(name_len, size))
            .and_then(|next| next.checked_add(size))
            .ok_or_else(overflow)?;
        central_size = central_size
            .checked_add(central_record_len(name_len, size, entry.offset))
            .ok_or_else(overflow)?;
    }
    let tail = tail_len(entries.len() as u64, central_size, offset);
    let byte_length = offset
        .checked_add(central_size)
        .and_then(|next| next.checked_add(tail))
        .ok_or_else(overflow)?;
    Ok((offset, central_size, byte_length))
}

#[cfg(test)]
mod tests {
    use super::*;

    const S32: u64 = SENTINEL_32;

    fn fields(name: &[u8], size: u64, offset: u64) -> MemberFields<'_> {
        MemberFields {
            name,
            version_needed: writer_version(size, offset),
            flags: UTF8_FLAG,
            time: DOS_TIME,
            date: DOS_DATE,
            crc: 0x1234_5678,
            size,
        }
    }

    #[test]
    fn local_headers_use_zip64_exactly_at_the_sentinel() {
        for (size, zip64) in [(0, false), (S32 - 1, false), (S32, true), (6 << 30, true)] {
            let header = local_header(&fields(b"a", size, 0));
            assert_eq!(header.len() as u64, local_header_len(1, size));
            assert_eq!(le16(&header, 4), if zip64 { 45 } else { 20 });
            assert_eq!(le16(&header, 28), if zip64 { 20 } else { 0 });
            assert_eq!(u64::from(le32(&header, 18)), size.min(S32));
            assert_eq!(u64::from(le32(&header, 22)), size.min(S32));
            if zip64 {
                assert_eq!(le16(&header, 31), ZIP64_EXTRA_ID);
                assert_eq!(le16(&header, 33), 16);
                assert_eq!(le64(&header, 35), size);
                assert_eq!(le64(&header, 43), size);
            }
        }
    }

    #[test]
    fn central_records_carry_only_the_fields_that_overflow_and_round_trip() {
        for (size, offset, payload) in [
            (S32 - 1, S32 - 1, 0),
            (S32, S32 - 1, 16),
            (S32 - 1, S32, 8),
            (S32, S32, 24),
            (7 << 30, 9 << 30, 24),
        ] {
            let mut record = Vec::new();
            central_record(&mut record, &fields(b"a", size, offset), offset);
            assert_eq!(record.len() as u64, central_record_len(1, size, offset));
            assert_eq!(
                le16(&record, 30),
                if payload == 0 { 0 } else { 4 + payload }
            );
            let (entry, used) = parse_central_record(&record).unwrap().unwrap();
            assert_eq!(used, record.len());
            assert_eq!((entry.size, entry.local_offset), (size, offset));
            assert_eq!(entry.version_needed, writer_version(size, offset));
        }
    }

    fn central_sizes_refusal(extra: &[u8], size: u32, offset: u32) -> bool {
        parse_central_sizes(extra, size, size, offset).is_err()
    }

    #[test]
    fn central_extra_field_must_be_exactly_what_the_values_require() {
        let mut extra = Vec::new();
        push16(&mut extra, ZIP64_EXTRA_ID);
        push16(&mut extra, 16);
        push64(&mut extra, S32);
        push64(&mut extra, S32);
        assert!(parse_central_sizes(&extra, u32::MAX, u32::MAX, 0).is_ok());
        // An extra on a member that needs none, and a value that fits 32 bits.
        assert!(central_sizes_refusal(&extra, 5, 0));
        let mut small = extra.clone();
        small[4..12].copy_from_slice(&5u64.to_le_bytes());
        small[12..20].copy_from_slice(&5u64.to_le_bytes());
        assert!(central_sizes_refusal(&small, u32::MAX, 0));
        // Differing sizes, a foreign extra ID, a wrong declared length.
        let mut differing = extra.clone();
        differing[12..20].copy_from_slice(&(S32 + 1).to_le_bytes());
        assert!(central_sizes_refusal(&differing, u32::MAX, 0));
        let mut foreign = extra.clone();
        foreign[0] = 2;
        assert!(central_sizes_refusal(&foreign, u32::MAX, 0));
        let mut wrong_length = extra.clone();
        wrong_length[2] = 8;
        assert!(central_sizes_refusal(&wrong_length, u32::MAX, 0));
        // A sentinel with no extra, and only one of the two size fields set.
        assert!(central_sizes_refusal(b"", u32::MAX, 0));
        assert!(parse_central_sizes(&extra, u32::MAX, 5, 0).is_err());
        assert!(central_sizes_refusal(&extra, u32::MAX, u32::MAX));
    }

    fn tail_round_trip(count: u64, size: u64, offset: u64) -> CentralBounds {
        let end = offset + size;
        let tail = tail_bytes(count, size, offset, end);
        assert_eq!(tail.len() as u64, tail_len(count, size, offset));
        parse_tail(&tail, end + tail.len() as u64).unwrap()
    }

    #[test]
    fn end_records_use_zip64_exactly_at_each_sentinel() {
        for (count, size, offset, zip64) in [
            (0xfffe, 0xfffe * 46, 0, false),
            (0xffff, 0xffff * 46, 0, true),
            (2, S32 - 1, 0, false),
            (2, S32, 0, true),
            (2, 92, S32 - 1, false),
            (2, 92, S32, true),
            (70_000, 70_000 * 46, 9 << 30, true),
        ] {
            let bounds = tail_round_trip(count, size, offset);
            assert_eq!((bounds.count, bounds.size), (count, size));
            assert_eq!((bounds.offset, bounds.end), (offset, offset + size));
            assert_eq!(tail_needs_zip64(count, size, offset), zip64);
        }
    }

    #[test]
    fn forged_zip64_end_records_refuse() {
        let (count, size, offset) = (2u64, 92u64, S32);
        let good = tail_bytes(count, size, offset, offset + size);
        let archive_len = offset + size + good.len() as u64;
        assert!(parse_tail(&good, archive_len).is_ok());
        let mutate = |at: usize, value: u8| {
            let mut bad = good.clone();
            bad[at] = value;
            parse_tail(&bad, archive_len)
        };
        assert!(mutate(0, 0).is_err(), "EOCD64 signature");
        assert!(mutate(4, 45).is_err(), "EOCD64 record size");
        assert!(mutate(12, 44).is_err(), "made-by");
        assert!(mutate(14, 44).is_err(), "version-needed");
        assert!(mutate(16, 1).is_err(), "disk");
        assert!(mutate(24, 3).is_err(), "disk count differs from total");
        assert!(mutate(56, 0).is_err(), "locator signature");
        assert!(mutate(64, 1).is_err(), "locator offset");
        assert!(mutate(72, 2).is_err(), "disk total");
        // Classic field disagreeing with the 64-bit record.
        assert!(mutate(ZIP64_TAIL_LEN - EOCD_LEN + 10, 9).is_err());
        let mut forged = tail_bytes(2, 92, 10, 102);
        // Claim ZIP64 while every value fits.
        forged[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_tail(&forged, 124).is_err());
    }

    #[test]
    fn hostile_central_bounds_refuse_before_any_reservation() {
        for (count, size) in [(0u64, 0u64), (3, 92)] {
            let mut archive = vec![0; 10 + size as usize];
            archive.extend(tail_bytes(count, size, 10, 10 + size));
            let mut scanner =
                N4aScanner::new(archive.len() as u64, N4aMemoryBudget::default()).unwrap();
            let request = scanner.next_read().unwrap();
            let error = scanner
                .feed(&archive[request.offset as usize..])
                .unwrap_err();
            assert!(matches!(error, N4aError::Physical(_)), "{error}");
            assert_eq!(scanner.entries.capacity(), 0);
        }
    }

    #[test]
    fn records_split_across_tiny_feeds_scan_like_a_single_pass() {
        let paths = ["a.bin", "empty", "z.bin"];
        let manifest: N4aManifest = serde_json::from_value(serde_json::json!({
            "schema": crate::n4a::N4A_SCHEMA, "schema_version": 1, "minimum_reader_version": 1,
            "container_id": "storage-only:chunks", "required_features": [],
            "physical_profile": crate::n4a::N4A_PHYSICAL_PROFILE,
            "members": paths.map(|path| serde_json::json!({
                "path": path, "byte_length": 0, "sha256": "0".repeat(64),
            })),
            "artifacts": [{
                "id": "opaque", "kind": "attachment", "owner": "synthetic-test",
                "content_type": {"id": "synthetic.opaque.v1", "version": 1},
                "identity": {
                    "owner": "synthetic-test", "schema": "synthetic.identity.v1",
                    "schema_version": 1,
                    "fingerprint_profile": "synthetic-only.no-scientific-validation",
                    "fingerprint": "opaque-owner-test-token",
                },
                "storage": {"kind": "embedded", "root_member": paths[0], "member_paths": paths},
                "requirements": null,
            }],
            "links": [], "entrypoints": [],
        }))
        .unwrap();
        let payloads: [&[u8]; 3] = [b"first payload", b"", b"last payload"];
        let members = paths
            .iter()
            .zip(payloads)
            .map(|(path, bytes)| crate::n4a::N4aMemberInput { path, bytes })
            .collect();
        let request = crate::n4a::N4aWriteRequest { manifest, members };
        let (receipt, bytes) =
            crate::n4a::write_n4a_bytes(request, N4aMemoryBudget::default()).unwrap();

        let mut scanner = N4aScanner::new(bytes.len() as u64, N4aMemoryBudget::default()).unwrap();
        scanner.chunk = 7;
        while let Some(request) = scanner.next_read() {
            let start = request.offset as usize;
            scanner
                .feed(&bytes[start..start + request.length as usize])
                .unwrap();
        }
        let scanned = scanner.finish().unwrap();
        assert_eq!(scanned.metadata.receipt(), &receipt);
        let location = scanned.metadata.member_location("z.bin").unwrap();
        let start = location.data_offset as usize;
        assert_eq!(
            &bytes[start..start + location.byte_length as usize],
            b"last payload"
        );
        assert_eq!(
            scanned
                .metadata
                .member_location("empty")
                .unwrap()
                .byte_length,
            0
        );
    }

    #[test]
    fn budget_chunk_is_derived_and_charged() {
        assert_eq!(N4aMemoryBudget::default().chunk_bytes(), MAX_CHUNK);
        assert_eq!(N4aMemoryBudget::bytes(0).chunk_bytes(), MIN_CHUNK);
        assert_eq!(N4aMemoryBudget::bytes(1 << 16).chunk_bytes(), 1 << 14);
        assert_eq!(N4aMemoryBudget::bytes(u64::MAX).chunk_bytes(), MAX_CHUNK);
        assert!(matches!(
            N4aScanner::new(22, N4aMemoryBudget::bytes(MIN_CHUNK - 1)),
            Err(N4aError::Memory(_))
        ));
        assert!(N4aMemoryBudget::bytes(9).admit(10).is_err());
        assert!(N4aMemoryBudget::default().admit(u64::MAX).is_ok());
    }
}
