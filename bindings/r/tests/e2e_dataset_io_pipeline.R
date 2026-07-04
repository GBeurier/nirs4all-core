args <- commandArgs(trailingOnly = TRUE)

arg_value <- function(flag) {
  idx <- match(flag, args)
  if (is.na(idx) || idx == length(args)) {
    return(NULL)
  }
  args[[idx + 1L]]
}

require_field <- function(value, name) {
  if (is.null(value)) {
    stop(sprintf("missing required field: %s", name), call. = FALSE)
  }
  value
}

out_dir <- arg_value("--out")
if (is.null(out_dir) || !nzchar(out_dir)) {
  out_dir <- file.path(tempdir(), "nirs4all-r-dataset-io-pipeline")
}
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

python <- Sys.getenv("PYTHON", "python3.11")
helper <- file.path("scripts", "e2e", "prepare_r_dataset_io_pipeline.py")
if (!file.exists(helper)) {
  stop(sprintf("dataset/io prepare helper not found: %s", helper), call. = FALSE)
}

status <- system2(
  python,
  c(helper, "--out", out_dir),
  stdout = "",
  stderr = ""
)
if (!identical(status, 0L)) {
  stop(sprintf("dataset/io prepare helper failed with status %s", status), call. = FALSE)
}

prepared_path <- file.path(out_dir, "reshaped-dataset.json")
if (!file.exists(prepared_path)) {
  stop(sprintf("dataset/io prepare helper did not write %s", prepared_path), call. = FALSE)
}
prepared <- jsonlite::fromJSON(prepared_path, simplifyVector = FALSE)

stopifnot(identical(prepared$schema_version, "n4a.e2e.r_dataset_io_pipeline/v2"))
stopifnot(identical(prepared$status, "prepared"))
invisible(require_field(prepared$source$dataset_id, "source.dataset_id"))
invisible(require_field(prepared$provider_contract$provider, "provider_contract.provider"))
invisible(require_field(prepared$io$io_spec_sha256, "io.io_spec_sha256"))
stopifnot(isTRUE(prepared$io_reshape$selected_values_preserved))
stopifnot(length(prepared$dataset$X) == prepared$dataset$rows)
stopifnot(length(prepared$dataset$y) == prepared$dataset$rows)
stopifnot(length(prepared$dataset$X[[1L]]) == prepared$dataset$cols)

session_payload <- list(
  schema_version = "n4a.e2e.r_session/v1",
  status = "prepared",
  r_version = as.character(getRversion()),
  platform = R.version$platform,
  attached = capture.output(sessionInfo()),
  prepared_dataset = normalizePath(prepared_path, mustWork = TRUE)
)
jsonlite::write_json(
  session_payload,
  file.path(out_dir, "r-session-info.json"),
  auto_unbox = TRUE,
  pretty = TRUE
)
