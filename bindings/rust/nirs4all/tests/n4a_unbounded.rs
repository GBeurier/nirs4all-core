//! `.n4a` V1 without intrinsic limits: real ZIP64 archives, file-backed handles,
//! the streaming writer and the sans-IO drivers. These bytes are synthetic
//! transport payloads, not executable models.
use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use nirs4all::n4a::*;
use nirs4all::n4a_file::*;
use serde_json::{json, Value};

const MIB: u64 = 1 << 20;

fn manifest_value(paths: &[String]) -> Value {
    json!({"schema":N4A_SCHEMA,"schema_version":1,"minimum_reader_version":1,
        "container_id":"storage-only:unbounded","required_features":[],"physical_profile":N4A_PHYSICAL_PROFILE,
        "members":paths.iter().map(|path| json!({"path":path,"byte_length":0,"sha256":"0".repeat(64)})).collect::<Vec<_>>(),
        "artifacts":[{"id":"opaque","kind":"attachment","owner":"synthetic-test",
            "content_type":{"id":"synthetic.opaque.v1","version":1},
            "identity":{"owner":"synthetic-test","schema":"synthetic.identity.v1","schema_version":1,
                "fingerprint_profile":"synthetic-only.no-scientific-validation","fingerprint":"opaque-owner-test-token"},
            "storage":{"kind":"embedded","root_member":paths[0],"member_paths":paths},"requirements":null}],
        "links":[],"entrypoints":[]})
}
fn manifest(paths: &[String]) -> N4aManifest {
    serde_json::from_value(manifest_value(paths)).unwrap()
}
fn names(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}
fn inputs<'a>(parts: &[(&'a str, &'a [u8])]) -> Vec<N4aMemberInput<'a>> {
    parts
        .iter()
        .map(|(path, bytes)| N4aMemberInput { path, bytes })
        .collect()
}
fn sample(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let paths = parts
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect::<Vec<_>>();
    let request = N4aWriteRequest {
        manifest: manifest(&paths),
        members: inputs(parts),
    };
    write_n4a_bytes(request, N4aMemoryBudget::default())
        .unwrap()
        .1
}
fn digest(bytes: &[u8]) -> N4aMemberDigest {
    let mut hasher = N4aMemberHasher::new();
    hasher.update(bytes);
    hasher.finish()
}
fn u16at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn u32at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn u64at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

#[test]
fn sixty_five_thousand_tiny_members_use_a_real_zip64_directory() {
    let count = 65_536usize;
    let paths = (0..count)
        .map(|index| format!("m/{index:05}"))
        .collect::<Vec<_>>();
    let parts = paths
        .iter()
        .map(|path| N4aMemberInput {
            path,
            bytes: path.as_bytes(),
        })
        .collect::<Vec<_>>();
    let (receipt, bytes) = write_n4a_bytes(
        N4aWriteRequest {
            manifest: manifest(&paths),
            members: parts,
        },
        N4aMemoryBudget::default(),
    )
    .unwrap();

    // 65,537 entries: past the old 4096 entries and the 1 MiB manifest.
    let eocd = bytes.len() - 22;
    assert_eq!(u32at(&bytes, eocd), 0x0605_4b50);
    assert_eq!(u16at(&bytes, eocd + 10), 0xffff);
    assert_eq!(u32at(&bytes, eocd - 20), 0x0706_4b50);
    let eocd64 = eocd - 76;
    assert_eq!(u32at(&bytes, eocd64), 0x0606_4b50);
    assert_eq!(u64at(&bytes, eocd64 + 32), count as u64 + 1);

    let opened = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    assert_eq!(opened.receipt(), &receipt);
    assert!(opened.manifest_bytes().len() as u64 > MIB);
    for path in [&paths[0], &paths[40_000], &paths[count - 1]] {
        assert_eq!(opened.member(path).unwrap().bytes, path.as_bytes());
    }

    // Independent oracle: the Python standard library reads the same bytes.
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("many.n4a");
    std::fs::write(&file, &bytes).unwrap();
    let result = std::process::Command::new("python3")
        .arg("-c")
        .arg(
            r#"
import sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    names = archive.namelist()
    assert len(names) == 65537, len(names)
    assert names[0] == 'manifest.json', names[0]
    assert archive.read('m/40000') == b'm/40000'
    assert archive.testzip() is None
"#,
        )
        .arg(&file)
        .output()
        .expect("Python 3 is required for this interoperability test");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

/// Deterministic bytes that never exist in memory as a whole.
struct Generated {
    length: u64,
    seed: u8,
}
struct GeneratedReader {
    position: u64,
    length: u64,
    seed: u8,
}
impl N4aMemberSource for Generated {
    fn open(&self) -> io::Result<Box<dyn Read + '_>> {
        Ok(Box::new(GeneratedReader {
            position: 0,
            length: self.length,
            seed: self.seed,
        }))
    }
}
impl Read for GeneratedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        const BLOCK: u64 = 1 << 16;
        let total = (self.length - self.position).min(buffer.len() as u64) as usize;
        let mut filled = 0;
        while filled < total {
            let run = (BLOCK - self.position % BLOCK).min((total - filled) as u64) as usize;
            let value = self.seed ^ (self.position / BLOCK) as u8;
            buffer[filled..filled + run].fill(value);
            filled += run;
            self.position += run as u64;
        }
        Ok(total)
    }
}

/// Counts bytes and hashes them, so a read-back needs no buffer either.
struct HashingSink(N4aMemberHasher);
impl Write for HashingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn generated_stream_past_old_member_and_total_caps_stays_in_bounded_memory() {
    let big = Generated {
        length: 300 * MIB,
        seed: 0x5a,
    };
    let other = Generated {
        length: 230 * MIB,
        seed: 0xa5,
    };
    let budget = N4aMemoryBudget::bytes(4 * MIB);
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("large.n4a");
    let receipt = write_n4a_file(
        &destination,
        N4aStreamRequest {
            manifest: manifest(&names(&["big.bin", "other.bin"])),
            members: vec![
                N4aStreamInput {
                    path: "big.bin",
                    source: &big,
                },
                N4aStreamInput {
                    path: "other.bin",
                    source: &other,
                },
            ],
            base: None,
        },
        budget,
    )
    .unwrap();
    assert!(receipt.byte_length > 512 * MIB);
    assert_eq!(
        std::fs::metadata(&destination).unwrap().len(),
        receipt.byte_length
    );

    let file = open_n4a_file(&destination, budget).unwrap();
    assert_eq!(file.receipt(), &receipt);
    let mut sink = HashingSink(N4aMemberHasher::new());
    assert_eq!(
        file.copy_member_to("big.bin", &mut sink).unwrap(),
        300 * MIB
    );
    file.verify_member("big.bin", &sink.0.finish()).unwrap();

    // Materializing is the caller's choice, and a declared budget refuses it.
    assert!(matches!(
        file.read_member("big.bin", budget),
        Err(N4aError::Memory(_))
    ));
    assert!(matches!(
        file.clone().into_owned(budget),
        Err(N4aError::Memory(_))
    ));
}

#[test]
fn explicit_budget_too_small_for_the_working_set_refuses_on_the_file_route() {
    let bytes = sample(&[("a.bin", b"payload")]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("small.n4a");
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        open_n4a_file(&path, N4aMemoryBudget::bytes(1024)),
        Err(N4aError::Memory(_))
    ));
    open_n4a_file(&path, N4aMemoryBudget::bytes(MIB)).unwrap();
    open_n4a_file(&path, N4aMemoryBudget::host_allocator()).unwrap();
}

/// Returns `first` from the first `open` and `later` from every other one.
struct Unstable<'a> {
    first: &'a [u8],
    later: &'a [u8],
    opened: Cell<bool>,
}
impl N4aMemberSource for Unstable<'_> {
    fn open(&self) -> io::Result<Box<dyn Read + '_>> {
        let bytes = if self.opened.replace(true) {
            self.later
        } else {
            self.first
        };
        Ok(Box::new(bytes))
    }
}

#[test]
fn source_changed_between_passes_refuses_and_publishes_nothing() {
    for later in [b"PAYLOAD".as_slice(), b"short", b"payload and more"] {
        let source = Unstable {
            first: b"payload",
            later,
            opened: Cell::new(false),
        };
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("changed.n4a");
        let error = write_n4a_file(
            &destination,
            N4aStreamRequest {
                manifest: manifest(&names(&["a.bin"])),
                members: vec![N4aStreamInput {
                    path: "a.bin",
                    source: &source,
                }],
                base: None,
            },
            N4aMemoryBudget::default(),
        )
        .unwrap_err();
        assert!(matches!(error, N4aError::Integrity(_)), "{error}");
        assert!(!destination.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

fn overwrite(path: &Path, offset: u64, bytes: &[u8]) {
    let mut file = OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(bytes).unwrap();
}

/// A file opened and then changed in `a.bin`, leaving `b.bin` intact.
struct ChangedFile {
    _directory: tempfile::TempDir,
    file: N4aFile,
}
fn file_changed_after_open() -> ChangedFile {
    let bytes = sample(&[("a.bin", b"first payload"), ("b.bin", b"second payload")]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mutable.n4a");
    std::fs::write(&path, &bytes).unwrap();
    let file = open_n4a_file(&path, N4aMemoryBudget::default()).unwrap();
    let location = file.member_location("a.bin").unwrap();
    overwrite(&path, location.data_offset, b"F");
    ChangedFile {
        _directory: directory,
        file,
    }
}

#[test]
fn file_changed_after_open_refuses_on_every_read_of_the_changed_bytes() {
    let changed = file_changed_after_open();
    let file = &changed.file;
    let budget = N4aMemoryBudget::default();
    assert!(matches!(
        file.read_member("a.bin", budget),
        Err(N4aError::Integrity(_))
    ));
    assert!(matches!(
        file.copy_member_to("a.bin", &mut Vec::<u8>::new()),
        Err(N4aError::Integrity(_))
    ));
    assert!(matches!(
        file.copy_archive_to(&mut Vec::<u8>::new()),
        Err(N4aError::Integrity(_))
    ));
    // Old validation is not trusted for changed bytes, and nothing is reparsed.
    assert!(matches!(
        file.clone().into_owned(budget),
        Err(N4aError::Integrity(_))
    ));

    let mut reader = file.member_reader("a.bin").unwrap();
    let mut buffer = [0xffu8; 64];
    let error = reader.read(&mut buffer).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(matches!(N4aError::from(error), N4aError::Integrity(_)));
    assert!(buffer.iter().take(13).all(|byte| *byte == 0));
    assert!(!reader.is_verified());
    assert!(reader.read(&mut buffer).is_err(), "failure is not EOF");

    // The guarantee is per read: an untouched member still verifies.
    assert_eq!(
        file.read_member("b.bin", budget).unwrap(),
        b"second payload"
    );
}

#[test]
fn truncated_file_refuses_and_verified_reader_reports_eof() {
    let bytes = sample(&[("a.bin", b"first payload"), ("empty", b"")]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shrinking.n4a");
    std::fs::write(&path, &bytes).unwrap();
    let file = open_n4a_file(&path, N4aMemoryBudget::default()).unwrap();

    let mut reader = file.member_reader("a.bin").unwrap();
    let mut first = [0u8; 5];
    assert_eq!(reader.read(&mut first).unwrap(), 5);
    assert!(!reader.is_verified());
    assert_eq!(reader.remaining(), 8);
    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).unwrap();
    assert_eq!(rest, b" payload");
    assert!(reader.is_verified());
    assert_eq!(reader.read(&mut first).unwrap(), 0);

    let mut empty = file.member_reader("empty").unwrap();
    assert!(!empty.is_verified());
    assert_eq!(empty.read(&mut first).unwrap(), 0);
    assert!(empty.is_verified());

    let shrunk = OpenOptions::new().write(true).open(&path).unwrap();
    shrunk.set_len(10).unwrap();
    assert!(matches!(
        file.read_member("a.bin", N4aMemoryBudget::default()),
        Err(N4aError::Integrity(_))
    ));
}

#[test]
fn verified_into_owned_reuses_metadata_and_shared_handles_stay_usable() {
    let bytes = sample(&[("a.bin", b"first payload")]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owned.n4a");
    std::fs::write(&path, &bytes).unwrap();
    let file = open_n4a_file(&path, N4aMemoryBudget::default()).unwrap();
    let shared = file.clone();
    let reader = shared.member_reader("a.bin").unwrap();
    let owned = file.into_owned(N4aMemoryBudget::default()).unwrap();
    assert_eq!(owned.archive_bytes(), bytes);
    assert_eq!(owned.member("a.bin").unwrap().bytes, b"first payload");
    assert_eq!(owned.receipt(), shared.receipt());
    assert_eq!(owned.manifest_bytes(), shared.manifest_bytes());
    assert_eq!(reader.byte_length(), 13);
    let mut out = Vec::new();
    shared.copy_archive_to(&mut out).unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn byte_writer_stream_writer_and_host_driven_plan_emit_identical_bytes() {
    let parts: [(&str, &[u8]); 3] = [
        ("z.bin", b"last by path, first as supplied"),
        ("a.bin", b"first payload"),
        ("empty", b""),
    ];
    let expected = sample(&parts);
    let paths = parts
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect::<Vec<_>>();

    let sources = parts.map(|(path, bytes)| (path, bytes));
    let mut streamed = Vec::new();
    let receipt = write_n4a_to(
        N4aStreamRequest {
            manifest: manifest(&paths),
            members: sources
                .iter()
                .map(|(path, bytes)| N4aStreamInput {
                    path,
                    source: bytes,
                })
                .collect(),
            base: None,
        },
        &mut streamed,
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert_eq!(streamed, expected);
    assert_eq!(receipt.byte_length, expected.len() as u64);

    let planned = parts
        .iter()
        .map(|(path, bytes)| N4aPlannedMember {
            path: (*path).to_owned(),
            digest: digest(bytes),
        })
        .collect();
    let mut plan =
        N4aWritePlan::new(manifest(&paths), planned, N4aMemoryBudget::default()).unwrap();
    assert_eq!(plan.byte_length(), expected.len() as u64);
    let mut driven = Vec::new();
    while let Some(step) = plan.next_step().unwrap() {
        match step {
            N4aWriteStep::Frame(frame) => driven.extend_from_slice(&frame),
            N4aWriteStep::Member { path, byte_length } => {
                let (_, bytes) = parts.iter().find(|(name, _)| *name == path).unwrap();
                assert_eq!(byte_length, bytes.len() as u64);
                for chunk in bytes.chunks(4) {
                    plan.update_member(chunk).unwrap();
                    driven.extend_from_slice(chunk);
                }
            }
        }
    }
    assert_eq!(plan.finish().unwrap(), receipt);
    assert_eq!(driven, expected);
}

#[test]
fn host_driven_plan_refuses_changed_and_mismatched_members() {
    let paths = names(&["a.bin"]);
    let plan_for = || {
        let planned = vec![N4aPlannedMember {
            path: "a.bin".into(),
            digest: digest(b"payload"),
        }];
        let mut plan =
            N4aWritePlan::new(manifest(&paths), planned, N4aMemoryBudget::default()).unwrap();
        while !matches!(plan.next_step().unwrap(), Some(N4aWriteStep::Member { .. })) {}
        plan
    };
    let mut changed = plan_for();
    changed.update_member(b"PAYLOAD").unwrap();
    assert!(matches!(changed.next_step(), Err(N4aError::Integrity(_))));
    let mut under = plan_for();
    under.update_member(b"pay").unwrap();
    assert!(matches!(under.next_step(), Err(N4aError::Integrity(_))));
    let mut over = plan_for();
    assert!(matches!(
        over.update_member(b"payload!"),
        Err(N4aError::Integrity(_))
    ));
    assert!(
        plan_for().finish().is_err(),
        "unfinished plan has no receipt"
    );

    let planned = vec![N4aPlannedMember {
        path: "undeclared".into(),
        digest: digest(b""),
    }];
    assert!(matches!(
        N4aWritePlan::new(manifest(&paths), planned, N4aMemoryBudget::default()),
        Err(N4aError::Manifest(_))
    ));
}

fn scan(bytes: &[u8], budget: N4aMemoryBudget) -> Result<N4aScanned, N4aError> {
    let mut scanner = N4aScanner::new(bytes.len() as u64, budget)?;
    while let Some(request) = scanner.next_read() {
        let start = request.offset as usize;
        scanner.feed(&bytes[start..start + request.length as usize])?;
    }
    scanner.finish()
}

#[test]
fn foreign_manifest_refuses_before_bulk_payload_scanning() {
    let payload = vec![0x5a; 64 * 1024];
    let mut bytes = sample(&[("payload.bin", &payload)]);
    let manifest_start = 30 + u16at(&bytes, 26) as usize + u16at(&bytes, 28) as usize;
    let manifest_end = manifest_start + u32at(&bytes, 18) as usize;
    let version = b"\"schema_version\":1";
    let at = bytes[manifest_start..manifest_end]
        .windows(version.len())
        .position(|part| part == version)
        .unwrap();
    bytes[manifest_start + at + version.len() - 1] = b'2';
    // This is now foreign content. Its stale CRC must not force a full scan
    // before format dispatch; accepted V1 archives still check every CRC/SHA.
    let central = u32at(&bytes, bytes.len() - 22 + 16) as u64;
    let mut scanner =
        N4aScanner::new(bytes.len() as u64, N4aMemoryBudget::bytes(32 * 1024)).unwrap();
    while let Some(request) = scanner.next_read() {
        let start = request.offset as usize;
        match scanner.feed(&bytes[start..start + request.length as usize]) {
            Ok(()) => {}
            Err(N4aError::UnsupportedFormat(_)) => {
                assert!(request.offset < manifest_end as u64);
                assert!(request.offset + request.length < central);
                return;
            }
            Err(error) => panic!("expected early format refusal, got {error}"),
        }
    }
    panic!("foreign manifest was scanned to completion");
}

#[test]
fn owned_budget_override_reuses_verified_bytes_and_preserves_the_file() {
    let bytes = sample(&[("a.bin", b"payload")]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("budget.n4a");
    std::fs::write(&path, &bytes).unwrap();
    let opening = N4aMemoryBudget::bytes(32 * 1024);
    let file = open_n4a_file(&path, opening).unwrap();
    let request = N4aMemoryBudget::bytes(1024 * 1024);
    let owned = file
        .clone()
        .into_owned(request)
        .unwrap()
        .with_memory_budget(request)
        .unwrap();
    assert_eq!(owned.memory_budget(), request);
    assert_eq!(owned.receipt(), file.receipt());
    assert_eq!(owned.archive_bytes(), bytes);
    assert_eq!(file.memory_budget(), opening);
    assert_eq!(file.read_member("a.bin", opening).unwrap(), b"payload");
    assert!(matches!(
        owned.with_memory_budget(N4aMemoryBudget::bytes(1)),
        Err(N4aError::Memory(_))
    ));
}

#[test]
fn external_source_scans_once_and_verifies_each_later_read_itself() {
    let bytes = sample(&[("a.bin", b"first payload"), ("b.bin", b"second payload")]);
    let opened = open_n4a_bytes(&bytes, N4aMemoryBudget::default()).unwrap();
    let scanned = scan(&bytes, N4aMemoryBudget::bytes(16 * 1024)).unwrap();
    assert_eq!(scanned.metadata.receipt(), opened.receipt());
    assert_eq!(&*scanned.manifest_bytes, opened.manifest_bytes());

    let metadata = &scanned.metadata;
    let location = metadata.member_location("b.bin").unwrap();
    let start = location.data_offset as usize;
    let read = &bytes[start..start + location.byte_length as usize];
    assert_eq!(read, b"second payload");
    metadata.verify_member("b.bin", &digest(read)).unwrap();
    assert!(matches!(
        metadata.verify_member("b.bin", &digest(b"second PAYLOAD")),
        Err(N4aError::Integrity(_))
    ));
    metadata.verify_archive(&digest(&bytes)).unwrap();
    let mut changed = bytes.clone();
    changed[start] ^= 1;
    assert!(matches!(
        metadata.verify_archive(&digest(&changed)),
        Err(N4aError::Integrity(_))
    ));
    assert_eq!(metadata.member_digest("b.bin").unwrap(), digest(read));
    assert_eq!(metadata.artifact_member_paths("opaque").unwrap().len(), 2);
    assert!(metadata.member_location("absent").is_none());
}

#[test]
fn scanner_refuses_short_reads_and_a_directory_that_changed_between_passes() {
    let bytes = sample(&[("a.bin", b"first payload")]);
    let mut scanner = N4aScanner::new(bytes.len() as u64, N4aMemoryBudget::default()).unwrap();
    let request = scanner.next_read().unwrap();
    let start = request.offset as usize;
    assert_eq!(scanner.next_read(), Some(request), "repeats until fed");
    assert!(matches!(
        scanner.feed(&bytes[start..start + 1]),
        Err(N4aError::Integrity(_))
    ));

    // Serve the directory from the original bytes, then a changed copy for the
    // contiguous pass: the two passes must describe the same bytes.
    let mut changed = bytes.clone();
    let eocd = bytes.len() - 22;
    let central = u32at(&bytes, eocd + 16) as usize;
    changed[central + 36] ^= 1;
    let mut scanner = N4aScanner::new(bytes.len() as u64, N4aMemoryBudget::default()).unwrap();
    let mut result = Ok(());
    let mut reads = 0;
    while let Some(request) = scanner.next_read() {
        let source = if reads < 2 { &bytes } else { &changed };
        reads += 1;
        let start = request.offset as usize;
        result = scanner.feed(&source[start..start + request.length as usize]);
        if result.is_err() {
            break;
        }
    }
    assert!(matches!(result, Err(N4aError::Integrity(_))));
}

#[test]
fn update_reuses_a_file_base_and_verifies_the_bytes_it_copies() {
    let base_bytes = sample(&[("keep.bin", b"retained"), ("old.bin", b"replaced")]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("base.n4a");
    std::fs::write(&path, &base_bytes).unwrap();
    let base = open_n4a_file(&path, N4aMemoryBudget::default()).unwrap();
    let update = |base: N4aStreamBase<'_>| {
        let replacement: &[u8] = b"new payload";
        let mut out = Vec::new();
        write_n4a_to(
            N4aStreamRequest {
                manifest: manifest(&names(&["keep.bin", "old.bin"])),
                members: vec![N4aStreamInput {
                    path: "old.bin",
                    source: &replacement,
                }],
                base: Some(base),
            },
            &mut out,
            N4aMemoryBudget::default(),
        )
        .map(|_| out)
    };
    let written = update(N4aStreamBase::File(&base)).unwrap();
    let reopened = open_n4a_bytes(&written, N4aMemoryBudget::default()).unwrap();
    assert_eq!(reopened.member("keep.bin").unwrap().bytes, b"retained");
    assert_eq!(reopened.member("old.bin").unwrap().bytes, b"new payload");
    let owned = open_owned_n4a_bytes(
        base_bytes.clone().into_boxed_slice(),
        N4aMemoryBudget::default(),
    )
    .unwrap();
    assert_eq!(update(N4aStreamBase::Owned(&owned)).unwrap(), written);

    let location = base.member_location("keep.bin").unwrap();
    overwrite(&path, location.data_offset, b"R");
    assert!(matches!(
        update(N4aStreamBase::File(&base)),
        Err(N4aError::Integrity(_))
    ));
}

/// A manifest with an unknown `deep` field holding `value`, as raw JSON text.
fn manifest_with_deep_field(schema_version: u32, value: &str) -> String {
    let mut manifest = manifest_value(&names(&["a.bin"]));
    manifest["schema_version"] = json!(schema_version);
    manifest["deep"] = json!("placeholder");
    serde_json::to_string(&manifest)
        .unwrap()
        .replace("\"placeholder\"", value)
}

#[test]
fn deep_manifests_keep_their_format_and_shape_refusals_without_a_depth_policy() {
    let budget = N4aMemoryBudget::default();
    let wrap = |inner: &str| format!("{}{inner}{}", "[".repeat(200), "]".repeat(200));
    let nested = wrap("0");
    assert!(matches!(
        parse_n4a_manifest(manifest_with_deep_field(1, &nested).as_bytes(), budget),
        Err(N4aError::Manifest(_))
    ));
    assert!(matches!(
        parse_n4a_manifest(manifest_with_deep_field(2, &nested).as_bytes(), budget),
        Err(N4aError::UnsupportedFormat(_))
    ));
    assert!(matches!(
        parse_n4a_manifest(nested.as_bytes(), budget),
        Err(N4aError::UnsupportedFormat(_))
    ));
    // Duplicate keys stay refused at any depth, as a physical refusal.
    let duplicate = wrap(r#"{"a":0,"\u0061":1}"#);
    assert!(matches!(
        parse_n4a_manifest(manifest_with_deep_field(1, &duplicate).as_bytes(), budget),
        Err(N4aError::Physical(_))
    ));
}

#[test]
fn small_archives_stay_zip32_with_no_extra_fields() {
    let bytes = sample(&[("a.bin", b"payload")]);
    let eocd = bytes.len() - 22;
    let central = u32at(&bytes, eocd + 16) as usize;
    assert_eq!(u16at(&bytes, eocd + 10), 2);
    assert_eq!(central + u32at(&bytes, eocd + 12) as usize, eocd);
    let mut at = central;
    for _ in 0..2 {
        assert_eq!(u16at(&bytes, at + 4), 20, "made-by");
        assert_eq!(u16at(&bytes, at + 6), 20, "version needed");
        assert_eq!(u16at(&bytes, at + 8), 0x800, "flags");
        assert_eq!(u16at(&bytes, at + 14), 0x21, "date");
        assert_eq!(u16at(&bytes, at + 30), 0, "extra");
        let local = u32at(&bytes, at + 42) as usize;
        assert_eq!(u16at(&bytes, local + 4), 20, "local version");
        assert_eq!(u16at(&bytes, local + 28), 0, "local extra");
        at += 46 + u16at(&bytes, at + 28) as usize;
    }
}
