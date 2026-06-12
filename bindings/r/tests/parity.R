strict <- identical(Sys.getenv("NIRS4ALL_LITE_REQUIRE_METHODS_PARITY"), "1")

if (!requireNamespace("n4m", quietly = TRUE)) {
  if (strict) stop("n4m R binding is required for strict parity")
  message("Skipping R execution parity: n4m is not installed")
} else {
  oracle_path <- Sys.getenv("NIRS4ALL_LITE_PARITY_ORACLE")
  if (!nzchar(oracle_path)) {
    oracle_path <- file.path("tests", "parity", "expected", "portable_python_oracle.json")
  }
  if (!file.exists(oracle_path)) {
    if (strict) stop(sprintf("Portable parity oracle not found: %s", oracle_path))
    message("Skipping R execution parity: oracle is not available")
  } else {
    fixture_root <- Sys.getenv("NIRS4ALL_LITE_PARITY_FIXTURES")
    if (!nzchar(fixture_root)) {
      fixture_root <- system.file("extdata", package = "nirs4all")
    }

    numeric_vec <- function(value) as.numeric(unlist(value, use.names = FALSE))
    integer_vec <- function(value) as.integer(unlist(value, use.names = FALSE))
    max_abs_diff <- function(actual, expected) {
      actual <- numeric_vec(actual)
      expected <- numeric_vec(expected)
      stopifnot(length(actual) == length(expected))
      if (length(actual) == 0L) return(0)
      max(abs(actual - expected))
    }
    expect_same_split <- function(actual, expected) {
      stopifnot(identical(actual$kind, expected$kind))
      stopifnot(identical(actual$trainIndices, integer_vec(expected$trainIndices)))
      stopifnot(identical(actual$testIndices, integer_vec(expected$testIndices)))
    }

    oracle <- jsonlite::fromJSON(oracle_path, simplifyVector = FALSE)
    dataset <- list(
      X = oracle$dataset$X,
      y = oracle$dataset$y,
      rows = oracle$dataset$rows,
      cols = oracle$dataset$cols
    )
    tol <- oracle$metadata$tolerances
    stopifnot(length(oracle$cases) >= 4L)

    for (expected in oracle$cases) {
      fixture <- file.path(fixture_root, paste0(expected$name, ".json"))
      stopifnot(file.exists(fixture))
      actual <- nirs4all::nirs4all_run_portable_pipeline(fixture, dataset)

      expect_same_split(actual$split, expected$split)
      stopifnot(max_abs_diff(actual$targets, expected$targets) <= tol$targets_abs)
      stopifnot(length(actual$variants) == length(expected$variants))
      for (i in seq_along(expected$variants)) {
        stopifnot(identical(actual$variants[[i]]$n_components,
                            as.integer(expected$variants[[i]]$n_components)))
        stopifnot(abs(actual$variants[[i]]$rmse - expected$variants[[i]]$rmse) <= tol$rmse_abs)
        stopifnot(
          max_abs_diff(actual$variants[[i]]$predictions, expected$variants[[i]]$predictions) <=
            tol$predictions_abs
        )
      }
      stopifnot(identical(actual$selected$n_components,
                          as.integer(expected$selected$n_components)))
    }
  }
}
