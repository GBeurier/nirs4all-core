//! File-backed `.n4a` V1 handle and streaming writer, for hosts with a filesystem.
//!
//! Data larger than RAM stays on disk: a handle keeps only metadata, and every
//! payload moves through one bounded buffer. The file is neither locked nor
//! mapped, so another process can change it. Each completed read therefore hashes
//! the bytes it actually returns and compares them to the validated receipt,
//! during that one read, with no second scan and no reopening. The guarantee is
//! about the bytes a read returned, not about the file afterwards.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::durability::sync_directory;
use crate::n4a::{
    invalid, N4aError, N4aManifest, N4aMemberBytes, N4aMemberDigest, N4aMemberHasher,
    N4aMemoryBudget, N4aMetadata, N4aPlannedMember, N4aReceipt, N4aScanned, N4aScanner,
    N4aWritePlan, N4aWriteStep, OwnedN4a,
};
use crate::n4a_physical::{host_len, Ledger};

#[cfg(unix)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buffer, offset)
}
#[cfg(windows)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buffer, offset)
}

/// Positional read, so concurrent readers of one handle share no cursor.
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> Result<(), N4aError> {
    let mut filled = 0;
    while filled < buffer.len() {
        match read_at(file, &mut buffer[filled..], offset + filled as u64) {
            Ok(0) => {
                return Err(N4aError::Integrity(
                    "archive shrank while being read".into(),
                ))
            }
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[derive(Debug)]
struct FileState {
    file: File,
    metadata: N4aMetadata,
    manifest_bytes: Box<[u8]>,
}

/// A validated file. Cheap to clone; clones share one open file and metadata.
///
/// Metadata queries come from [`N4aMetadata`] through `Deref`.
#[derive(Clone, Debug)]
pub struct N4aFile {
    state: Arc<FileState>,
}

impl Deref for N4aFile {
    type Target = N4aMetadata;
    fn deref(&self) -> &N4aMetadata {
        &self.state.metadata
    }
}

/// Validate a file in one sequential pass, keeping only its metadata in memory.
pub fn open_n4a_file(path: &Path, budget: N4aMemoryBudget) -> Result<N4aFile, N4aError> {
    let file = File::open(path)
        .map_err(|e| N4aError::Physical(format!("cannot open {}: {e}", path.display())))?;
    let properties = file.metadata()?;
    if !properties.is_file() {
        return Err(N4aError::Physical(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    let mut scanner = N4aScanner::new(properties.len(), budget)?;
    let mut buffer = Vec::new();
    while let Some(request) = scanner.next_read() {
        buffer.resize(host_len(request.length)?, 0);
        read_exact_at(&file, &mut buffer, request.offset)?;
        scanner.feed(&buffer)?;
    }
    drop(buffer);
    let N4aScanned {
        metadata,
        manifest_bytes,
    } = scanner.finish()?;
    Ok(N4aFile {
        state: Arc::new(FileState {
            file,
            metadata,
            manifest_bytes,
        }),
    })
}

impl N4aFile {
    pub fn metadata(&self) -> &N4aMetadata {
        &self.state.metadata
    }

    /// Exact manifest bytes, retained in memory at open.
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.state.manifest_bytes
    }

    fn ledger(&self, budget: N4aMemoryBudget) -> Result<Ledger, N4aError> {
        let mut ledger = Ledger::new(budget);
        ledger.charge(self.metadata().retained_bytes())?;
        Ok(ledger)
    }

    /// One charged copy buffer under this handle's budget.
    fn copy_buffer(&self) -> Result<Vec<u8>, N4aError> {
        let budget = self.memory_budget();
        let length = host_len(budget.chunk_bytes())?;
        let mut buffer = Vec::new();
        self.ledger(budget)?.reserve(&mut buffer, length)?;
        buffer.resize(length, 0);
        Ok(buffer)
    }

    /// Stream one member. The reader owns its handle, so it can outlive `self`.
    pub fn member_reader(&self, path: &str) -> Result<N4aMemberReader, N4aError> {
        let location = self
            .member_location(path)
            .ok_or_else(|| invalid("member is absent from the validated inventory"))?;
        Ok(N4aMemberReader {
            state: Arc::clone(&self.state),
            path: path.to_owned(),
            offset: location.data_offset,
            byte_length: location.byte_length,
            remaining: location.byte_length,
            verification: Verification::Pending(N4aMemberHasher::new()),
        })
    }

    /// Materialize one member, verified by the same read that returns it.
    pub fn read_member(&self, path: &str, budget: N4aMemoryBudget) -> Result<Vec<u8>, N4aError> {
        let mut reader = self.member_reader(path)?;
        let length = host_len(reader.byte_length())?;
        let mut bytes = Vec::new();
        self.ledger(budget)?.reserve(&mut bytes, length)?;
        bytes.resize(length, 0);
        reader.read_all_into(&mut bytes)?;
        Ok(bytes)
    }

    /// Copy one member through a bounded buffer. On `Err`, `out` already holds
    /// unverified bytes and the caller must discard it.
    pub fn copy_member_to<W: Write>(&self, path: &str, out: &mut W) -> Result<u64, N4aError> {
        let mut reader = self.member_reader(path)?;
        let mut buffer = self.copy_buffer()?;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                return Ok(reader.byte_length());
            }
            out.write_all(&buffer[..read]).map_err(write_refusal)?;
        }
    }

    /// Copy the exact archive through a bounded buffer, verifying
    /// `container_sha256`. The last chunk is withheld until that succeeds. On
    /// `Err`, `out` already holds unverified bytes and the caller must discard it.
    pub fn copy_archive_to<W: Write>(&self, out: &mut W) -> Result<u64, N4aError> {
        let mut buffer = self.copy_buffer()?;
        let length = self.receipt().byte_length;
        let mut hasher = N4aMemberHasher::new();
        let mut offset = 0;
        while offset < length {
            let chunk = (length - offset).min(buffer.len() as u64) as usize;
            read_exact_at(&self.state.file, &mut buffer[..chunk], offset)?;
            hasher.update(&buffer[..chunk]);
            offset += chunk as u64;
            if offset == length {
                self.verify_archive(&std::mem::take(&mut hasher).finish())?;
            }
            out.write_all(&buffer[..chunk]).map_err(write_refusal)?;
        }
        Ok(length)
    }

    /// Materialize the archive for owner actions that need it in memory.
    ///
    /// The bytes are hashed by the read that fills the buffer and compared to the
    /// validated receipt, so a file changed since opening refuses. Metadata is
    /// then reused for those proved bytes, with no reparse.
    pub fn into_owned(self, budget: N4aMemoryBudget) -> Result<OwnedN4a, N4aError> {
        let length = host_len(self.receipt().byte_length)?;
        let mut bytes = Vec::new();
        self.ledger(budget)?.reserve(&mut bytes, length)?;
        bytes.resize(length, 0);
        let chunk = host_len(budget.chunk_bytes())?;
        let mut hasher = N4aMemberHasher::new();
        let mut offset = 0;
        for piece in bytes.chunks_mut(chunk) {
            read_exact_at(&self.state.file, piece, offset)?;
            hasher.update(piece);
            offset += piece.len() as u64;
        }
        self.verify_archive(&hasher.finish())?;
        let metadata = match Arc::try_unwrap(self.state) {
            Ok(state) => state.metadata,
            Err(shared) => shared.metadata.clone(),
        };
        Ok(OwnedN4a::from_verified(bytes.into_boxed_slice(), metadata))
    }
}

impl N4aMemberBytes for N4aFile {
    fn metadata(&self) -> &N4aMetadata {
        &self.state.metadata
    }
    fn member_bytes(&self, path: &str) -> Result<Cow<'_, [u8]>, N4aError> {
        self.read_member(path, self.memory_budget()).map(Cow::Owned)
    }
    fn resident_bytes(&self) -> u64 {
        0
    }
}

enum Verification {
    Pending(N4aMemberHasher),
    Verified,
    Failed,
}

/// Sequential verified member stream. It fills the caller's buffer directly.
///
/// Bytes returned before the end are not yet verified. The `read` that would
/// deliver the last byte first compares size, SHA-256 and CRC-32 to the validated
/// inventory; on a mismatch it returns `Err`, clears those bytes, and every later
/// `read` fails too. So a delivered last byte means the whole sequence returned
/// matched the receipt, and [`is_verified`](Self::is_verified) is then true. A
/// zero-length member verifies on the first `read`. A reader abandoned early has
/// verified nothing.
///
/// Errors are `io::Error`s that convert back with `N4aError::from`.
pub struct N4aMemberReader {
    state: Arc<FileState>,
    path: String,
    offset: u64,
    byte_length: u64,
    remaining: u64,
    verification: Verification,
}

impl N4aMemberReader {
    pub fn byte_length(&self) -> u64 {
        self.byte_length
    }
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
    pub fn is_verified(&self) -> bool {
        matches!(self.verification, Verification::Verified)
    }

    fn refused(error: N4aError) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, error)
    }

    fn verify(&mut self) -> Result<(), N4aError> {
        let state = std::mem::replace(&mut self.verification, Verification::Failed);
        if let Verification::Pending(hasher) = state {
            self.state
                .metadata
                .verify_member(&self.path, &hasher.finish())?;
            self.verification = Verification::Verified;
        }
        Ok(())
    }

    fn fill(&mut self, buffer: &mut [u8]) -> Result<usize, N4aError> {
        let Verification::Pending(hasher) = &mut self.verification else {
            return Ok(0);
        };
        let length = self.remaining.min(buffer.len() as u64) as usize;
        let target = &mut buffer[..length];
        read_exact_at(&self.state.file, target, self.offset)?;
        hasher.update(target);
        self.offset += length as u64;
        self.remaining -= length as u64;
        Ok(length)
    }

    /// Fill `buffer`, exactly the member length, in one verified read.
    fn read_all_into(&mut self, buffer: &mut [u8]) -> Result<(), N4aError> {
        self.fill(buffer)?;
        self.verify()
    }
}

impl Read for N4aMemberReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.verification {
            Verification::Verified => return Ok(0),
            Verification::Failed => {
                return Err(Self::refused(N4aError::Integrity(format!(
                    "member `{}` failed verification",
                    self.path
                ))))
            }
            Verification::Pending(_) => {}
        }
        if buffer.is_empty() && self.remaining != 0 {
            return Ok(0);
        }
        let read = match self.fill(buffer) {
            Ok(read) => read,
            Err(error) => {
                self.verification = Verification::Failed;
                return Err(Self::refused(error));
            }
        };
        if self.remaining == 0 {
            if let Err(error) = self.verify() {
                buffer[..read].fill(0);
                return Err(Self::refused(error));
            }
        }
        Ok(read)
    }
}

/// A member whose bytes can be read more than once.
pub trait N4aMemberSource {
    /// Called up to twice: the hash pass and the write pass. Each call starts at
    /// byte 0. A source that differs between passes refuses the write.
    fn open(&self) -> io::Result<Box<dyn Read + '_>>;
}
impl N4aMemberSource for &[u8] {
    fn open(&self) -> io::Result<Box<dyn Read + '_>> {
        Ok(Box::new(&**self))
    }
}
impl N4aMemberSource for Vec<u8> {
    fn open(&self) -> io::Result<Box<dyn Read + '_>> {
        Ok(Box::new(self.as_slice()))
    }
}
impl N4aMemberSource for &Path {
    fn open(&self) -> io::Result<Box<dyn Read + '_>> {
        Ok(Box::new(File::open(self)?))
    }
}
impl N4aMemberSource for PathBuf {
    fn open(&self) -> io::Result<Box<dyn Read + '_>> {
        Ok(Box::new(File::open(self)?))
    }
}

pub struct N4aStreamInput<'a> {
    pub path: &'a str,
    pub source: &'a dyn N4aMemberSource,
}

/// Immutable base of an update. Declared paths absent from the inputs come from it.
pub enum N4aStreamBase<'a> {
    File(&'a N4aFile),
    Owned(&'a OwnedN4a),
}

impl N4aStreamBase<'_> {
    fn digest(&self, path: &str) -> Option<N4aMemberDigest> {
        match self {
            Self::File(file) => file.member_digest(path),
            Self::Owned(container) => container.member_digest(path),
        }
    }
}

pub struct N4aStreamRequest<'a> {
    pub manifest: N4aManifest,
    pub members: Vec<N4aStreamInput<'a>>,
    pub base: Option<N4aStreamBase<'a>>,
}

fn write_refusal(error: io::Error) -> N4aError {
    N4aError::Physical(format!("cannot write output: {error}"))
}

/// Hash a source in one pass through `buffer`.
fn digest_source(
    source: &dyn N4aMemberSource,
    buffer: &mut [u8],
) -> Result<N4aMemberDigest, N4aError> {
    let mut reader = source.open()?;
    let mut hasher = N4aMemberHasher::new();
    loop {
        let read = reader.read(buffer)?;
        if read == 0 {
            return Ok(hasher.finish());
        }
        hasher.update(&buffer[..read]);
    }
}

/// Pass one payload stream to the plan and the output.
fn copy_payload<W: Write>(
    reader: &mut dyn Read,
    plan: &mut N4aWritePlan,
    buffer: &mut [u8],
    out: &mut W,
) -> Result<(), N4aError> {
    loop {
        let read = reader.read(buffer)?;
        if read == 0 {
            return Ok(());
        }
        plan.update_member(&buffer[..read])?;
        out.write_all(&buffer[..read]).map_err(write_refusal)?;
    }
}

/// Stream a container to `out` in bounded memory.
///
/// The manifest comes first and contains member hashes, so new sources are read
/// twice: pass one digests them, pass two writes them and compares. Retained base
/// members reuse their validated inventory and skip pass one, and a file base has
/// its actual bytes verified as they are copied. The output is hashed as it is
/// written and never re-read. On `Err`, `out` holds an incomplete container.
pub fn write_n4a_to<W: Write>(
    request: N4aStreamRequest<'_>,
    out: &mut W,
    budget: N4aMemoryBudget,
) -> Result<N4aReceipt, N4aError> {
    let N4aStreamRequest {
        manifest,
        members,
        base,
    } = request;
    // Charged by the plan, which needs its digests first.
    let mut buffer = vec![0u8; host_len(budget.chunk_bytes())?];
    let mut sources = BTreeMap::new();
    let mut planned = Vec::with_capacity(manifest.members.len());
    for input in &members {
        if sources.insert(input.path, input.source).is_some() {
            return Err(invalid("duplicate input path"));
        }
        planned.push(N4aPlannedMember {
            path: input.path.to_owned(),
            digest: digest_source(input.source, &mut buffer)?,
        });
    }
    if let Some(base) = &base {
        for member in &manifest.members {
            if !sources.contains_key(member.path.as_str()) {
                planned.push(N4aPlannedMember {
                    path: member.path.clone(),
                    digest: base
                        .digest(&member.path)
                        .ok_or_else(|| invalid("declared member bytes are absent"))?,
                });
            }
        }
    }
    let mut plan = N4aWritePlan::new(manifest, planned, budget)?;
    while let Some(step) = plan.next_step()? {
        match step {
            N4aWriteStep::Frame(frame) => out.write_all(&frame).map_err(write_refusal)?,
            N4aWriteStep::Member { path, .. } => {
                if let Some(source) = sources.get(path.as_str()) {
                    copy_payload(&mut *source.open()?, &mut plan, &mut buffer, out)?;
                    continue;
                }
                // Only retained members remain: the plan was built from them.
                plan.trust_current_member();
                match &base {
                    Some(N4aStreamBase::File(file)) => {
                        copy_payload(&mut file.member_reader(&path)?, &mut plan, &mut buffer, out)?
                    }
                    Some(N4aStreamBase::Owned(container)) => {
                        let bytes = container
                            .member(&path)
                            .ok_or_else(|| invalid("declared member bytes are absent"))?
                            .bytes;
                        plan.update_member(bytes)?;
                        out.write_all(bytes).map_err(write_refusal)?;
                    }
                    None => return Err(invalid("declared member bytes are absent")),
                }
            }
        }
    }
    plan.finish()
}

/// Write a complete, synced container to a new stage path.
pub(crate) fn write_n4a_stage(
    stage: &Path,
    request: N4aStreamRequest<'_>,
    budget: N4aMemoryBudget,
) -> Result<N4aReceipt, N4aError> {
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage)
        .map_err(write_refusal)?;
    let receipt = write_n4a_to(request, &mut output, budget)?;
    output.sync_all().map_err(write_refusal)?;
    Ok(receipt)
}

/// Stage beside `destination`, then publish with an exclusive hard link: complete
/// bytes appear atomically and an existing path is never replaced. A failed write
/// discards the stage.
pub fn write_n4a_file(
    destination: &Path,
    request: N4aStreamRequest<'_>,
    budget: N4aMemoryBudget,
) -> Result<N4aReceipt, N4aError> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = tempfile::tempdir_in(parent).map_err(write_refusal)?;
    let stage = directory.path().join("container.n4a");
    let receipt = write_n4a_stage(&stage, request, budget)?;
    std::fs::hard_link(&stage, destination).map_err(write_refusal)?;
    sync_directory(parent).map_err(|error| {
        N4aError::Physical(format!(
            "container was published at {} but its directory could not be synced: {error}",
            destination.display()
        ))
    })?;
    Ok(receipt)
}
