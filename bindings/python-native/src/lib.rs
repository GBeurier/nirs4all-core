//! Native Archive V2 access for the Python aggregate facade.
//!
//! The extension deliberately exposes validated member bytes only.  ZIP
//! parsing, schema dispatch, inventory validation, and raw-integrity checks
//! remain in the aggregate Rust reader; DAG-ML remains the sole owner of
//! package parsing and replay.

#![allow(clippy::useless_conversion)] // PyO3's exported-function wrapper converts PyErr to itself.

use std::path::Path;

use nirs4all::load_archive_v2;
use pyo3::{exceptions::PyValueError, prelude::*, types::PyBytes};

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

/// Python extension module installed as ``nirs4all_core._native``.
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        read_portable_predictor_package_v2,
        module
    )?)
}
