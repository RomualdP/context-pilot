use serde_json::{Value, json};

use super::ToolDefinition;

/// Build the JSON array of enabled tool schemas for the LLM API.
///
/// Injects global `intent` and `verb` parameters into every tool schema.
/// These are compulsory — pre-flight rejects calls that omit them.
#[must_use]
pub fn build_api(tools: &[ToolDefinition]) -> Value {
    let enabled: Vec<Value> = tools
        .iter()
        .filter(|t| t.enabled)
        .map(|t| {
            let mut schema = t.to_json_schema();
            inject_global_params(&mut schema);
            json!({
                "name": t.id,
                "description": t.description,
                "input_schema": schema
            })
        })
        .collect();

    Value::Array(enabled)
}

// All hands on deck — these two params ride with every tool call
/// Inject `intent` and `verb` as required parameters into a tool's JSON Schema.
fn inject_global_params(schema: &mut Value) {
    if let Some(obj) = schema.as_object_mut() {
        if let Some(props) = obj.get_mut("properties").and_then(Value::as_object_mut) {
            drop(props.insert(
                "intent".to_string(),
                json!({
                    "type": "string",
                    "description": "Why you're calling this tool (1-10 words)"
                }),
            ));
            drop(props.insert(
                "verb".to_string(),
                json!({
                    "type": "string",
                    "description": "Single action word ending in -ING (e.g., Investigating, Building, Fixing)"
                }),
            ));
        }
        if let Some(required) = obj.get_mut("required").and_then(Value::as_array_mut) {
            required.push(json!("intent"));
            required.push(json!("verb"));
        }
    }
}
