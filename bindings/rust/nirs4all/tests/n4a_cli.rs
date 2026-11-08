//! Real `nirs4all-core-archive n4a-*` behaviour over the canonical V1 container.
//!
//! Every CLI call is a fresh process against the owner APIs. Nothing here is a
//! stub: the runtime tests need an actual Methods library and are skipped, with
//! an explicit message, only when it is not configured. Set
//! `N4A_CLI_REQUIRE_RUNTIME=1` to turn that skip into a failure.
//!
//! Runtime environment (same variables as the other native tests):
//! `N4M_LIBRARY_PATH` and `N4M_EXPECTED_LIBRARY_SHA256`.
//!
//! Qualification profile: `standard_scale` + `ridge`, two ordered targets
//! (`protein`, `moisture`). This is a profile witness, not universal support.
//!
//! `N4A_CLI_WRITE_FIXTURES=<dir>` additionally writes the shared fixture files
//! (`train.json`, `train-v2.json`, `future.json`, `future-permuted.json`,
//! `recipe.json`) used by the R and MATLAB/Octave tests.
#![allow(dead_code)]
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "fixtures/n4a_native_actions_v1/data.rs"]
mod data;

const TOLERANCE: f64 = 1e-9;

struct Runtime {
    library: PathBuf,
    sha256: String,
}

fn runtime() -> Option<Runtime> {
    let library = std::env::var_os("N4M_LIBRARY_PATH");
    let sha256 = std::env::var("N4M_EXPECTED_LIBRARY_SHA256").ok();
    match (library, sha256) {
        (Some(library), Some(sha256)) if !sha256.is_empty() => Some(Runtime {
            library: PathBuf::from(library),
            sha256,
        }),
        _ => {
            assert!(
                std::env::var("N4A_CLI_REQUIRE_RUNTIME").as_deref() != Ok("1"),
                "N4A_CLI_REQUIRE_RUNTIME=1 but N4M_LIBRARY_PATH / N4M_EXPECTED_LIBRARY_SHA256 are unset"
            );
            eprintln!(
                "SKIPPED: N4M_LIBRARY_PATH / N4M_EXPECTED_LIBRARY_SHA256 are unset; \
                 the Methods runtime assertions of this test did NOT run"
            );
            None
        }
    }
}

struct Fixtures {
    train: Value,
    train_v2: Value,
    future: Value,
    permuted: Value,
    recipe: Value,
}

fn reorder(record: &mut Value, pointer: &str, order: &[usize]) {
    let old = record.pointer(pointer).unwrap().as_array().unwrap().clone();
    *record.pointer_mut(pointer).unwrap() =
        Value::Array(order.iter().map(|&i| old[i].clone()).collect());
}

/// Same samples in another physical row order, every per-row field moved together.
fn permute(mut record: Value, order: &[usize]) -> Value {
    for pointer in [
        "/origin_ids",
        "/fold_ids",
        "/dataset/sample_ids",
        "/dataset/sources/0/sample_ids",
        "/dataset/sources/0/array/values",
        "/dataset/partitions/values",
    ] {
        reorder(&mut record, pointer, order);
    }
    record
}

fn fixtures() -> Fixtures {
    let train = data::record("train", 18, true, true);
    // Same samples and features, different targets: a retrained model must differ.
    let mut train_v2 = train.clone();
    for row in train_v2["dataset"]["y"]["values"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
    {
        for value in row.as_array_mut().unwrap() {
            *value = json!(value.as_f64().unwrap() * 1.5 + 3.0);
        }
    }
    let future = data::record("future", 5, false, false);
    let permuted = permute(future.clone(), &[3, 0, 4, 1, 2]);
    Fixtures {
        train,
        train_v2,
        future,
        permuted,
        recipe: data::recipe_value(false),
    }
}

fn write_json(directory: &Path, name: &str, value: &Value) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
    path
}

fn sha256_file(path: &Path) -> String {
    format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nirs4all-core-archive"))
        .args(args)
        .output()
        .expect("spawn nirs4all-core-archive")
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Success with a JSON `--output` file.
fn succeeds(args: &[&str], output: &Path) -> Value {
    let result = run(args);
    assert!(
        result.status.success(),
        "{} failed: {}",
        args[0],
        text(&result)
    );
    serde_json::from_slice(&std::fs::read(output).expect("output file")).unwrap()
}

/// Explicit refusal: non-zero exit, a message, and no output file.
fn refuses(args: &[&str], output: &Path) -> String {
    let result = run(args);
    assert!(
        !result.status.success(),
        "{} unexpectedly succeeded",
        args[0]
    );
    assert!(!output.exists(), "a refusal must not create its output");
    let message = text(&result);
    assert!(!message.trim().is_empty(), "a refusal must explain itself");
    message
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

struct Source {
    archive: PathBuf,
    sha256: String,
}

fn produce(directory: &Path, fixtures: &Fixtures, runtime: &Runtime, embed: bool) -> Source {
    let record = write_json(directory, "train.json", &fixtures.train);
    let recipe = write_json(directory, "recipe.json", &fixtures.recipe);
    let archive = directory.join("model.n4a");
    let output = directory.join("produce.json");
    let report = succeeds(
        &[
            "n4a-produce",
            "--input",
            s(&record),
            "--recipe",
            s(&recipe),
            "--source-id",
            "spectra",
            "--methods-library",
            s(&runtime.library),
            "--methods-library-sha256",
            &runtime.sha256,
            "--run-id",
            "run:cli:produce",
            "--container-id",
            "cli-produce",
            "--archive",
            s(&archive),
            "--embed-training-record",
            if embed { "true" } else { "false" },
            "--output",
            s(&output),
        ],
        &output,
    );
    // The reported identity is the identity of the bytes that were published.
    let sha256 = sha256_file(&archive);
    assert_eq!(report["operation"], "n4a-produce");
    assert_eq!(report["archive_sha256"], sha256.as_str());
    assert_eq!(
        report["byte_length"].as_u64().unwrap(),
        std::fs::metadata(&archive).unwrap().len()
    );
    assert_eq!(report["container_id"], "cli-produce");
    assert!(report["training_outcome"].is_object());
    Source { archive, sha256 }
}

#[allow(clippy::too_many_arguments)]
fn predict(
    directory: &Path,
    name: &str,
    source: &Source,
    runtime: &Runtime,
    entrypoint: &str,
    input: Option<&Path>,
    partition: &str,
) -> Value {
    let output = directory.join(format!("{name}.json"));
    let request = format!("req:cli:{name}");
    let run_id = format!("run:cli:{name}");
    let mut args = vec![
        "n4a-predict",
        "--archive",
        s(&source.archive),
        "--expected-archive-sha256",
        source.sha256.as_str(),
        "--entrypoint",
        entrypoint,
        "--source-id",
        "spectra",
        "--partition",
        partition,
        "--request-id",
        request.as_str(),
        "--run-id",
        run_id.as_str(),
        "--methods-library",
        s(&runtime.library),
        "--methods-library-sha256",
        runtime.sha256.as_str(),
        "--output",
        s(&output),
    ];
    if let Some(input) = input {
        args.extend(["--input", s(input)]);
    }
    succeeds(&args, &output)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn matrix(value: &Value) -> Vec<Vec<f64>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|item| item.as_f64().unwrap())
                .collect()
        })
        .collect()
}

fn row_for<'a>(prediction: &'a Value, matrix: &'a [Vec<f64>], id: &str) -> &'a [f64] {
    let index = strings(&prediction["sample_ids"])
        .iter()
        .position(|candidate| candidate == id)
        .unwrap_or_else(|| panic!("sample `{id}` missing from the prediction"));
    &matrix[index]
}

#[test]
fn write_shared_fixtures() {
    let Some(directory) = std::env::var_os("N4A_CLI_WRITE_FIXTURES") else {
        return;
    };
    let directory = PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let fixtures = fixtures();
    write_json(&directory, "train.json", &fixtures.train);
    write_json(&directory, "train-v2.json", &fixtures.train_v2);
    write_json(&directory, "future.json", &fixtures.future);
    write_json(&directory, "future-permuted.json", &fixtures.permuted);
    write_json(&directory, "recipe.json", &fixtures.recipe);
}

#[test]
fn routing_is_explicit_and_options_are_strict_without_a_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let garbage = directory.path().join("garbage.n4a");
    std::fs::write(&garbage, b"not a container").unwrap();
    let output = directory.path().join("out.json");
    // A non-container is refused by the canonical reader, with no output.
    refuses(
        &[
            "n4a-inspect",
            "--archive",
            s(&garbage),
            "--output",
            s(&output),
        ],
        &output,
    );
    // Retrain has no recipe option: the owner retrieves the stored recipe.
    let message = refuses(
        &[
            "n4a-retrain",
            "--archive",
            s(&garbage),
            "--recipe",
            "recipe.json",
            "--output",
            s(&output),
        ],
        &output,
    );
    assert!(message.contains("unsupported n4a-retrain option: --recipe"));
    // Nor a source ID: the owner takes it from the validated package.
    let message = refuses(
        &[
            "n4a-retrain",
            "--archive",
            s(&garbage),
            "--source-id",
            "spectra",
            "--output",
            s(&output),
        ],
        &output,
    );
    assert!(message.contains("unsupported n4a-retrain option: --source-id"));
    // Unknown n4a-* names are not silently routed anywhere.
    let message = refuses(&["n4a-fit", "--output", s(&output)], &output);
    assert!(message.contains("unknown archive command: n4a-fit"));
    // Legacy commands remain available and keep their own option table.
    let message = refuses(
        &["inspect", "--archive", s(&garbage), "--output", s(&output)],
        &output,
    );
    assert!(!message.contains("unknown archive command"));
}

#[test]
fn produce_inspect_and_predict_keep_physical_row_order_and_target_axes() {
    let Some(runtime) = runtime() else { return };
    let directory = tempfile::tempdir().unwrap();
    let fixtures = fixtures();
    let source = produce(directory.path(), &fixtures, &runtime, true);

    // Producing again to the same destination never replaces the archive.
    let again = directory.path().join("again.json");
    refuses(
        &[
            "n4a-produce",
            "--input",
            s(&directory.path().join("train.json")),
            "--recipe",
            s(&directory.path().join("recipe.json")),
            "--source-id",
            "spectra",
            "--methods-library",
            s(&runtime.library),
            "--methods-library-sha256",
            &runtime.sha256,
            "--run-id",
            "run:cli:produce2",
            "--container-id",
            "cli-produce2",
            "--archive",
            s(&source.archive),
            "--output",
            s(&again),
        ],
        &again,
    );
    assert_eq!(sha256_file(&source.archive), source.sha256);

    // Inspect: readable, and explicit that executability was not evaluated.
    let inspect_output = directory.path().join("inspect.json");
    let inspect = succeeds(
        &[
            "n4a-inspect",
            "--archive",
            s(&source.archive),
            "--expected-archive-sha256",
            &source.sha256,
            "--output",
            s(&inspect_output),
        ],
        &inspect_output,
    );
    assert_eq!(inspect["executability"], "not_evaluated");
    assert!(inspect.get("executable").is_none());
    let readability = &inspect["readability"];
    assert_eq!(readability["storage"], "verified");
    assert_eq!(readability["container_sha256"], source.sha256.as_str());
    assert_eq!(
        readability["manifest"]["schema"],
        "nirs4all.n4a.container.v1"
    );
    assert_eq!(readability["semantics"]["status"], "report_only");
    assert_eq!(readability["training_view"]["status"], "readable");
    // The legacy loader must not claim a canonical container.
    let legacy = directory.path().join("legacy.json");
    refuses(
        &[
            "inspect",
            "--archive",
            s(&source.archive),
            "--output",
            s(&legacy),
        ],
        &legacy,
    );

    // External prediction, original and permuted physical row order.
    let future = write_json(directory.path(), "future.json", &fixtures.future);
    let permuted = write_json(directory.path(), "future-permuted.json", &fixtures.permuted);
    let first = predict(
        directory.path(),
        "first",
        &source,
        &runtime,
        "predict-external",
        Some(&future),
        "predict",
    );
    let second = predict(
        directory.path(),
        "second",
        &source,
        &runtime,
        "predict-external",
        Some(&permuted),
        "predict",
    );
    assert_eq!(
        sha256_file(&source.archive),
        source.sha256,
        "predict must not change the archive"
    );
    for (prediction, record) in [(&first, &fixtures.future), (&second, &fixtures.permuted)] {
        assert_eq!(prediction["operation"], "n4a-predict");
        assert_eq!(prediction["input_mode"], "external");
        assert_eq!(prediction["source_id"], "spectra");
        assert_eq!(prediction["partition"], "predict");
        // Physical record order, exactly as in the file.
        assert_eq!(
            strings(&prediction["sample_ids"]),
            strings(&record["dataset"]["sample_ids"])
        );
        assert_eq!(
            strings(&prediction["target_names"]),
            data::expected_names(false)
        );
        assert_eq!(prediction["target_units"].as_array().unwrap().len(), 2);
        assert_eq!(prediction["replay"]["phase"], "PREDICT");
        assert_eq!(
            prediction["receipt"]["binding"]["operation"],
            "predict-external"
        );
        assert_eq!(prediction["receipt"]["authenticity"], "not_established");
        for row in matrix(&prediction["values"]) {
            assert_eq!(row.len(), 2);
            assert!(row.iter().all(|value| value.is_finite()));
        }
        // Independent re-alignment against the untouched replay block.
        let block = &prediction["replay"]["outputs"][0]["predictions"][0];
        let replay_rows = matrix(&block["values"]);
        let values = matrix(&prediction["values"]);
        for (index, id) in strings(&prediction["sample_ids"]).iter().enumerate() {
            let position = strings(&block["sample_ids"])
                .iter()
                .position(|candidate| candidate == id)
                .unwrap();
            assert_eq!(values[index], replay_rows[position], "row for `{id}`");
        }
    }
    // The permuted record has another owner-sorted order than physical order.
    let replay_ids = strings(&second["replay"]["outputs"][0]["predictions"][0]["sample_ids"]);
    assert_ne!(replay_ids, strings(&second["sample_ids"]));
    // Each sample keeps its prediction wherever it sits in the file.
    let first_values = matrix(&first["values"]);
    let second_values = matrix(&second["values"]);
    for id in strings(&first["sample_ids"]) {
        let a = row_for(&first, &first_values, &id);
        let b = row_for(&second, &second_values, &id);
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < TOLERANCE, "`{id}`: {x} vs {y}");
        }
    }
    // Non-vacuous: the permutation is visible positionally.
    assert!(first_values
        .iter()
        .zip(&second_values)
        .any(|(a, b)| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-6)));

    // Stored in-sample prediction: columns follow the declared target axes.
    let stored = predict(
        directory.path(),
        "stored",
        &source,
        &runtime,
        "predict",
        None,
        "train",
    );
    assert_eq!(stored["input_mode"], "stored");
    assert_eq!(
        strings(&stored["sample_ids"]),
        strings(&fixtures.train["dataset"]["sample_ids"])
    );
    let truth = matrix(&fixtures.train["dataset"]["y"]["values"]);
    let fitted = matrix(&stored["values"]);
    for column in 0..2 {
        let error = |target: usize| -> f64 {
            fitted
                .iter()
                .zip(&truth)
                .map(|(p, t)| (p[column] - t[target]).abs())
                .sum::<f64>()
                / truth.len() as f64
        };
        assert!(
            error(column) < error(1 - column),
            "target column {column} is closer to the other target: axes are swapped"
        );
    }
}

fn predict_raw(
    source: &Source,
    expected_sha: &str,
    entrypoint: &str,
    input: &Path,
    methods: (&str, &str),
    output: &Path,
) -> Output {
    run(&[
        "n4a-predict",
        "--archive",
        s(&source.archive),
        "--expected-archive-sha256",
        expected_sha,
        "--entrypoint",
        entrypoint,
        "--input",
        s(input),
        "--source-id",
        "spectra",
        "--partition",
        "predict",
        "--request-id",
        "req:cli:refused",
        "--run-id",
        "run:cli:refused",
        "--methods-library",
        methods.0,
        "--methods-library-sha256",
        methods.1,
        "--output",
        s(output),
    ])
}

#[test]
fn predict_refuses_through_the_owner_and_never_writes_output() {
    let Some(runtime) = runtime() else { return };
    let directory = tempfile::tempdir().unwrap();
    let fixtures = fixtures();
    let source = produce(directory.path(), &fixtures, &runtime, false);
    let future = write_json(directory.path(), "future.json", &fixtures.future);
    let output = directory.path().join("refused.json");
    let library = s(&runtime.library);
    let zeros = "0".repeat(64);
    let cases: [(&str, &str, &str, &str); 5] = [
        // Wrong archive identity.
        ("predict-external", &zeros, library, &runtime.sha256),
        // Wrong Methods identity.
        ("predict-external", &source.sha256, library, &zeros),
        // Missing Methods library.
        (
            "predict-external",
            &source.sha256,
            "/missing/n4a-cli/libn4m.so",
            &runtime.sha256,
        ),
        // An operation the owner does not offer for this action, and an unknown one.
        (
            "calibrate-external",
            &source.sha256,
            library,
            &runtime.sha256,
        ),
        (
            "no-such-entrypoint",
            &source.sha256,
            library,
            &runtime.sha256,
        ),
    ];
    for (entrypoint, expected, library, sha) in cases {
        let result = predict_raw(
            &source,
            expected,
            entrypoint,
            &future,
            (library, sha),
            &output,
        );
        assert!(
            !result.status.success(),
            "{entrypoint} (library {library}) must be refused"
        );
        assert!(!output.exists(), "a refusal must not create its output");
        assert!(!text(&result).trim().is_empty());
    }
    assert_eq!(sha256_file(&source.archive), source.sha256);

    // A source schema that differs from the training anchor is an owner refusal.
    let mut changed = fixtures.future.clone();
    changed["dataset"]["sources"][0]["axis_units"] = json!({"wavelength": "nm"});
    let changed = write_json(directory.path(), "changed-units.json", &changed);
    refuses(
        &[
            "n4a-predict",
            "--archive",
            s(&source.archive),
            "--expected-archive-sha256",
            &source.sha256,
            "--entrypoint",
            "predict-external",
            "--input",
            s(&changed),
            "--source-id",
            "spectra",
            "--partition",
            "predict",
            "--request-id",
            "req:cli:schema",
            "--run-id",
            "run:cli:schema",
            "--methods-library",
            s(&runtime.library),
            "--methods-library-sha256",
            &runtime.sha256,
            "--output",
            s(&output),
        ],
        &output,
    );
}

/// Retrain goes through the owner `retrain_n4a_native`: source ID and recipe
/// come from the validated stored package. With the runtime configured this
/// test requires success; owner errors fail it as they are.
#[test]
fn retrain_uses_the_owner_api_and_leaves_the_source_unchanged() {
    let Some(runtime) = runtime() else { return };
    let directory = tempfile::tempdir().unwrap();
    let fixtures = fixtures();
    let source = produce(directory.path(), &fixtures, &runtime, true);
    let train_v2 = write_json(directory.path(), "train-v2.json", &fixtures.train_v2);
    let destination = directory.path().join("retrained.n4a");
    let output = directory.path().join("retrain.json");
    let args = [
        "n4a-retrain",
        "--archive",
        s(&source.archive),
        "--expected-archive-sha256",
        &source.sha256,
        "--input",
        s(&train_v2),
        "--run-id",
        "run:cli:retrain",
        "--container-id",
        "cli-retrain",
        "--destination",
        s(&destination),
        "--embed-training-record",
        "true",
        "--methods-library",
        s(&runtime.library),
        "--methods-library-sha256",
        &runtime.sha256,
        "--output",
        s(&output),
    ];
    let result = run(&args);
    assert!(
        result.status.success(),
        "n4a-retrain failed: {}",
        text(&result)
    );
    let report: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(report["operation"], "n4a-retrain");
    assert_eq!(report["source_archive_sha256"], source.sha256.as_str());
    assert_eq!(
        sha256_file(&source.archive),
        source.sha256,
        "the source is immutable"
    );
    let fresh = Source {
        sha256: sha256_file(&destination),
        archive: destination,
    };
    assert_eq!(report["archive_sha256"], fresh.sha256.as_str());
    assert_ne!(fresh.sha256, source.sha256);
    assert_eq!(report["container_id"], "cli-retrain");
    assert!(report.get("source_id").is_none());
    assert!(report["training_outcome"].is_object());

    // The new container is predictable and reflects the new targets.
    let future = write_json(directory.path(), "future.json", &fixtures.future);
    let before = predict(
        directory.path(),
        "before",
        &source,
        &runtime,
        "predict-external",
        Some(&future),
        "predict",
    );
    let after = predict(
        directory.path(),
        "after",
        &fresh,
        &runtime,
        "predict-external",
        Some(&future),
        "predict",
    );
    assert_eq!(before["sample_ids"], after["sample_ids"]);
    assert_eq!(before["target_names"], after["target_names"]);
    let (a, b) = (matrix(&before["values"]), matrix(&after["values"]));
    assert!(a
        .iter()
        .zip(&b)
        .any(|(x, y)| x.iter().zip(y).any(|(p, q)| (p - q).abs() > 1e-6)));

    // A wrong source identity is refused before any output exists.
    let refused = directory.path().join("refused.n4a");
    let refused_output = directory.path().join("refused.json");
    refuses(
        &[
            "n4a-retrain",
            "--archive",
            s(&source.archive),
            "--expected-archive-sha256",
            &"0".repeat(64),
            "--input",
            s(&train_v2),
            "--run-id",
            "run:cli:retrain-refused",
            "--container-id",
            "cli-retrain-refused",
            "--destination",
            s(&refused),
            "--methods-library",
            s(&runtime.library),
            "--methods-library-sha256",
            &runtime.sha256,
            "--output",
            s(&refused_output),
        ],
        &refused_output,
    );
    assert!(!refused.exists());
}
