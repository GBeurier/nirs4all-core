args <- commandArgs(trailingOnly = TRUE)

arg_value <- function(flag) {
  idx <- match(flag, args)
  if (is.na(idx) || idx == length(args)) {
    return(NULL)
  }
  args[[idx + 1L]]
}

in_path <- arg_value("--in")
out_dir <- arg_value("--out")
strict <- identical(Sys.getenv("NIRS4ALL_LITE_REQUIRE_METHODS_PARITY"), "1")
if (is.null(out_dir) || !nzchar(out_dir)) {
  out_dir <- file.path(tempdir(), "nirs4all-r-run-save-pipeline")
}
if (is.null(in_path) || !nzchar(in_path)) {
  if (strict) {
    stop("missing --in argument", call. = FALSE)
  }
  message("Not running R pipeline save E2E: --in was not provided")
  quit(save = "no", status = 0, runLast = FALSE)
}
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

local_lib <- normalizePath(".r-parity-lib", mustWork = FALSE)
if (dir.exists(local_lib)) {
  .libPaths(c(local_lib, .libPaths()))
}
if (!requireNamespace("nirs4all", quietly = TRUE)) {
  if (strict) {
    stop("nirs4all R package is not installed; run `make test-r-parity` first", call. = FALSE)
  }
  message("Not running R pipeline save E2E: nirs4all R package is not installed")
  quit(save = "no", status = 0, runLast = FALSE)
}
if (!requireNamespace("n4m", quietly = TRUE)) {
  if (strict) {
    stop("n4m R package is not installed; run `make test-r-parity` first", call. = FALSE)
  }
  message("Not running R pipeline save E2E: n4m R package is not installed")
  quit(save = "no", status = 0, runLast = FALSE)
}
n4m_abi <- n4m::n4m_abi_version()
if (length(n4m_abi) < 1L || as.integer(n4m_abi[[1L]]) < 2L) {
  stop(sprintf("n4m ABI major >= 2 required, got %s", paste(n4m_abi, collapse = ".")), call. = FALSE)
}

prepared <- jsonlite::fromJSON(in_path, simplifyVector = FALSE)
if (!identical(prepared$schema_version, "n4a.e2e.r_dataset_io_pipeline/v2")) {
  stop(sprintf("unsupported prepared dataset schema: %s", prepared$schema_version), call. = FALSE)
}
if (!isTRUE(prepared$io_reshape$selected_values_preserved)) {
  stop("prepared dataset did not preserve selected IO values", call. = FALSE)
}
if (is.null(prepared$io$io_spec_sha256) || is.null(prepared$io_reshape$dataset_sha256)) {
  stop("prepared dataset is missing IO/package provenance hashes", call. = FALSE)
}
pipeline_path <- prepared$source$pipeline
result <- nirs4all::nirs4all_run_portable_pipeline(pipeline_path, prepared$dataset)

pipeline <- nirs4all::nirs4all_load_pipeline(pipeline_path)
finite_predictions <- all(is.finite(as.numeric(unlist(result$selected$predictions, use.names = FALSE))))
finite_targets <- all(is.finite(as.numeric(unlist(result$targets, use.names = FALSE))))
if (!finite_predictions || !finite_targets) {
  stop("R pipeline produced non-finite predictions or targets", call. = FALSE)
}
workspace <- list(
  schema_version = "n4a.e2e.r_workspace/v1",
  status = "passed",
  engine = list(
    nirs4all_r = as.character(utils::packageVersion("nirs4all")),
    n4m = as.character(utils::packageVersion("n4m")),
    n4m_abi = paste(n4m_abi, collapse = ".")
  ),
  source = list(
    prepared_dataset = normalizePath(in_path, mustWork = TRUE),
    pipeline = normalizePath(pipeline_path, mustWork = TRUE),
    dataset_id = prepared$source$dataset_id,
    dataset_source = prepared$source$source,
    io_spec_sha256 = prepared$io$io_spec_sha256,
    dataset_sha256 = prepared$io_reshape$dataset_sha256
  ),
  io = prepared$io,
  io_reshape = prepared$io_reshape,
  result = result
)
predictions <- list(
  schema_version = "n4a.e2e.r_predictions/v1",
  status = "passed",
  selected_n_components = result$selected$n_components,
  targets = result$targets,
  predictions = result$selected$predictions,
  checks = list(
    finite_predictions = finite_predictions,
    finite_targets = finite_targets,
    variants = length(result$variants),
    dataset_rows = prepared$dataset$rows,
    dataset_cols = prepared$dataset$cols
  ),
  variants = lapply(result$variants, function(item) {
    list(n_components = item$n_components, rmse = item$rmse)
  })
)

jsonlite::write_json(workspace, file.path(out_dir, "workspace.n4a.json"), auto_unbox = TRUE, pretty = TRUE)
jsonlite::write_json(
  list(
    schema_version = "n4a.e2e.r_pipeline/v1",
    status = "passed",
    definition = unclass(pipeline)
  ),
  file.path(out_dir, "pipeline.n4a.json"),
  auto_unbox = TRUE,
  pretty = TRUE
)
jsonlite::write_json(predictions, file.path(out_dir, "r-predictions.json"), auto_unbox = TRUE, pretty = TRUE)
