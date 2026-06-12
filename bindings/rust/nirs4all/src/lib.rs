//! Rust surface for the nirs4all-lite aggregate distribution.
//!
//! This crate intentionally starts as a registry and namespace layer. Runtime
//! functionality must delegate to the owning upstream crates.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upstream {
    pub key: &'static str,
    pub package: &'static str,
    pub role: &'static str,
}

pub const UPSTREAMS: &[Upstream] = &[
    Upstream {
        key: "dag_ml",
        package: "dag-ml",
        role: "Leakage-safe DAG/ML execution coordinator",
    },
    Upstream {
        key: "dag_ml_data",
        package: "dag-ml-data",
        role: "Sample-aligned data contracts for DAG/ML runtimes",
    },
    Upstream {
        key: "formats",
        package: "nirs4all-formats",
        role: "Spectroscopy/NIRS vendor file readers",
    },
    Upstream {
        key: "io",
        package: "nirs4all-io",
        role: "Dataset assembly bridge",
    },
    Upstream {
        key: "datasets",
        package: "nirs4all-datasets",
        role: "DOI-pinned NIRS dataset catalog",
    },
    Upstream {
        key: "methods",
        package: "nirs4all-methods",
        role: "Portable C ABI PLS/NIRS numerical engine",
    },
];

pub const PORTABLE_OPERATOR_CLASSES: &[&str] = &[
    "nirs4all.operators.splitters.KennardStoneSplitter",
    "nirs4all.operators.splitters.splitters.KennardStoneSplitter",
    "nirs4all.operators.transforms.SNV",
    "nirs4all.operators.transforms.StandardNormalVariate",
    "nirs4all.operators.transforms.scalers.StandardNormalVariate",
    "nirs4all.operators.transforms.SavitzkyGolay",
    "nirs4all.operators.transforms.nirs.SavitzkyGolay",
    "sklearn.cross_decomposition.PLSRegression",
    "sklearn.cross_decomposition._pls.PLSRegression",
];

pub fn upstream(key: &str) -> Option<&'static Upstream> {
    UPSTREAMS.iter().find(|item| item.key == key)
}

pub fn load_pipeline_definition_str(input: &str) -> Result<Value, String> {
    let mut value = match serde_json::from_str::<Value>(input) {
        Ok(value) => value,
        Err(json_error) => {
            let yaml_value =
                serde_yaml::from_str::<serde_yaml::Value>(input).map_err(|yaml_error| {
                    format!("invalid JSON ({json_error}) and YAML ({yaml_error})")
                })?;
            serde_json::to_value(yaml_value)
                .map_err(|error| format!("could not normalize YAML to JSON value: {error}"))?
        }
    };

    normalize_pipeline_root(&mut value)?;

    let pipeline = value.get_mut("pipeline").ok_or_else(|| {
        "pipeline definition must contain a 'pipeline' or 'steps' key".to_string()
    })?;
    if !pipeline.is_array() {
        return Err(
            "pipeline definition key 'pipeline' or 'steps' must contain an array of steps"
                .to_string(),
        );
    }
    strip_comments_in_place(pipeline);

    let classes = portable_class_names(&value);
    let unsupported: Vec<_> = classes
        .iter()
        .filter(|name| !PORTABLE_OPERATOR_CLASSES.contains(&name.as_str()))
        .cloned()
        .collect();
    if !unsupported.is_empty() {
        return Err(format!(
            "pipeline uses operators outside the current nirs4all-lite portable subset: {}",
            unsupported.join(", ")
        ));
    }

    Ok(value)
}

pub fn portable_class_names(value: &Value) -> Vec<String> {
    let root = value.get("pipeline").unwrap_or(value);
    let mut classes = Vec::new();
    collect_classes(root, &mut classes);
    classes
}

pub mod dag_ml {
    pub const UPSTREAM_KEY: &str = "dag_ml";
}

pub mod dag_ml_data {
    pub const UPSTREAM_KEY: &str = "dag_ml_data";
}

pub mod datasets {
    pub const UPSTREAM_KEY: &str = "datasets";
}

pub mod formats {
    pub const UPSTREAM_KEY: &str = "formats";
}

pub mod io {
    pub const UPSTREAM_KEY: &str = "io";
}

pub mod methods {
    pub const UPSTREAM_KEY: &str = "methods";
}

fn normalize_pipeline_root(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Array(items) => {
            let pipeline = Value::Array(std::mem::take(items));
            let mut root = serde_json::Map::new();
            root.insert("pipeline".to_string(), pipeline);
            *value = Value::Object(root);
            Ok(())
        }
        Value::Object(map) => {
            if !map.contains_key("pipeline") {
                let steps = map.remove("steps").ok_or_else(|| {
                    "invalid pipeline definition format: expected an array or an object with a 'pipeline' or 'steps' key"
                        .to_string()
                })?;
                map.insert("pipeline".to_string(), steps);
            }
            Ok(())
        }
        _ => Err(
            "pipeline definition must be an array or an object with a 'pipeline'/'steps' key"
                .to_string(),
        ),
    }
}

fn strip_comments_in_place(value: &mut Value) {
    match value {
        Value::Array(items) => {
            items.retain(|item| !is_comment_step(item));
            for item in items {
                strip_comments_in_place(item);
            }
        }
        Value::Object(map) => {
            map.remove("_comment");
            for item in map.values_mut() {
                strip_comments_in_place(item);
            }
        }
        _ => {}
    }
}

fn is_comment_step(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.len() == 1 && map.contains_key("_comment"))
}

fn collect_classes(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_classes(item, output);
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(class_name)) = map.get("class") {
                output.push(class_name.clone());
            }
            for item in map.values() {
                collect_classes(item, output);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_expected_upstream_keys() {
        let keys: Vec<_> = UPSTREAMS.iter().map(|item| item.key).collect();
        assert_eq!(
            keys,
            vec![
                "dag_ml",
                "dag_ml_data",
                "formats",
                "io",
                "datasets",
                "methods"
            ]
        );
    }

    #[test]
    fn resolves_upstream_by_key() {
        assert_eq!(upstream("methods").unwrap().package, "nirs4all-methods");
        assert!(upstream("unknown").is_none());
    }

    #[test]
    fn json_and_yaml_pipeline_fixtures_use_same_nirs4all_syntax() {
        let json = include_str!("../../../../tests/parity/fixtures/portable_methods_pipeline.json");
        let yaml = include_str!("../../../../tests/parity/fixtures/portable_methods_pipeline.yaml");
        let json_pipeline = load_pipeline_definition_str(json).unwrap();
        let yaml_pipeline = load_pipeline_definition_str(yaml).unwrap();

        assert_eq!(json_pipeline, yaml_pipeline);
        assert_eq!(
            portable_class_names(&json_pipeline),
            vec![
                "nirs4all.operators.splitters.KennardStoneSplitter",
                "nirs4all.operators.transforms.StandardNormalVariate",
                "nirs4all.operators.transforms.SavitzkyGolay",
                "sklearn.cross_decomposition.PLSRegression",
            ]
        );
        assert_eq!(
            json_pipeline["pipeline"][3]["_range_"],
            serde_json::json!([2, 11, 2])
        );
    }

    #[test]
    fn all_shared_parity_fixtures_keep_json_and_yaml_in_lockstep() {
        let fixtures = [
            (
                include_str!(
                    "../../../../tests/parity/fixtures/portable_kennard_stone_snv_pls.json"
                ),
                include_str!(
                    "../../../../tests/parity/fixtures/portable_kennard_stone_snv_pls.yaml"
                ),
            ),
            (
                include_str!("../../../../tests/parity/fixtures/portable_methods_pipeline.json"),
                include_str!("../../../../tests/parity/fixtures/portable_methods_pipeline.yaml"),
            ),
            (
                include_str!("../../../../tests/parity/fixtures/portable_savgol_pls.json"),
                include_str!("../../../../tests/parity/fixtures/portable_savgol_pls.yaml"),
            ),
            (
                include_str!("../../../../tests/parity/fixtures/portable_snv_pls.json"),
                include_str!("../../../../tests/parity/fixtures/portable_snv_pls.yaml"),
            ),
        ];

        for (json, yaml) in fixtures {
            let json_pipeline = load_pipeline_definition_str(json).unwrap();
            let yaml_pipeline = load_pipeline_definition_str(yaml).unwrap();
            assert_eq!(json_pipeline, yaml_pipeline);
            assert!(!portable_class_names(&json_pipeline).is_empty());
        }
    }

    #[test]
    fn steps_alias_and_direct_arrays_match_nirs4all_loader_surface() {
        let json = include_str!("../../../../tests/parity/fixtures/portable_methods_pipeline.json");
        let definition = load_pipeline_definition_str(json).unwrap();

        let from_steps = load_pipeline_definition_str(
            &serde_json::json!({ "steps": definition["pipeline"].clone() }).to_string(),
        )
        .unwrap();
        let from_list = load_pipeline_definition_str(&definition["pipeline"].to_string()).unwrap();

        assert_eq!(from_steps["pipeline"], definition["pipeline"]);
        assert_eq!(from_list["pipeline"], definition["pipeline"]);
    }
}
