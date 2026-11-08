//! Native Archive V2/V3 access, `.n4a` byte and file handles, and Methods replay
//! for the Python facade.
//!
//! ZIP parsing, schema dispatch, inventory validation, and raw-integrity checks
//! remain in the aggregate Rust reader. DAG-ML remains the sole owner of
//! package parsing, scheduling, and invocation-local N4MM execution. The
//! replay functions accept strict JSON and never accept Python callbacks or
//! serialized host-model handles.

#![allow(clippy::useless_conversion)] // PyO3's exported-function wrapper converts PyErr to itself.

use std::borrow::Cow;
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use nirs4all::n4a::{
    open_owned_n4a_bytes, parse_n4a_manifest, write_n4a_bytes, write_n4a_update, N4aError,
    N4aMemberInput, N4aMemoryBudget, N4aWriteRequest, OwnedN4a,
};
use nirs4all::n4a_file::{
    open_n4a_file as core_open_n4a_file, write_n4a_file as core_write_n4a_file, N4aFile,
    N4aMemberReader, N4aMemberSource, N4aStreamBase, N4aStreamInput, N4aStreamRequest,
};
use nirs4all::n4a_native_package::{
    native_package_transport_error, read_native_package_from_file,
    write_calibrated_native_package_file, write_native_package_file, LoadedNativePackage,
};
use nirs4all::n4a_owner_portable::{n4a_container_view, n4a_view_json};
use nirs4all::{
    inspect_methods_archive_v2_predictors,
    predict_methods_archive_v2_matrix_json as core_predict_methods_archive_v2_matrix_json,
    predict_tuning_archive_v2_matrix_json as core_predict_tuning_archive_v2_matrix_json,
    replay_methods_archive_v2_conformal_presentation_v1_loaded_json as core_replay_methods_archive_v2_conformal_presentation_v1_json,
    replay_methods_archive_v2_conformal_presentation_v2_loaded_json as core_replay_methods_archive_v2_conformal_presentation_v2_json,
    replay_methods_archive_v2_loaded_json as core_replay_methods_archive_v2_json,
    replay_methods_archive_v3_loaded_json as core_replay_methods_archive_v3_json,
    MethodsArchiveReplayJsonRequest,
};
use nirs4all::{
    parse_native_recipe_json, predict_n4a_native as core_predict_n4a_native,
    produce_n4a_native_with_view as core_produce_n4a_native_with_view, read_n4a_training_view,
    retrain_n4a_native as core_retrain_n4a_native, NativeActionRequest, NativeInput,
    NativeProduceRequest, NativeRetrainRequest, NativeRuntime, NativeSchemaPolicy,
};
use pyo3::{
    buffer::PyBuffer,
    exceptions::{PyKeyError, PyTypeError, PyValueError},
    prelude::*,
    types::{PyAny, PyBytes, PyDict, PyList, PySlice, PyString, PyTuple},
};

fn archive_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("Archive V2 validation refused: {error}"))
}

fn archive_v3_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("Archive V3 validation refused: {error}"))
}

fn n4a_error(action: &str, detail: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("native n4a {action} refused: {detail}"))
}

fn replay_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("native Methods archive replay refused: {error}"))
}

/// The Core working-memory budget a call declares. `None` leaves only the
/// allocator and real host limits; a number is the authorized working set for
/// Core's own allocations, never a content-size cap.
fn memory_budget(bytes: Option<u64>) -> N4aMemoryBudget {
    N4aMemoryBudget::new(bytes)
}

fn core_error(error: N4aError) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn io_error(error: std::io::Error) -> PyErr {
    core_error(N4aError::from(error))
}

fn closed_error(what: &str) -> PyErr {
    PyValueError::new_err(format!("I/O operation on a closed {what}"))
}

fn not_addressable(_: std::num::TryFromIntError) -> PyErr {
    core_error(N4aError::Memory(
        "size is not addressable on this host".into(),
    ))
}

fn ended_early() -> PyErr {
    core_error(N4aError::Integrity(
        "member ended before its recorded length".into(),
    ))
}

/// Build a `bytes` object after admitting the known resident source and the
/// new copy, then allocate fallibly through CPython and fill once.
fn materialize<'py>(
    py: Python<'py>,
    data: &[u8],
    resident_bytes: u64,
    budget: N4aMemoryBudget,
) -> PyResult<Bound<'py, PyBytes>> {
    let total = resident_bytes
        .checked_add(data.len() as u64)
        .ok_or_else(|| core_error(N4aError::Memory("working set size overflow".into())))?;
    budget.admit(total).map_err(core_error)?;
    PyBytes::new_with(py, data.len(), |buffer| {
        buffer.copy_from_slice(data);
        Ok(())
    })
}

/// Copy caller-supplied archive bytes into an owned buffer (admitted, fallible)
/// and let the owner validate it once.
fn open_caller_bytes(bytes: &[u8], budget: N4aMemoryBudget) -> Result<OwnedN4a, N4aError> {
    budget.admit(bytes.len() as u64)?;
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len()).map_err(|error| {
        N4aError::Memory(format!("cannot allocate {} bytes: {error}", bytes.len()))
    })?;
    copy.extend_from_slice(bytes);
    open_owned_n4a_bytes(copy.into_boxed_slice(), budget)
}

/// Exact bytes or a cloned real Core file handle, never a Python lookalike.
enum NativeArchiveInput<'a> {
    Bytes(&'a [u8]),
    File(N4aFile),
}

impl NativeArchiveInput<'_> {
    fn into_owned(self, budget: N4aMemoryBudget) -> Result<OwnedN4a, N4aError> {
        match self {
            Self::Bytes(bytes) => open_caller_bytes(bytes, budget),
            Self::File(file) => file.into_owned(budget)?.with_memory_budget(budget),
        }
    }
}

fn native_archive_input<'a>(
    archive: &'a Bound<'_, PyAny>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<(NativeArchiveInput<'a>, N4aMemoryBudget)> {
    if let Ok(file) = archive.cast::<N4aFileHandle>() {
        let file = file.get().file()?;
        let budget = memory_budget_bytes
            .map(N4aMemoryBudget::bytes)
            .unwrap_or_else(|| file.memory_budget());
        return Ok((NativeArchiveInput::File(file), budget));
    }
    if let Ok(bytes) = archive.cast::<PyBytes>() {
        return Ok((
            NativeArchiveInput::Bytes(bytes.as_bytes()),
            memory_budget(memory_budget_bytes),
        ));
    }
    Err(PyTypeError::new_err(
        "archive_bytes must be bytes or an N4aFile",
    ))
}

fn member_reader(file: &N4aFile, path: &str) -> PyResult<N4aMemberReader> {
    if file.member_location(path).is_none() {
        return Err(PyKeyError::new_err(path.to_owned()));
    }
    file.member_reader(path).map_err(core_error)
}

/// One blocking read without the GIL; the buffer is Rust memory.
fn read_detached(
    py: Python<'_>,
    reader: &mut N4aMemberReader,
    buffer: &mut [u8],
) -> PyResult<usize> {
    py.detach(|| reader.read(buffer)).map_err(io_error)
}

/// A fully delivered stream must have been verified; this also covers a
/// zero-length member, which verifies on its first read.
fn settle(py: Python<'_>, reader: &mut N4aMemberReader) -> PyResult<()> {
    if reader.remaining() == 0 && !reader.is_verified() {
        let mut probe = [0u8; 1];
        if read_detached(py, reader, &mut probe)? != 0 || !reader.is_verified() {
            return Err(core_error(N4aError::Integrity(
                "member was not verified at its end".into(),
            )));
        }
    }
    Ok(())
}

/// Deliver the next `length` bytes as one `bytes` object: admitted first, read
/// straight into the CPython allocation (no second copy), and never returned
/// unless the reader verified the member when it delivered its last byte.
///
/// Releasing the GIL inside `PyBytes::new_with` is sound only because that
/// object is private until `new_with` returns: it is not yet reachable from
/// Python, no alias or Python callback is exposed while the buffer is filled,
/// and nothing else reads or resizes it. Do not copy this pattern for a buffer
/// the caller already holds (see `N4aMemberStream::readinto`).
fn read_into_bytes<'py>(
    py: Python<'py>,
    reader: &mut N4aMemberReader,
    length: u64,
    budget: N4aMemoryBudget,
) -> PyResult<Bound<'py, PyBytes>> {
    budget.admit(length).map_err(core_error)?;
    let size = usize::try_from(length).map_err(not_addressable)?;
    let bytes = PyBytes::new_with(py, size, |buffer| {
        let mut filled = 0;
        while filled < size {
            let count = read_detached(py, reader, &mut buffer[filled..])?;
            if count == 0 {
                return Err(ended_early());
            }
            filled += count;
        }
        Ok(())
    })?;
    settle(py, reader)?;
    Ok(bytes)
}

/// `std::io::Write` over a Python binary file-like object. The GIL is taken
/// only around each `write` call; a Python exception is kept and re-raised
/// as itself instead of being flattened into a Core refusal.
struct PyWriter {
    target: Py<PyAny>,
    failure: Option<PyErr>,
}

impl Write for PyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let target = &self.target;
        let outcome = Python::attach(|py| -> PyResult<usize> {
            let result = target
                .bind(py)
                .call_method1("write", (PyBytes::new(py, buf),))?;
            if result.is_none() {
                Ok(buf.len())
            } else {
                result.extract::<usize>()
            }
        });
        match outcome {
            Ok(count) if count <= buf.len() => Ok(count),
            Ok(_) => Err(std::io::Error::other(
                "write() accepted more bytes than it was given",
            )),
            Err(error) => {
                self.failure = Some(error);
                Err(std::io::Error::other("destination write failed"))
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn copy_to<F>(py: Python<'_>, destination: &Bound<'_, PyAny>, copy: F) -> PyResult<u64>
where
    F: FnOnce(&mut PyWriter) -> Result<u64, N4aError> + Send,
{
    if !destination.hasattr("write")? {
        return Err(PyTypeError::new_err(
            "destination must be a binary file-like object with write()",
        ));
    }
    let mut writer = PyWriter {
        target: destination.clone().unbind(),
        failure: None,
    };
    let outcome = py.detach(|| copy(&mut writer));
    if let Some(error) = writer.failure.take() {
        return Err(error);
    }
    outcome.map_err(core_error)
}

/// Manifest as exact bytes: `str` is encoded, `bytes` is borrowed unchanged.
fn manifest_input<'a>(manifest_json: &'a Bound<'_, PyAny>) -> PyResult<Cow<'a, [u8]>> {
    if let Ok(text) = manifest_json.extract::<String>() {
        return Ok(Cow::Owned(text.into_bytes()));
    }
    if let Ok(bytes) = manifest_json.cast::<PyBytes>() {
        return Ok(Cow::Borrowed(bytes.as_bytes()));
    }
    Err(PyTypeError::new_err("manifest_json must be str or bytes"))
}

/// A member payload for the streaming writer: borrowed bytes, or a file that
/// Core reads itself (never loaded into Python).
enum Payload<'a> {
    Bytes(&'a [u8]),
    Path(PathBuf),
}

fn payload<'a>(item: &'a Bound<'_, PyAny>) -> PyResult<Payload<'a>> {
    if let Ok(bytes) = item.cast::<PyBytes>() {
        return Ok(Payload::Bytes(bytes.as_bytes()));
    }
    if item.is_instance_of::<PyString>() {
        return Err(PyTypeError::new_err(
            "a member must be bytes or an os.PathLike; a str is ambiguous",
        ));
    }
    item.extract::<PathBuf>()
        .map(Payload::Path)
        .map_err(|_| PyTypeError::new_err("a member must be bytes or an os.PathLike"))
}

/// The immutable base of an update: either kind of container handle.
enum BaseHandle<'a> {
    File(N4aFile),
    Owned(&'a OwnedN4a),
}

fn base_input<'a>(base: &'a Bound<'_, PyAny>) -> PyResult<BaseHandle<'a>> {
    if let Ok(file) = base.cast::<N4aFileHandle>() {
        return Ok(BaseHandle::File(file.get().file()?));
    }
    if let Ok(container) = base.cast::<N4aContainerHandle>() {
        return Ok(BaseHandle::Owned(&container.get().inner));
    }
    Err(PyTypeError::new_err(
        "base must be an N4aFile or an N4aContainer",
    ))
}

#[allow(clippy::too_many_arguments)]
fn replay_json_input(
    request_json: &str,
    data_envelopes_json: &str,
    methods_inputs_json: &str,
    methods_library_path: &str,
    outcome_id: &str,
    run_id: &str,
    warnings_json: &str,
    diagnostics_json: &str,
) -> MethodsArchiveReplayJsonRequest {
    MethodsArchiveReplayJsonRequest {
        request_json: request_json.to_owned(),
        data_envelopes_json: data_envelopes_json.to_owned(),
        methods_inputs_json: methods_inputs_json.to_owned(),
        methods_library_path: PathBuf::from(methods_library_path),
        outcome_id: outcome_id.to_owned(),
        run_id: run_id.to_owned(),
        warnings_json: warnings_json.to_owned(),
        diagnostics_json: diagnostics_json.to_owned(),
    }
}

/// One retained native proof, selected binding and original immutable source.
/// This process-local handle has no persisted schema or scientific validator.
struct NativePackageState {
    archive: LoadedNativePackage,
    file: N4aFile,
}

#[pyclass(name = "NativePackage", frozen, from_py_object)]
#[derive(Clone)]
struct NativePackageHandle {
    state: Arc<NativePackageState>,
    budget: N4aMemoryBudget,
}

impl std::ops::Deref for NativePackageHandle {
    type Target = LoadedNativePackage;
    fn deref(&self) -> &Self::Target {
        &self.state.archive
    }
}

fn native_view<'py>(py: Python<'py>, archive: &LoadedNativePackage) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?
        .call_method1("loads", (archive.view().to_string(),))
}

fn native_payloads<'py>(
    py: Python<'py>,
    archive: &LoadedNativePackage,
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item(
        "manifest",
        py.import("json")?.call_method1(
            "loads",
            (serde_json::to_string(archive.manifest()).map_err(archive_error)?,),
        )?,
    )?;
    let members = PyDict::new(py);
    for (path, bytes) in archive.members() {
        members.set_item(path, PyBytes::new(py, bytes))?;
    }
    result.set_item("members", members)?;
    result.set_item(
        "package",
        PyBytes::new(py, archive.closure().root().original_bytes()),
    )?;
    if let Some(evidence) = archive.closure().evidence() {
        result.set_item(
            "training_outcome",
            PyBytes::new(py, evidence.original_bytes()),
        )?;
    } else {
        result.set_item("training_outcome", py.None())?;
    }
    result.set_item("view", native_view(py, archive)?)?;
    Ok(result)
}

#[pymethods]
impl NativePackageHandle {
    fn view<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        native_view(py, &self.state.archive)
    }
    fn package_bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.closure().root().original_bytes())
    }
    fn training_outcome_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        Ok(PyBytes::new(
            py,
            self.state
                .archive
                .training_outcome_bytes()
                .map_err(archive_error)?,
        ))
    }
    fn payloads<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        native_payloads(py, &self.state.archive)
    }
}

fn native_package_for_operation(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    operation: &str,
    memory_budget_bytes: Option<u64>,
) -> PyResult<NativePackageHandle> {
    if let Ok(handle) = path.cast::<NativePackageHandle>() {
        let retained = handle.get();
        retained
            .require_operation(operation)
            .map_err(archive_error)?;
        if entrypoint.is_some_and(|id| id != retained.entrypoint().id.as_str()) {
            return Err(archive_error(
                "explicit entrypoint differs from retained native admission",
            ));
        }
        let budget = memory_budget_bytes
            .map(N4aMemoryBudget::bytes)
            .unwrap_or(retained.budget);
        retained
            .admit_memory_budget(budget)
            .map_err(archive_error)?;
        return Ok(NativePackageHandle {
            state: Arc::clone(&retained.state),
            budget,
        });
    }
    let (file, budget) = native_file_input(py, path, memory_budget_bytes)?;
    let entrypoint = entrypoint.map(str::to_owned);
    let operation = operation.to_owned();
    py.detach(move || {
        let archive =
            read_native_package_from_file(&file, entrypoint.as_deref(), &operation, budget)
                .map_err(archive_error)?;
        Ok(NativePackageHandle {
            state: Arc::new(NativePackageState { archive, file }),
            budget,
        })
    })
}

fn native_package_input(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<NativePackageHandle> {
    native_package_for_operation(
        py,
        path,
        entrypoint,
        "predict-external",
        memory_budget_bytes,
    )
}

#[pyfunction]
#[pyo3(signature = (path, *, entrypoint=None, operation="predict-external", memory_budget_bytes=None))]
fn open_native_package(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    operation: &str,
    memory_budget_bytes: Option<u64>,
) -> PyResult<NativePackageHandle> {
    native_package_for_operation(py, path, entrypoint, operation, memory_budget_bytes)
}

#[pyfunction]
#[pyo3(signature = (path, *, entrypoint=None, operation="predict-external", memory_budget_bytes=None))]
fn read_native_package_view<'py>(
    py: Python<'py>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    operation: &str,
    memory_budget_bytes: Option<u64>,
) -> PyResult<Bound<'py, PyAny>> {
    let handle =
        native_package_for_operation(py, path, entrypoint, operation, memory_budget_bytes)?;
    native_view(py, &handle.state.archive)
}

fn native_file_input(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<(N4aFile, N4aMemoryBudget)> {
    let file = if let Ok(handle) = path.cast::<N4aFileHandle>() {
        handle.get().file()?
    } else {
        let path: PathBuf = path
            .extract()
            .map_err(|_| PyTypeError::new_err("path must be os.PathLike or an N4aFile"))?;
        py.detach(move || core_open_n4a_file(&path, memory_budget(memory_budget_bytes)))
            .map_err(|error| archive_error(native_package_transport_error(error)))?
    };
    let budget = memory_budget_bytes
        .map(N4aMemoryBudget::bytes)
        .unwrap_or_else(|| file.memory_budget());
    Ok((file, budget))
}

#[pyfunction]
#[pyo3(signature = (path, *, entrypoint=None, memory_budget_bytes=None))]
fn read_portable_predictor_package_v2<'py>(
    py: Python<'py>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    Ok(PyBytes::new(
        py,
        archive
            .portable_predictor_package()
            .map_err(archive_error)?,
    ))
}

#[pyfunction]
#[pyo3(signature = (path, *, entrypoint=None, memory_budget_bytes=None))]
fn read_portable_refit_package_v3<'py>(
    py: Python<'py>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    Ok(PyBytes::new(
        py,
        archive.portable_refit_package().map_err(archive_v3_error)?,
    ))
}

#[pyfunction]
#[pyo3(signature = (path, *, entrypoint=None, memory_budget_bytes=None))]
fn read_archive_v2_payloads<'py>(
    py: Python<'py>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<Bound<'py, PyDict>> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    archive.predictor_package().map_err(archive_error)?;
    native_payloads(py, &archive.state.archive)
}

#[pyfunction]
#[pyo3(signature = (path, *, entrypoint=None, memory_budget_bytes=None))]
fn read_archive_v3_view(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<Py<PyAny>> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    archive.refit_package().map_err(archive_v3_error)?;
    Ok(py
        .import("json")?
        .call_method1("loads", (archive.state.archive.view().to_string(),))?
        .unbind())
}

/// Inspect every native predictor in a validated Archive V2.
///
/// The descriptor JSON is emitted from DAG-ML's typed contract after Core has
/// attested the exact libn4m bytes and Methods has inspected each complete
/// N4MM payload. Python neither supplies nor reconstructs descriptor fields.
#[pyfunction]
#[pyo3(signature = (path, methods_library_path, methods_library_sha256, *, entrypoint=None, memory_budget_bytes=None))]
fn inspect_methods_archive_v2_predictors_json(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    methods_library_path: &str,
    methods_library_sha256: &str,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    let library = PathBuf::from(methods_library_path);
    let digest = methods_library_sha256.to_owned();
    py.detach(move || {
        let descriptors = inspect_methods_archive_v2_predictors(&archive, &library, &digest)
            .map_err(replay_error)?;
        serde_json::to_string(&descriptors).map_err(replay_error)
    })
}

/// Replay one Core-validated Archive V2 through DAG-ML's callback-free Methods
/// runtime. All structured values are strict JSON contracts; no Python object,
/// estimator handle, callback, or joblib payload crosses this boundary.
#[pyfunction]
#[pyo3(signature = (
    path,
    request_json,
    data_envelopes_json,
    methods_inputs_json,
    methods_library_path,
    outcome_id,
    run_id,
    warnings_json = "[]",
    diagnostics_json = "{}",
    *, entrypoint=None, memory_budget_bytes=None
))]
#[allow(clippy::too_many_arguments)]
fn replay_methods_archive_v2_json(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    request_json: &str,
    data_envelopes_json: &str,
    methods_inputs_json: &str,
    methods_library_path: &str,
    outcome_id: &str,
    run_id: &str,
    warnings_json: &str,
    diagnostics_json: &str,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    let input = replay_json_input(
        request_json,
        data_envelopes_json,
        methods_inputs_json,
        methods_library_path,
        outcome_id,
        run_id,
        warnings_json,
        diagnostics_json,
    );
    py.detach(move || core_replay_methods_archive_v2_json(&archive, input))
        .map_err(replay_error)
}

/// Execute Core's closed X-only Archive V2 matrix prediction surface.
///
/// Core owns request composition, target-order validation, libn4m SHA-256 and
/// ABI attestation, the private library snapshot and replay outcome checks.
#[pyfunction]
#[pyo3(signature = (path, input_json, *, entrypoint=None, memory_budget_bytes=None))]
fn predict_methods_archive_v2_matrix_json(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    input_json: &str,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    let input_json = input_json.to_owned();
    py.detach(move || core_predict_methods_archive_v2_matrix_json(&archive, &input_json))
        .map_err(replay_error)
}

#[pyfunction]
#[pyo3(signature = (path, input_json, *, entrypoint=None, memory_budget_bytes=None))]
fn predict_tuning_archive_v2_matrix_json(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    input_json: &str,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    let input_json = input_json.to_owned();
    py.detach(move || core_predict_tuning_archive_v2_matrix_json(&archive, &input_json))
        .map_err(replay_error)
}

/// Replay one calibrated Archive V2 and return DAG-ML's self-validating
/// conformal presentation. Python transports JSON only; it never calculates
/// residual quantiles, interval endpoints, fingerprints, or sample joins.
#[pyfunction]
#[pyo3(signature = (
    path,
    request_json,
    data_envelopes_json,
    methods_inputs_json,
    methods_library_path,
    outcome_id,
    run_id,
    warnings_json = "[]",
    diagnostics_json = "{}",
    *, entrypoint=None, memory_budget_bytes=None
))]
#[allow(clippy::too_many_arguments)]
fn replay_methods_archive_v2_conformal_presentation_v1_json(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    request_json: &str,
    data_envelopes_json: &str,
    methods_inputs_json: &str,
    methods_library_path: &str,
    outcome_id: &str,
    run_id: &str,
    warnings_json: &str,
    diagnostics_json: &str,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    let input = replay_json_input(
        request_json,
        data_envelopes_json,
        methods_inputs_json,
        methods_library_path,
        outcome_id,
        run_id,
        warnings_json,
        diagnostics_json,
    );
    py.detach(move || {
        core_replay_methods_archive_v2_conformal_presentation_v1_json(&archive, input)
    })
    .map_err(replay_error)
}

/// Replay one calibrated Archive V2 and return DAG-ML's complete,
/// content-bound multi-target conformal presentation V2. Python transports
/// strict JSON only and cannot reconstruct predictor or archive fingerprints.
#[pyfunction]
#[pyo3(signature = (
    path,
    request_json,
    data_envelopes_json,
    methods_inputs_json,
    methods_library_path,
    outcome_id,
    run_id,
    warnings_json = "[]",
    diagnostics_json = "{}",
    *, entrypoint=None, memory_budget_bytes=None
))]
#[allow(clippy::too_many_arguments)]
fn replay_methods_archive_v2_conformal_presentation_v2_json(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    request_json: &str,
    data_envelopes_json: &str,
    methods_inputs_json: &str,
    methods_library_path: &str,
    outcome_id: &str,
    run_id: &str,
    warnings_json: &str,
    diagnostics_json: &str,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    let input = replay_json_input(
        request_json,
        data_envelopes_json,
        methods_inputs_json,
        methods_library_path,
        outcome_id,
        run_id,
        warnings_json,
        diagnostics_json,
    );
    py.detach(move || {
        core_replay_methods_archive_v2_conformal_presentation_v2_json(&archive, input)
    })
    .map_err(replay_error)
}

/// Replay one Core-validated Archive V3 through a fresh Methods-only runtime.
/// Supplemental host controllers are deliberately unavailable from Python.
#[pyfunction]
#[pyo3(signature = (
    path,
    request_json,
    data_envelopes_json,
    methods_inputs_json,
    methods_library_path,
    outcome_id,
    run_id,
    warnings_json = "[]",
    diagnostics_json = "{}",
    *, entrypoint=None, memory_budget_bytes=None
))]
#[allow(clippy::too_many_arguments)]
fn replay_methods_archive_v3_json(
    py: Python<'_>,
    path: &Bound<'_, PyAny>,
    request_json: &str,
    data_envelopes_json: &str,
    methods_inputs_json: &str,
    methods_library_path: &str,
    outcome_id: &str,
    run_id: &str,
    warnings_json: &str,
    diagnostics_json: &str,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let archive = native_package_input(py, path, entrypoint, memory_budget_bytes)?;
    let input = replay_json_input(
        request_json,
        data_envelopes_json,
        methods_inputs_json,
        methods_library_path,
        outcome_id,
        run_id,
        warnings_json,
        diagnostics_json,
    );
    py.detach(move || core_replay_methods_archive_v3_json(&archive, input))
        .map_err(replay_error)
}

/// Write exact owner documents through Core V1, without replacing a destination.
#[pyfunction]
#[pyo3(signature = (path, outcome, package, *, container_id, memory_budget_bytes=None))]
fn write_native_predictor_package(
    py: Python<'_>,
    path: PathBuf,
    outcome: Vec<u8>,
    package: Vec<u8>,
    container_id: String,
    memory_budget_bytes: Option<u64>,
) -> PyResult<(String, String)> {
    py.detach(move || {
        let reference = write_native_package_file(
            &path,
            Some(&outcome),
            &package,
            &container_id,
            memory_budget(memory_budget_bytes),
        )
        .map_err(archive_error)?;
        Ok((
            reference.archive_id().to_owned(),
            reference.archive_sha256().to_owned(),
        ))
    })
}

#[pyfunction]
#[pyo3(signature = (path, package, *, container_id, memory_budget_bytes=None))]
fn write_native_refit_package(
    py: Python<'_>,
    path: PathBuf,
    package: Vec<u8>,
    container_id: String,
    memory_budget_bytes: Option<u64>,
) -> PyResult<(String, String)> {
    py.detach(move || {
        let reference = write_native_package_file(
            &path,
            None,
            &package,
            &container_id,
            memory_budget(memory_budget_bytes),
        )
        .map_err(archive_v3_error)?;
        Ok((
            reference.archive_id().to_owned(),
            reference.archive_sha256().to_owned(),
        ))
    })
}

/// Persist a DAG-produced calibration while retaining the exact immutable base.
#[pyfunction]
#[pyo3(signature = (path, base, outcome, package, *, container_id, entrypoint=None, memory_budget_bytes=None))]
#[allow(clippy::too_many_arguments)]
fn write_calibrated_native_package(
    py: Python<'_>,
    path: PathBuf,
    base: &Bound<'_, PyAny>,
    outcome: Vec<u8>,
    package: Vec<u8>,
    container_id: String,
    entrypoint: Option<&str>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<(String, String)> {
    let before = native_package_for_operation(
        py,
        base,
        entrypoint,
        "calibrate-external",
        memory_budget_bytes,
    )?;
    py.detach(move || {
        let reference = write_calibrated_native_package_file(
            &path,
            &before.state.file,
            &before.state.archive,
            &outcome,
            &package,
            &container_id,
            before.budget,
        )
        .map_err(archive_error)?;
        Ok((
            reference.archive_id().to_owned(),
            reference.archive_sha256().to_owned(),
        ))
    })
}

/// Train once through Core's native producer and return the sealed immutable
/// n4a bytes together with the view of that same fit. The recipe is parsed by
/// the owner's strict parser; nothing is trained, replayed or reopened again.
/// `memory_budget_bytes=None` leaves only the allocator and host limits; a
/// number is the authorized Core working set, never a content-size cap.
#[pyfunction]
#[pyo3(signature = (
    record,
    source_id,
    recipe_json,
    *,
    methods_library_path,
    methods_library_sha256,
    run_id,
    container_id,
    embed_training_record = true,
    memory_budget_bytes = None
))]
#[allow(clippy::too_many_arguments)]
fn produce_n4a_native_with_view<'py>(
    py: Python<'py>,
    record: &[u8],
    source_id: &str,
    recipe_json: &str,
    methods_library_path: &str,
    methods_library_sha256: &str,
    run_id: &str,
    container_id: &str,
    embed_training_record: bool,
    memory_budget_bytes: Option<u64>,
) -> PyResult<(Bound<'py, PyBytes>, String)> {
    let budget = memory_budget(memory_budget_bytes);
    let (archive, view) = py
        .detach(move || -> Result<(Box<[u8]>, String), String> {
            let recipe =
                parse_native_recipe_json(recipe_json, budget).map_err(|e| e.to_string())?;
            let produced = core_produce_n4a_native_with_view(NativeProduceRequest {
                record,
                source_id,
                recipe: &recipe,
                runtime: NativeRuntime {
                    library: Path::new(methods_library_path),
                    expected_sha256: methods_library_sha256,
                },
                run_id,
                container_id,
                embed_training_record,
                memory_budget: budget,
            })
            .map_err(|e| e.to_string())?;
            let view = n4a_view_json(&produced.container, &produced.training)?;
            // Move the sealed allocation out; the only copy left is into PyBytes.
            Ok((produced.container.into_archive_bytes(), view))
        })
        .map_err(|detail| n4a_error("production", detail))?;
    Ok((
        materialize(py, &archive, archive.len() as u64, budget)?,
        view,
    ))
}

/// Retrain a stored model on new IO record bytes. Recipe and source come from
/// the validated owner package; the producer fits once and seals a new archive.
#[pyfunction]
#[pyo3(signature = (
    archive_bytes,
    *,
    record,
    methods_library_path,
    methods_library_sha256,
    run_id,
    container_id,
    embed_training_record = true,
    memory_budget_bytes = None
))]
#[allow(clippy::too_many_arguments)]
fn retrain_n4a_native_with_view<'py>(
    py: Python<'py>,
    archive_bytes: &Bound<'_, PyAny>,
    record: &[u8],
    methods_library_path: &str,
    methods_library_sha256: &str,
    run_id: &str,
    container_id: &str,
    embed_training_record: bool,
    memory_budget_bytes: Option<u64>,
) -> PyResult<(Bound<'py, PyBytes>, String)> {
    let (archive_input, budget) = native_archive_input(archive_bytes, memory_budget_bytes)?;
    let (archive, view) = py
        .detach(move || -> Result<(Box<[u8]>, String), String> {
            let container = archive_input
                .into_owned(budget)
                .map_err(|e| e.to_string())?;
            let produced = core_retrain_n4a_native(
                &container,
                NativeRetrainRequest {
                    record,
                    runtime: NativeRuntime {
                        library: Path::new(methods_library_path),
                        expected_sha256: methods_library_sha256,
                    },
                    run_id,
                    container_id,
                    embed_training_record,
                    memory_budget: budget,
                },
            )
            .map_err(|e| e.to_string())?;
            let view = n4a_view_json(&produced.container, &produced.training)?;
            Ok((produced.container.into_archive_bytes(), view))
        })
        .map_err(|detail| n4a_error("retraining", detail))?;
    Ok((
        materialize(py, &archive, archive.len() as u64, budget)?,
        view,
    ))
}

/// Inspect bytes or an open Core N4aFile as a closed native-produced model.
/// Files reuse validated metadata and verify the bytes actually materialized.
#[pyfunction]
#[pyo3(signature = (archive_bytes, *, memory_budget_bytes = None))]
fn inspect_n4a_bytes(
    py: Python<'_>,
    archive_bytes: &Bound<'_, PyAny>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let (archive_input, budget) = native_archive_input(archive_bytes, memory_budget_bytes)?;
    py.detach(move || -> Result<String, String> {
        let container = archive_input
            .into_owned(budget)
            .map_err(|e| e.to_string())?;
        let training = read_n4a_training_view(&container).map_err(|e| e.to_string())?;
        n4a_view_json(&container, &training)
    })
    .map_err(|detail| n4a_error("inspection", detail))
}

/// Replay one n4a entrypoint through Core's owner admission. `record=None`
/// selects the stored input; bytes select an external record. The training
/// schema policy never ignores a present bad anchor. `sample_ids` are the
/// selected partition's IDs in IO record order, outside the signed replay.
#[pyfunction]
#[pyo3(signature = (
    archive_bytes,
    *,
    record,
    entrypoint,
    source_id,
    partition,
    request_id,
    run_id,
    methods_library_path,
    methods_library_sha256,
    require_training_schema = true,
    memory_budget_bytes = None
))]
#[allow(clippy::too_many_arguments)]
fn predict_n4a_native_json(
    py: Python<'_>,
    archive_bytes: &Bound<'_, PyAny>,
    record: Option<&[u8]>,
    entrypoint: &str,
    source_id: &str,
    partition: &str,
    request_id: &str,
    run_id: &str,
    methods_library_path: &str,
    methods_library_sha256: &str,
    require_training_schema: bool,
    memory_budget_bytes: Option<u64>,
) -> PyResult<String> {
    let (archive_input, budget) = native_archive_input(archive_bytes, memory_budget_bytes)?;
    py.detach(move || -> Result<String, String> {
        let container = archive_input
            .into_owned(budget)
            .map_err(|e| e.to_string())?;
        let prediction = core_predict_n4a_native(
            &container,
            NativeActionRequest {
                entrypoint,
                input: match record {
                    Some(bytes) => NativeInput::External(bytes),
                    None => NativeInput::Stored,
                },
                source_id,
                partition,
                schema_policy: if require_training_schema {
                    NativeSchemaPolicy::RequireTrainingSchema
                } else {
                    NativeSchemaPolicy::AllowUnavailable
                },
                request_id,
                run_id,
                memory_budget: budget,
                runtime: NativeRuntime {
                    library: Path::new(methods_library_path),
                    expected_sha256: methods_library_sha256,
                },
            },
        )
        .map_err(|e| e.to_string())?;
        let encoded = serde_json::json!({
            "replay": serde_json::to_value(&prediction.replay).map_err(|e| e.to_string())?,
            "receipt": serde_json::to_value(&prediction.receipt).map_err(|e| e.to_string())?,
            "sample_ids": prediction.sample_ids,
        });
        serde_json::to_string(&encoded).map_err(|e| e.to_string())
    })
    .map_err(|detail| n4a_error("prediction", detail))
}

#[pyfunction]
fn project_training_predictions_json(
    outcome_json: &str,
    dataset_label: &str,
    task_type_label: &str,
) -> PyResult<String> {
    nirs4all::result_projection::project_training_predictions_json(
        outcome_json,
        dataset_label,
        task_type_label,
    )
    .map_err(PyValueError::new_err)
}

#[pyfunction]
fn validate_training_predictions_json(outcome_json: &str, predictions_json: &str) -> PyResult<()> {
    nirs4all::result_projection::validate_training_predictions_json(outcome_json, predictions_json)
        .map_err(PyValueError::new_err)
}

#[pyfunction]
fn publish_directory_noreplace(source: &str, target: &str) -> PyResult<()> {
    nirs4all::publish_directory_noreplace(Path::new(source), Path::new(target))
        .map_err(PyValueError::new_err)
}

/// Shared Rust dispatcher used when no external CLI was requested.
#[pyfunction]
fn execute_archive_command(args: Vec<String>) -> PyResult<()> {
    nirs4all::archive_command::execute_archive_command(
        args.into_iter().map(std::ffi::OsString::from),
    )
    .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))
}

/// Immutable validated owner-neutral `.n4a` container held in memory.
///
/// Opening validates the physical layout, manifest and every raw size/SHA once.
/// Member reads are then plain slices of the validated buffer: nothing is hashed,
/// parsed or deserialized again, and host payloads are never interpreted. Every
/// `bytes` result is a materialization admitted against the handle's budget.
#[pyclass(frozen, name = "N4aContainer", module = "nirs4all_core._native")]
struct N4aContainerHandle {
    inner: OwnedN4a,
}

#[pymethods]
impl N4aContainerHandle {
    /// Owner-neutral readability report (`readability` + `executability`).
    fn view<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let view = py.detach(|| n4a_container_view(&self.inner));
        pythonize::pythonize(py, &view).map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// The exact manifest bytes stored in the container.
    fn manifest_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        materialize(
            py,
            self.inner.manifest_bytes(),
            self.inner.archive_bytes().len() as u64,
            self.inner.memory_budget(),
        )
    }

    /// The exact bytes of one member, or `KeyError` when it is not in the inventory.
    fn member<'py>(&self, py: Python<'py>, path: &str) -> PyResult<Bound<'py, PyBytes>> {
        let view = self
            .inner
            .member(path)
            .ok_or_else(|| PyKeyError::new_err(path.to_owned()))?;
        materialize(
            py,
            view.bytes,
            self.inner.archive_bytes().len() as u64,
            self.inner.memory_budget(),
        )
    }

    /// Embedded member bytes of one artifact by archive path (empty for a
    /// reference artifact), or `KeyError` for an unknown artifact.
    fn artifact_members<'py>(
        &self,
        py: Python<'py>,
        artifact_id: &str,
    ) -> PyResult<Bound<'py, PyDict>> {
        let views = self
            .inner
            .artifact_members(artifact_id)
            .ok_or_else(|| PyKeyError::new_err(artifact_id.to_owned()))?;
        let budget = self.inner.memory_budget();
        let resident = self.inner.archive_bytes().len() as u64;
        let total = views.iter().try_fold(resident, |total, view| {
            total
                .checked_add(view.bytes.len() as u64)
                .ok_or_else(|| core_error(N4aError::Memory("working set size overflow".into())))
        })?;
        budget.admit(total).map_err(core_error)?;
        let members = PyDict::new(py);
        for view in views {
            members.set_item(view.path, materialize(py, view.bytes, resident, budget)?)?;
        }
        Ok(members)
    }

    /// The exact original container bytes.
    fn archive_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        materialize(
            py,
            self.inner.archive_bytes(),
            self.inner.archive_bytes().len() as u64,
            self.inner.memory_budget(),
        )
    }
}

/// Validate caller bytes once and return an immutable container handle.
/// The handle owns one copy of `archive`, admitted against the budget.
#[pyfunction]
#[pyo3(signature = (archive, *, memory_budget_bytes = None))]
fn open_n4a_container(
    py: Python<'_>,
    archive: &[u8],
    memory_budget_bytes: Option<u64>,
) -> PyResult<N4aContainerHandle> {
    let budget = memory_budget(memory_budget_bytes);
    let inner = py
        .detach(move || open_caller_bytes(archive, budget))
        .map_err(core_error)?;
    Ok(N4aContainerHandle { inner })
}

/// File-backed validated owner-neutral `.n4a` container.
///
/// Opening makes one sequential verified pass and keeps only the validated index
/// in memory; payload stays on disk. The file is not locked, so nothing is
/// trusted from the open: every read below hashes the bytes it actually returns
/// and compares them with the validated receipt, and a changed or truncated file
/// is refused with `ValueError`. `close()` (or leaving a `with` block) releases
/// the handle; streams already opened keep their own reference until closed.
#[pyclass(frozen, name = "N4aFile", module = "nirs4all_core._native")]
struct N4aFileHandle {
    inner: RwLock<Option<N4aFile>>,
}

impl N4aFileHandle {
    /// Cheap shared reference to the open file; the lock is never held across I/O.
    fn file(&self) -> PyResult<N4aFile> {
        let guard = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        (*guard)
            .clone()
            .ok_or_else(|| closed_error("n4a file handle"))
    }
}

#[pymethods]
impl N4aFileHandle {
    /// Owner-neutral readability report. DAG-owned members are read and verified
    /// on demand; a failed read is reported as `refused`, never passed silently.
    fn view<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let file = self.file()?;
        let view = py.detach(|| n4a_container_view(&file));
        pythonize::pythonize(py, &view).map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// The exact manifest bytes stored in the container.
    fn manifest_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let file = self.file()?;
        materialize(
            py,
            file.manifest_bytes(),
            file.manifest_bytes().len() as u64,
            file.memory_budget(),
        )
    }

    /// Archive paths declared by one artifact (empty for a reference artifact),
    /// or `KeyError` for an unknown artifact. No payload is read.
    fn artifact_member_paths(&self, artifact_id: &str) -> PyResult<Vec<String>> {
        let file = self.file()?;
        file.artifact_member_paths(artifact_id)
            .map(<[String]>::to_vec)
            .ok_or_else(|| PyKeyError::new_err(artifact_id.to_owned()))
    }

    /// The exact bytes of one member, verified to its end before they are
    /// returned. `KeyError` for an unknown path. `memory_budget_bytes` overrides
    /// the handle's budget for this materialization only.
    #[pyo3(signature = (path, *, memory_budget_bytes = None))]
    fn member<'py>(
        &self,
        py: Python<'py>,
        path: &str,
        memory_budget_bytes: Option<u64>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let file = self.file()?;
        let budget = match memory_budget_bytes {
            Some(bytes) => N4aMemoryBudget::bytes(bytes),
            None => file.memory_budget(),
        };
        let mut reader = member_reader(&file, path)?;
        let length = reader.byte_length();
        read_into_bytes(py, &mut reader, length, budget)
    }

    /// Open one member as a binary, non-seekable stream (`read`, `readinto`).
    /// Bytes are unverified until the call that delivers the last byte returns;
    /// that call raises `ValueError` and withholds the bytes if the member does
    /// not match the validated receipt.
    fn open_member(&self, path: &str) -> PyResult<N4aMemberStream> {
        let file = self.file()?;
        let reader = member_reader(&file, path)?;
        Ok(N4aMemberStream {
            reader: Mutex::new(Some(reader)),
            budget: file.memory_budget(),
        })
    }

    /// Stream one member into a binary file-like `destination` (`write(bytes)`)
    /// and return the byte count. Verification happens during this one read; on
    /// `ValueError` the destination already holds unverified bytes and the
    /// caller must discard it.
    fn copy_member_to(
        &self,
        py: Python<'_>,
        path: &str,
        destination: &Bound<'_, PyAny>,
    ) -> PyResult<u64> {
        let file = self.file()?;
        if file.member_location(path).is_none() {
            return Err(PyKeyError::new_err(path.to_owned()));
        }
        copy_to(py, destination, |writer| file.copy_member_to(path, writer))
    }

    /// Stream the exact archive bytes into `destination` and return the byte
    /// count, checked against the validated container SHA-256 as written. The
    /// same discard rule as `copy_member_to` applies on `ValueError`.
    fn copy_archive_to(&self, py: Python<'_>, destination: &Bound<'_, PyAny>) -> PyResult<u64> {
        let file = self.file()?;
        copy_to(py, destination, |writer| file.copy_archive_to(writer))
    }

    /// Release this handle. Later calls raise `ValueError`.
    fn close(&self) {
        *self.inner.write().unwrap_or_else(PoisonError::into_inner) = None;
    }

    #[getter]
    fn closed(&self) -> bool {
        self.inner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (*_exc))]
    fn __exit__(&self, _exc: &Bound<'_, PyTuple>) {
        self.close();
    }
}

/// Binary read-only stream over one member of an `N4aFile`.
///
/// `read(size)` and `readinto(buffer)` follow `io.RawIOBase`, so
/// `io.BufferedReader(stream)` works. The call that would deliver the last byte
/// verifies the whole member first and raises `ValueError` instead of returning
/// those bytes; before that, bytes are unverified, and a stream abandoned early
/// has verified nothing (see `verified`).
#[pyclass(name = "N4aMemberStream", module = "nirs4all_core._native")]
struct N4aMemberStream {
    // The Mutex only makes the reader `Sync`; PyO3's borrow flag already
    // serializes access and `get_mut` never blocks.
    reader: Mutex<Option<N4aMemberReader>>,
    budget: N4aMemoryBudget,
}

impl N4aMemberStream {
    fn reader_mut(&mut self) -> PyResult<&mut N4aMemberReader> {
        self.reader
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .ok_or_else(|| closed_error("n4a member stream"))
    }
}

#[pymethods]
impl N4aMemberStream {
    /// Up to `size` bytes (all remaining when omitted or negative).
    #[pyo3(signature = (size = None))]
    fn read<'py>(&mut self, py: Python<'py>, size: Option<i64>) -> PyResult<Bound<'py, PyBytes>> {
        let budget = self.budget;
        let reader = self.reader_mut()?;
        let remaining = reader.remaining();
        let wanted = match size {
            Some(count) if count >= 0 => (count as u64).min(remaining),
            _ => remaining,
        };
        read_into_bytes(py, reader, wanted, budget)
    }

    /// All remaining bytes, verified.
    fn readall<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.read(py, None)
    }

    /// Fill `buffer` with at most one chunk of the member and return the count;
    /// `0` means end of member (and that it was verified).
    fn readinto(&mut self, py: Python<'_>, buffer: &Bound<'_, PyAny>) -> PyResult<usize> {
        let budget = self.budget;
        let reader = self.reader_mut()?;
        let target = PyBuffer::<u8>::get(buffer)?;
        if target.readonly() {
            return Err(PyTypeError::new_err(
                "readinto() needs a writable bytes-like object",
            ));
        }
        let wanted = (target.item_count() as u64)
            .min(reader.remaining())
            .min(budget.chunk_bytes());
        if wanted == 0 {
            settle(py, reader)?;
            return Ok(0);
        }
        let size = usize::try_from(wanted).map_err(not_addressable)?;
        let mut scratch = Vec::new();
        scratch.try_reserve_exact(size).map_err(|error| {
            core_error(N4aError::Memory(format!(
                "cannot allocate {size} bytes: {error}"
            )))
        })?;
        scratch.resize(size, 0);
        let mut filled = 0;
        while filled < size {
            let count = read_detached(py, reader, &mut scratch[filled..])?;
            if count == 0 {
                return Err(ended_early());
            }
            filled += count;
        }
        // `buffer` is already visible to the caller, so no read happens while the
        // GIL is released against it: the reader fills a bounded Rust scratch
        // (detached), and only that verified scratch is copied under the GIL.
        // Hence the final bytes of a member that failed verification never reach
        // the caller's buffer. The cost is one copy of at most one chunk. The
        // `target` export is held across the detached reads only so the buffer
        // cannot be resized meanwhile; its memory is not read or written until
        // the copy below, which runs with the GIL held.
        let view = py
            .import("builtins")?
            .getattr("memoryview")?
            .call1((buffer,))?
            .get_item(PySlice::new(py, 0, size as isize, 1))?;
        PyBuffer::<u8>::get(&view)?.copy_from_slice(py, &scratch)?;
        settle(py, reader)?;
        Ok(size)
    }

    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        false
    }

    fn seekable(&self) -> bool {
        false
    }

    /// A read-only stream has nothing to flush; closed streams still refuse.
    fn flush(&mut self) -> PyResult<()> {
        self.reader_mut()?;
        Ok(())
    }

    /// Bytes delivered so far.
    fn tell(&mut self) -> PyResult<u64> {
        let reader = self.reader_mut()?;
        Ok(reader.byte_length() - reader.remaining())
    }

    /// Total member size in bytes.
    #[getter]
    fn byte_length(&mut self) -> PyResult<u64> {
        Ok(self.reader_mut()?.byte_length())
    }

    /// Bytes not yet delivered.
    #[getter]
    fn remaining(&mut self) -> PyResult<u64> {
        Ok(self.reader_mut()?.remaining())
    }

    /// `True` only after the whole member was delivered and matched the receipt.
    #[getter]
    fn verified(&mut self) -> PyResult<bool> {
        Ok(self.reader_mut()?.is_verified())
    }

    #[getter]
    fn closed(&mut self) -> bool {
        self.reader
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
    }

    fn close(&mut self) {
        *self
            .reader
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (*_exc))]
    fn __exit__(&mut self, _exc: &Bound<'_, PyTuple>) {
        self.close();
    }
}

/// Validated file open: one sequential verified pass, payload stays on disk.
#[pyfunction]
#[pyo3(signature = (path, *, memory_budget_bytes = None))]
fn open_n4a_file(
    py: Python<'_>,
    path: PathBuf,
    memory_budget_bytes: Option<u64>,
) -> PyResult<N4aFileHandle> {
    let budget = memory_budget(memory_budget_bytes);
    let file = py
        .detach(move || core_open_n4a_file(&path, budget))
        .map_err(core_error)?;
    Ok(N4aFileHandle {
        inner: RwLock::new(Some(file)),
    })
}

/// Write a new container from a manifest and member bytes, or from an immutable
/// base plus replacements. Core derives member sizes/hashes and refuses
/// duplicate or undeclared members; the result is always a new byte string.
#[pyfunction]
#[pyo3(signature = (manifest_json, members, *, base = None, memory_budget_bytes = None))]
fn write_n4a_container<'py>(
    py: Python<'py>,
    manifest_json: &Bound<'py, PyAny>,
    members: Vec<(String, Bound<'py, PyBytes>)>,
    base: Option<&Bound<'py, N4aContainerHandle>>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let budget = memory_budget(memory_budget_bytes);
    let manifest_bytes = manifest_input(manifest_json)?;
    let payloads: Vec<(&str, &[u8])> = members
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_bytes()))
        .collect();
    let base: Option<&OwnedN4a> = base.map(|handle| &handle.get().inner);
    let bytes = py
        .detach(|| {
            let manifest = parse_n4a_manifest(&manifest_bytes, budget)?;
            let inputs = payloads
                .iter()
                .map(|&(path, bytes)| N4aMemberInput { path, bytes })
                .collect::<Vec<_>>();
            let (_, bytes) = match base {
                Some(base) => write_n4a_update(base, manifest, inputs, budget)?,
                None => write_n4a_bytes(
                    N4aWriteRequest {
                        manifest,
                        members: inputs,
                    },
                    budget,
                )?,
            };
            Ok::<_, N4aError>(bytes)
        })
        .map_err(core_error)?;
    let resident = (bytes.len() as u64)
        .checked_add(base.map_or(0, |base| base.archive_bytes().len() as u64))
        .ok_or_else(|| core_error(N4aError::Memory("working set size overflow".into())))?;
    materialize(py, &bytes, resident, budget)
}

/// Stream a new container straight to `destination`, from `(path, bytes |
/// os.PathLike)` members, optionally updating an immutable `base` (an
/// `N4aFile` or `N4aContainer`). A path member is read from disk twice (digest
/// pass, write pass) and never held in memory; a source that changes between
/// the passes is refused. The destination is staged and published without
/// replacing anything. Returns `container_sha256`, `manifest_sha256`,
/// `byte_length` and the final receipt `members` (`path`, `byte_length`,
/// `sha256` of every stored member), all as computed while writing.
#[pyfunction]
#[pyo3(signature = (destination, manifest_json, members, *, base = None, memory_budget_bytes = None))]
fn write_n4a_file<'py>(
    py: Python<'py>,
    destination: PathBuf,
    manifest_json: &Bound<'py, PyAny>,
    members: Vec<(String, Bound<'py, PyAny>)>,
    base: Option<&Bound<'py, PyAny>>,
    memory_budget_bytes: Option<u64>,
) -> PyResult<Bound<'py, PyDict>> {
    let budget = memory_budget(memory_budget_bytes);
    let manifest_bytes = manifest_input(manifest_json)?;
    let payloads = members
        .iter()
        .map(|(path, item)| Ok((path.as_str(), payload(item)?)))
        .collect::<PyResult<Vec<(&str, Payload<'_>)>>>()?;
    let base = base.map(base_input).transpose()?;
    let (container_sha256, manifest_sha256, byte_length, receipt_members) = py
        .detach(|| -> Result<_, N4aError> {
            let manifest = parse_n4a_manifest(&manifest_bytes, budget)?;
            let mut sources: Vec<Box<dyn N4aMemberSource + '_>> =
                Vec::with_capacity(payloads.len());
            for (_, payload) in &payloads {
                match payload {
                    Payload::Bytes(bytes) => sources.push(Box::new(*bytes)),
                    Payload::Path(path) => sources.push(Box::new(path.clone())),
                }
            }
            let members = payloads
                .iter()
                .zip(&sources)
                .map(|((path, _), source)| N4aStreamInput {
                    path,
                    source: &**source,
                })
                .collect::<Vec<_>>();
            let base = base.as_ref().map(|base| match base {
                BaseHandle::File(file) => N4aStreamBase::File(file),
                BaseHandle::Owned(container) => N4aStreamBase::Owned(container),
            });
            let receipt = core_write_n4a_file(
                &destination,
                N4aStreamRequest {
                    manifest,
                    members,
                    base,
                },
                budget,
            )?;
            Ok((
                receipt.container_sha256,
                receipt.manifest_sha256,
                receipt.byte_length,
                receipt
                    .members
                    .into_iter()
                    .map(|member| (member.path, member.byte_length, member.sha256))
                    .collect::<Vec<_>>(),
            ))
        })
        .map_err(core_error)?;
    let members = PyList::empty(py);
    for (path, byte_length, sha256) in receipt_members {
        let member = PyDict::new(py);
        member.set_item("path", path)?;
        member.set_item("byte_length", byte_length)?;
        member.set_item("sha256", sha256)?;
        members.append(member)?;
    }
    let result = PyDict::new(py);
    result.set_item("container_sha256", container_sha256)?;
    result.set_item("manifest_sha256", manifest_sha256)?;
    result.set_item("byte_length", byte_length)?;
    result.set_item("members", members)?;
    Ok(result)
}

/// Python extension module installed as ``nirs4all_core._native``.
#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(execute_archive_command, module)?)?;
    module.add_function(wrap_pyfunction!(project_training_predictions_json, module)?)?;
    module.add_function(wrap_pyfunction!(
        validate_training_predictions_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(publish_directory_noreplace, module)?)?;
    module.add_function(wrap_pyfunction!(produce_n4a_native_with_view, module)?)?;
    module.add_function(wrap_pyfunction!(retrain_n4a_native_with_view, module)?)?;
    module.add_function(wrap_pyfunction!(inspect_n4a_bytes, module)?)?;
    module.add_class::<N4aContainerHandle>()?;
    module.add_class::<N4aFileHandle>()?;
    module.add_class::<NativePackageHandle>()?;
    module.add_function(wrap_pyfunction!(open_native_package, module)?)?;
    module.add_function(wrap_pyfunction!(read_native_package_view, module)?)?;
    module.add_class::<N4aMemberStream>()?;
    module.add_function(wrap_pyfunction!(open_n4a_container, module)?)?;
    module.add_function(wrap_pyfunction!(write_n4a_container, module)?)?;
    module.add_function(wrap_pyfunction!(open_n4a_file, module)?)?;
    module.add_function(wrap_pyfunction!(write_n4a_file, module)?)?;
    module.add_function(wrap_pyfunction!(predict_n4a_native_json, module)?)?;
    module.add_function(wrap_pyfunction!(read_archive_v2_payloads, module)?)?;
    module.add_function(wrap_pyfunction!(
        read_portable_predictor_package_v2,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(read_portable_refit_package_v3, module)?)?;
    module.add_function(wrap_pyfunction!(read_archive_v3_view, module)?)?;
    module.add_function(wrap_pyfunction!(write_native_predictor_package, module)?)?;
    module.add_function(wrap_pyfunction!(write_native_refit_package, module)?)?;
    module.add_function(wrap_pyfunction!(write_calibrated_native_package, module)?)?;
    module.add_function(wrap_pyfunction!(
        predict_tuning_archive_v2_matrix_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        inspect_methods_archive_v2_predictors_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(replay_methods_archive_v2_json, module)?)?;
    module.add_function(wrap_pyfunction!(
        predict_methods_archive_v2_matrix_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        replay_methods_archive_v2_conformal_presentation_v1_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        replay_methods_archive_v2_conformal_presentation_v2_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(replay_methods_archive_v3_json, module)?)?;
    Ok(())
}
