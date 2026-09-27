//! Generic n4m role recipes and trained pipeline envelopes (version 8).
//!
//! A recipe step is the language-neutral token `"n4m:<catalog method id>"`
//! (a string, or `{"class": "n4m:<id>", "params": {...}}`), resolved through
//! the native n4m manifest. [`N4mRolePipeline`] fits such a recipe (sample
//! filters, transformers and selectors, then one regressor or classifier) and
//! exchanges every fitted step as its native N4ME state, so the Python, R,
//! JS/WASM and Rust bindings replay one another's pipelines. Parameters,
//! role checks and all numerics stay in libn4m; the runtime is selected with
//! `n4m::configure_library` or `N4M_LIBRARY_PATH`. A process fixes a single
//! libn4m file, so a process that also replays Archive V2 selects it once
//! through [`crate::preflight_methods_archive_v2_library`], whose attested
//! snapshot then serves both the role recipes and the replay.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use n4m::roles::{
    self, Estimator, FitInput, FitInputs, InputRequirement, MethodInfo, MethodKind, ParamType,
    ParamValue, Params,
};
use n4m::{Context, MatrixRef};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const N4M_ROLE_PREFIX: &str = "n4m:";
pub const N4M_TRAINED_PIPELINE_SCHEMA: &str = "nirs4all.n4m.trained_pipeline.v8";

const RECIPE_ROLES: &[(u32, &str)] = &[
    (roles::ROLE_TRANSFORMER, "transformer"),
    (roles::ROLE_REGRESSOR, "regressor"),
    (roles::ROLE_CLASSIFIER, "classifier"),
    (roles::ROLE_SELECTOR, "selector"),
    (roles::ROLE_SAMPLE_FILTER, "sample_filter"),
];
const RECIPE_ROLE_MASK: u32 = roles::ROLE_TRANSFORMER
    | roles::ROLE_REGRESSOR
    | roles::ROLE_CLASSIFIER
    | roles::ROLE_SELECTOR
    | roles::ROLE_SAMPLE_FILTER;

/// Catalog method id of an `n4m:<id>` class name.
pub fn n4m_role_method_id(class_name: &str) -> Option<&str> {
    class_name.strip_prefix(N4M_ROLE_PREFIX)
}

/// Manifest entry of an `n4m:<id>` class name.
pub fn resolve_n4m_role(class_name: &str) -> Result<MethodInfo, String> {
    let method_id = n4m_role_method_id(class_name)
        .ok_or_else(|| format!("'{class_name}' is not an n4m:<method id> token"))?;
    roles::method_info(method_id)
        .map_err(|error| format!("unknown n4m role token '{class_name}': {error}"))
}

/// Recipe steps the native manifest offers, in catalog order.
pub fn n4m_role_capabilities() -> Result<Value, String> {
    let methods = roles::methods().map_err(|error| error.to_string())?;
    Ok(Value::Array(
        methods
            .iter()
            .filter(|info| info.kind == MethodKind::Estimator && info.roles & RECIPE_ROLE_MASK != 0)
            .map(|info| {
                json!({
                    "token": format!("{N4M_ROLE_PREFIX}{}", info.method_id),
                    "method_id": info.method_id,
                    "roles": RECIPE_ROLES
                        .iter()
                        .filter(|(bit, _)| info.roles & bit != 0)
                        .map(|(_, name)| *name)
                        .collect::<Vec<_>>(),
                    "parameters": info.params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect(),
    ))
}

/// Training target of a recipe.
#[derive(Debug, Clone, PartialEq)]
pub enum RoleTarget {
    /// Row-major responses, `cols` per row (regressors).
    Values { data: Vec<f64>, cols: usize },
    /// Class label names (classifiers); encoded as ids in sorted order.
    Labels(Vec<String>),
    /// Integer class ids (classifiers), passed through.
    ClassIds(Vec<i64>),
}

/// Predictions of the final step.
#[derive(Debug, Clone, PartialEq)]
pub enum RolePredictions {
    Values {
        data: Vec<f64>,
        rows: usize,
        cols: usize,
    },
    Labels(Vec<String>),
    ClassIds(Vec<i64>),
}

struct RecipeStep {
    info: MethodInfo,
    params: Value,
}

impl RecipeStep {
    fn parse(token: &Value) -> Result<Self, String> {
        let (class_name, params) = match token {
            Value::String(name) => (name.as_str(), Value::Null),
            Value::Object(map) => (
                map.get("class").and_then(Value::as_str).ok_or_else(|| {
                    format!("n4m role recipe step needs a string 'class': {token}")
                })?,
                map.get("params").cloned().unwrap_or(Value::Null),
            ),
            _ => return Err(format!("n4m role recipe step must be a token: {token}")),
        };
        if n4m_role_method_id(class_name).is_none() {
            return Err(format!(
                "portable n4m role recipes contain n4m:<method id> steps only, got {token}"
            ));
        }
        Ok(Self {
            info: resolve_n4m_role(class_name)?,
            params,
        })
    }

    fn has_role(&self, role: u32) -> bool {
        self.info.roles & role != 0
    }

    fn is_sample_filter(&self) -> bool {
        self.has_role(roles::ROLE_SAMPLE_FILTER)
    }

    fn needs_y(&self) -> bool {
        self.info.input(FitInput::Y) == InputRequirement::Required
    }

    fn estimator(&self, ctx: &Context) -> Result<Estimator, String> {
        let mut params = Params::new(ctx, &self.info.method_id).map_err(native)?;
        match &self.params {
            Value::Null => {}
            Value::Object(values) => {
                for (name, value) in values {
                    let param = self
                        .info
                        .params
                        .iter()
                        .find(|param| &param.name == name)
                        .ok_or_else(|| {
                            format!("{}: unknown parameter '{name}'", self.info.method_id)
                        })?;
                    let typed = param_value(param.param_type, value).ok_or_else(|| {
                        format!(
                            "{}: invalid value for '{name}': {value}",
                            self.info.method_id
                        )
                    })?;
                    params.set(name, &typed).map_err(native)?;
                }
            }
            other => {
                return Err(format!(
                    "{}: params must be a mapping, got {other}",
                    self.info.method_id
                ))
            }
        }
        Estimator::new(ctx, &self.info.method_id, Some(&params)).map_err(native)
    }
}

fn param_value(kind: ParamType, value: &Value) -> Option<ParamValue> {
    let int = |value: &Value| {
        value.as_i64().or_else(|| {
            value
                .as_f64()
                .filter(|v| v.fract() == 0.0 && v.abs() < 9.0e15)
                .map(|v| v as i64)
        })
    };
    Some(match kind {
        ParamType::Int => ParamValue::Int(int(value)?),
        ParamType::Double => ParamValue::Double(value.as_f64()?),
        ParamType::Bool => ParamValue::Bool(value.as_bool()?),
        ParamType::Enum => ParamValue::Enum(value.as_str()?.to_owned()),
        ParamType::IntArray => ParamValue::IntArray(
            value
                .as_array()?
                .iter()
                .map(int)
                .collect::<Option<Vec<_>>>()?,
        ),
        ParamType::DoubleArray => ParamValue::DoubleArray(
            value
                .as_array()?
                .iter()
                .map(Value::as_f64)
                .collect::<Option<Vec<_>>>()?,
        ),
    })
}

fn native(error: n4m::Error) -> String {
    error.to_string()
}

fn recipe_steps(recipe: &Value) -> Result<Vec<RecipeStep>, String> {
    recipe
        .get("pipeline")
        .and_then(Value::as_array)
        .ok_or_else(|| "an n4m role recipe is {\"pipeline\": [steps]}".to_string())?
        .iter()
        .map(RecipeStep::parse)
        .collect()
}

fn keep_rows<T: Copy>(data: &[T], cols: usize, keep: &[bool]) -> Vec<T> {
    data.chunks(cols)
        .zip(keep)
        .filter(|(_, keep)| **keep)
        .flat_map(|(row, _)| row.iter().copied())
        .collect()
}

fn matrix(data: &[f64], rows: usize, cols: usize) -> Result<MatrixRef<'_>, String> {
    MatrixRef::row_major(data, rows, cols).map_err(native)
}

struct FittedStep {
    estimator: Estimator,
    method_id: String,
    classifier: bool,
    class_names: Option<Vec<String>>,
}

/// A fitted recipe of n4m role steps, portable as N4ME states.
pub struct N4mRolePipeline {
    recipe: Value,
    n_features: usize,
    steps: Vec<FittedStep>,
}

impl N4mRolePipeline {
    /// Fit every step of `recipe` natively on row-major `x` (`rows` x `cols`).
    pub fn fit_recipe(
        recipe: &Value,
        x: &[f64],
        rows: usize,
        cols: usize,
        y: &RoleTarget,
    ) -> Result<Self, String> {
        let parsed = recipe_steps(recipe)?;
        let (last, intermediate) = parsed
            .split_last()
            .ok_or("a portable n4m role recipe ends with one regressor or classifier")?;
        let classifier = last.has_role(roles::ROLE_CLASSIFIER);
        if !classifier && !last.has_role(roles::ROLE_REGRESSOR) {
            return Err("a portable n4m role recipe ends with one regressor or classifier".into());
        }
        matrix(x, rows, cols)?;
        let ctx = Context::new().map_err(native)?;
        let (mut values, mut rows, mut width) = (x.to_vec(), rows, cols);
        let (mut target, target_cols) = match y {
            RoleTarget::Values { data, cols } => (data.clone(), *cols),
            RoleTarget::Labels(_) | RoleTarget::ClassIds(_) => (Vec::new(), 0),
        };
        let (mut class_ids, class_names) = match y {
            RoleTarget::Values { .. } => (Vec::new(), None),
            RoleTarget::ClassIds(ids) => (ids.clone(), None),
            RoleTarget::Labels(labels) => {
                let mut names = labels.clone();
                names.sort();
                names.dedup();
                let ids = labels
                    .iter()
                    .map(|label| names.binary_search(label).map(|index| index as i64))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "label encoding failed".to_string())?;
                (ids, Some(names))
            }
        };
        if classifier == matches!(y, RoleTarget::Values { .. }) {
            return Err(
                "a classifier recipe needs labels and a regressor recipe needs values".into(),
            );
        }
        let target_rows = if classifier {
            class_ids.len()
        } else {
            target.len() / target_cols.max(1)
        };
        if target_rows != rows || (!classifier && target_cols == 0) {
            return Err(format!(
                "target rows ({target_rows}) must match X rows ({rows})"
            ));
        }

        let mut steps = Vec::new();
        for step in intermediate {
            let mut estimator = step.estimator(&ctx)?;
            let x_view = matrix(&values, rows, width)?;
            let y_view = if step.needs_y() {
                if classifier {
                    return Err(format!(
                        "{} needs a numeric target, but the recipe is a classifier",
                        step.info.method_id
                    ));
                }
                Some(matrix(&target, rows, target_cols)?)
            } else {
                None
            };
            let inputs = match y_view {
                Some(y_view) => FitInputs::new(x_view).y(y_view),
                None => FitInputs::new(x_view),
            };
            estimator.fit(&ctx, &inputs).map_err(native)?;
            if step.is_sample_filter() {
                let keep = estimator.apply_mask(&ctx, x_view, y_view).map_err(native)?;
                values = keep_rows(&values, width, &keep);
                if classifier {
                    class_ids = keep_rows(&class_ids, 1, &keep);
                } else {
                    target = keep_rows(&target, target_cols, &keep);
                }
                rows = keep.iter().filter(|keep| **keep).count();
            } else if step.has_role(roles::ROLE_TRANSFORMER | roles::ROLE_SELECTOR) {
                let transformed = estimator.transform(&ctx, x_view).map_err(native)?;
                (values, width) = (transformed.data, transformed.cols);
                steps.push(FittedStep {
                    estimator,
                    method_id: step.info.method_id.clone(),
                    classifier: false,
                    class_names: None,
                });
            } else {
                return Err(format!(
                    "{} is not a portable pipeline step",
                    step.info.method_id
                ));
            }
        }

        let mut estimator = last.estimator(&ctx)?;
        let x_view = matrix(&values, rows, width)?;
        if classifier {
            estimator
                .fit(&ctx, &FitInputs::new(x_view).labels(&class_ids))
                .map_err(native)?;
        } else {
            estimator
                .fit(
                    &ctx,
                    &FitInputs::new(x_view).y(matrix(&target, rows, target_cols)?),
                )
                .map_err(native)?;
        }
        steps.push(FittedStep {
            estimator,
            method_id: last.info.method_id.clone(),
            classifier,
            class_names,
        });
        Ok(Self {
            recipe: recipe.clone(),
            n_features: cols,
            steps,
        })
    }

    /// Read a version 8 envelope and rebuild its estimators from N4ME bytes.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let document: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if document.get("schema").and_then(Value::as_str) != Some(N4M_TRAINED_PIPELINE_SCHEMA) {
            return Err("unsupported trained n4m pipeline envelope".into());
        }
        let recipe = document
            .get("recipe")
            .ok_or_else(|| "envelope has no recipe".to_string())?;
        let n_features = document
            .get("n_features")
            .and_then(Value::as_u64)
            .ok_or_else(|| "envelope has no n_features".to_string())?
            as usize;
        let states = document
            .get("states")
            .and_then(Value::as_array)
            .ok_or_else(|| "envelope has no states".to_string())?;
        let parsed = recipe_steps(recipe)?;
        if !parsed.last().is_some_and(|step| {
            !step.is_sample_filter()
                && step.has_role(roles::ROLE_REGRESSOR | roles::ROLE_CLASSIFIER)
        }) {
            return Err("a portable n4m role recipe ends with one regressor or classifier".into());
        }
        let stateful: Vec<_> = parsed
            .iter()
            .filter(|step| !step.is_sample_filter())
            .collect();
        if stateful.len() != states.len() {
            return Err("envelope states do not match the recipe steps".into());
        }
        let last = stateful.len() - 1;
        let ctx = Context::new().map_err(native)?;
        let mut steps = Vec::with_capacity(states.len());
        for (index, (step, state)) in stateful.into_iter().zip(states).enumerate() {
            let method_id = state
                .get("method_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "N4ME state has no method_id".to_string())?;
            let payload = STANDARD
                .decode(
                    state
                        .get("n4me_base64")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                .map_err(|error| format!("N4ME state of {method_id} is not base64: {error}"))?;
            let digest = format!("{:x}", Sha256::digest(&payload));
            if state.get("sha256").and_then(Value::as_str) != Some(digest.as_str()) {
                return Err(format!("N4ME state of {method_id} fails its checksum"));
            }
            let estimator = Estimator::from_n4me(&ctx, &payload).map_err(native)?;
            let restored = estimator.method_id().map_err(native)?;
            if restored != method_id || method_id != step.info.method_id {
                return Err(format!(
                    "N4ME state {method_id} does not match its recipe step"
                ));
            }
            let class_names = match state.get("class_names") {
                None => None,
                Some(names) => Some(
                    names
                        .as_array()
                        .and_then(|names| {
                            names
                                .iter()
                                .map(|name| name.as_str().map(str::to_owned))
                                .collect::<Option<Vec<_>>>()
                        })
                        .ok_or_else(|| format!("class_names of {method_id} must be strings"))?,
                ),
            };
            steps.push(FittedStep {
                estimator,
                method_id: restored,
                classifier: index == last && step.has_role(roles::ROLE_CLASSIFIER),
                class_names,
            });
        }
        Ok(Self {
            recipe: recipe.clone(),
            n_features,
            steps,
        })
    }

    /// The version 8 envelope.
    pub fn to_json(&self) -> Result<String, String> {
        let ctx = Context::new().map_err(native)?;
        let states = self
            .steps
            .iter()
            .map(|step| {
                let payload = step.estimator.to_n4me(&ctx, true).map_err(native)?;
                let mut state = json!({
                    "method_id": step.method_id,
                    "n4me_base64": STANDARD.encode(&payload),
                    "sha256": format!("{:x}", Sha256::digest(&payload)),
                });
                if let Some(names) = &step.class_names {
                    state["class_names"] = json!(names);
                }
                Ok(state)
            })
            .collect::<Result<Vec<_>, String>>()?;
        serde_json::to_string_pretty(&json!({
            "schema": N4M_TRAINED_PIPELINE_SCHEMA,
            "recipe": self.recipe,
            "n_features": self.n_features,
            "states": states,
        }))
        .map_err(|error| error.to_string())
    }

    /// Predictions (regressor) or class labels (classifier) of the final model.
    pub fn predict(&self, x: &[f64], rows: usize) -> Result<RolePredictions, String> {
        matrix(x, rows, self.n_features)
            .map_err(|_| format!("expected {} input columns", self.n_features))?;
        let ctx = Context::new().map_err(native)?;
        let (last, transforms) = self
            .steps
            .split_last()
            .ok_or_else(|| "fitted pipeline has no steps".to_string())?;
        let (mut values, mut width) = (x.to_vec(), self.n_features);
        for step in transforms {
            let out = step
                .estimator
                .transform(&ctx, matrix(&values, rows, width)?)
                .map_err(native)?;
            (values, width) = (out.data, out.cols);
        }
        let x_view = matrix(&values, rows, width)?;
        if !last.classifier {
            let out = last.estimator.predict(&ctx, x_view).map_err(native)?;
            return Ok(RolePredictions::Values {
                data: out.data,
                rows: out.rows,
                cols: out.cols,
            });
        }
        let ids = last
            .estimator
            .predict_labels(&ctx, x_view)
            .map_err(native)?;
        match &last.class_names {
            None => Ok(RolePredictions::ClassIds(ids)),
            Some(names) => ids
                .iter()
                .map(|&id| {
                    usize::try_from(id)
                        .ok()
                        .and_then(|index| names.get(index).cloned())
                        .ok_or_else(|| format!("class id {id} has no label name"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(RolePredictions::Labels),
        }
    }

    pub fn recipe(&self) -> &Value {
        &self.recipe
    }

    pub fn n_features(&self) -> usize {
        self.n_features
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> bool {
        match std::env::var("NIRS4ALL_METHODS_LIB") {
            Ok(path) => {
                // Share the attested process identity of the Archive V2 tests
                // that run in this binary instead of a second libn4m file.
                crate::native_methods_replay::configure_methods_runtime_for_source(
                    std::path::Path::new(&path),
                )
                .expect("NIRS4ALL_METHODS_LIB must name libn4m");
                true
            }
            Err(error) => {
                if std::env::var("NIRS4ALL_CORE_REQUIRE_METHODS_PARITY").as_deref() == Ok("1") {
                    panic!("strict n4m role parity requires NIRS4ALL_METHODS_LIB: {error}");
                }
                eprintln!("skipping n4m role parity: NIRS4ALL_METHODS_LIB is not set");
                false
            }
        }
    }

    fn rows(value: &Value) -> (Vec<f64>, usize, usize) {
        let rows = value.as_array().unwrap();
        let data: Vec<f64> = rows
            .iter()
            .flat_map(|row| row.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()))
            .collect();
        let n = rows.len();
        (data.clone(), n, data.len() / n)
    }

    fn numbers(value: &Value) -> Vec<f64> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect()
    }

    fn strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }

    fn values(predictions: RolePredictions) -> Vec<f64> {
        match predictions {
            RolePredictions::Values { data, .. } => data,
            other => panic!("expected values, got {other:?}"),
        }
    }

    fn max_diff(actual: &[f64], expected: &[f64]) -> f64 {
        assert_eq!(actual.len(), expected.len());
        actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max)
    }

    /// Fits the recipe in Rust, round trips the v8 envelope and returns the
    /// replayed predictions (equal to the in-memory ones).
    fn refit(
        envelope: &Value,
        x: &[f64],
        n: usize,
        p: usize,
        y: &RoleTarget,
        x_test: &[f64],
        m: usize,
    ) -> RolePredictions {
        let fitted = N4mRolePipeline::fit_recipe(&envelope["recipe"], x, n, p, y).unwrap();
        let direct = fitted.predict(x_test, m).unwrap();
        let text = fitted.to_json().unwrap();
        let replayed = N4mRolePipeline::from_json(&text).unwrap();
        assert_eq!(replayed.n_features(), p);
        assert_eq!(replayed.recipe(), &envelope["recipe"]);
        let again = replayed.predict(x_test, m).unwrap();
        assert_eq!(again, direct);
        again
    }

    #[test]
    fn python_trained_v8_envelopes_replay_and_refit_in_rust() {
        if !configured() {
            return;
        }
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/parity/fixtures/n4m_roles_v8_python_trained.json"
        ))
        .unwrap();
        let (x, n, p) = rows(&fixture["x_train"]);
        let (x_test, m, _) = rows(&fixture["x_test"]);

        let regression = &fixture["regression"];
        let expected = numbers(&regression["predict"]);
        let replayed = N4mRolePipeline::from_json(&regression["envelope"].to_string()).unwrap();
        let diff = max_diff(&values(replayed.predict(&x_test, m).unwrap()), &expected);
        eprintln!("python regression replay max diff {diff:e}");
        assert!(diff <= 1e-12);
        let y = RoleTarget::Values {
            data: numbers(&regression["y_train"]),
            cols: 1,
        };
        let diff = max_diff(
            &values(refit(&regression["envelope"], &x, n, p, &y, &x_test, m)),
            &expected,
        );
        eprintln!("rust-fitted regression vs python max diff {diff:e}");
        assert!(diff <= 1e-9);

        let classification = &fixture["classification"];
        let expected = strings(&classification["predict"]);
        let replayed = N4mRolePipeline::from_json(&classification["envelope"].to_string()).unwrap();
        assert_eq!(
            replayed.predict(&x_test, m).unwrap(),
            RolePredictions::Labels(expected.clone())
        );
        let y = RoleTarget::Labels(strings(&classification["y_train"]));
        assert_eq!(
            refit(&classification["envelope"], &x, n, p, &y, &x_test, m),
            RolePredictions::Labels(expected)
        );
    }

    #[test]
    fn r_trained_v8_envelope_replays_and_refits_in_rust() {
        if !configured() {
            return;
        }
        let envelope: Value = serde_json::from_str(include_str!(
            "../tests/parity/fixtures/n4m_roles_v8_r_trained.json"
        ))
        .unwrap();
        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/parity/expected/n4m_roles_v8_r_trained_oracle.json"
        ))
        .unwrap();
        let (x, n, p) = rows(&oracle["x_train"]);
        let (x_test, m, _) = rows(&oracle["x_test"]);
        let expected = numbers(&oracle["predict"]);
        let replayed = N4mRolePipeline::from_json(&envelope.to_string()).unwrap();
        let diff = max_diff(&values(replayed.predict(&x_test, m).unwrap()), &expected);
        eprintln!("R regression replay max diff {diff:e}");
        assert!(diff <= 1e-12);
        let y = RoleTarget::Values {
            data: numbers(&oracle["y_train"]),
            cols: 1,
        };
        let diff = max_diff(
            &values(refit(&envelope, &x, n, p, &y, &x_test, m)),
            &expected,
        );
        eprintln!("rust-fitted regression vs R max diff {diff:e}");
        assert!(diff <= 1e-9);
    }

    #[test]
    fn envelopes_refuse_tampered_states_and_other_schemas() {
        if !configured() {
            return;
        }
        let envelope: Value = serde_json::from_str(include_str!(
            "../tests/parity/fixtures/n4m_roles_v8_r_trained.json"
        ))
        .unwrap();
        let mut other = envelope.clone();
        other["schema"] = json!("nirs4all.n4m.trained_pipeline.v7");
        assert!(N4mRolePipeline::from_json(&other.to_string()).is_err());
        let mut tampered = envelope.clone();
        tampered["states"][0]["sha256"] = json!("0".repeat(64));
        assert!(N4mRolePipeline::from_json(&tampered.to_string())
            .err()
            .unwrap()
            .contains("checksum"));
        let mut missing = envelope;
        missing["states"].as_array_mut().unwrap().pop();
        assert!(N4mRolePipeline::from_json(&missing.to_string()).is_err());
    }

    #[test]
    fn n4m_tokens_resolve_through_the_manifest() {
        if !configured() {
            return;
        }
        let definition = crate::load_pipeline_definition_str(
            r#"{"pipeline": ["n4m:preprocessing.scatter.snv", {"class": "n4m:models.pls.cppls", "params": {"n_components": 3}}]}"#,
        )
        .unwrap();
        assert_eq!(
            crate::portable_class_names(&definition),
            ["n4m:preprocessing.scatter.snv", "n4m:models.pls.cppls"]
        );
        let error = crate::load_pipeline_definition_str(r#"["n4m:not.a.method"]"#).unwrap_err();
        assert!(error.contains("n4m:not.a.method"), "{error}");
        assert!(crate::parse_execution_plan(&definition)
            .unwrap_err()
            .contains("N4mRolePipeline"));

        let capabilities = n4m_role_capabilities().unwrap();
        let cppls = capabilities
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["token"] == "n4m:models.pls.cppls")
            .expect("cppls is a recipe step");
        assert_eq!(cppls["roles"], json!(["regressor"]));
        assert!(cppls["parameters"]
            .as_array()
            .unwrap()
            .contains(&json!("n_components")));
    }
}
