//! Native <-> WASM canonical V1 `.n4a` exchange gate (Linux x86-64 native
//! against Node WASM, one Methods ABI/catalog).
//!
//! The fixture is the retained synthetic `data.rs` record: unsorted sample IDs,
//! targets `protein` and `moisture`, `standard_scale` + `Ridge`. This target
//! owns the native side and every assertion; `bindings/wasm/tests/n4a-exchange-driver.mjs`
//! runs the real WASM actions. A missing qualification input FAILS here, it is
//! never skipped. Nothing in this file is a numerical implementation.
//!
//! Required environment:
//! - native runtime: `N4M_LIBRARY_PATH`, `N4M_LIB_PATH`, `N4M_EXPECTED_LIBRARY_SHA256`
//! - WASM runtime from the SAME Methods revision: `NIRS4ALL_METHODS_JS`,
//!   `NIRS4ALL_DAG_JS`, `NIRS4ALL_ESTIMATOR_ADAPTER_JS`; optional `NIRS4ALL_NODE`
//! - `bindings/wasm/native` built from this source (`npm run build:native`).
use nirs4all::n4a::*;
use nirs4all::n4a_owner_native::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[path = "fixtures/n4a_native_actions_v1/support.rs"]
mod support;
use support::*;

/// Same-engine determinism and cross-engine replay of one frozen model.
const PREDICT_TOLERANCE: f64 = 1e-12;
/// Proposed refit tolerance; set once from the Methods tolerances, never widened to pass.
const RETRAIN_TOLERANCE: f64 = 1e-9;
const SOURCE: &str = "spectra";
const RUN_ID: &str = "run:exchange";
const CONTAINER_ID: &str = "container:exchange";
const NATIVE_ENV: [&str; 3] = [
    "N4M_LIBRARY_PATH",
    "N4M_LIB_PATH",
    "N4M_EXPECTED_LIBRARY_SHA256",
];
const WASM_ENV: [&str; 3] = [
    "NIRS4ALL_METHODS_JS",
    "NIRS4ALL_DAG_JS",
    "NIRS4ALL_ESTIMATOR_ADAPTER_JS",
];

struct Qualification {
    node: String,
    wasm_dir: PathBuf,
    library: PathBuf,
    sha: String,
}

/// The explicit gate: every test starts here and fails, with the complete list
/// of missing inputs, when the qualification environment is incomplete.
fn qualification() -> Qualification {
    let wasm_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wasm");
    let mut missing: Vec<String> = NATIVE_ENV
        .iter()
        .chain(WASM_ENV.iter())
        .filter(|name| std::env::var_os(name).is_none())
        .map(|name| (*name).to_owned())
        .collect();
    for needed in [
        "native/nirs4all_core_wasm_native.js",
        "native/nirs4all_core_wasm_native_bg.wasm",
        "tests/n4a-exchange-driver.mjs",
    ] {
        if !wasm_dir.join(needed).is_file() {
            missing.push(format!("bindings/wasm/{needed}"));
        }
    }
    assert!(
        missing.is_empty(),
        "n4a native/WASM exchange qualification is incomplete; this gate never skips. Missing: {}",
        missing.join(", ")
    );
    Qualification {
        node: std::env::var("NIRS4ALL_NODE").unwrap_or_else(|_| "node".into()),
        wasm_dir,
        library: library(),
        sha: std::env::var("N4M_EXPECTED_LIBRARY_SHA256").unwrap(),
    }
}

fn node(q: &Qualification, dir: &Path, name: &str, mut job: Value) -> Value {
    let out = dir.join(format!("{name}.result.json"));
    job["out"] = json!(out);
    let job_path = dir.join(format!("{name}.job.json"));
    std::fs::write(&job_path, serde_json::to_vec(&job).unwrap()).unwrap();
    let run = Command::new(&q.node)
        .current_dir(&q.wasm_dir)
        .arg(q.wasm_dir.join("tests/n4a-exchange-driver.mjs"))
        .arg(&job_path)
        .output()
        .expect("start the Node exchange driver");
    assert!(
        run.status.success(),
        "exchange driver '{name}' failed ({}):\n{}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    serde_json::from_slice(&std::fs::read(out).unwrap()).unwrap()
}

fn recipe() -> nirs4all::pipeline_workflow::NativePipelineRecipe {
    serde_json::from_value(recipe_value(false)).unwrap()
}

fn open(bytes: Vec<u8>) -> OwnedN4a {
    open_owned_n4a_bytes(bytes.into_boxed_slice(), N4aMemoryBudget::default()).unwrap()
}

fn runtime(q: &Qualification) -> NativeRuntime<'_> {
    NativeRuntime {
        library: &q.library,
        expected_sha256: &q.sha,
    }
}

fn permute_rows(record: &Value, order: &[usize], partitions: Option<&[&str]>) -> Value {
    let pick = |value: &Value| Value::Array(order.iter().map(|&row| value[row].clone()).collect());
    let mut out = record.clone();
    out["origin_ids"] = pick(&record["origin_ids"]);
    out["fold_ids"] = pick(&record["fold_ids"]);
    out["dataset"]["sample_ids"] = pick(&record["dataset"]["sample_ids"]);
    let source = &record["dataset"]["sources"][0];
    out["dataset"]["sources"][0]["sample_ids"] = pick(&source["sample_ids"]);
    out["dataset"]["sources"][0]["array"]["values"] = pick(&source["array"]["values"]);
    if !record["dataset"]["y"].is_null() {
        out["dataset"]["y"]["values"] = pick(&record["dataset"]["y"]["values"]);
    }
    out["dataset"]["partitions"]["values"] = match partitions {
        Some(labels) => json!(labels),
        None => pick(&record["dataset"]["partitions"]["values"]),
    };
    out
}

fn record_ids(record: &Value) -> Vec<String> {
    serde_json::from_value(record["dataset"]["sample_ids"].clone()).unwrap()
}

/// Fixtures shared by the gate; written once, so every test sees the same bytes.
struct Fixture {
    dir: PathBuf,
    train: Vec<u8>,
    train_permuted: Vec<u8>,
    natural: Vec<u8>,
    natural_ids: Vec<String>,
    permuted: Vec<u8>,
    permuted_ids: Vec<String>,
    multi: Vec<u8>,
    multi_predict_ids: Vec<String>,
    multi_test_ids: Vec<String>,
    native: OwnedN4a,
    native_outcome: Value,
}

const MULTI_LABELS: [&str; 9] = [
    "predict", "test", "predict", "predict", "test", "predict", "test", "predict", "predict",
];

fn fixture(q: &Qualification) -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("n4a-wasm-exchange-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let train = record("train", 18, true, true);
        let train_permuted = permute_rows(
            &train,
            &[11, 3, 17, 0, 8, 14, 5, 1, 9, 16, 2, 12, 7, 15, 4, 10, 13, 6],
            None,
        );
        let natural = record("future", 5, false, false);
        let permuted = permute_rows(&natural, &[2, 0, 4, 1, 3], None);
        let multi = permute_rows(
            &record("multi", 9, false, false),
            &[4, 1, 7, 0, 8, 2, 6, 3, 5],
            Some(&MULTI_LABELS),
        );
        let multi_ids = record_ids(&multi);
        let select = |wanted: &str| -> Vec<String> {
            multi_ids
                .iter()
                .zip(MULTI_LABELS)
                .filter(|(_, label)| *label == wanted)
                .map(|(id, _)| id.clone())
                .collect()
        };
        let bytes = |value: &Value| serde_json::to_vec(value).unwrap();
        let produced = produce_n4a_native_with_view(NativeProduceRequest {
            record: &bytes(&train),
            source_id: SOURCE,
            recipe: &recipe(),
            runtime: runtime(q),
            run_id: RUN_ID,
            container_id: CONTAINER_ID,
            embed_training_record: true,
            memory_budget: N4aMemoryBudget::default(),
        })
        .unwrap();
        let fixture = Fixture {
            train: bytes(&train),
            train_permuted: bytes(&train_permuted),
            natural_ids: record_ids(&natural),
            natural: bytes(&natural),
            permuted_ids: record_ids(&permuted),
            permuted: bytes(&permuted),
            multi_predict_ids: select("predict"),
            multi_test_ids: select("test"),
            multi: bytes(&multi),
            native_outcome: serde_json::to_value(&produced.training.outcome).unwrap(),
            native: produced.container,
            dir,
        };
        let put =
            |name: &str, content: &[u8]| std::fs::write(fixture.dir.join(name), content).unwrap();
        put("train.json", &fixture.train);
        put("train_permuted.json", &fixture.train_permuted);
        put("natural.json", &fixture.natural);
        put("permuted.json", &fixture.permuted);
        put("multi.json", &fixture.multi);
        put(
            "recipe.json",
            &serde_json::to_vec(&recipe_value(false)).unwrap(),
        );
        put("native.n4a", &copy_n4a_bytes(&fixture.native));
        assert_ne!(fixture.natural_ids, sorted(&fixture.natural_ids));
        assert_ne!(fixture.permuted_ids, sorted(&fixture.permuted_ids));
        assert_ne!(fixture.permuted_ids, fixture.natural_ids);
        assert!(!fixture.multi_predict_ids.is_empty() && !fixture.multi_test_ids.is_empty());
        fixture
    })
}

fn sorted(ids: &[String]) -> Vec<String> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids
}

fn path(fixture: &Fixture, name: &str) -> String {
    fixture.dir.join(name).to_string_lossy().into_owned()
}

/// A prediction reduced to what must agree across engines: values keyed by
/// sample ID and target name, never by position.
struct Rows {
    sample_ids: Vec<String>,
    values: BTreeMap<(String, String), f64>,
}

fn rows_of(replay: &Value, sample_ids: Vec<String>) -> Rows {
    let phases: BTreeSet<_> = replay["lineage"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["phase"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        phases,
        BTreeSet::from(["PREDICT".to_owned()]),
        "replay must contain only PREDICT lineage"
    );
    let block = &replay["outputs"][0]["predictions"][0];
    let ids: Vec<String> = serde_json::from_value(block["sample_ids"].clone()).unwrap();
    let names: Vec<String> = serde_json::from_value(block["target_names"].clone()).unwrap();
    assert_eq!(names, expected_names(false));
    assert_eq!(
        ids.iter().collect::<BTreeSet<_>>(),
        sample_ids.iter().collect::<BTreeSet<_>>(),
        "replay covers exactly the selected partition"
    );
    let mut values = BTreeMap::new();
    for (id, row) in ids.iter().zip(block["values"].as_array().unwrap()) {
        for (name, value) in names.iter().zip(row.as_array().unwrap()) {
            let previous = values.insert((id.clone(), name.clone()), value.as_f64().unwrap());
            assert!(previous.is_none(), "duplicate prediction for {id}/{name}");
        }
    }
    Rows { sample_ids, values }
}

fn assert_close(label: &str, left: &Rows, right: &Rows, tolerance: f64) {
    assert_eq!(
        left.values.keys().collect::<Vec<_>>(),
        right.values.keys().collect::<Vec<_>>(),
        "{label}: sample/target keys"
    );
    for (key, a) in &left.values {
        let b = right.values[key];
        assert!((a - b).abs() <= tolerance, "{label}: {key:?} {a} vs {b}");
    }
}

fn native_predict(
    q: &Qualification,
    container: &OwnedN4a,
    record: Option<&[u8]>,
    partition: &str,
) -> (Rows, Value) {
    let mut request = action(
        record.unwrap_or(&[]),
        "predict-external",
        &q.library,
        &q.sha,
    );
    request.partition = partition;
    if record.is_none() {
        request.entrypoint = "predict";
        request.input = NativeInput::Stored;
    }
    let prediction = predict_n4a_native(container, request).unwrap();
    let replay = serde_json::to_value(&prediction.replay).unwrap();
    let receipt = serde_json::to_value(&prediction.receipt).unwrap();
    (rows_of(&replay, prediction.sample_ids), receipt)
}

fn wasm_case<'a>(result: &'a Value, name: &str) -> &'a Value {
    result["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("driver returned no case {name}"))
}

fn wasm_rows(result: &Value, name: &str) -> (Rows, Value) {
    let case = wasm_case(result, name);
    let ids: Vec<String> = serde_json::from_value(case["sampleIds"].clone()).unwrap();
    (rows_of(&case["replay"], ids), case["receipt"].clone())
}

fn prediction_cases(fixture: &Fixture, stored: bool) -> Value {
    let mut cases = vec![
        json!({"name":"natural","record":path(fixture,"natural.json"),"entrypoint":"predict-external","partition":"predict"}),
        json!({"name":"permuted","record":path(fixture,"permuted.json"),"entrypoint":"predict-external","partition":"predict"}),
        json!({"name":"multi_predict","record":path(fixture,"multi.json"),"entrypoint":"predict-external","partition":"predict"}),
        json!({"name":"multi_test","record":path(fixture,"multi.json"),"entrypoint":"predict-external","partition":"test"}),
    ];
    if stored {
        cases.push(json!({"name":"stored","entrypoint":"predict","partition":"train"}));
    }
    json!(cases)
}

/// All predictions of one archive on both engines, compared by ID and name.
fn assert_engines_agree(
    q: &Qualification,
    fixture: &Fixture,
    label: &str,
    container: &OwnedN4a,
    archive: &str,
) {
    let result = node(
        q,
        &fixture.dir,
        &format!("predict-{label}"),
        json!({"command":"predict","archive":path(fixture,archive),"sourceId":SOURCE,"cases":prediction_cases(fixture, true)}),
    );
    // Y was never requested and the predict plan has no target accessor.
    assert_eq!(
        result["targetRequests"], 0,
        "{label}: predict requested fitting labels"
    );
    for member in result["planMembers"].as_array().unwrap() {
        let member = member.as_str().unwrap();
        let target_accessor = member == "y"
            || member.starts_with("y_")
            || member == "targets"
            || member.starts_with("targets_");
        assert!(!target_accessor, "{label}: predict plan exposes {member}");
    }
    // Each explicit case keeps its record, partition and expected sample IDs together.
    #[allow(clippy::type_complexity)]
    let natives: [(&str, Option<&[u8]>, &str, Option<&[String]>); 5] = [
        (
            "natural",
            Some(&fixture.natural),
            "predict",
            Some(&fixture.natural_ids),
        ),
        (
            "permuted",
            Some(&fixture.permuted),
            "predict",
            Some(&fixture.permuted_ids),
        ),
        (
            "multi_predict",
            Some(&fixture.multi),
            "predict",
            Some(&fixture.multi_predict_ids),
        ),
        (
            "multi_test",
            Some(&fixture.multi),
            "test",
            Some(&fixture.multi_test_ids),
        ),
        ("stored", None, "train", None),
    ];
    for (name, record, partition, expected_ids) in natives {
        let (native, native_receipt) = native_predict(q, container, record, partition);
        let (wasm, wasm_receipt) = wasm_rows(&result, name);
        // Physical IO record order, not the replay's order.
        assert_eq!(
            native.sample_ids, wasm.sample_ids,
            "{label}/{name}: sample_ids order"
        );
        if let Some(expected) = expected_ids {
            assert_eq!(wasm.sample_ids, expected, "{label}/{name}: record order");
        }
        assert_close(
            &format!("{label}/{name}"),
            &native,
            &wasm,
            PREDICT_TOLERANCE,
        );
        assert_eq!(native_receipt, wasm_receipt, "{label}/{name}: receipt");
    }
    // Permutation changes order only: same values for the same ID.
    let (natural, _) = wasm_rows(&result, "natural");
    assert_eq!(natural.sample_ids, fixture.natural_ids);
    let (permuted, _) = wasm_rows(&result, "permuted");
    assert_eq!(permuted.sample_ids, fixture.permuted_ids);
    let by_id = |rows: &Rows| -> BTreeMap<(String, String), f64> {
        rows.values.iter().map(|(k, v)| (k.clone(), *v)).collect()
    };
    let (natural_future, permuted_future) = (by_id(&natural), by_id(&permuted));
    for (key, value) in &natural_future {
        // Permuted rows reuse the natural IDs and spectra, so IDs map to equal inputs.
        assert!(
            (value - permuted_future[key]).abs() <= PREDICT_TOLERANCE,
            "{label}: permutation changed {key:?}"
        );
    }
}

fn identity(outcome: &Value) -> Value {
    let plan = &outcome["effective_plan"];
    let controllers: BTreeMap<String, Value> = plan["node_plans"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(node, plan)| {
            (
                node.clone(),
                json!([plan["controller_id"], plan["controller_version"]]),
            )
        })
        .collect();
    json!({
        "request": outcome["training_request_fingerprint"],
        "data_identities": outcome["data_identities"],
        "selected_variant": outcome["selected_variant_id"],
        "controllers": controllers,
    })
}

/// Fold membership independent of the physical row order of the record.
fn fold_map(outcome: &Value) -> BTreeMap<String, (Vec<String>, Vec<String>)> {
    let strings = |value: &Value| -> Vec<String> {
        let mut ids: Vec<String> = serde_json::from_value(value.clone()).unwrap();
        ids.sort();
        ids
    };
    outcome["effective_plan"]["fold_set"]["folds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fold| {
            (
                fold["fold_id"].as_str().unwrap().to_owned(),
                (
                    strings(&fold["train_sample_ids"]),
                    strings(&fold["validation_sample_ids"]),
                ),
            )
        })
        .collect()
}

fn phases(outcome: &Value) -> BTreeSet<String> {
    outcome["lineage"]
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["phase"].as_str().unwrap().to_owned())
        .collect()
}

fn wasm_produced(q: &Qualification, fixture: &Fixture) -> &'static (Value, OwnedN4a) {
    static PRODUCED: OnceLock<(Value, OwnedN4a)> = OnceLock::new();
    PRODUCED.get_or_init(|| {
        let result = node(
            q,
            &fixture.dir,
            "produce",
            json!({"command":"produce","record":path(fixture,"train.json"),"recipe":path(fixture,"recipe.json"),
                "sourceId":SOURCE,"runId":RUN_ID,"containerId":CONTAINER_ID,"archiveOut":path(fixture,"wasm.n4a")}),
        );
        let container = open(std::fs::read(fixture.dir.join("wasm.n4a")).unwrap());
        assert_eq!(result["containerSha256"], result["inspectedContainerSha256"]);
        assert_eq!(result["sourceId"], SOURCE);
        (result, container)
    })
}

#[test]
#[ignore = "dedicated cross-language qualification; run this target with --ignored"]
fn qualification_environment_is_complete() {
    qualification();
}

/// The retained training record with its first row held out: the same 18 rows,
/// IDs and fold labels, with that one row relabelled `test` and given no fold
/// (a valid IO record). Returns the exact bytes, the train IDs in record order
/// and the held-out ID.
fn mixed_record() -> (Vec<u8>, Vec<String>, String) {
    let mut mixed = record("train", 18, true, true);
    let ids = record_ids(&mixed);
    mixed["dataset"]["partitions"]["values"][0] = json!("test");
    mixed["fold_ids"][0] = Value::Null;
    (
        serde_json::to_vec(&mixed).unwrap(),
        ids[1..].to_vec(),
        ids[0].clone(),
    )
}

/// The training cohort of one outcome is exactly the train rows: the FoldSet
/// universe, every fold, the signed input IDs and the selected OOF units. The
/// held-out row appears nowhere in it.
fn assert_train_only_cohort(label: &str, outcome: &Value, train_ids: &[String], heldout: &str) {
    let expected: BTreeSet<String> = train_ids.iter().cloned().collect();
    let ids = |value: &Value| -> BTreeSet<String> {
        serde_json::from_value::<Vec<String>>(value.clone())
            .unwrap()
            .into_iter()
            .collect()
    };
    let plan = &outcome["effective_plan"];
    assert_eq!(
        ids(&plan["fold_set"]["sample_ids"]),
        expected,
        "{label}: fold universe"
    );
    assert_eq!(
        ids(&plan["campaign"]["metadata"]["input_sample_ids"]),
        expected,
        "{label}: signed input IDs"
    );
    for fold in plan["fold_set"]["folds"].as_array().unwrap() {
        let (train, validation) = (
            ids(&fold["train_sample_ids"]),
            ids(&fold["validation_sample_ids"]),
        );
        assert!(
            !train.contains(heldout) && !validation.contains(heldout),
            "{label}: held-out row in a fold"
        );
        assert!(train.is_disjoint(&validation), "{label}: fold overlap");
        assert_eq!(
            train.union(&validation).cloned().collect::<BTreeSet<_>>(),
            expected,
            "{label}: fold coverage"
        );
    }
    let averages = outcome["oof_averages"].as_array().unwrap();
    assert!(!averages.is_empty(), "{label}: no OOF average");
    for average in averages {
        let oof: BTreeSet<String> = average["predictions"]["unit_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(oof, expected, "{label}: OOF units");
    }
    assert_eq!(outcome["refit"]["status"], "completed", "{label}: refit");
    let selections = outcome["execution_bundle"]["selections"]
        .as_object()
        .unwrap();
    assert!(!selections.is_empty(), "{label}: no selection");
    for selection in selections.values() {
        assert_eq!(selection["evaluation_scope"], "oof", "{label}: selection");
        assert!(selection["selected_score"].as_f64().unwrap().is_finite());
    }
}

#[test]
#[ignore = "dedicated cross-language qualification; run this target with --ignored"]
fn mixed_partition_record_trains_on_train_rows_and_replays_heldout() {
    let q = qualification();
    let fixture = fixture(&q);
    let (bytes, train_ids, heldout) = mixed_record();
    let heldout_ids = vec![heldout.clone()];
    std::fs::write(fixture.dir.join("mixed.json"), &bytes).unwrap();
    let native = produce_n4a_native_with_view(NativeProduceRequest {
        record: &bytes,
        source_id: SOURCE,
        recipe: &recipe(),
        runtime: runtime(&q),
        run_id: RUN_ID,
        container_id: CONTAINER_ID,
        embed_training_record: true,
        memory_budget: N4aMemoryBudget::default(),
    })
    .unwrap();
    let native_outcome = serde_json::to_value(&native.training.outcome).unwrap();
    std::fs::write(
        fixture.dir.join("mixed-native.n4a"),
        copy_n4a_bytes(&native.container),
    )
    .unwrap();
    let produced = node(
        &q,
        &fixture.dir,
        "produce-mixed",
        json!({"command":"produce","record":path(fixture,"mixed.json"),"recipe":path(fixture,"recipe.json"),
            "sourceId":SOURCE,"runId":RUN_ID,"containerId":CONTAINER_ID,"archiveOut":path(fixture,"mixed-wasm.n4a")}),
    );
    let wasm_container = open(std::fs::read(fixture.dir.join("mixed-wasm.n4a")).unwrap());
    assert_eq!(
        produced["containerSha256"],
        produced["inspectedContainerSha256"]
    );
    let wasm_outcome = &produced["outcome"];

    // Both engines train on the train rows only and keep the complete record.
    for (label, outcome, container) in [
        ("native", &native_outcome, &native.container),
        ("wasm", wasm_outcome, &wasm_container),
    ] {
        assert_train_only_cohort(label, outcome, &train_ids, &heldout);
        let anchor = container.member("io/training.json").expect("anchor");
        assert_eq!(anchor.bytes, bytes.as_slice(), "{label}: anchor bytes");
    }
    // The same record and recipe give the same identities and folds on both engines.
    assert_eq!(identity(wasm_outcome), identity(&native_outcome));
    assert_eq!(
        wasm_outcome["effective_plan"]["fold_set"],
        native_outcome["effective_plan"]["fold_set"]
    );

    // Stored and external held-out replay, by ID, on each archive and each engine.
    let cases = json!([
        {"name":"stored_test","entrypoint":"predict","partition":"test"},
        {"name":"external_test","record":path(fixture,"mixed.json"),"entrypoint":"predict-external","partition":"test"},
    ]);
    let mut stored = Vec::new();
    for (label, container, archive) in [
        ("native", &native.container, "mixed-native.n4a"),
        ("wasm", &wasm_container, "mixed-wasm.n4a"),
    ] {
        let result = node(
            &q,
            &fixture.dir,
            &format!("predict-mixed-{label}"),
            json!({"command":"predict","archive":path(fixture,archive),"sourceId":SOURCE,"cases":cases.clone()}),
        );
        assert_eq!(
            result["targetRequests"], 0,
            "{label}: held-out replay requested fitting labels"
        );
        let mut replays = Vec::new();
        for (name, record) in [("stored_test", None), ("external_test", Some(&bytes[..]))] {
            let (native_rows, native_receipt) = native_predict(&q, container, record, "test");
            let (wasm_replay, wasm_receipt) = wasm_rows(&result, name);
            assert_eq!(native_rows.sample_ids, heldout_ids, "{label}/{name}");
            assert_eq!(wasm_replay.sample_ids, heldout_ids, "{label}/{name}");
            assert_close(
                &format!("{label}/{name}"),
                &native_rows,
                &wasm_replay,
                PREDICT_TOLERANCE,
            );
            assert_eq!(native_receipt, wasm_receipt, "{label}/{name}: receipt");
            replays.push(native_rows);
        }
        assert_close(
            &format!("{label}: stored vs external held-out"),
            &replays[0],
            &replays[1],
            PREDICT_TOLERANCE,
        );
        stored.push(replays.swap_remove(0));
    }
    // Each engine's own refit predicts the held-out row alike.
    assert_close(
        "native vs wasm refit on the held-out row",
        &stored[0],
        &stored[1],
        RETRAIN_TOLERANCE,
    );
}

#[test]
#[ignore = "dedicated cross-language qualification; run this target with --ignored"]
fn same_record_and_recipe_give_equal_identities_on_both_engines() {
    let q = qualification();
    let fixture = fixture(&q);
    let (wasm, _) = wasm_produced(&q, fixture);
    let wasm_outcome = &wasm["outcome"];
    assert_eq!(identity(wasm_outcome), identity(&fixture.native_outcome));
    assert_eq!(
        wasm_outcome["effective_plan"]["fold_set"],
        fixture.native_outcome["effective_plan"]["fold_set"]
    );
    assert_eq!(
        wasm["inspectedRequestFingerprint"],
        wasm_outcome["training_request_fingerprint"]
    );
    for outcome in [wasm_outcome, &fixture.native_outcome] {
        assert!(
            phases(outcome).is_superset(&BTreeSet::from(["FIT_CV".to_owned(), "REFIT".to_owned()]))
        );
    }
}

#[test]
#[ignore = "dedicated cross-language qualification; run this target with --ignored"]
fn native_produced_archive_predicts_in_wasm() {
    let q = qualification();
    let fixture = fixture(&q);
    assert_engines_agree(&q, fixture, "native", &fixture.native, "native.n4a");
}

#[test]
#[ignore = "dedicated cross-language qualification; run this target with --ignored"]
fn wasm_produced_archive_predicts_natively() {
    let q = qualification();
    let fixture = fixture(&q);
    let (_, container) = wasm_produced(&q, fixture);
    read_n4a_training_view(container).expect("native owner reads the WASM-produced training view");
    assert_engines_agree(&q, fixture, "wasm", container, "wasm.n4a");
}

#[test]
#[ignore = "dedicated cross-language qualification; run this target with --ignored"]
fn stored_recipe_retrain_works_in_both_directions() {
    let q = qualification();
    let fixture = fixture(&q);
    let (wasm_produced_result, wasm_container) = wasm_produced(&q, fixture);
    let before_native = identity(&fixture.native_outcome);
    let before_wasm = identity(&wasm_produced_result["outcome"]);

    // WASM retrains the native archive from its stored recipe: no caller recipe exists.
    for (label, record_name) in [
        ("natural", "train.json"),
        ("permuted", "train_permuted.json"),
    ] {
        let out = format!("wasm-retrained-{label}.n4a");
        let retrained = node(
            &q,
            &fixture.dir,
            &format!("retrain-wasm-{label}"),
            json!({"command":"retrain","archive":path(fixture,"native.n4a"),"record":path(fixture,record_name),
                "runId":RUN_ID,"containerId":format!("{CONTAINER_ID}:{label}"),"archiveOut":path(fixture,&out)}),
        );
        let outcome = &retrained["outcome"];
        assert!(
            phases(outcome).is_superset(&BTreeSet::from(["FIT_CV".to_owned(), "REFIT".to_owned()])),
            "{label}"
        );
        assert_eq!(
            fold_map(outcome),
            fold_map(&fixture.native_outcome),
            "{label}: folds"
        );
        assert_eq!(
            outcome["selected_variant_id"], fixture.native_outcome["selected_variant_id"],
            "{label}: variant"
        );
        assert_eq!(
            identity(outcome)["controllers"],
            before_native["controllers"],
            "{label}"
        );
        // The retrained archive is predicted by the other engine (native) and by WASM.
        let container = open(std::fs::read(fixture.dir.join(&out)).unwrap());
        read_n4a_training_view(&container).unwrap();
        for record in [&fixture.natural, &fixture.permuted] {
            let (retrained_rows, _) = native_predict(&q, &container, Some(record), "predict");
            let (original_rows, _) = native_predict(&q, &fixture.native, Some(record), "predict");
            assert_close(
                &format!("wasm retrain {label} vs original"),
                &retrained_rows,
                &original_rows,
                RETRAIN_TOLERANCE,
            );
        }
        assert_engines_agree(
            &q,
            fixture,
            &format!("wasm-retrained-{label}"),
            &container,
            &out,
        );
    }

    // Native retrains the WASM archive from its stored recipe.
    for (label, record) in [
        ("natural", &fixture.train),
        ("permuted", &fixture.train_permuted),
    ] {
        let produced = retrain_n4a_native(
            wasm_container,
            NativeRetrainRequest {
                record,
                runtime: runtime(&q),
                run_id: RUN_ID,
                container_id: &format!("{CONTAINER_ID}:{label}"),
                embed_training_record: true,
                memory_budget: N4aMemoryBudget::default(),
            },
        )
        .unwrap();
        let outcome = serde_json::to_value(&produced.training.outcome).unwrap();
        assert!(
            phases(&outcome)
                .is_superset(&BTreeSet::from(["FIT_CV".to_owned(), "REFIT".to_owned()])),
            "{label}"
        );
        assert_eq!(
            fold_map(&outcome),
            fold_map(&wasm_produced_result["outcome"]),
            "{label}: folds"
        );
        assert_eq!(
            outcome["selected_variant_id"], wasm_produced_result["outcome"]["selected_variant_id"],
            "{label}"
        );
        assert_eq!(
            identity(&outcome)["controllers"],
            before_wasm["controllers"],
            "{label}"
        );
        let out = format!("native-retrained-{label}.n4a");
        std::fs::write(fixture.dir.join(&out), copy_n4a_bytes(&produced.container)).unwrap();
        assert_engines_agree(
            &q,
            fixture,
            &format!("native-retrained-{label}"),
            &produced.container,
            &out,
        );
    }
}

#[test]
#[ignore = "dedicated cross-language qualification; run this target with --ignored"]
fn unsupported_profiles_and_untrusted_manifests_refuse_explicitly() {
    let q = qualification();
    let fixture = fixture(&q);
    let result = node(
        &q,
        &fixture.dir,
        "boundaries",
        json!({"command":"boundaries","archive":path(fixture,"native.n4a"),"trainRecord":path(fixture,"train.json"),
            "recipe":path(fixture,"recipe.json"),"externalRecord":path(fixture,"natural.json"),"sourceId":SOURCE}),
    );
    let keywords: [(&str, &[&str]); 5] = [
        ("classifier", &["classif", "regressor", "target", "label"]),
        ("nativeTargetIndex", &["native_target_index", "target"]),
        ("partialTargetMask", &["mask", "target", "complete"]),
        ("schemaChange", &["schema"]),
        ("manifestMismatch", &["manifest", "controller"]),
    ];
    for (case, words) in keywords {
        let outcome = &result[case];
        assert_eq!(
            outcome["refused"], true,
            "{case} must be refused: {outcome}"
        );
        let message = outcome["message"].as_str().unwrap().to_lowercase();
        assert!(
            words.iter().any(|word| message.contains(word)),
            "{case}: unexpected refusal '{message}'"
        );
    }
    assert_eq!(
        result["manifestMismatch"]["callbacks"], 0,
        "an untrusted manifest must be refused before any estimator callback"
    );
}
