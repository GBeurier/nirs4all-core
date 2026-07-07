NIRS4ALL_RUNTIME_SURFACES <- c(
  "python",
  "r",
  "javascript_wasm",
  "rust",
  "matlab_octave"
)

parity_runtime <- function() {
  stats::setNames(
    rep("parity-validated", length(NIRS4ALL_RUNTIME_SURFACES)),
    NIRS4ALL_RUNTIME_SURFACES
  )
}

nirs4all_runtime_surfaces <- function() {
  NIRS4ALL_RUNTIME_SURFACES
}

nirs4all_controller_capabilities <- function() {
  runtime <- parity_runtime()
  list(
    list(
      id = "split.kennard_stone",
      kind = "splitter",
      domain = "methods",
      label = "Kennard-Stone split",
      operator_classes = NIRS4ALL_KENNARD_STONE_CLASSES,
      ports = list(inputs = c("X"), outputs = c("train_indices", "test_indices")),
      parameters = c("test_size"),
      runtime = runtime,
      execution_path = "portable_pipeline"
    ),
    list(
      id = "preprocess.snv",
      kind = "transform",
      domain = "methods",
      label = "Standard normal variate",
      operator_classes = NIRS4ALL_SNV_CLASSES,
      ports = list(inputs = c("X"), outputs = c("X_transformed")),
      parameters = character(),
      runtime = runtime,
      execution_path = "portable_pipeline"
    ),
    list(
      id = "preprocess.savgol",
      kind = "transform",
      domain = "methods",
      label = "Savitzky-Golay",
      operator_classes = NIRS4ALL_SAVGOL_CLASSES,
      ports = list(inputs = c("X"), outputs = c("X_transformed")),
      parameters = c("window_length", "polyorder", "deriv", "mode", "cval"),
      runtime = runtime,
      execution_path = "portable_pipeline"
    ),
    list(
      id = "model.pls_regression",
      kind = "model",
      domain = "methods",
      label = "PLS regression",
      operator_classes = NIRS4ALL_PLS_CLASSES,
      ports = list(inputs = c("X", "y"), outputs = c("predictions", "model")),
      parameters = c("n_components", "_range_"),
      runtime = runtime,
      execution_path = "portable_pipeline"
    ),
    list(
      id = "pipeline.portable_methods",
      kind = "pipeline",
      domain = "methods",
      label = "Portable methods pipeline",
      operator_classes = character(),
      ports = list(
        inputs = c("pipeline", "dataset"),
        outputs = c("execution_result", "predictions", "model")
      ),
      parameters = character(),
      runtime = runtime,
      execution_path = "run_portable_pipeline",
      composes = c(
        "split.kennard_stone",
        "preprocess.snv",
        "preprocess.savgol",
        "model.pls_regression"
      )
    )
  )
}

nirs4all_capability_manifest <- function() {
  list(
    schema = "nirs4all-core.capabilities.v1",
    aggregate = "nirs4all-core",
    runtime_surfaces = nirs4all_runtime_surfaces(),
    portable_operator_classes = NIRS4ALL_PORTABLE_OPERATOR_CLASSES,
    controllers = nirs4all_controller_capabilities()
  )
}
