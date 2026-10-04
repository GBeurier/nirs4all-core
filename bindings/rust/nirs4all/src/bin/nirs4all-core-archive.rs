//! Native archive transport for hosts that do not embed the Rust aggregate.
//!
//! Core validates the container and passes the original package bytes to DAG-ML.
//! DAG-ML owns package validation, process callbacks, hydration and replay. This
//! binary never builds a graph, fits a model or interprets numerical state.

use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitCode};

use nirs4all::{
    archive_v2_view, archive_v3_view, archive_view, load_archive,
    preflight_methods_archive_v2_library, replay_methods_archive_v2_json,
    replay_methods_archive_v3_json, LoadedArchive, MethodsArchiveReplayJsonRequest,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// Core's existing physical transport bounds. This binary does not parse ZIP
// structures: these limits only bound copying the already-validated container
// and caller JSON into owning native entry points.
const MAX_JSON_TRANSPORT_BYTES: u64 = 134_217_728;
const MAX_ARCHIVE_TRANSPORT_BYTES: u64 = 536_870_912 + 256 * (4096 + 76) + 22;

fn options(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(String, BTreeMap<String, OsString>)> {
    let mut args = args.into_iter();
    let command = args
        .next()
        .ok_or("expected inspect, read-member, replay-process or replay-methods")?;
    let command = command.into_string().map_err(|_| "command must be UTF-8")?;
    if !matches!(
        command.as_str(),
        "inspect" | "read-member" | "replay-process" | "replay-methods"
    ) {
        return Err(format!("unknown archive command: {command}").into());
    }
    let allowed: &[&str] = match command.as_str() {
        "inspect" => &["archive", "output"],
        "read-member" => &["archive", "member", "output", "expected-archive-sha256"],
        "replay-methods" => &[
            "archive",
            "expected-archive-sha256",
            "request",
            "envelopes",
            "methods-inputs",
            "methods-library",
            "methods-library-sha256",
            "output",
            "outcome-id",
            "run-id",
        ],
        _ => &[
            "archive",
            "expected-archive-sha256",
            "dag-cli",
            "request",
            "envelopes",
            "trusted-controllers",
            "adapter",
            "output",
            "outcome-id",
            "run-id",
            "process-timeout-ms",
        ],
    };
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        let key = key
            .into_string()
            .map_err(|_| "option names must be UTF-8")?;
        let key = key.strip_prefix("--").ok_or("expected a named --option")?;
        if !allowed.contains(&key) {
            return Err(format!("unsupported {command} option: --{key}").into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for --{key}"))?;
        if value.is_empty() || values.insert(key.to_owned(), value).is_some() {
            return Err(format!("empty or duplicate --{key}").into());
        }
    }
    Ok((command, values))
}

fn required<'a>(values: &'a BTreeMap<String, OsString>, key: &str) -> Result<&'a OsString> {
    values
        .get(key)
        .ok_or_else(|| format!("missing --{key}").into())
}

fn validated_view(archive: &LoadedArchive) -> Result<Value> {
    Ok(match archive {
        LoadedArchive::V1(archive) => serde_json::to_value(archive_view(archive)?)?,
        LoadedArchive::V2(archive) => serde_json::to_value(archive_v2_view(archive)?)?,
        LoadedArchive::V3(archive) => serde_json::to_value(archive_v3_view(archive)?)?,
    })
}

fn ensure_archive_identity(view: &Value, expected: &OsString) -> Result<()> {
    let expected = expected.to_str().ok_or("archive SHA must be UTF-8")?;
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
        || view.get("archive_sha256").and_then(Value::as_str) != Some(expected)
    {
        return Err("archive bytes changed or expected archive SHA is invalid".into());
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(bytes)?;
    output.flush()?;
    Ok(())
}

fn read_json_transport(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_JSON_TRANSPORT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_JSON_TRANSPORT_BYTES {
        return Err("native JSON transport exceeds the Core member budget".into());
    }
    Ok(String::from_utf8(bytes)?)
}

fn replay_methods(
    archive: &LoadedArchive,
    view: &Value,
    values: &BTreeMap<String, OsString>,
) -> Result<()> {
    if matches!(archive, LoadedArchive::V1(_)) {
        return Err("native Methods replay requires Core Archive V2 or V3".into());
    }
    // The public JSON replay API reopens a path. Pin that path to the original
    // validated bytes in a private temporary directory, so a concurrently
    // replaced user file cannot bypass the archive identity anchor.
    let temporary = tempfile::tempdir()?;
    let snapshot_path = temporary.path().join("original.n4a");
    let mut source = File::open(Path::new(required(values, "archive")?))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&snapshot_path)?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_ARCHIVE_TRANSPORT_BYTES {
            return Err("archive transport exceeds Core physical budget".into());
        }
        hasher.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    output.flush()?;
    drop(output);
    let snapshot_sha = format!("{:x}", hasher.finalize());
    if view.get("archive_sha256").and_then(Value::as_str) != Some(snapshot_sha.as_str()) {
        return Err("archive bytes changed before native Methods replay".into());
    }
    let library = Path::new(required(values, "methods-library")?).to_path_buf();
    let library_sha = required(values, "methods-library-sha256")?
        .to_str()
        .ok_or("Methods library SHA must be UTF-8")?;
    // This existing native preflight pins an exact private library snapshot and
    // checks the Methods ABI. No model state or fit is created by this bridge.
    preflight_methods_archive_v2_library(&library, library_sha)?;
    let input = MethodsArchiveReplayJsonRequest {
        request_json: read_json_transport(Path::new(required(values, "request")?))?,
        data_envelopes_json: read_json_transport(Path::new(required(values, "envelopes")?))?,
        methods_inputs_json: read_json_transport(Path::new(required(values, "methods-inputs")?))?,
        methods_library_path: library,
        outcome_id: required(values, "outcome-id")?
            .to_str()
            .ok_or("outcome ID must be UTF-8")?
            .to_owned(),
        run_id: required(values, "run-id")?
            .to_str()
            .ok_or("run ID must be UTF-8")?
            .to_owned(),
        warnings_json: "[]".to_owned(),
        diagnostics_json: "{}".to_owned(),
    };
    let outcome = match archive {
        LoadedArchive::V2(_) => replay_methods_archive_v2_json(&snapshot_path, input)?,
        LoadedArchive::V3(_) => replay_methods_archive_v3_json(&snapshot_path, input)?,
        LoadedArchive::V1(_) => unreachable!("V1 refused before opening any runtime"),
    };
    write_new(Path::new(required(values, "output")?), outcome.as_bytes())
}

fn execute(command: &str, values: &BTreeMap<String, OsString>) -> Result<()> {
    // Container dispatch, size limits, closure and raw hashes are validated by
    // the owning Core loader before any host process or member output exists.
    let archive = load_archive(Path::new(required(values, "archive")?))?;
    let view = validated_view(&archive)?;
    if command == "inspect" {
        let manifest = match &archive {
            LoadedArchive::V1(archive) => archive.manifest(),
            LoadedArchive::V2(archive) => archive.manifest(),
            LoadedArchive::V3(archive) => archive.manifest(),
        };
        let output = serde_json::to_vec_pretty(&json!({
            "schema_version": 1, "archive": view, "manifest": manifest,
        }))?;
        if let Some(path) = values.get("output") {
            write_new(Path::new(path), &output)?;
        } else {
            std::io::stdout().write_all(&output)?;
        }
        return Ok(());
    }
    ensure_archive_identity(&view, required(values, "expected-archive-sha256")?)?;
    if command == "replay-methods" {
        return replay_methods(&archive, &view, values);
    }
    if command == "read-member" {
        let member = required(values, "member")?
            .to_str()
            .ok_or("member path must be UTF-8")?;
        let bytes = match &archive {
            LoadedArchive::V1(_) => return Err("member transport requires Archive V2 or V3".into()),
            LoadedArchive::V2(archive) => archive.member(member)?,
            LoadedArchive::V3(archive) => archive.member(member)?,
        };
        return write_new(Path::new(required(values, "output")?), bytes);
    }
    let (native_command, package) = match &archive {
        LoadedArchive::V1(_) => {
            return Err("native process replay requires Archive V2 or V3".into())
        }
        LoadedArchive::V2(archive) => (
            "run-process-loaded-predictor-replay",
            archive.portable_predictor_package()?,
        ),
        LoadedArchive::V3(archive) => (
            "run-process-loaded-refit-replay",
            archive.portable_refit_package()?,
        ),
    };
    // These are transport files, not an alternate persisted package: preserve
    // the exact signed bytes, and remove them on success and on every refusal.
    let temporary = tempfile::tempdir()?;
    let package_path = temporary.path().join("package.json");
    write_new(&package_path, package)?;
    let mut process = Command::new(required(values, "dag-cli")?);
    process
        .arg(native_command)
        .arg("--package")
        .arg(&package_path);
    for key in [
        "request",
        "envelopes",
        "trusted-controllers",
        "adapter",
        "output",
        "outcome-id",
        "run-id",
    ] {
        process.arg(format!("--{key}")).arg(required(values, key)?);
    }
    if let Some(timeout) = values.get("process-timeout-ms") {
        process.arg("--process-timeout-ms").arg(timeout);
    }
    let status = process.status()?;
    if !status.success() {
        return Err(format!("native DAG-ML replay refused: {status}").into());
    }
    Ok(())
}

fn main() -> ExitCode {
    match options(std::env::args_os().skip(1))
        .and_then(|(command, values)| execute(&command, &values))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Core archive refusal: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_transport_options_do_not_accept_fit_or_arbitrary_passthrough() {
        for args in [
            vec!["replay-process", "--phase", "REFIT"],
            vec!["replay-methods", "--adapter", "borrowed-controller"],
            vec!["replay-methods", "--phase", "FIT_CV"],
            vec!["inspect", "--archive", "a", "--archive", "b"],
            vec!["read-member", "--output"],
            vec!["train"],
        ] {
            assert!(options(args.into_iter().map(OsString::from)).is_err());
        }
    }

    #[test]
    fn archive_identity_requires_exact_lowercase_raw_sha() {
        let sha = "a".repeat(64);
        let view = json!({"archive_sha256": sha});
        assert!(ensure_archive_identity(&view, &OsString::from("a".repeat(64))).is_ok());
        for value in ["A".repeat(64), "b".repeat(64), "a".repeat(63)] {
            assert!(ensure_archive_identity(&view, &OsString::from(value)).is_err());
        }
    }

    #[test]
    fn invalid_archive_is_refused_before_any_worker_or_output() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("invalid.n4a");
        std::fs::write(&archive, b"not a Core archive").unwrap();
        let output = directory.path().join("must-not-exist.json");
        let mut args = BTreeMap::new();
        args.insert("archive".to_owned(), archive.into_os_string());
        args.insert("output".to_owned(), output.clone().into_os_string());
        args.insert("dag-cli".to_owned(), OsString::from("must-not-spawn"));
        assert!(execute("replay-process", &args).is_err());
        assert!(execute("replay-methods", &args).is_err());
        assert!(!output.exists());
    }
}
