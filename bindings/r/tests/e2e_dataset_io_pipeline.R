args <- commandArgs(trailingOnly = TRUE)

arg_value <- function(flag) {
  idx <- match(flag, args)
  if (is.na(idx) || idx == length(args)) {
    return(NULL)
  }
  args[[idx + 1L]]
}

out_dir <- arg_value("--out")
if (is.null(out_dir) || !nzchar(out_dir)) {
  out_dir <- file.path(tempdir(), "nirs4all-r-dataset-io-pipeline")
}
dir.create(out_dir, recursive = TRUE, showWarnings = FALSE)

oracle_path <- Sys.getenv("NIRS4ALL_LITE_PARITY_ORACLE")
if (!nzchar(oracle_path)) {
  oracle_path <- file.path("tests", "parity", "expected", "portable_python_oracle.json")
}
pipeline_path <- system.file("extdata", "portable_methods_pipeline.json", package = "nirs4all")
if (!nzchar(pipeline_path)) {
  pipeline_path <- file.path("bindings", "r", "inst", "extdata", "portable_methods_pipeline.json")
}

if (!file.exists(pipeline_path)) {
  stop(sprintf("pipeline fixture not found: %s", pipeline_path), call. = FALSE)
}

if (file.exists(oracle_path)) {
  oracle <- jsonlite::fromJSON(oracle_path, simplifyVector = FALSE)
  dataset <- list(
    X = oracle$dataset$X,
    y = oracle$dataset$y,
    rows = oracle$dataset$rows,
    cols = oracle$dataset$cols
  )
} else {
  rows <- 40L
  cols <- 28L
  dataset <- list(
    X = lapply(seq_len(rows), function(row) {
      round(sin(seq_len(cols) / 5 + row / 7) + row / 20, 12)
    }),
    y = round(seq_len(rows) / 13 + cos(seq_len(rows) / 6), 12),
    rows = rows,
    cols = cols
  )
}

flat_x <- as.numeric(unlist(dataset$X, use.names = FALSE))
reshaped_x <- lapply(seq_len(dataset$rows), function(row) {
  offset <- (row - 1L) * dataset$cols
  flat_x[seq.int(offset + 1L, offset + dataset$cols)]
})

payload <- list(
  schema_version = "n4a.e2e.r_dataset_io_pipeline/v1",
  status = "prepared",
  source = list(
    oracle = if (file.exists(oracle_path)) normalizePath(oracle_path, mustWork = TRUE) else NULL,
    pipeline = normalizePath(pipeline_path, mustWork = TRUE)
  ),
  dataset = list(
    X = reshaped_x,
    y = dataset$y,
    rows = dataset$rows,
    cols = dataset$cols
  ),
  io_reshape = list(
    from = "flat-row-major",
    to = "list-of-row-vectors",
    values_preserved = identical(flat_x, as.numeric(unlist(reshaped_x, use.names = FALSE)))
  )
)
if (!isTRUE(payload$io_reshape$values_preserved)) {
  stop("IO reshape changed spectral values", call. = FALSE)
}

jsonlite::write_json(
  payload,
  file.path(out_dir, "reshaped-dataset.json"),
  auto_unbox = TRUE,
  pretty = TRUE
)
