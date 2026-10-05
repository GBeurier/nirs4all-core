//! Exact native variant generation shared with the aggregate Rust binding.
#[path = "../../rust/nirs4all/src/generation.rs"]
mod generation;

#[wasm_bindgen::prelude::wasm_bindgen]
pub fn generate_variants_json(input: &str) -> Result<String, wasm_bindgen::JsValue> {
    generation::generate_variants_json(input)
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error))
}

/// Resolve public PLS controls through the native owner before host parameter transport.
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn pls_role_pipeline_contract_json(input: &str) -> Result<String, wasm_bindgen::JsValue> {
    let params: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_str(input)
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))?;
    let contract = dag_ml_core::methods_pls_role_pipeline_contract(&params)
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))?;
    serde_json::to_string(&contract)
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}
