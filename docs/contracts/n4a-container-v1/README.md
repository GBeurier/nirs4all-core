# Canonical n4a V1 transport

The Rust `nirs4all::n4a` module stores opaque owner bytes in one canonical
`zip-stored.v1` container. The exact manifest identity is
`nirs4all.n4a.container.v1`, with integer schema and minimum-reader versions 1.
The [schema](../n4a-container.v1.schema.json) rejects unknown structural fields.
All structural fields are required, including nullable `requirements` on
artifacts and entrypoints. Only reference `root_sha256` may be absent; when
present it must be a raw lowercase SHA-256 string, never null.

All structural integer fields require **unsigned decimal JSON integer tokens**:
`0` or digits beginning with `1` through `9`, with no sign, fraction or exponent.
For example, `26.0`, `9.0`, `17.0`, `1.0` and `1e0` are refused even when their
mathematical values are integral. Existing u32 version ranges, u64 member-length
ranges and supported-version values also apply.
These lexical rules concern the manifest, not opaque owner payload contents.
The Rust reader is authoritative. JSON Schema `integer` and `const` intentionally
accept forms such as `1.0` and `1e0` by mathematical value; JSON Schema cannot
express these token restrictions or fully validate stored-container bytes.


## Storage and identity

`write_n4a_bytes` accepts a typed manifest and borrowed member inputs, and
`write_n4a_owned` returns the same result as a handle without reparsing it. The
member paths must match the declaration exactly. Core replaces declared raw
sizes and hashes with values derived from the original bytes. It never drops
an undeclared input, an artifact or a link. At least one embedded artifact and
one payload member are required. Every physical payload has exactly one
embedded artifact owner; the embedded root is among its declared members.
References have no local members. Reuse an artifact ID for shared semantic use.
Artifact, link and entrypoint IDs are unique within their respective lists.
All artifact endpoints and requirement references must exist.

Every artifact, embedded or reference, carries its complete owner identity.
`identity.owner` must equal `artifact.owner`. The identity schema/version are
independent of the content type ID/version. Core checks nonempty typed shapes
and positive u32 versions; the owner port checks identity domains, algorithms,
fingerprints and scientific meaning. Raw size/SHA and semantic fingerprints
are separate values. Link relations are opaque nonempty IDs in Core. DAG owns
its closed scientific vocabulary and endpoint semantics. Entrypoint closure
checks declared artifact IDs only; Core does not infer allowed root kinds.

`open_n4a_bytes` verifies all headers, CRCs, inventory sizes and raw SHA-256s.
It returns the full parsed manifest, raw borrowed manifest, raw container and
manifest digests, inventory and borrowed payload ranges. Unknown owner content
can be inspected and copied. Inspection reports verified storage and unverified
semantics, regardless of producer owner strings. Core calls no owner serializer,
JSON payload parser, model loader, callback, fitting or numerical code.
Payload binary, uint64/decimal JSON tokens and Unicode text retain their exact
bytes. Old JSON or ZIP container manifests and future versions are refused;
there is no historical reader dispatch in this API.

`open_owned_n4a_bytes` uses the same validation path and retains the supplied
`Box<[u8]>` allocation in `OwnedN4a`. Its immutable views borrow that handle;
the borrowed API remains available through `ValidatedN4a`. Each form retains
one validated index per handle and shares read operations. `archive_bytes()`
returns the exact original bytes of an opened handle.

The owner-neutral exchange adds, without changing the codec:
`parse_n4a_manifest` (duplicate-key scan, format probe, strict typed decoding and
structural validation of manifest bytes; declared `byte_length` and `sha256` are
only shape-checked, so placeholders are accepted), `write_n4a_update` (a new
container from an immutable base: each declared path takes its bytes from the
replacements, else from the base; omission deletes; an undeclared or duplicate
replacement is refused; it delegates to `write_n4a_bytes`) and the generic
`n4a_owner_portable::n4a_container_view`, whose report states `readability` and
leaves `executability` as `not_evaluated`. Python exposes `open_n4a_container`
and `write_n4a_container`, and the CLI exposes `n4a-write` and `n4a-read`, as
thin byte transports over these functions. Readable is not executable: no
host payload is interpreted, deserialized or admitted by this exchange, and an
unknown owner's artifact stays `opaque`.

`copy_n4a_bytes` allocates one output buffer containing the exact complete
validated input, including unknown artifacts and original manifest bytes.
There is no in-place rewrite. Constructing or updating a container is a separate
writer action that always yields new bytes and a new digest, never touches its
base, and does not promise the original manifest serialization or ZIP order.
`copy_n4a_bytes` or `archive_bytes()` is the only route that keeps them.

## Physical profile and budgets

The profile has **no intrinsic cap**: no limit on entries, member size, total
size, manifest size, JSON nesting or path length beyond the ZIP format itself.
Only these refuse:

- the operation's declared working-memory budget, or allocator failure;
- real representation limits: a ZIP name of 65,535 UTF-8 bytes, u64 sizes,
  offsets and counts with checked arithmetic, and the host address space for
  anything held in memory;
- every path, layout, integrity and corruption check below.

`N4aMemoryBudget` is optional. `None`, the default, adds no Core ceiling: only
what the allocator and host grant applies. That is not a statement about machine
RAM, and on an overcommitting system an allocation can succeed and the process
can be killed later, so operators who need a hard guarantee declare bytes or use
cgroups/ulimit. `Some(n)` authorizes `n` working bytes. It covers Core's own
allocations (central index, retained hashes, manifest, copy buffer, materialized
outputs), never on-disk payload size. Allocations made inside IO, DAG, Methods or
serde are outside it. Streaming buffers are `clamp(n / 4, 4096, 1 MiB)` bytes, or
1 MiB without a budget; this sizes a buffer and limits no content. The budget's
metadata cost is an estimate proportional to the manifest, not an exact count.

A structural guard is not a cap: a declared directory must fit its own bytes
(`count * 46 <= central_size`, and the central region must lie inside the file),
so a hostile header cannot reserve memory the file does not back.

Files are regular and stored, with no compression, encryption, descriptors,
symlinks, executable mode bits or multi-disk data. Extra fields carry ZIP64
values only. Writer flags are `0x800` for every member. Reader flags may be 0 or `0x800`, with
`0x800` mandatory for non-ASCII names; local and central metadata must match.
The canonical writer emits the deterministic DOS timestamp
`1980-01-01 00:00:00` (time `0`, date `0x0021`) in every local and central header,
including the manifest. Timestamp selection is independent of UTF-8 flags.
Canonical readers require this exact time/date on every local and central
header, in addition to checking their agreement. Interim wrappers retain their
historical zero time/date fields for byte parity until atomic removal.
The first local header starts at 0; all local records are contiguous; the
central directory immediately follows the final payload; EOCD ends the file.
Prefixes, gaps, comments and trailers are refused. ZIP version-needed is 10
or 20, or 45 on an entry that carries ZIP64 values.

`zip-stored.v1` is one canonical encoding per content, ZIP32 unless a value
requires ZIP64, following PKWARE APPNOTE 6.3.10 sections 4.3.14-4.3.16 and 4.5.3.
A value needs ZIP64 when it is at or above the classic sentinel: `0xFFFFFFFF`
for sizes and offsets, `0xFFFF` for the entry count. A value equal to the
sentinel uses ZIP64. Below every threshold, output is byte-identical to the
original ZIP32 form.

- A local header has a ZIP64 extra (ID `0x0001`, size then compressed size, 20
  bytes) only when the member size needs it. Its 32-bit fields hold the sentinel.
- A central record's extra holds only the fields that overflow, in APPNOTE order:
  size, compressed size, local offset.
- Version-needed is 45 in the local and central headers of any entry with a
  ZIP64 value, and the writer's made-by matches it.
- The file ends `central | EOCD64 (56 bytes, record size 44) | locator (20 bytes) |
  EOCD (22 bytes)` when the count, central size or central offset needs ZIP64.
  Each classic EOCD field holds `min(actual, sentinel)`.

ZIP64 must appear exactly where required. Readers refuse it where a value fits,
along with extensible EOCD64 data, nonzero disk fields, a locator that does not
name the record directly before it, and any disagreement between classic and
64-bit fields.

The reader tolerates a bounded superset of the writer's header constants:
version-needed 10 or 20, flag 0 for ASCII names, differing version-made-by and
internal attributes, and allowed non-executable regular-file permissions and
low external attributes (excluding directory entries). Member order may differ,
including a manifest after payloads. The writer emits version-needed 20, UTF-8
flags on all names, made-by value 20, zero internal/external attributes, and
manifest-first order. These tolerated variations do not relax strict timestamps,
UTF-8 requirements for non-ASCII names, contiguous layout, regular-file
restrictions or integrity checks.

Container identity is SHA-256 of the exact complete raw container bytes.
Different accepted headers or member order yield different container identities,
even with identical raw payload bytes. This is not semantic dataset/model
identity; owners define those identities independently. Exact copying preserves
the original container encoding and raw SHA.

A canonical index is built and the manifest's duplicate keys are refused by an
iterative scanner, before strict typed decoding. No JSON depth policy exists: the
scanner is not recursive, and the typed schema is closed, so an unknown field
refuses before its value is decoded. The four container discriminator fields
(schema, schema version, minimum reader version, physical profile) are probed
first, but only after canonical physical and CRC guards have passed.
`UnsupportedFormat` is reachable only at that stage: a stored legacy-shaped ZIP
with a valid physical shape can return it, while compressed legacy ZIPs
and raw JSON return `Physical` before their claimed format is considered. Core
does not inflate a compressed archive to identify it. Once the probe is reached,
unsupported old/future headers return `UnsupportedFormat` even when they carry
extra fields or historical zero dates. Hosts must not route migration solely by
error category. Refusal never activates a legacy decoder or migration fallback.
Only timestamp admission is deferred
for this refusal ordering; canonical path/UTF-8/layout/header-consistency and CRC
checks stay in force. A current V1 header must then pass strict epoch
timestamp admission and the full typed closed manifest contract. Unknown fields
under a supported header return `Manifest`. There is no historical decoder,
dispatch, permissive profile fallback or scientific payload interpretation.

Paths are NFC, relative, case-sensitive UTF-8 identifiers. Empty, dot and
dot-dot segments, slash-absolute paths, drive prefixes in any segment,
backslashes and Unicode control characters are refused. The same guard is
used for writing and reading. Paths are never used as OS paths: `n4a-read`
exports numbered files (`member-000000.bin`) and a report that maps each archive
path to its file, next to the exact `manifest.json`. JSON
Schema describes path syntax; the transport additionally checks NFC, the ZIP
name length, closure, sums and exactly-one ownership.

## Handles and streaming

Every host runs one parser and one writer, `n4a_physical`. It performs no I/O:
`N4aScanner` asks for byte ranges and `N4aWritePlan` emits framing, so a
synchronous file driver and an asynchronous JavaScript source use the same code.
The scanner reads the tail, the central directory, and then one contiguous pass
that checks local headers and computes CRC, member SHA and container SHA together.
Every handle shares the resulting `N4aMetadata`.

Borrowed opening uses the caller's immutable buffer; owned opening retains
the original Box after the same pass. Byte handles are immutable, so they validate
once and reuse their metadata with no per-member re-hash. They do not copy
payloads into a map. Safe views expose immutable data only. Member lookup is
`O(log n)`. The byte writer is the same engine over one pre-reserved output
buffer; its peak is the input payloads, that buffer and metadata.

`open_n4a_file` keeps only metadata in memory, so data larger than RAM stays on
disk. The file is neither mapped nor locked, and another process can change it.
The guarantee is therefore per completed read, on the bytes that read actually
returned: `member_reader`, `read_member`, `copy_member_to`, `copy_archive_to` and
`into_owned` each hash during their one read and compare to the validated receipt,
with no second scan or reopening. A changed file refuses instead of being trusted
from the earlier validation. `N4aMemberReader` delivers unverified bytes until its
end; the read that would deliver the last byte verifies first and on a mismatch
withholds those bytes, so delivering the last byte means the whole sequence
matched. A reader abandoned early has verified nothing. On `Err`, the `copy_*`
routes have already written unverified bytes and the caller must discard them.

`write_n4a_to` and `write_n4a_file` stream known sources. The manifest comes first
and contains member hashes, so a source is read twice: once to digest it, then
again to write and compare. A source that changed between passes refuses and
nothing is published. Retained base members reuse their validated inventory and
are verified as they are copied from a file base. Output is hashed as it is
written and never re-read. `write_n4a_file` stages beside the destination and
publishes with an exclusive hard link, so complete bytes appear atomically and an
existing path is never replaced.

Hosts with their own byte source, such as a browser `File`, drive the scanner once
and keep the metadata beside that source. They never reopen or reparse the archive:
each later read hashes its bytes with `N4aMemberHasher` and checks them with
`N4aMetadata::verify_member` or `verify_archive`.

## Live admission and separate owner ports

Every requested operation requires a live caller-supplied `OwnerAdmission`.
`operation_binding` prepares a request; it grants no admission. The binding
includes exact container SHA, entrypoint ID, owner, operation, root/input
artifact IDs, artifact owners and content types. `require_operation` refuses
absent, refused, unverified or mismatched admissions before the caller can
proceed. The live admission must support manifest-level required features and
those of the selected entrypoint. Unknown global features block every action;
unknown selected features block that action. An unrelated entrypoint cannot
disable an independently owner-admitted action. Inspection still reports the
complete union of manifest and all entrypoint features. Full storage inspection,
extraction and exact copying remain available. Order of input IDs is part of
the binding. Artifact requirement semantics remain the live owner port's
responsibility.

Admissions are caller assertions, not authentication, signatures or proof
that user code is trustworthy. They have no archive/serde representation and
are never read from producer metadata. A caller must invoke the appropriate
live owner port for the current operation and validate requirements there;
reusing an assertion does not create a freshness or authenticity guarantee.
Core detects changes to the bound bytes/entrypoint/context, not elapsed time.
The returned `AdmittedOperation` exposes the bound request and executes nothing.

IO, DMD, DAG and Methods remain separate semantic ports. They validate payload
meaning, reference resolution, runtime/trust capabilities and scientific
links. Core does not decode scientific JSON/native state, compile DAG objects,
compute residuals or metrics, reinterpret coefficients, or add identity
algorithms. Storage-only synthetic hostile-byte tests have no executable-model
claim. Owner fixture qualification is tracked separately with actual producer
and runtime identities; unavailable owners remain unverified.

Historical wrappers retain their 256-entry, flag-0 behavior and inherited
on-disk guard in this batch. Canonical host exports, product cutover and old
reader deletion belong to later reviewed work.

## Compile and qualification evidence

The tracked WASM-native crate unconditionally includes the same `n4a.rs` Rust
module and shared neutral codec. A `wasm32-unknown-unknown` target check therefore
compiles canonical transport on a 32-bit target. This is Rust compile wiring;
it adds no wasm-bindgen or JavaScript canonical host exports by itself. Earlier target
checks covered only the shared codec and old V2 bridge and did not compile
`n4a.rs`; their retained receipts must be read with that scope.

The retained Opus adversarial evidence separately records actual Rust 1.96
WASI 32-bit execution. That runtime proof is distinct from the tracked
`wasm32-unknown-unknown` compile and from the native scientific qualification described below.
The Node 22 WASI 128 MiB benchmark host segfault is a host benchmark limitation;
Node 24 completed it. It is not an observed transport correctness failure.
No large benchmark rerun is claimed. Current frozen-source joint scientific
execution passed in immutable workspace receipt
`_implementation/2026-11-03-ecosystem-consolidation/phase2a-c-qualification/runs/final-cohort-03/receipt.json`;
final joint Core/DAG adversarial review remains pending. The published corpus
pins exact producer container lengths/SHAs and original owner raw bytes; its
provenance points to the actual Rust 1.88 producer and fresh OS-process replay.
Run02 uses the frozen DAG D1/D2/D3 guards and validates every known embedded DAG
facet in both producer and reader; all six complete containers and owner raw
members were separately compared byte-identical to preserved Run01 output.
Actual Cargo patches select local Core 0.4.7 and DAG `dag-ml`/`dag-ml-core`/
`dag-ml-results` 0.3.44 only. IO 0.2.14, DMD 0.2.16, Formats 0.2.14 and n4m Rust
0.4.1 resolve to locked registry packages. The selected native C++ Methods
library is ABI 2.20, SHA-256
`05ae1a2e82491439f0e71ada0ea500ed79d03af227a11c4a2dbb317a99558529`.
The run's resolved owner source inventory and archived inputs record actual
linked sources; local sibling HEADs alone do not establish compiled provenance.

Replay is in-sample native state roundtrip with targets absent, not independent
test/generalization evidence. Dataset byte identity and unverified
`dataset_of`/`evaluates` remain explicit. The six Python stdlib ZIP/schema/date
checks do not establish canonical Python/WASM/R/MATLAB host-facade execution,
SDK/Python full pipeline or full cross-language pipeline parity. Those host
facades remain unsupported until B; compilation and scratch WASI transport
execution are distinct from the actual native replay qualification.
