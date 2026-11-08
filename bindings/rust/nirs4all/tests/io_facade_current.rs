//! Core's public `nirs4all::io` facade reaches the current IO owner.
//!
//! Both cases only translate owner behavior: no parser, imputation, fit or numerics live here. The headerless
//! fixture is the IO owner's own regression literal (`nirs4all-io` `header_inference.rs`).

use std::fs;

use nirs4all::io::core::infer::InferOptions;
use nirs4all::io::core::spec::DatasetSpec;
use nirs4all::io::infer::infer_path;
use nirs4all::io::materialize::assemble;
use serde_json::{json, Value};

/// 20 rows x 700 numeric columns, no header line. Column 0 varies per row; the rest are 0.2.
fn headerless_twenty_by_seven_hundred() -> String {
    let mut text = String::new();
    for row in 0..20 {
        text.push_str(&format!("0.{:07}", 465041 + row * 10000));
        for _ in 1..700 {
            text.push_str(";0.2");
        }
        text.push('\n');
    }
    text
}

#[test]
fn headerless_data_csv_infers_no_header_and_keeps_all_twenty_rows() {
    let directory = tempfile::tempdir().expect("temporary fixture directory");
    let path = directory.path().join("data.csv");
    fs::write(&path, headerless_twenty_by_seven_hundred()).expect("write headerless fixture");

    let input = path.to_str().expect("UTF-8 path");

    // Without a user choice the owner defaults to a header and flags the ambiguity; it never guesses `false`.
    let ambiguous = infer_path(input, &InferOptions::default()).expect("IO infers the file");
    assert_eq!(
        ambiguous.params["data.csv"]["header_ambiguous"],
        json!(true)
    );
    assert!(ambiguous
        .warnings
        .iter()
        .any(|warning| warning.contains("header_ambiguous") && warning.contains("data.csv")));
    assert_eq!(
        ambiguous.resolved_spec.expect("ambiguous spec").to_value()["sources"][0]["params"]
            ["has_header"],
        json!(true)
    );

    // The explicit choice is declared BEFORE the first inference; the plan is never patched afterwards.
    let options = InferOptions {
        params: json!({"has_header": false}),
        ..Default::default()
    };
    let plan = infer_path(input, &options).expect("IO infers the file");
    assert_eq!(plan.params["data.csv"]["has_header"]["value"], json!(false));
    assert!(plan.params["data.csv"].get("header_ambiguous").is_none());
    // A declared choice suppresses the ambiguity warning (the owner warns only for undeclared headers).
    assert!(!plan
        .warnings
        .iter()
        .any(|warning| warning.contains("header_ambiguous")));
    let spec = plan.resolved_spec.expect("inference resolves a spec");
    assert_eq!(
        spec.to_value()["sources"][0]["params"]["has_header"],
        json!(false)
    );

    let data = assemble(&spec, directory.path()).expect("IO assembles the resolved spec");
    let block = &data.blocks["train"];
    assert_eq!(block.source_ids, vec!["data"]);
    assert_eq!(
        block.n_samples, 20,
        "the first line is a data row, not a header"
    );
    assert_eq!(block.x[0].n_rows, 20);
    assert_eq!(block.x[0].n_cols, 699);
    assert_eq!(block.feature_headers[0].len(), 699);
    let x = &block.x[0].data;
    assert_eq!(x.len(), 20 * 699);
    assert_eq!(x[0], 0.0465041);
    assert!(x[1..699].iter().all(|value| *value == 0.2));
    assert_eq!(x[19 * 699], 0.0655041);
    assert!(x[19 * 699 + 1..].iter().all(|value| *value == 0.2));
    assert_eq!(block.y.as_ref().expect("target column").data, vec![0.2; 20]);
    assert!(data.identity.sample_id.is_none());

    // Control: forcing a header line consumes the first data row and the result is the 19-row defect shape.
    let mut explicit = spec.to_value();
    explicit["sources"][0]["params"]["has_header"] = json!(true);
    explicit["sources"][0]["role"] = json!("features");
    explicit["sources"][0]
        .as_object_mut()
        .expect("source object")
        .remove("columns");
    let explicit = DatasetSpec::from_value(&explicit).expect("explicit-header spec");
    let data = assemble(&explicit, directory.path()).expect("explicit-header assembly");
    assert_eq!(data.blocks["train"].n_samples, 19);
    assert_eq!(data.blocks["train"].x[0].data[0], 0.0475041);
}

const LITERAL: &str =
    "Sample_ID;set;1100;target\nS_missing;train;;1.0\nS_finite;train;2.0;2.0\nS_test;test;10.0;3.0\n";

/// The IO owner's literal probe spec; only the NA configuration varies.
fn probe_spec(na: Value) -> Value {
    json!({
        "schema_version": 1,
        "name": "statistical-na-mean-heldout-probe",
        "task_type": "regression",
        "sample_index": {"by": "id", "key": "Sample_ID"},
        "params": {"delimiter": ";", "decimal": ".", "has_header": true, "na": na},
        "sources": [{
            "id": "x", "role": "mixed", "input": "literal.csv", "key": "Sample_ID",
            "params": {"header_unit": "nm"},
            "columns": {"features": ["1100"], "targets": ["target"], "metadata": ["set"]}
        }],
        "partitions": {
            "by": "column", "column": "set", "train_values": ["train"],
            "test_values": ["test"], "unknown_policy": "error"
        }
    })
}

#[test]
fn mean_and_median_na_fills_are_refused_by_the_owner_even_when_inactive() {
    // A statistical fill is refused as a specification, before any file is read or missingness considered.
    for method in ["mean", "median"] {
        // `ignore` makes the fill inactive; the owner still refuses the statistical method.
        for policy in ["replace", "ignore"] {
            for source_override in [false, true] {
                let na = json!({"policy": policy, "fill": {"method": method, "per_column": true}});
                let mut value = probe_spec(json!({"policy": "ignore"}));
                if source_override {
                    value["sources"][0]["params"]["na"] = na;
                } else {
                    value["params"]["na"] = na;
                }
                let error = DatasetSpec::from_value(&value)
                    .expect_err("statistical NA fill must be refused");
                let message = error.message;
                assert!(message.contains("na.fill.method:"), "{message}");
                assert!(message.contains("is refused"), "{message}");
                assert!(message.contains("fitted preprocessing"), "{message}");
                assert!(message.contains("within each sample"), "{message}");
            }
        }
    }

    // Control: the identical structure with only the method changed is a valid owner specification.
    let supported = probe_spec(json!({"policy": "replace", "fill": {"method": "forward_fill"}}));
    let parsed = DatasetSpec::from_value(&supported).expect("forward_fill stays supported");
    assert_eq!(
        parsed.to_value()["params"]["na"]["fill"]["method"],
        json!("forward_fill")
    );

    // The supported `ignore` control still assembles the same file with its exact values and masks.
    let directory = tempfile::tempdir().expect("temporary fixture directory");
    fs::write(directory.path().join("literal.csv"), LITERAL).expect("write literal probe");

    let spec = DatasetSpec::from_value(&probe_spec(json!({"policy": "ignore"})))
        .expect("ignore policy is a supported control");
    let data = assemble(&spec, directory.path()).expect("IO assembles the supported control");
    let train = &data.blocks["train"];
    let test = &data.blocks["test"];
    assert_eq!(
        train
            .metadata
            .as_ref()
            .expect("train metadata")
            .str_column("Sample_ID"),
        vec!["S_missing", "S_finite"]
    );
    assert_eq!(
        test.metadata
            .as_ref()
            .expect("test metadata")
            .str_column("Sample_ID"),
        ["S_test"]
    );
    assert_eq!(train.x[0].validity, vec![false, true]);
    assert_eq!(train.x[0].data.last(), Some(&2.0));
    assert_eq!(train.y.as_ref().expect("train target").data, vec![1.0, 2.0]);
    assert_eq!(test.x[0].data, [10.0]);
}
