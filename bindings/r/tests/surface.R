library(nirs4all)

expected_exports <- c(
  "dag_ml",
  "dag_ml_data",
  "datasets",
  "formats",
  "io",
  "methods",
  "nirs4all_capability_manifest",
  "nirs4all_controller_capabilities",
  "nirs4all_load_pipeline",
  "nirs4all_parse_execution_plan",
  "nirs4all_portable_class_names",
  "nirs4all_require",
  "nirs4all_runtime_contracts",
  "nirs4all_run_portable_pipeline",
  "nirs4all_runtime_surfaces",
  "nirs4all_upstreams"
)

namespace <- asNamespace("nirs4all")
stopifnot(identical(packageDescription("nirs4all")$Package, "nirs4all"))
stopifnot(identical(sort(getNamespaceExports("nirs4all")), sort(expected_exports)))

for (name in expected_exports) {
  stopifnot(exists(name, envir = namespace, inherits = FALSE))
}

stopifnot(identical(nirs4all::formats, get("formats", envir = namespace)))
stopifnot(identical(nirs4all::methods, get("methods", envir = namespace)))

manifest <- nirs4all::nirs4all_capability_manifest()
stopifnot(identical(manifest$schema, "nirs4all-core.capabilities.v1"))
contracts <- nirs4all::nirs4all_runtime_contracts()
stopifnot(identical(manifest$runtime_contracts, contracts))
stopifnot(identical(
  vapply(contracts, function(item) item$surface, character(1)),
  c("python", "r", "javascript_wasm", "rust", "matlab_octave")
))
stopifnot(identical(
  vapply(contracts, function(item) isTRUE(item$serialized_model_predict), logical(1)),
  c(FALSE, FALSE, TRUE, FALSE, FALSE)
))
stopifnot(identical(
  nirs4all::nirs4all_runtime_surfaces(),
  c("python", "r", "javascript_wasm", "rust", "matlab_octave")
))
stopifnot(identical(
  vapply(manifest$controllers, function(item) item$id, character(1)),
  c(
    "split.kennard_stone",
    "preprocess.snv",
    "preprocess.savgol",
    "model.pls_regression",
    "pipeline.portable_methods"
  )
))
