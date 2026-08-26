//! Native Archive V2 access for the Python aggregate facade.
//!
//! The extension deliberately exposes validated member bytes only.  ZIP
//! parsing, schema dispatch, inventory validation, and raw-integrity checks
//! remain in the aggregate Rust reader; DAG-ML remains the sole owner of
//! package parsing and replay.

#![allow(clippy::useless_conversion)] // PyO3's exported-function wrapper converts PyErr to itself.

use std::path::Path;

use nirs4all::{
    load_archive_v2, load_archive_v3, write_archive_v2, write_archive_v3, ArchivePayload,
    ArchiveV2WriteRequest, ArchiveV3WriteRequest,
};
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyAny, PyBytes},
};
use serde_json::Value;

fn archive_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("Archive V2 validation refused: {error}"))
}

/// Return the exact DAG-ML PortablePredictorPackage V2 bytes from a validated
/// Archive V2.  This does not parse, deserialize, or execute the package.
#[pyfunction]
fn read_portable_predictor_package_v2<'py>(
    py: Python<'py>,
    path: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let archive = load_archive_v2(Path::new(path)).map_err(archive_error)?;
    let package = archive
        .portable_predictor_package()
        .map_err(archive_error)?;
    Ok(PyBytes::new_bound(py, package))
}

/// Write a fully assembled Archive V2 without implementing any archive or
/// DAG-ML semantics in Python. The manifest must come from DAG-ML's native
/// assembler; Core validates every declaration and derives the inventory/raw
/// hashes immediately before its atomic no-replace write.
#[pyfunction]
fn write_archive_v2_from_native_payloads(
    path: &str,
    manifest: &Bound<'_, PyAny>,
    members: Vec<(String, Vec<u8>)>,
) -> PyResult<(String, String)> {
    let manifest: Value = pythonize::depythonize(manifest).map_err(|error| {
        PyValueError::new_err(format!(
            "Archive V2 manifest is not JSON-compatible: {error}"
        ))
    })?;
    let payloads = members
        .into_iter()
        .map(|(path, bytes)| ArchivePayload { path, bytes })
        .collect();
    let reference = write_archive_v2(
        Path::new(path),
        ArchiveV2WriteRequest { manifest, payloads },
    )
    .map_err(archive_error)?;
    Ok((
        reference.archive_id().to_owned(),
        reference.archive_sha256().to_owned(),
    ))
}

/// Return exact DAG-ML PortableRefitPackage V3 bytes from a Core-validated
/// Archive V3. Core never interprets the package; DAG-ML owns that semantic
/// validation and PREDICT replay.
#[pyfunction]
fn read_portable_refit_package_v3<'py>(
    py: Python<'py>,
    path: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let archive = load_archive_v3(Path::new(path)).map_err(archive_error)?;
    let package = archive.portable_refit_package().map_err(archive_error)?;
    Ok(PyBytes::new_bound(py, package))
}

/// Write a DAG-ML-assembled Archive V3. Raw member hashes/sizes, closed
/// inventory, strict container rules, and atomic publication are Core-owned.
#[pyfunction]
fn write_archive_v3_from_native_payloads(
    path: &str,
    manifest: &Bound<'_, PyAny>,
    members: Vec<(String, Vec<u8>)>,
) -> PyResult<(String, String)> {
    let manifest: Value = pythonize::depythonize(manifest).map_err(|error| {
        PyValueError::new_err(format!(
            "Archive V3 manifest is not JSON-compatible: {error}"
        ))
    })?;
    let payloads = members
        .into_iter()
        .map(|(path, bytes)| ArchivePayload { path, bytes })
        .collect();
    let reference = write_archive_v3(
        Path::new(path),
        ArchiveV3WriteRequest { manifest, payloads },
    )
    .map_err(archive_error)?;
    Ok((
        reference.archive_id().to_owned(),
        reference.archive_sha256().to_owned(),
    ))
}

/// Python extension module installed as ``nirs4all_core._native``.
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        read_portable_predictor_package_v2,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        write_archive_v2_from_native_payloads,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(read_portable_refit_package_v3, module)?)?;
    module.add_function(wrap_pyfunction!(
        write_archive_v3_from_native_payloads,
        module
    )?)
}
