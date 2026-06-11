test_that("upstream registry exposes expected keys", {
  expect_equal(
    nirs4all_upstreams()$key,
    c("dag_ml", "dag_ml_data", "formats", "io", "datasets", "methods")
  )
})
