args <- commandArgs(trailingOnly = TRUE)

arg_value <- function(flag) {
  idx <- match(flag, args)
  if (is.na(idx) || idx == length(args)) {
    stop(sprintf("missing %s argument", flag), call. = FALSE)
  }
  args[[idx + 1L]]
}

in_path <- arg_value("--in")
out_dir <- arg_value("--out")
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

local_lib <- normalizePath(".r-parity-lib", mustWork = FALSE)
if (dir.exists(local_lib)) {
  .libPaths(c(local_lib, .libPaths()))
}
if (!requireNamespace("nirs4all", quietly = TRUE)) {
  stop("nirs4all R package is not installed; run `make test-r-parity` first", call. = FALSE)
}
if (!requireNamespace("n4m", quietly = TRUE)) {
  stop("n4m R package is not installed; run `make test-r-parity` first", call. = FALSE)
}
n4m_abi <- n4m::n4m_abi_version()
if (length(n4m_abi) < 1L || as.integer(n4m_abi[[1L]]) < 2L) {
  stop(sprintf("n4m ABI major >= 2 required, got %s", paste(n4m_abi, collapse = ".")), call. = FALSE)
}

prepared <- jsonlite::fromJSON(in_path, simplifyVector = FALSE)
pipeline_path <- prepared$source$pipeline
result <- nirs4all::nirs4all_run_portable_pipeline(pipeline_path, prepared$dataset)

pipeline <- nirs4all::nirs4all_load_pipeline(pipeline_path)
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
    pipeline = normalizePath(pipeline_path, mustWork = TRUE)
  ),
  result = result
)
predictions <- list(
  schema_version = "n4a.e2e.r_predictions/v1",
  status = "passed",
  selected_n_components = result$selected$n_components,
  targets = result$targets,
  predictions = result$selected$predictions,
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
