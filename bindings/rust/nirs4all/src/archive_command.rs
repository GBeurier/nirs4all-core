//! Native archive transport for hosts that do not embed the Rust aggregate.
//!
//! Core validates the container and passes the original package bytes to DAG-ML.
//! DAG-ML owns package validation, process callbacks, hydration and replay. This
//! Dataset commands delegate construction to IO. Workflow commands compose
//! existing upstream entry points; the binary never interprets numerical state.

use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::archive_view::native_package_view as archive_v2_view;
use crate::n4a::{parse_n4a_manifest, N4aMemoryBudget};
use crate::n4a_file::{
    open_n4a_file, write_n4a_stage, N4aFile, N4aStreamBase, N4aStreamInput, N4aStreamRequest,
};
use crate::n4a_native_package::{open_native_package, LoadedNativePackage};
use crate::n4a_owner_native::{
    parse_native_recipe_json, predict_n4a_native, produce_n4a_native_with_view, retrain_n4a_native,
    NativeActionRequest, NativeInput, NativeProduceRequest, NativeProduction, NativeRetrainRequest,
    NativeRuntime, NativeSchemaPolicy,
};
use crate::n4a_owner_portable::n4a_container_view;
use crate::{
    predict_methods_archive_v2_matrix, preflight_methods_archive_v2_library,
    replay_methods_archive_v2_loaded_json, replay_methods_archive_v3_loaded_json,
    run_dense_regression_workflow, DenseRegressionPreprocessing, DenseRegressionWorkflowRequest,
    MethodsArchiveMatrixPredictRequest, MethodsArchiveReplayJsonRequest,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn options(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(String, BTreeMap<String, OsString>)> {
    let mut args = args.into_iter();
    let command = args
        .next()
        .ok_or("expected inspect, read-member, replay-process, replay-methods, dataset-normalize or dataset-u07-sources")?;
    let command = command.into_string().map_err(|_| "command must be UTF-8")?;
    if !matches!(
        command.as_str(),
        "inspect"
            | "read-member"
            | "replay-process"
            | "replay-methods"
            | "dataset-normalize"
            | "dataset-content"
            | "dataset-compatible-schemas"
            | "dataset-u07-sources"
            | "multimodal-replay-inputs"
            | "native-multimodal-run"
            | "native-multimodal-load"
            | "native-multimodal-predict"
            | "native-multimodal-export"
            | "pipeline-retrain"
            | "native-multimodal-retrain"
            | "pipeline-run"
            | "pipeline-export"
            | "pipeline-load"
            | "pipeline-predict"
            | "workflow-run"
            | "workflow-predict"
            | "workflow-load"
            | "training-result-view"
            | "experiment-open"
            | "experiment-save"
            | "publish-directory"
            | "generate"
            | "tuning-run"
            | "tuning-load"
            | "conformal-calibrate"
            | "conformal-predict"
            | "conformal-load"
            | "conformal-metrics"
            | "robustness"
            | "n4a-inspect"
            | "n4a-write"
            | "n4a-read"
            | "n4a-produce"
            | "n4a-predict"
            | "n4a-retrain"
    ) {
        return Err(format!("unknown archive command: {command}").into());
    }
    let allowed: &[&str] = match command.as_str() {
        "dataset-normalize"
        | "dataset-u07-sources"
        | "dataset-content"
        | "dataset-compatible-schemas" => &["input", "output"],
        "multimodal-replay-inputs" => &["archive", "input", "output", "workdir"],
        "experiment-open" => &["input", "output"],
        "experiment-save" => &["input", "destination", "archive", "output"],
        "publish-directory" => &["input", "destination", "output"],
        "workflow-load" | "training-result-view" => &["archive", "output"],
        "generate" | "conformal-metrics" => &["input", "output"],
        "robustness" => &["input", "archive", "methods-library", "run-id", "output"],
        "conformal-load" | "tuning-load" => &["archive", "output"],
        "tuning-run" => &[
            "input",
            "source-id",
            "trials",
            "seed",
            "sampler",
            "metric",
            "methods-library",
            "checkpoint-archive",
            "archive",
            "run-id",
            "output",
            "results-directory",
        ],
        "conformal-calibrate" => &[
            "input",
            "archive",
            "destination",
            "source-id",
            "methods-library",
            "run-id",
            "coverages",
            "small-sample-policy",
            "output",
        ],
        "pipeline-export" | "native-multimodal-export" => &["input", "output", "destination"],
        "native-multimodal-run"
        | "native-multimodal-load"
        | "native-multimodal-predict"
        | "native-multimodal-retrain" => &["input", "output", "methods-library", "run-id"],
        "pipeline-run" | "pipeline-load" | "pipeline-predict" | "pipeline-retrain" => {
            &["input", "output", "source-id", "methods-library", "run-id"]
        }
        "workflow-run" => &[
            "input",
            "source-id",
            "components",
            "preprocessing",
            "methods-library",
            "archive",
            "run-id",
            "output",
            "results-directory",
        ],
        "workflow-predict" | "conformal-predict" => &[
            "input",
            "archive",
            "methods-library",
            "methods-library-sha256",
            "run-id",
            "output",
        ],
        // Canonical V1 `.n4a` operations. None of them accepts a recipe on
        // retrain, a phase, an adapter or a controller: owners decide.
        "n4a-inspect" => &[
            "archive",
            "expected-archive-sha256",
            "output",
            "memory-budget-bytes",
        ],
        "n4a-write" => &[
            "manifest",
            "members",
            "destination",
            "archive",
            "expected-archive-sha256",
            "output",
            "memory-budget-bytes",
        ],
        "n4a-read" => &[
            "archive",
            "expected-archive-sha256",
            "member",
            "artifact",
            "directory",
            "output",
            "memory-budget-bytes",
        ],
        "n4a-produce" => &[
            "input",
            "recipe",
            "source-id",
            "methods-library",
            "methods-library-sha256",
            "run-id",
            "container-id",
            "archive",
            "embed-training-record",
            "output",
            "memory-budget-bytes",
        ],
        "n4a-predict" => &[
            "archive",
            "expected-archive-sha256",
            "entrypoint",
            "input",
            "source-id",
            "partition",
            "request-id",
            "run-id",
            "methods-library",
            "methods-library-sha256",
            "output",
            "memory-budget-bytes",
        ],
        "n4a-retrain" => &[
            "archive",
            "expected-archive-sha256",
            "input",
            "run-id",
            "container-id",
            "destination",
            "embed-training-record",
            "methods-library",
            "methods-library-sha256",
            "output",
            "memory-budget-bytes",
        ],
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
        let native_options = matches!(
            command.as_str(),
            "inspect"
                | "read-member"
                | "replay-methods"
                | "replay-process"
                | "workflow-load"
                | "training-result-view"
                | "workflow-predict"
                | "conformal-predict"
                | "conformal-load"
                | "conformal-calibrate"
                | "tuning-load"
                | "multimodal-replay-inputs"
                | "robustness"
        );
        if !(allowed.contains(&key)
            || native_options && matches!(key, "entrypoint" | "memory-budget-bytes"))
        {
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

fn validated_view(archive: &LoadedNativePackage) -> Result<Value> {
    Ok(archive.view())
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
    if let Err(error) = output.write_all(bytes).and_then(|()| output.flush()) {
        drop(output);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

fn new_destination(path: &Path) -> Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Err(format!("training output already exists: {}", path.display()).into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent)?;
    let filename = path
        .file_name()
        .ok_or("training destination must name a new file or directory")?;
    // Probe writability before native training. The temporary file is removed
    // without reserving or publishing any caller destination.
    tempfile::NamedTempFile::new_in(&parent)?.close()?;
    Ok(parent.join(filename))
}

struct TrainingDestinations {
    archive: PathBuf,
    results: Option<PathBuf>,
}

fn training_destinations(values: &BTreeMap<String, OsString>) -> Result<TrainingDestinations> {
    let archive = new_destination(Path::new(required(values, "archive")?))?;
    let results = values
        .get("results-directory")
        .map(|path| new_destination(Path::new(path)))
        .transpose()?;
    let output = values
        .get("output")
        .map(|path| new_destination(Path::new(path)))
        .transpose()?;
    let paths = std::iter::once(&archive)
        .chain(results.iter())
        .chain(output.iter())
        .collect::<Vec<_>>();
    for (index, path) in paths.iter().enumerate() {
        for other in paths.iter().skip(index + 1) {
            if path.starts_with(other) || other.starts_with(path) {
                return Err("training destinations overlap or refer to the same path".into());
            }
        }
    }
    Ok(TrainingDestinations { archive, results })
}

struct PublishedPathIdentity {
    #[cfg(unix)]
    handle: File,
}

impl PublishedPathIdentity {
    fn capture(path: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self {
                handle: File::open(path)?,
            })
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(Self {})
        }
    }

    fn matches(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let Ok(expected) = self.handle.metadata() else {
                return false;
            };
            let Ok(current) = std::fs::symlink_metadata(path) else {
                return false;
            };
            // Retaining the handle prevents reuse of a removed inode. Never
            // follow a replacement symlink, even when it points to our bytes.
            !current.file_type().is_symlink()
                && expected.dev() == current.dev()
                && expected.ino() == current.ino()
        }
        #[cfg(not(unix))]
        {
            // Stable std exposes no portable strong filesystem identity.
            // Preserve outputs on rollback rather than guess with timestamps
            // or delete another writer's replacement, including identical bytes.
            let _ = path;
            false
        }
    }
}

struct PublishedTrainingPath {
    path: PathBuf,
    directory: bool,
    identity: PublishedPathIdentity,
}

impl PublishedTrainingPath {
    fn staged(source: &Path, destination: &Path) -> Result<Self> {
        let kind = std::fs::symlink_metadata(source)?.file_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err("training stage must contain only regular files and directories".into());
        }
        let identity = PublishedPathIdentity::capture(source)?;
        #[cfg(unix)]
        if !identity.matches(source) {
            return Err("training stage changed while recording publication identity".into());
        }
        Ok(Self {
            path: destination.to_owned(),
            directory: kind.is_dir(),
            identity,
        })
    }
}

fn staged_training_inventory(
    source: &Path,
    destination: &Path,
) -> Result<Vec<PublishedTrainingPath>> {
    let root = PublishedTrainingPath::staged(source, destination)?;
    let mut inventory = Vec::new();
    if root.directory {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            inventory.extend(staged_training_inventory(
                &entry.path(),
                &destination.join(entry.file_name()),
            )?);
        }
    }
    // Parent-first ordering lets rollback remove children before their empty
    // directories, without enumerating or claiming any later foreign member.
    inventory.insert(0, root);
    Ok(inventory)
}

#[derive(Default)]
struct PublishedTrainingOutputs {
    owned: Vec<PublishedTrainingPath>,
    committed: bool,
}

impl PublishedTrainingOutputs {
    fn publish(
        archive_stage: &Path,
        results_stage: Option<&Path>,
        destinations: &TrainingDestinations,
    ) -> Result<Self> {
        let mut published = Self::default();
        let archive = PublishedTrainingPath::staged(archive_stage, &destinations.archive)?;
        let results = match (results_stage, destinations.results.as_ref()) {
            (Some(stage), Some(target)) => staged_training_inventory(stage, target)?,
            _ => Vec::new(),
        };
        // The stage is in the destination parent: an exclusive hard link
        // publishes complete bytes and never replaces another invocation.
        std::fs::hard_link(archive_stage, &destinations.archive)?;
        published.owned.push(archive);
        if let (Some(stage), Some(target)) = (results_stage, destinations.results.as_ref()) {
            crate::publish_directory_noreplace(stage, target)?;
            published.owned.extend(results);
        }
        Ok(published)
    }
}

impl Drop for PublishedTrainingOutputs {
    fn drop(&mut self) {
        if !self.committed {
            for entry in self.owned.iter().rev() {
                let parents_unchanged = self
                    .owned
                    .iter()
                    .filter(|parent| {
                        parent.directory
                            && entry.path != parent.path
                            && entry.path.starts_with(&parent.path)
                    })
                    .all(|parent| parent.identity.matches(&parent.path));
                if !parents_unchanged || !entry.identity.matches(&entry.path) {
                    continue;
                }
                if entry.directory {
                    // A foreign member keeps this directory nonempty and safe.
                    let _ = std::fs::remove_dir(&entry.path);
                } else {
                    let _ = std::fs::remove_file(&entry.path);
                }
            }
        }
    }
}

fn staged_results(
    training: &dag_ml_core::TrainingOutcome,
    dataset: &crate::DatasetPackage,
    source_id: &str,
    destination: Option<&Path>,
) -> Result<Option<(tempfile::TempDir, PathBuf)>> {
    destination
        .map(|target| {
            let temporary = tempfile::tempdir_in(target.parent().ok_or("missing results parent")?)?;
            let stage = temporary.path().join("results");
            crate::results::write_training_results(training, dataset, source_id, &stage)?;
            Ok((temporary, stage))
        })
        .transpose()
}

fn read_json_transport(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

fn selected_native_entry(values: &BTreeMap<String, OsString>) -> Result<Option<&str>> {
    values
        .get("entrypoint")
        .map(|value| {
            value
                .to_str()
                .ok_or_else(|| "entrypoint must be UTF-8".into())
        })
        .transpose()
}

fn selected_native_archive(
    path: &Path,
    values: &BTreeMap<String, OsString>,
) -> Result<LoadedNativePackage> {
    let archive =
        open_native_package(path, selected_native_entry(values)?, memory_budget(values)?)?;
    archive.predictor_package()?;
    Ok(archive)
}

fn replay_methods(
    archive: &LoadedNativePackage,
    _view: &Value,
    values: &BTreeMap<String, OsString>,
) -> Result<()> {
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
    let outcome = if archive.reference().schema_version() == 2 {
        replay_methods_archive_v2_loaded_json(archive, input)?
    } else {
        replay_methods_archive_v3_loaded_json(archive, input)?
    };
    write_new(Path::new(required(values, "output")?), outcome.as_bytes())
}

fn execute(command: &str, values: &BTreeMap<String, OsString>) -> Result<()> {
    // Canonical V1 containers are routed before every legacy archive loader.
    if command.starts_with("n4a-") {
        return n4a_command(command, values);
    }
    if command == "publish-directory" {
        let source = Path::new(required(values, "input")?);
        let target = Path::new(required(values, "destination")?);
        // Finish the transport response before making the export visible.
        emit_json(values, &json!({"published": target}))?;
        crate::publish_directory_noreplace(source, target)?;
        return Ok(());
    }
    if command == "training-result-view" {
        return training_result_view(values);
    }
    if command == "dataset-content" || command == "dataset-compatible-schemas" {
        let input: Value =
            serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
        if command == "dataset-content" {
            let bytes = nirs4all_io_crate::core::public_dataset::dataset_content_bytes(&input)?;
            let fingerprint = format!("{:x}", Sha256::digest(&bytes));
            return emit_json(
                values,
                &json!({"canonical_content_utf8": String::from_utf8(bytes)?, "fingerprint": fingerprint}),
            );
        }
        let saved = nirs4all_io_crate::core::public_dataset::compatible_source_schemas(
            &input["current"],
            &input["saved"],
        )?;
        return emit_json(values, &saved);
    }
    if command == "multimodal-replay-inputs" {
        return multimodal_replay_inputs(values);
    }
    if command == "robustness" {
        return robustness(values);
    }
    if command == "generate" {
        let input = read_json_transport(Path::new(required(values, "input")?))?;
        let result = crate::generation::generate_variants_json(&input)?;
        let variants: Value = serde_json::from_str(&result)?;
        let seeds: Vec<String> = variants
            .as_array()
            .ok_or("native generator variants must be an array")?
            .iter()
            .map(|variant| {
                variant["seed"]
                    .as_u64()
                    .map(|seed| seed.to_string())
                    .ok_or("native generator variant lacks its u64 seed")
            })
            .collect::<std::result::Result<_, _>>()?;
        return emit_json(
            values,
            &json!({"schema": "nirs4all.generation.v1", "variants": variants, "seed_decimals": seeds}),
        );
    }
    if command == "tuning-run" {
        return tuning_run(values);
    }
    if command == "tuning-load" {
        return tuning_load(values);
    }
    if command == "conformal-calibrate" {
        return conformal_calibrate(values);
    }
    if command == "conformal-predict" {
        return workflow_predict(values, true);
    }
    if command == "conformal-load" {
        let archive = selected_native_archive(Path::new(required(values, "archive")?), values)?;
        let package = archive.predictor_package()?.clone();
        let calibration = package
            .conformal_calibration
            .ok_or("archive has no calibrator")?;
        return emit_json(
            values,
            &json!({"schema": "nirs4all.calibrated.v1", "calibration": calibration}),
        );
    }
    if command == "conformal-metrics" {
        let input: Value =
            serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
        let calibration = if let Some(raw) = input.get("calibration_json") {
            if input.get("calibration").is_some() {
                return Err("ambiguous calibration transport".into());
            }
            let envelope: Value =
                serde_json::from_str(raw.as_str().ok_or("calibration_json must be a string")?)?;
            envelope["calibration"].clone()
        } else {
            input["calibration"].clone()
        };
        let intervals = if let Some(raw) = input.get("prediction_json") {
            if input.get("intervals").is_some() {
                return Err("ambiguous interval transport".into());
            }
            let prediction: Value =
                serde_json::from_str(raw.as_str().ok_or("prediction_json must be a string")?)?;
            prediction["interval_block"].clone()
        } else {
            input["intervals"].clone()
        };
        let result = crate::conformal_json::conformal_metrics_json(
            &serde_json::to_string(&calibration)?,
            &serde_json::to_string(&intervals)?,
            &serde_json::to_string(&input["truth"])?,
        )?;
        return emit_json(values, &serde_json::from_str(&result)?);
    }
    if command == "workflow-load" {
        return workflow_load(values);
    }
    if command == "experiment-save" {
        let experiment = crate::results::save_experiment_results(
            Path::new(required(values, "input")?),
            Path::new(required(values, "destination")?),
            values.get("archive").map(Path::new),
        )
        .map_err(|error| format!("experiment publication refused: {error}"))?;
        return emit_json(values, &serde_json::to_value(experiment.native)?);
    }
    if command == "experiment-open" {
        let experiment =
            crate::results::open_experiment_results(Path::new(required(values, "input")?))
                .map_err(|error| format!("experiment validation refused: {error}"))?;
        return emit_json(values, &serde_json::to_value(experiment.native)?);
    }
    if command.starts_with("pipeline-") || command.starts_with("native-multimodal-") {
        let record: Value =
            serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
        let result = match command {
            "pipeline-retrain" | "native-multimodal-retrain" => {
                if record.as_object().is_none_or(|fields| {
                    fields.len() != 2
                        || !fields.contains_key("model")
                        || !fields.contains_key("dataset")
                }) {
                    return Err("native retrain requires exactly model and dataset".into());
                }
                let model = &record["model"];
                let library = Path::new(required(values, "methods-library")?);
                let run_id = text_option(values, "run-id")?;
                if command == "pipeline-retrain" {
                    crate::pipeline_workflow::validate_pipeline(model)?;
                    crate::pipeline_workflow::run_pipeline(
                        &json!({"dataset":record["dataset"], "pipeline":model["config"]["pipeline"]}),
                        model["config"]["source_id"]
                            .as_str()
                            .ok_or("Invalid native source ID")?,
                        library,
                        run_id,
                    )?
                } else {
                    crate::multimodal_workflow::validate_multimodal(model)?;
                    crate::multimodal_workflow::run_multimodal(
                        &json!({"dataset":record["dataset"], "pipeline":model["config"]["pipeline"],
                            "source_policies":model["config"]["source_policies"]}),
                        library,
                        run_id,
                    )?
                }
            }
            "native-multimodal-run" => crate::multimodal_workflow::run_multimodal(
                &record,
                Path::new(required(values, "methods-library")?),
                text_option(values, "run-id")?,
            )?,
            "native-multimodal-predict" => crate::multimodal_workflow::predict_multimodal(
                &record,
                Path::new(required(values, "methods-library")?),
                text_option(values, "run-id")?,
            )?,
            "native-multimodal-load" => {
                crate::multimodal_workflow::validate_multimodal(&record)?;
                record
            }
            "pipeline-run" => crate::pipeline_workflow::run_pipeline(
                &record,
                text_option(values, "source-id")?,
                Path::new(required(values, "methods-library")?),
                text_option(values, "run-id")?,
            )?,
            "pipeline-load" => {
                crate::pipeline_workflow::validate_pipeline(&record)?;
                record
            }
            "pipeline-export" | "native-multimodal-export" => {
                if command == "pipeline-export" {
                    crate::pipeline_workflow::validate_pipeline(&record)?;
                } else {
                    crate::multimodal_workflow::validate_multimodal(&record)?;
                }
                let path = Path::new(required(values, "destination")?);
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                let mut file = tempfile::NamedTempFile::new_in(parent)?;
                serde_json::to_writer(file.as_file_mut(), &record)?;
                file.as_file_mut().sync_all()?;
                file.persist_noclobber(path)?;
                json!({"path":path})
            }
            "pipeline-predict" => crate::pipeline_workflow::predict_pipeline(
                &record,
                Path::new(required(values, "methods-library")?),
                text_option(values, "run-id")?,
            )?,
            _ => unreachable!(),
        };
        return emit_json(values, &result);
    }
    if command == "workflow-run" {
        return workflow_run(values);
    }
    if command == "workflow-predict" {
        return workflow_predict(values, false);
    }
    if matches!(command, "dataset-normalize" | "dataset-u07-sources") {
        let input = read_json_transport(Path::new(required(values, "input")?))?;
        let raw: Value = serde_json::from_str(&input)?;
        let normalized = if command == "dataset-normalize" {
            nirs4all_io_crate::core::public_dataset::normalize_dataset(&raw)
        } else {
            nirs4all_io_crate::core::public_dataset::u07_sources(&raw)
        }
        .map_err(|error| format!("IO dataset validation refused: {error}"))?;
        let output = serde_json::to_vec_pretty(&normalized)?;
        if let Some(path) = values.get("output") {
            write_new(Path::new(path), &output)?;
        } else {
            std::io::stdout().write_all(&output)?;
        }
        return Ok(());
    }
    // Container dispatch, size limits, closure and raw hashes are validated by
    // the owning Core loader before any host process or member output exists.
    let archive = open_native_package(
        Path::new(required(values, "archive")?),
        selected_native_entry(values)?,
        memory_budget(values)?,
    )?;
    let view = validated_view(&archive)?;
    if command == "inspect" {
        let manifest = archive.manifest();
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
        let bytes = archive.member(member)?;
        return write_new(Path::new(required(values, "output")?), bytes);
    }
    let (native_command, package) = if archive.reference().schema_version() == 2 {
        (
            "run-process-loaded-predictor-replay",
            archive.portable_predictor_package()?,
        )
    } else {
        (
            "run-process-loaded-refit-replay",
            archive.portable_refit_package()?,
        )
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

fn text_option<'a>(values: &'a BTreeMap<String, OsString>, key: &str) -> Result<&'a str> {
    required(values, key)?
        .to_str()
        .ok_or_else(|| format!("--{key} must be UTF-8").into())
}

fn emit_json(values: &BTreeMap<String, OsString>, value: &Value) -> Result<()> {
    let output = serde_json::to_vec_pretty(value)?;
    if let Some(path) = values.get("output") {
        write_new(Path::new(path), &output)
    } else {
        std::io::stdout().write_all(&output)?;
        Ok(())
    }
}

fn multimodal_replay_inputs(values: &BTreeMap<String, OsString>) -> Result<()> {
    let archive = selected_native_archive(Path::new(required(values, "archive")?), values)?;
    let package = archive.predictor_package()?.clone();
    let input: Value =
        serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
    let mut current = nirs4all_io_crate::core::public_dataset::multimodal_runtime_input(&input)
        .map_err(|error| format!("IO raw cohort validation refused: {error}"))?;
    let package_value = serde_json::to_value(&package)?;
    let saved = package_value
        .pointer("/effective_plan/graph_plan/graph/nodes/0/operator/source_schemas")
        .ok_or("multimodal archive has no fitted source schemas")?;
    // IO owns logical schema compatibility. Preserve the exact fitted native
    // descriptors after validation, including archives made by older hosts.
    current["source_schemas"] = nirs4all_io_crate::core::public_dataset::compatible_source_schemas(
        &current["source_schemas"],
        saved,
    )
    .map_err(|error| format!("IO source schema compatibility refused: {error}"))?;
    let mut prepared =
        dag_ml_core::multimodal_replay_inputs::prepare_multimodal_replay(&package, &current)?;
    let view = serde_json::to_value(archive_v2_view(&archive)?)?;
    prepared["archive_sha256"] = view["archive_sha256"].clone();
    if let Some(path) = values.get("workdir") {
        let path = Path::new(path);
        // A fresh directory preserves every native contract as exact JSON.
        std::fs::create_dir(path)?;
        let directory = std::fs::canonicalize(path)?;
        prepared["config"]["audit_path"] = json!(directory.join("lifecycle.jsonl"));
        for (key, filename) in [
            ("request", "request.json"),
            ("envelopes", "envelopes.json"),
            ("trusted_controllers", "trusted-controllers.json"),
            ("config", "config.json"),
        ] {
            write_new(
                &directory.join(filename),
                &serde_json::to_vec_pretty(&prepared[key])?,
            )?;
        }
    }
    emit_json(values, &prepared)
}

fn workflow_load(values: &BTreeMap<String, OsString>) -> Result<()> {
    let path = Path::new(required(values, "archive")?);
    let archive = selected_native_archive(path, values)?;
    let view = serde_json::to_value(archive_v2_view(&archive)?)?;
    let package = archive.predictor_package()?.clone();
    let outcome = archive.training_outcome()?.clone();
    if outcome.outcome_fingerprint != package.training_outcome.outcome_fingerprint {
        return Err("workflow outcome and signed package disagree".into());
    }
    let config = crate::dense_regression_workflow_config(&package)?;
    emit_json(
        values,
        &json!({
            "schema": "nirs4all.workflow.v1", "run_id": outcome.run_id,
            "winner_variant_id": outcome.selected_variant_id, "training_outcome": outcome,
            "model_archive": std::fs::canonicalize(path)?,
            "archive_id": view["archive_id"], "archive_sha256": view["archive_sha256"],
            "config": config, "numeric_storage": "float32",
        }),
    )
}

fn training_result_view(values: &BTreeMap<String, OsString>) -> Result<()> {
    let path = Path::new(required(values, "archive")?);
    let archive = selected_native_archive(path, values)?;
    let package = archive.predictor_package()?.clone();
    let outcome = archive.training_outcome()?.clone();
    if outcome.outcome_fingerprint != package.training_outcome.outcome_fingerprint {
        return Err("native result outcome differs from the archived predictor".into());
    }
    let predictions = crate::result_projection::project_training_predictions(
        &outcome,
        outcome.effective_plan.id.as_str(),
        "regression",
    )?;
    emit_json(
        values,
        &json!({
            "schema": "nirs4all.result-view.v1", "run_id": outcome.run_id,
            "training_outcome_fingerprint": outcome.outcome_fingerprint,
            "winner_variant_id": outcome.selected_variant_id, "score_set": outcome.score_set,
            "predictions": predictions, "model_archive": std::fs::canonicalize(path)?,
            "validation_level": "native_model_projection", "label_provenance": "transport_only",
        }),
    )
}

fn workflow_run(values: &BTreeMap<String, OsString>) -> Result<()> {
    let destinations = training_destinations(values)?;
    let archive_temporary = tempfile::tempdir_in(
        destinations
            .archive
            .parent()
            .ok_or("missing archive parent")?,
    )?;
    let archive_stage = archive_temporary.path().join("model.n4a");
    let record: Value =
        serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
    let source_id = text_option(values, "source-id")?;
    let components: Vec<u32> = serde_json::from_str(text_option(values, "components")?)?;
    let preprocessing_name = values
        .get("preprocessing")
        .map(|value| value.to_str().ok_or("preprocessing must be UTF-8"))
        .transpose()?
        .unwrap_or("snv_savgol");
    let preprocessing = match preprocessing_name {
        "raw" => DenseRegressionPreprocessing::Raw,
        "snv_savgol" => DenseRegressionPreprocessing::SnvSavitzkyGolay,
        _ => return Err("workflow preprocessing must be raw or snv_savgol".into()),
    };
    let dataset =
        nirs4all_io_crate::core::public_dataset::dense_dataset_package(&record, source_id)
            .map_err(|error| format!("IO dataset validation refused: {error}"))?;
    let library = std::fs::canonicalize(Path::new(required(values, "methods-library")?))?;
    let outcome = run_dense_regression_workflow(DenseRegressionWorkflowRequest {
        dataset: &dataset,
        source_id,
        components: &components,
        preprocessing,
        methods_library_path: &library,
        archive_path: &archive_stage,
        run_id: text_option(values, "run-id")?,
    })
    .map_err(|error| format!("native workflow refused: {error}"))?;
    let config = json!({"source_id": source_id, "components": components, "preprocessing": preprocessing_name});
    let results = staged_results(
        &outcome.training,
        &dataset,
        source_id,
        destinations.results.as_deref(),
    )?;
    let mut published = PublishedTrainingOutputs::publish(
        &archive_stage,
        results.as_ref().map(|(_, path)| path.as_path()),
        &destinations,
    )?;
    emit_json(
        values,
        &json!({
            "schema": "nirs4all.workflow.v1", "run_id": outcome.training.run_id,
            "winner_variant_id": outcome.training.selected_variant_id, "training_outcome": outcome.training,
            "model_archive": destinations.archive, "archive_id": outcome.archive.archive_id(),
            "archive_sha256": outcome.archive.archive_sha256(), "native_results_dir": destinations.results,
            "config": config,
            "numeric_storage": "float32",
        }),
    )?;
    published.committed = true;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowPredictInput {
    sample_ids: Vec<String>,
    x: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncertaintyDatasetInput {
    dataset: Value,
    source_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ConformalPredictInput {
    Dataset(UncertaintyDatasetInput),
    Positional(WorkflowPredictInput),
}

fn workflow_predict(values: &BTreeMap<String, OsString>, conformal: bool) -> Result<()> {
    let input_json = read_json_transport(Path::new(required(values, "input")?))?;
    let archive = selected_native_archive(Path::new(required(values, "archive")?), values)?;
    let (input, diagnostics) = if conformal {
        let input: ConformalPredictInput = serde_json::from_str(&input_json)?;
        let cohort = match &input {
            ConformalPredictInput::Dataset(input) => {
                crate::conformal::UncertaintyCohortInput::Dataset {
                    record: &input.dataset,
                    source_id: input.source_id.as_deref(),
                }
            }
            ConformalPredictInput::Positional(input) => {
                crate::conformal::UncertaintyCohortInput::Positional {
                    x: &input.x,
                    sample_ids: &input.sample_ids,
                }
            }
        };
        let resolved = crate::conformal::resolve_uncertainty_cohort(&archive, cohort)?;
        (
            WorkflowPredictInput {
                sample_ids: resolved.sample_ids,
                x: resolved.x,
            },
            resolved.diagnostics,
        )
    } else {
        (
            serde_json::from_str::<WorkflowPredictInput>(&input_json)?,
            BTreeMap::new(),
        )
    };
    let package = archive.predictor_package()?.clone();
    let [binding] = package.output_bindings.as_slice() else {
        return Err("workflow matrix prediction requires exactly one output binding".into());
    };
    let library = std::fs::canonicalize(Path::new(required(values, "methods-library")?))?;
    let mut source = File::open(&library)?.take(64 * 1024 * 1024 + 1);
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Methods library exceeds the native identity budget".into());
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    let hash = values
        .get("methods-library-sha256")
        .map(|value| value.to_str().ok_or("Methods hash must be UTF-8"))
        .transpose()?
        .unwrap_or(&actual)
        .to_owned();
    let run_id = text_option(values, "run-id")?;
    let request = MethodsArchiveMatrixPredictRequest {
        sample_ids: input.sample_ids,
        x: input.x,
        expected_target_names: binding.target_names.clone(),
        methods_library_path: library,
        methods_library_sha256: hash,
        request_id: format!("predict:{run_id}"),
        outcome_id: format!("outcome:{run_id}"),
        run_id: dag_ml_core::RunId::new(run_id)?,
        warnings: Vec::new(),
        diagnostics,
    };
    if conformal {
        if package.execution_bundle.methods_hpo_resume_state.is_some() {
            let presentation = crate::native_methods_replay::predict_tuning_archive_v2_calibrated(
                &archive, request,
            )?;
            return emit_json(values, &presentation);
        }
        let presentation =
            crate::predict_methods_archive_v2_matrix_conformal_presentation_v2(&archive, request)?;
        return emit_json(values, &serde_json::to_value(presentation)?);
    }
    let predicted = if package.execution_bundle.methods_hpo_resume_state.is_some() {
        crate::predict_tuning_archive_v2_matrix(&archive, request)?
    } else {
        predict_methods_archive_v2_matrix(&archive, request)?
    };
    emit_json(values, &serde_json::to_value(predicted)?)
}

fn conformal_calibrate(values: &BTreeMap<String, OsString>) -> Result<()> {
    let mut publication_values = values.clone();
    publication_values.insert("archive".into(), required(values, "destination")?.clone());
    let destinations = training_destinations(&publication_values)?;
    let temporary = tempfile::tempdir_in(
        destinations
            .archive
            .parent()
            .ok_or("missing calibration parent")?,
    )?;
    let archive_stage = temporary.path().join("calibrated.n4a");
    let record: Value =
        serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
    let source_id = text_option(values, "source-id")?;
    let library = std::fs::canonicalize(Path::new(required(values, "methods-library")?))?;
    let mut result = crate::conformal::calibrate_archive_selected(
        crate::conformal::CalibrateArchiveRequest {
            model_archive: Path::new(required(values, "archive")?),
            calibration_record: &record,
            source_id: Some(source_id),
            methods_library_path: &library,
            archive_path: &archive_stage,
            run_id: text_option(values, "run-id")?,
            coverages: serde_json::from_str(text_option(values, "coverages")?)?,
            small_sample_policy: serde_json::from_str(text_option(values, "small-sample-policy")?)?,
        },
        selected_native_entry(values)?,
        memory_budget(values)?,
    )?;
    let mut published = PublishedTrainingOutputs::publish(&archive_stage, None, &destinations)?;
    result["model_archive"] = json!(destinations.archive);
    emit_json(values, &result)?;
    published.committed = true;
    Ok(())
}

fn tuning_run(values: &BTreeMap<String, OsString>) -> Result<()> {
    let destinations = training_destinations(values)?;
    let archive_temporary = tempfile::tempdir_in(
        destinations
            .archive
            .parent()
            .ok_or("missing archive parent")?,
    )?;
    let archive_stage = archive_temporary.path().join("model.n4a");
    let record: Value =
        serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
    let source_id = text_option(values, "source-id")?;
    let dataset =
        nirs4all_io_crate::core::public_dataset::dense_dataset_package(&record, source_id)?;
    let library = std::fs::canonicalize(Path::new(required(values, "methods-library")?))?;
    let sampler: dag_ml_core::HpoSampler =
        serde_json::from_value(json!(text_option(values, "sampler")?))?;
    let metric = match text_option(values, "metric")? {
        "rmse" => dag_ml_core::RegressionMetricKind::Rmse,
        "mae" => dag_ml_core::RegressionMetricKind::Mae,
        "r2" => dag_ml_core::RegressionMetricKind::R2,
        _ => return Err("tuning metric must be rmse, mae or r2".into()),
    };
    let outcome = crate::tuning::run_dense_tuning(crate::tuning::DenseTuningRequest {
        dataset: &dataset,
        source_id,
        trials: text_option(values, "trials")?.parse()?,
        seed: text_option(values, "seed")?.parse()?,
        sampler,
        metric,
        methods_library_path: &library,
        checkpoint_archive: values.get("checkpoint-archive").map(Path::new),
        archive_path: &archive_stage,
        run_id: text_option(values, "run-id")?,
    })?;
    let results = staged_results(
        &outcome.training,
        &dataset,
        source_id,
        destinations.results.as_deref(),
    )?;
    let mut published = PublishedTrainingOutputs::publish(
        &archive_stage,
        results.as_ref().map(|(_, path)| path.as_path()),
        &destinations,
    )?;
    emit_json(
        values,
        &json!({"schema": "nirs4all.tuning.v1", "training_outcome": outcome.training,
        "model_archive": destinations.archive, "native_results_dir": destinations.results,
        "archive_sha256": outcome.archive.archive_sha256()}),
    )?;
    published.committed = true;
    Ok(())
}

fn tuning_load(values: &BTreeMap<String, OsString>) -> Result<()> {
    let path = Path::new(required(values, "archive")?);
    let archive = selected_native_archive(path, values)?;
    let package = archive.predictor_package()?.clone();
    if package.execution_bundle.methods_hpo_resume_state.is_none() {
        return Err("archive has no native tuning checkpoint".into());
    }
    let training = archive.training_outcome()?.clone();
    if training.outcome_fingerprint != package.training_outcome.outcome_fingerprint {
        return Err("tuning archive outcome differs from its package".into());
    }
    let template = serde_json::to_value(&package.template)?;
    let descriptor = &template["campaign"]["metadata"]["methods_hpo_operation"];
    let source_id = template["campaign"]["data_bindings"]["model:pls"][0]["source_ids"][0]
        .as_str()
        .ok_or("missing native tuning source")?;
    if !descriptor.is_object() {
        return Err("missing native tuning descriptor".into());
    }
    let view = serde_json::to_value(archive_v2_view(&archive)?)?;
    emit_json(
        values,
        &json!({"schema": "nirs4all.tuning.v1", "training_outcome": training,
        "model_archive": std::fs::canonicalize(path)?, "archive_sha256": view["archive_sha256"],
        "config": {"source_id": source_id, "trials": descriptor["trials"],
            "seed": descriptor["study"]["optimizer"]["seed"],
            "sampler": descriptor["study"]["optimizer"]["sampler"],
            "metric": descriptor["study"]["optimizer"]["metric"]}}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RobustnessInput {
    #[serde(default)]
    sample_ids: Vec<String>,
    #[serde(default)]
    x: Vec<Vec<f64>>,
    dataset: Option<Value>,
    source_id: Option<String>,
    truth: Vec<Vec<f64>>,
    scenarios: Vec<dag_ml_core::public_robustness::FrozenRobustnessScenario>,
}

fn robustness(values: &BTreeMap<String, OsString>) -> Result<()> {
    let input: RobustnessInput =
        serde_json::from_str(&read_json_transport(Path::new(required(values, "input")?))?)?;
    let library = std::fs::canonicalize(Path::new(required(values, "methods-library")?))?;
    let mut bytes = Vec::new();
    File::open(&library)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Methods library exceeds identity budget".into());
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    let archive = selected_native_archive(Path::new(required(values, "archive")?), values)?;
    let result = crate::robustness::robustness_archive_loaded(
        crate::robustness::FrozenRobustnessRequest {
            archive_path: Path::new(required(values, "archive")?),
            methods_library_path: &library,
            methods_library_sha256: &digest,
            sample_ids: input.sample_ids,
            x: input.x,
            dataset: input.dataset.as_ref(),
            source_id: input.source_id.as_deref(),
            truth: input.truth,
            scenarios: input.scenarios,
            run_id: text_option(values, "run-id")?,
        },
        &archive,
    )?;
    emit_json(values, &result)
}

// ---------------------------------------------------------------------------
// Canonical V1 `.n4a` container operations (`n4a-*`).
//
// These commands use generic V1 storage and the separate canonical action
// admission. Core opens the container and the owners (Core, DAG, Methods)
// decide what can be produced, predicted or retrained. This transport adds no
// method list, target-count or profile admission of its own, and it contains no
// parser, compiler or numerics: every record, recipe and archive is handed to
// the owner API as raw bytes or an opened file handle.
//
// `--memory-budget-bytes` declares authorized working memory. Absent, only the
// allocator and real host limits apply. It covers Core's allocations, never the
// on-disk size of a payload, and no other limit applies to archive content.
// ---------------------------------------------------------------------------

fn memory_budget(values: &BTreeMap<String, OsString>) -> Result<N4aMemoryBudget> {
    let Some(value) = values.get("memory-budget-bytes") else {
        return Ok(N4aMemoryBudget::host_allocator());
    };
    let text = value
        .to_str()
        .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or("--memory-budget-bytes must be a decimal unsigned integer")?;
    let bytes = text
        .parse::<u64>()
        .map_err(|_| "--memory-budget-bytes exceeds the unsigned 64-bit range")?;
    Ok(N4aMemoryBudget::bytes(bytes))
}

fn n4a_command(command: &str, values: &BTreeMap<String, OsString>) -> Result<()> {
    let budget = memory_budget(values)?;
    match command {
        "n4a-inspect" => n4a_inspect(values, budget),
        "n4a-write" => n4a_write(values, budget),
        "n4a-read" => n4a_read(values, budget),
        "n4a-produce" => n4a_produce(values, budget),
        "n4a-predict" => n4a_predict(values, budget),
        "n4a-retrain" => n4a_retrain(values, budget),
        _ => Err(format!("unknown archive command: {command}").into()),
    }
}

/// Materialize caller JSON through one charged, fallible allocation.
fn read_budgeted(path: &Path, budget: N4aMemoryBudget, what: &str) -> Result<Vec<u8>> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open {what} {}: {error}", path.display()))?;
    let length = file.metadata()?.len();
    budget.admit(length)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(usize::try_from(length)?)?;
    file.take(length).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn bool_option(values: &BTreeMap<String, OsString>, key: &str) -> Result<bool> {
    match values.get(key).map(|value| value.to_str()) {
        None => Ok(false),
        Some(Some("true")) => Ok(true),
        Some(Some("false")) => Ok(false),
        Some(_) => Err(format!("--{key} must be true or false").into()),
    }
}

/// Open the canonical V1 container as a file handle: only metadata stays in
/// memory. A legacy Archive V2/V3 is refused here by the container reader.
fn open_canonical_archive(
    values: &BTreeMap<String, OsString>,
    budget: N4aMemoryBudget,
) -> Result<N4aFile> {
    Ok(open_n4a_file(
        Path::new(required(values, "archive")?),
        budget,
    )?)
}

fn require_expected_sha(container: &N4aFile, expected: &OsString) -> Result<()> {
    ensure_archive_identity(
        &json!({"archive_sha256": container.receipt().container_sha256}),
        expected,
    )
}

fn n4a_inspect(values: &BTreeMap<String, OsString>, budget: N4aMemoryBudget) -> Result<()> {
    let container = open_canonical_archive(values, budget)?;
    if let Some(expected) = values.get("expected-archive-sha256") {
        require_expected_sha(&container, expected)?;
    }
    // The shared owner-neutral view loads no Methods runtime and grants no operation.
    let mut report = n4a_container_view(&container);
    if let Some(object) = report.as_object_mut() {
        object.insert("operation".into(), json!("n4a-inspect"));
    }
    emit_json(values, &report)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct N4aMemberFile {
    path: String,
    file: PathBuf,
}

/// Two distinct new outputs must not alias each other.
fn disjoint_outputs(first: &Path, second: &Path) -> Result<()> {
    if first.starts_with(second) || second.starts_with(first) {
        return Err("n4a outputs overlap or refer to the same path".into());
    }
    Ok(())
}

/// Write a new container from caller manifest bytes and member files, or from
/// an immutable base plus replacements. Core derives sizes and hashes; payloads
/// stream from their files into the stage and are never interpreted.
fn n4a_write(values: &BTreeMap<String, OsString>, budget: N4aMemoryBudget) -> Result<()> {
    let destination = new_destination(Path::new(required(values, "destination")?))?;
    let output = new_destination(Path::new(required(values, "output")?))?;
    disjoint_outputs(&destination, &output)?;
    let base = values
        .get("archive")
        .map(|_| open_canonical_archive(values, budget))
        .transpose()?;
    if let Some(expected) = values.get("expected-archive-sha256") {
        let base = base
            .as_ref()
            .ok_or("--expected-archive-sha256 requires --archive")?;
        require_expected_sha(base, expected)?;
    }
    let manifest_bytes =
        read_budgeted(Path::new(required(values, "manifest")?), budget, "manifest")?;
    let manifest = parse_n4a_manifest(&manifest_bytes, budget)?;
    let container_id = manifest.container_id.clone();
    let list_path = Path::new(required(values, "members")?);
    let list = read_budgeted(list_path, budget, "member list")?;
    let entries: Vec<N4aMemberFile> = serde_json::from_slice(&list)
        .map_err(|error| format!("member list must be a JSON array of {{path, file}}: {error}"))?;
    // Relative files resolve against the list's directory, so a list is portable.
    let list_directory = list_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let sources = entries
        .into_iter()
        .map(|entry| (entry.path, list_directory.join(entry.file)))
        .collect::<Vec<_>>();
    let stage_directory =
        tempfile::tempdir_in(destination.parent().ok_or("missing destination parent")?)?;
    let stage = stage_directory.path().join("container.n4a");
    let receipt = write_n4a_stage(
        &stage,
        N4aStreamRequest {
            manifest,
            members: sources
                .iter()
                .map(|(path, file)| N4aStreamInput { path, source: file })
                .collect(),
            base: base.as_ref().map(N4aStreamBase::File),
        },
        budget,
    )?;
    let destinations = TrainingDestinations {
        archive: destination.clone(),
        results: None,
    };
    let mut published = PublishedTrainingOutputs::publish(&stage, None, &destinations)?;
    emit_json(
        values,
        &json!({
            "operation": "n4a-write",
            "destination": destination,
            "archive_sha256": receipt.container_sha256,
            "manifest_sha256": receipt.manifest_sha256,
            "byte_length": receipt.byte_length,
            "container_id": container_id,
            "base_archive_sha256": base.as_ref().map(|base| base.receipt().container_sha256.clone()),
            "members": receipt.members,
        }),
    )?;
    published.committed = true;
    Ok(())
}

/// Export exact bytes into a new directory using fixed numbered file names.
/// Archive paths are only data in the report; they never become OS paths.
fn n4a_read(values: &BTreeMap<String, OsString>, budget: N4aMemoryBudget) -> Result<()> {
    if values.contains_key("member") && values.contains_key("artifact") {
        return Err("--member and --artifact are mutually exclusive".into());
    }
    let directory = new_destination(Path::new(required(values, "directory")?))?;
    let output = new_destination(Path::new(required(values, "output")?))?;
    disjoint_outputs(&directory, &output)?;
    let container = open_canonical_archive(values, budget)?;
    if let Some(expected) = values.get("expected-archive-sha256") {
        require_expected_sha(&container, expected)?;
    }
    let (selection, selected): (Value, Vec<&str>) = if values.contains_key("member") {
        let path = text_option(values, "member")?;
        container
            .member_location(path)
            .ok_or_else(|| format!("archive has no member `{path}`"))?;
        (json!({"kind": "member", "value": path}), vec![path])
    } else if values.contains_key("artifact") {
        let id = text_option(values, "artifact")?;
        let paths = container
            .artifact_member_paths(id)
            .ok_or_else(|| format!("archive has no artifact `{id}`"))?;
        (
            json!({"kind": "artifact", "value": id}),
            paths.iter().map(String::as_str).collect(),
        )
    } else {
        let paths = container.manifest().members.iter();
        (
            json!({"kind": "all"}),
            paths.map(|member| member.path.as_str()).collect(),
        )
    };
    let receipt = container.receipt();
    let stage_directory =
        tempfile::tempdir_in(directory.parent().ok_or("missing directory parent")?)?;
    write_new(
        &stage_directory.path().join("manifest.json"),
        container.manifest_bytes(),
    )?;
    let mut members = Vec::with_capacity(selected.len());
    for (index, path) in selected.iter().enumerate() {
        let file = format!("member-{index:06}.bin");
        let mut staged = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage_directory.path().join(&file))?;
        // A failed copy leaves unverified bytes only in the discarded stage.
        container.copy_member_to(path, &mut staged)?;
        let declaration = container
            .member_digest(path)
            .ok_or("validated inventory member is absent")?;
        members.push(json!({
            "path": path, "file": file,
            "byte_length": declaration.byte_length, "sha256": declaration.sha256,
        }));
    }
    let owned = staged_training_inventory(stage_directory.path(), &directory)?;
    crate::publish_directory_noreplace(stage_directory.path(), &directory)?;
    let mut published = PublishedTrainingOutputs {
        owned,
        committed: false,
    };
    emit_json(
        values,
        &json!({
            "operation": "n4a-read",
            "directory": directory,
            "archive_sha256": receipt.container_sha256,
            "manifest_sha256": receipt.manifest_sha256,
            "byte_length": receipt.byte_length,
            "container_id": container.manifest().container_id,
            "manifest_file": "manifest.json",
            "selection": selection,
            "members": members,
        }),
    )?;
    published.committed = true;
    Ok(())
}

/// Write the owner's sealed bytes once, publish them without replacing an
/// existing file, then emit the report. A failed emit rolls the archive back.
fn publish_production(
    operation: &str,
    values: &BTreeMap<String, OsString>,
    destinations: &TrainingDestinations,
    production: &NativeProduction,
    context: Value,
) -> Result<()> {
    let container = &production.container;
    let receipt = container.receipt();
    let stage_directory = tempfile::tempdir_in(
        destinations
            .archive
            .parent()
            .ok_or("missing archive parent")?,
    )?;
    let stage = stage_directory.path().join("model.n4a");
    write_new(&stage, container.archive_bytes())?;
    let mut published = PublishedTrainingOutputs::publish(&stage, None, destinations)?;
    let mut report = json!({
        "operation": operation,
        "archive": destinations.archive,
        "archive_sha256": receipt.container_sha256,
        "byte_length": receipt.byte_length,
        "container_id": container.manifest().container_id,
        "entrypoints": container
            .manifest()
            .entrypoints
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        "training_source_id": production.training.source_id,
        "training_outcome": production.training.outcome,
    });
    if let (Some(report), Value::Object(context)) = (report.as_object_mut(), context) {
        report.extend(context);
    }
    emit_json(values, &report)?;
    published.committed = true;
    Ok(())
}

fn n4a_produce(values: &BTreeMap<String, OsString>, budget: N4aMemoryBudget) -> Result<()> {
    let destinations = training_destinations(values)?;
    let record = read_budgeted(Path::new(required(values, "input")?), budget, "record")?;
    let recipe_text = String::from_utf8(read_budgeted(
        Path::new(required(values, "recipe")?),
        budget,
        "recipe",
    )?)
    .map_err(|error| format!("recipe is not UTF-8: {error}"))?;
    // The owner parses and validates the recipe; this transport only forwards it.
    let recipe = parse_native_recipe_json(&recipe_text, budget)?;
    let source_id = text_option(values, "source-id")?;
    let run_id = text_option(values, "run-id")?;
    let embed = bool_option(values, "embed-training-record")?;
    let production = produce_n4a_native_with_view(NativeProduceRequest {
        record: &record,
        source_id,
        recipe: &recipe,
        runtime: NativeRuntime {
            library: Path::new(required(values, "methods-library")?),
            expected_sha256: text_option(values, "methods-library-sha256")?,
        },
        run_id,
        container_id: text_option(values, "container-id")?,
        embed_training_record: embed,
        memory_budget: budget,
    })?;
    publish_production(
        "n4a-produce",
        values,
        &destinations,
        &production,
        json!({"run_id": run_id, "source_id": source_id, "embed_training_record": embed}),
    )
}

/// Reorder the replay rows into the physical record order exactly once.
/// The physical IDs must be unique and the replay IDs must be the same set;
/// any missing, extra or repeated ID is refused instead of guessed.
fn align_rows_by_id(
    physical: &[String],
    replay_ids: &[String],
    rows: &[Vec<f64>],
) -> Result<Vec<Vec<f64>>> {
    if replay_ids.len() != rows.len() {
        return Err("prediction alignment refused: replay IDs and rows differ in count".into());
    }
    if physical.len() != replay_ids.len() {
        return Err(format!(
            "prediction alignment refused: {} physical samples but {} replay rows",
            physical.len(),
            replay_ids.len()
        )
        .into());
    }
    let mut position = BTreeMap::new();
    for (index, id) in replay_ids.iter().enumerate() {
        if position.insert(id.as_str(), index).is_some() {
            return Err(format!("prediction alignment refused: replay repeats `{id}`").into());
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut aligned = Vec::with_capacity(physical.len());
    for id in physical {
        if !seen.insert(id.as_str()) {
            return Err(
                format!("prediction alignment refused: physical order repeats `{id}`").into(),
            );
        }
        let index = position
            .get(id.as_str())
            .ok_or_else(|| format!("prediction alignment refused: replay has no row for `{id}`"))?;
        aligned.push(rows[*index].clone());
    }
    Ok(aligned)
}

/// Extract the single output/block of a serialized replay and align it.
/// Returns target names, target units (from the owner output binding) and the
/// rows in physical sample order. The replay value itself is never modified.
fn aligned_prediction(
    replay: &Value,
    physical: &[String],
) -> Result<(Vec<String>, Value, Vec<Vec<f64>>)> {
    let outputs = replay["outputs"]
        .as_array()
        .ok_or("prediction alignment refused: replay has no outputs")?;
    let [output] = outputs.as_slice() else {
        return Err(format!(
            "prediction alignment refused: exactly one replay output is required, found {}",
            outputs.len()
        )
        .into());
    };
    let blocks = output["predictions"]
        .as_array()
        .ok_or("prediction alignment refused: replay output has no prediction blocks")?;
    let [block] = blocks.as_slice() else {
        return Err(format!(
            "prediction alignment refused: exactly one prediction block is required, found {}",
            blocks.len()
        )
        .into());
    };
    let target_names: Vec<String> =
        serde_json::from_value(output["binding"]["target_names"].clone())?;
    let units = output["binding"]["target_units"].clone();
    if units
        .as_array()
        .is_none_or(|units| units.len() != target_names.len())
    {
        return Err("prediction alignment refused: target units do not match target names".into());
    }
    let block_names: Vec<String> =
        serde_json::from_value(block["target_names"].clone()).unwrap_or_default();
    if !block_names.is_empty() && block_names != target_names {
        return Err("prediction alignment refused: block and binding target names differ".into());
    }
    let ids: Vec<String> = serde_json::from_value(block["sample_ids"].clone())?;
    let rows: Vec<Vec<f64>> = serde_json::from_value(block["values"].clone())?;
    if rows.iter().any(|row| row.len() != target_names.len()) {
        return Err("prediction alignment refused: row width differs from target names".into());
    }
    Ok((
        target_names,
        units,
        align_rows_by_id(physical, &ids, &rows)?,
    ))
}

fn n4a_predict(values: &BTreeMap<String, OsString>, budget: N4aMemoryBudget) -> Result<()> {
    let file = open_canonical_archive(values, budget)?;
    require_expected_sha(&file, required(values, "expected-archive-sha256")?)?;
    // Owner actions take memory: this one read verifies the bytes it returns.
    let container = file.into_owned(budget)?;
    let external = values
        .get("input")
        .map(|path| read_budgeted(Path::new(path), budget, "record"))
        .transpose()?;
    let entrypoint = text_option(values, "entrypoint")?;
    let source_id = text_option(values, "source-id")?;
    let partition = text_option(values, "partition")?;
    let prediction = predict_n4a_native(
        &container,
        NativeActionRequest {
            entrypoint,
            input: match &external {
                Some(bytes) => NativeInput::External(bytes),
                None => NativeInput::Stored,
            },
            source_id,
            partition,
            schema_policy: NativeSchemaPolicy::RequireTrainingSchema,
            request_id: text_option(values, "request-id")?,
            run_id: text_option(values, "run-id")?,
            memory_budget: budget,
            runtime: NativeRuntime {
                library: Path::new(required(values, "methods-library")?),
                expected_sha256: text_option(values, "methods-library-sha256")?,
            },
        },
    )?;
    // Replay and receipt are serialized once and emitted unchanged.
    let replay = serde_json::to_value(&prediction.replay)?;
    let receipt = serde_json::to_value(&prediction.receipt)?;
    let (target_names, target_units, aligned) =
        aligned_prediction(&replay, &prediction.sample_ids)?;
    emit_json(
        values,
        &json!({
            "operation": "n4a-predict",
            "archive_sha256": container.receipt().container_sha256,
            "entrypoint": entrypoint,
            "input_mode": if external.is_some() { "external" } else { "stored" },
            "source_id": source_id,
            "partition": partition,
            "row_order": "physical_record",
            "sample_ids": prediction.sample_ids,
            "target_names": target_names,
            "target_units": target_units,
            "values": aligned,
            "replay": replay,
            "receipt": receipt,
        }),
    )
}

/// Retraining is owned by Core/DAG: the owner obtains the source ID and the
/// validated stored recipe from the source container and performs one
/// production. This transport has no source-ID or recipe option and no
/// training loop of its own.
fn n4a_retrain(values: &BTreeMap<String, OsString>, budget: N4aMemoryBudget) -> Result<()> {
    // The new container goes to --destination; --archive is the source.
    let mut publication = values.clone();
    publication.insert("archive".into(), required(values, "destination")?.clone());
    let destinations = training_destinations(&publication)?;
    let file = open_canonical_archive(values, budget)?;
    require_expected_sha(&file, required(values, "expected-archive-sha256")?)?;
    let source = file.into_owned(budget)?;
    let record = read_budgeted(Path::new(required(values, "input")?), budget, "record")?;
    let run_id = text_option(values, "run-id")?;
    let embed = bool_option(values, "embed-training-record")?;
    let production = retrain_n4a_native(
        &source,
        NativeRetrainRequest {
            record: &record,
            runtime: NativeRuntime {
                library: Path::new(required(values, "methods-library")?),
                expected_sha256: text_option(values, "methods-library-sha256")?,
            },
            run_id,
            container_id: text_option(values, "container-id")?,
            embed_training_record: embed,
            memory_budget: budget,
        },
    )?;
    publish_production(
        "n4a-retrain",
        values,
        &destinations,
        &production,
        json!({
            "run_id": run_id, "embed_training_record": embed,
            "source_archive_sha256": source.receipt().container_sha256,
        }),
    )
}

/// Execute the shared archive command transport from a CLI or host binding.
/// The arguments exclude the executable name and use the same strict parser.
pub fn execute_archive_command(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    options(args).and_then(|(command, values)| execute(&command, &values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn training_destinations_refuse_collisions_before_any_fit() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("model.n4a");
        let mut args = BTreeMap::from([("archive".into(), archive.clone().into_os_string())]);
        args.insert(
            "output".into(),
            directory.path().join("./model.n4a").into_os_string(),
        );
        assert!(training_destinations(&args).is_err());
        args.remove("output");
        args.insert("results-directory".into(), archive.clone().into_os_string());
        assert!(training_destinations(&args).is_err());
        args.insert(
            "results-directory".into(),
            directory.path().join("missing/results").into_os_string(),
        );
        assert!(training_destinations(&args).is_err());
        args.remove("results-directory");
        std::fs::write(&archive, b"existing").unwrap();
        assert!(training_destinations(&args).is_err());
        assert_eq!(std::fs::read(&archive).unwrap(), b"existing");
    }

    #[test]
    #[cfg(unix)]
    fn raced_results_publication_rolls_back_only_own_archive() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("model.n4a");
        let results = directory.path().join("results");
        let args = BTreeMap::from([
            ("archive".into(), archive.clone().into_os_string()),
            ("results-directory".into(), results.clone().into_os_string()),
        ]);
        let destinations = training_destinations(&args).unwrap();
        let stage = directory.path().join("stage.n4a");
        std::fs::write(&stage, b"complete archive").unwrap();
        let stage_results = directory.path().join("stage-results");
        std::fs::create_dir(&stage_results).unwrap();
        std::fs::write(stage_results.join("experiment.json"), b"verified").unwrap();
        // Another invocation reserves its target after our preflight.
        std::fs::create_dir(&results).unwrap();
        std::fs::write(results.join("owned-by-other"), b"preserve").unwrap();
        assert!(
            PublishedTrainingOutputs::publish(&stage, Some(&stage_results), &destinations).is_err()
        );
        assert!(!archive.exists());
        assert_eq!(
            std::fs::read(results.join("owned-by-other")).unwrap(),
            b"preserve"
        );
        assert!(stage_results.join("experiment.json").exists());
    }

    #[test]
    #[cfg(unix)]
    fn failed_emit_preserves_foreign_insertions_and_replacements_after_publication() {
        for change in [
            "insert",
            "replace-member",
            "replace-directory",
            "replace-archive",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let archive = directory.path().join("model.n4a");
            let results = directory.path().join("results");
            let output = directory.path().join("output.json");
            let args = BTreeMap::from([
                ("archive".into(), archive.clone().into_os_string()),
                ("results-directory".into(), results.clone().into_os_string()),
                ("output".into(), output.clone().into_os_string()),
            ]);
            let destinations = training_destinations(&args).unwrap();
            let stage = directory.path().join("stage.n4a");
            let stage_results = directory.path().join("stage-results");
            std::fs::write(&stage, b"complete archive").unwrap();
            std::fs::create_dir(&stage_results).unwrap();
            std::fs::write(stage_results.join("manifest.json"), b"verified").unwrap();
            std::fs::create_dir(stage_results.join("nested")).unwrap();
            std::fs::write(stage_results.join("nested/owned"), b"owned bytes").unwrap();
            let published =
                PublishedTrainingOutputs::publish(&stage, Some(&stage_results), &destinations)
                    .unwrap();
            match change {
                "insert" => std::fs::write(results.join("foreign"), b"preserve").unwrap(),
                "replace-member" => {
                    std::fs::rename(
                        results.join("manifest.json"),
                        directory.path().join("original-manifest"),
                    )
                    .unwrap();
                    // Identical bytes defeat a content/size-only ownership test.
                    std::fs::write(results.join("manifest.json"), b"verified").unwrap();
                }
                "replace-directory" => {
                    std::fs::rename(&results, directory.path().join("original-results")).unwrap();
                    // Even a replacement empty directory belongs to its writer.
                    std::fs::create_dir(&results).unwrap();
                }
                "replace-archive" => {
                    std::fs::rename(&archive, directory.path().join("original-archive")).unwrap();
                    std::fs::write(&archive, b"complete archive").unwrap();
                }
                _ => unreachable!(),
            }
            // Inject a failure after publication, exactly at the final emit.
            std::fs::write(&output, b"foreign output").unwrap();
            assert!(emit_json(&args, &json!({"published": true})).is_err());
            drop(published);
            assert_eq!(std::fs::read(&output).unwrap(), b"foreign output");
            if change == "replace-archive" {
                assert_eq!(std::fs::read(&archive).unwrap(), b"complete archive");
                assert!(!results.exists());
            } else {
                assert!(!archive.exists());
                match change {
                    "insert" => {
                        assert_eq!(std::fs::read(results.join("foreign")).unwrap(), b"preserve");
                        assert!(!results.join("manifest.json").exists());
                        assert!(!results.join("nested").exists());
                    }
                    "replace-member" => {
                        assert_eq!(
                            std::fs::read(results.join("manifest.json")).unwrap(),
                            b"verified"
                        );
                        assert!(!results.join("nested").exists());
                    }
                    "replace-directory" => {
                        assert!(results.is_dir());
                        assert!(directory
                            .path()
                            .join("original-results/manifest.json")
                            .exists());
                    }
                    _ => unreachable!(),
                }
            }
            eprintln!("post-publication emit failure preserves foreign {change}");
        }
    }

    #[test]
    #[cfg(not(unix))]
    fn failed_emit_conservatively_preserves_published_paths_without_strong_identity() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("model.n4a");
        let stage = directory.path().join("stage.n4a");
        std::fs::write(&stage, b"complete archive").unwrap();
        let published = PublishedTrainingOutputs::publish(
            &stage,
            None,
            &TrainingDestinations {
                archive: archive.clone(),
                results: None,
            },
        )
        .unwrap();
        drop(published);
        assert_eq!(std::fs::read(archive).unwrap(), b"complete archive");
    }

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

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn n4a_commands_use_strict_options_and_retrain_has_no_recipe() {
        for args in [
            vec!["n4a-retrain", "--recipe", "recipe.json"],
            vec!["n4a-retrain", "--source-id", "spectra"],
            vec!["n4a-predict", "--phase", "FIT_CV"],
            vec!["n4a-predict", "--adapter", "borrowed-controller"],
            vec!["n4a-produce", "--dag-cli", "dag"],
            vec!["n4a-inspect", "--methods-library", "libn4m.so"],
            vec!["n4a-inspect", "--archive", "a", "--archive", "b"],
        ] {
            assert!(options(args.into_iter().map(OsString::from)).is_err());
        }
        for command in ["n4a-inspect", "n4a-produce", "n4a-predict", "n4a-retrain"] {
            assert!(options([OsString::from(command)]).is_ok());
        }
    }

    #[test]
    fn memory_budget_flag_is_optional_decimal_u64_on_every_n4a_command() {
        for command in [
            "n4a-inspect",
            "n4a-write",
            "n4a-read",
            "n4a-produce",
            "n4a-predict",
            "n4a-retrain",
        ] {
            let args = [command, "--memory-budget-bytes", "18446744073709551615"];
            let (_, values) = options(args.into_iter().map(OsString::from)).unwrap();
            assert_eq!(
                memory_budget(&values).unwrap(),
                N4aMemoryBudget::bytes(u64::MAX)
            );
        }
        let budget = |text: &str| {
            memory_budget(&BTreeMap::from([(
                "memory-budget-bytes".to_owned(),
                OsString::from(text),
            )]))
        };
        for bad in ["", "+1", "-1", "1.0", "1e9", " 1", "18446744073709551616"] {
            assert!(budget(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            memory_budget(&BTreeMap::new()).unwrap(),
            N4aMemoryBudget::host_allocator()
        );
    }

    #[test]
    fn n4a_inspect_refuses_a_non_container_without_output_or_legacy_loader() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("invalid.n4a");
        std::fs::write(&archive, b"not a canonical container").unwrap();
        let output = directory.path().join("must-not-exist.json");
        let args = BTreeMap::from([
            ("archive".to_owned(), archive.into_os_string()),
            ("output".to_owned(), output.clone().into_os_string()),
        ]);
        assert!(execute("n4a-inspect", &args).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn prediction_rows_are_aligned_once_by_physical_sample_id() {
        let physical = strings(&["c", "a", "b"]);
        // The owner replay keeps its own sorted order.
        let replay = strings(&["a", "b", "c"]);
        let rows = vec![vec![1.0, 10.0], vec![2.0, 20.0], vec![3.0, 30.0]];
        let aligned = align_rows_by_id(&physical, &replay, &rows).unwrap();
        assert_eq!(
            aligned,
            vec![vec![3.0, 30.0], vec![1.0, 10.0], vec![2.0, 20.0]]
        );
    }

    #[test]
    fn prediction_alignment_refuses_missing_extra_and_repeated_ids() {
        let rows = vec![vec![1.0], vec![2.0], vec![3.0]];
        let replay = strings(&["a", "b", "c"]);
        // Missing physical row, unknown physical ID, repeated physical ID.
        assert!(align_rows_by_id(&strings(&["a", "b"]), &replay, &rows).is_err());
        assert!(align_rows_by_id(&strings(&["a", "b", "x"]), &replay, &rows).is_err());
        assert!(align_rows_by_id(&strings(&["a", "a", "b"]), &replay, &rows).is_err());
        // Repeated replay ID and ragged ID/row counts.
        assert!(align_rows_by_id(&replay, &strings(&["a", "a", "c"]), &rows).is_err());
        assert!(align_rows_by_id(&replay, &replay, &rows[..2]).is_err());
    }

    #[test]
    fn aligned_prediction_preserves_names_units_and_requires_one_block() {
        let replay = json!({"outputs": [{
            "binding": {"target_names": ["protein", "moisture"], "target_units": ["g", null]},
            "predictions": [{
                "sample_ids": ["s-1", "s-2"], "target_names": ["protein", "moisture"],
                "values": [[1.0, 2.0], [3.0, 4.0]],
            }],
        }]});
        let (names, units, rows) = aligned_prediction(&replay, &strings(&["s-2", "s-1"])).unwrap();
        assert_eq!(names, strings(&["protein", "moisture"]));
        assert_eq!(units, json!(["g", null]));
        assert_eq!(rows, vec![vec![3.0, 4.0], vec![1.0, 2.0]]);
        let mut renamed = replay.clone();
        renamed["outputs"][0]["predictions"][0]["target_names"] = json!(["moisture", "protein"]);
        assert!(aligned_prediction(&renamed, &strings(&["s-2", "s-1"])).is_err());
        let mut two_blocks = replay.clone();
        let block = two_blocks["outputs"][0]["predictions"][0].clone();
        two_blocks["outputs"][0]["predictions"] = json!([block.clone(), block]);
        assert!(aligned_prediction(&two_blocks, &strings(&["s-2", "s-1"])).is_err());
        let mut narrow = replay;
        narrow["outputs"][0]["predictions"][0]["values"] = json!([[1.0], [3.0]]);
        assert!(aligned_prediction(&narrow, &strings(&["s-2", "s-1"])).is_err());
    }
}
