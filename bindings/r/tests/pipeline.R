fixture <- system.file("extdata", "portable_methods_pipeline.json", package = "nirs4all")
yaml_fixture <- system.file("extdata", "portable_methods_pipeline.yaml", package = "nirs4all")

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
stopifnot(identical(unlist(sweep$`_grid_`$n_components, use.names = FALSE), c(2L, 4L, 6L, 8L, 10L)))

from_steps <- nirs4all::nirs4all_load_pipeline(list(steps = json_pipeline$pipeline))
from_list <- nirs4all::nirs4all_load_pipeline(json_pipeline$pipeline)
stopifnot(isTRUE(all.equal(from_steps$pipeline, json_pipeline$pipeline, check.attributes = FALSE)))
stopifnot(isTRUE(all.equal(from_list$pipeline, json_pipeline$pipeline, check.attributes = FALSE)))

err <- tryCatch(
  nirs4all::nirs4all_load_pipeline(list(pipeline = list(list(class = "sklearn.ensemble.RandomForestRegressor")))),
  error = identity
)
stopifnot(inherits(err, "error"))
