fixture <- system.file("extdata", "portable_methods_pipeline.json", package = "nirs4all")
yaml_fixture <- system.file("extdata", "portable_methods_pipeline.yaml", package = "nirs4all")
fixture_dir <- dirname(fixture)

fixture_names <- sort(sub("\\.json$", "", basename(list.files(fixture_dir, pattern = "^portable_.*\\.json$", full.names = TRUE))))
stopifnot(length(fixture_names) >= 4L)
for (fixture_name in fixture_names) {
  json_path <- file.path(fixture_dir, paste0(fixture_name, ".json"))
  yaml_path <- file.path(fixture_dir, paste0(fixture_name, ".yaml"))
  json_definition <- nirs4all::nirs4all_load_pipeline(json_path)
  yaml_definition <- nirs4all::nirs4all_load_pipeline(yaml_path)
  stopifnot(isTRUE(all.equal(json_definition, yaml_definition, check.attributes = FALSE)))
  stopifnot(length(nirs4all::nirs4all_portable_class_names(json_definition)) > 0L)
}

json_pipeline <- nirs4all::nirs4all_load_pipeline(fixture)
yaml_pipeline <- nirs4all::nirs4all_load_pipeline(yaml_fixture)

stopifnot(isTRUE(all.equal(json_pipeline, yaml_pipeline, check.attributes = FALSE)))
stopifnot(identical(json_pipeline$random_state, 42L))
stopifnot(identical(
  nirs4all::nirs4all_portable_class_names(json_pipeline),
  c(
    "nirs4all.operators.splitters.KennardStoneSplitter",
    "nirs4all.operators.transforms.StandardNormalVariate",
    "nirs4all.operators.transforms.SavitzkyGolay",
    "sklearn.cross_decomposition.PLSRegression"
  )
))

sweep <- json_pipeline$pipeline[[4]]
stopifnot(identical(sweep$param, "n_components"))
stopifnot(identical(unlist(sweep$`_range_`, use.names = FALSE), c(2L, 11L, 2L)))

from_steps <- nirs4all::nirs4all_load_pipeline(list(steps = json_pipeline$pipeline))
from_list <- nirs4all::nirs4all_load_pipeline(json_pipeline$pipeline)
stopifnot(isTRUE(all.equal(from_steps$pipeline, json_pipeline$pipeline, check.attributes = FALSE)))
stopifnot(isTRUE(all.equal(from_list$pipeline, json_pipeline$pipeline, check.attributes = FALSE)))

err <- tryCatch(
  nirs4all::nirs4all_load_pipeline(list(pipeline = list(list(class = "sklearn.ensemble.RandomForestRegressor")))),
  error = identity
)
stopifnot(inherits(err, "error"))
