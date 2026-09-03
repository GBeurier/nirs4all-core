use std::fs;

use nirs4all::{
    formats, load_spectrum_dataset_package, load_spectrum_methods_provider, FormatsIoError,
};

#[test]
fn real_delimited_format_reaches_io_package_and_core_provider() {
    let directory = tempfile::tempdir().expect("temporary fixture directory");
    let path = directory.path().join("tiny_nirs.csv");
    fs::write(
        &path,
        concat!(
            "sample_id,protein,1100.0,1200.0,1300.0\n",
            "S001,10.1,0.10,0.20,0.30\n",
            "S002,11.2,0.15,0.25,0.35\n",
            "S003,12.3,0.20,0.30,0.40\n",
        ),
    )
    .expect("write real delimited-text fixture");

    let parsed = formats::open_path(&path).expect("Formats parses the fixture");
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].provenance.format, "delimited-text");

    let loaded = load_spectrum_dataset_package(&path).expect("Formats records assemble through IO");
    assert_eq!(loaded.format, "delimited-text");
    assert_eq!(loaded.record_count, 3);
    assert!(!loaded.package.row_position_fallback.used);
    assert_eq!(
        loaded.package.identity.sample_id.as_deref(),
        Some("sample_id")
    );
    let partition = loaded
        .package
        .partitions
        .get("train")
        .expect("train partition");
    assert_eq!(partition.n_samples, 3);
    assert_eq!(
        partition.source_ids.as_slice(),
        std::slice::from_ref(&loaded.source_id)
    );

    let provider = load_spectrum_methods_provider(&path).expect("IO package reaches Core provider");
    assert_eq!(provider.source_id(), loaded.source_id);
    assert_eq!(provider.relations().records.len(), 3);
}

#[test]
fn unsupported_format_refuses_without_fallback() {
    let directory = tempfile::tempdir().expect("temporary fixture directory");
    let path = directory.path().join("not-a-spectrum.bin");
    fs::write(&path, b"not a supported spectral format").expect("write refusal fixture");

    let error = load_spectrum_dataset_package(&path).expect_err("unknown format must fail closed");
    assert!(matches!(
        error,
        FormatsIoError::Format(formats::Error::UnsupportedFormat { .. })
    ));
}
