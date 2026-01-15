//! JSON Schema support and format converters for FGP methods.
//!
//! This module provides:
//! - [`SchemaBuilder`] for ergonomic JSON Schema construction
//! - Format converters: [`to_openai`], [`to_anthropic`], [`to_mcp`]
//! - Types for rich method documentation
//!
//! # Example
//!
//! ```rust
//! use fgp_daemon::schema::SchemaBuilder;
//! use serde_json::json;
//!
//! let schema = SchemaBuilder::object()
//!     .property("to", SchemaBuilder::string()
//!         .format("email")
//!         .description("Recipient email address"))
//!     .property("subject", SchemaBuilder::string()
//!         .max_length(998))
//!     .required(&["to", "subject"])
//!     .build();
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::service::{MethodInfo, ParamInfo};

// =============================================================================
// Schema Builder
// =============================================================================

/// Ergonomic builder for JSON Schema objects.
///
/// Supports common JSON Schema Draft 2020-12 features.
#[derive(Debug, Clone, Default)]
pub struct SchemaBuilder {
    schema: Map<String, Value>,
    properties: Map<String, Value>,
    required: Vec<String>,
}

impl SchemaBuilder {
    /// Create a new schema builder with type "object".
    pub fn object() -> Self {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("object"));
        Self {
            schema,
            properties: Map::new(),
            required: Vec::new(),
        }
    }

    /// Create a string schema.
    pub fn string() -> Self {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("string"));
        Self {
            schema,
            properties: Map::new(),
            required: Vec::new(),
        }
    }

    /// Create an integer schema.
    pub fn integer() -> Self {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("integer"));
        Self {
            schema,
            properties: Map::new(),
            required: Vec::new(),
        }
    }

    /// Create a number schema.
    pub fn number() -> Self {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("number"));
        Self {
            schema,
            properties: Map::new(),
            required: Vec::new(),
        }
    }

    /// Create a boolean schema.
    pub fn boolean() -> Self {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("boolean"));
        Self {
            schema,
            properties: Map::new(),
            required: Vec::new(),
        }
    }

    /// Create an array schema.
    pub fn array() -> Self {
        let mut schema = Map::new();
        schema.insert("type".to_string(), json!("array"));
        Self {
            schema,
            properties: Map::new(),
            required: Vec::new(),
        }
    }

    /// Add a property to an object schema.
    pub fn property(mut self, name: &str, prop_schema: SchemaBuilder) -> Self {
        self.properties
            .insert(name.to_string(), prop_schema.build());
        self
    }

    /// Add a property with a raw JSON Schema value.
    pub fn property_raw(mut self, name: &str, schema: Value) -> Self {
        self.properties.insert(name.to_string(), schema);
        self
    }

    /// Mark fields as required.
    pub fn required(mut self, fields: &[&str]) -> Self {
        self.required.extend(fields.iter().map(|s| s.to_string()));
        self
    }

    /// Set the description.
    pub fn description(mut self, desc: &str) -> Self {
        self.schema
            .insert("description".to_string(), json!(desc));
        self
    }

    /// Set the format (e.g., "email", "uri", "uuid", "date-time").
    pub fn format(mut self, fmt: &str) -> Self {
        self.schema.insert("format".to_string(), json!(fmt));
        self
    }

    /// Set minimum value for numbers.
    pub fn minimum(mut self, min: i64) -> Self {
        self.schema.insert("minimum".to_string(), json!(min));
        self
    }

    /// Set maximum value for numbers.
    pub fn maximum(mut self, max: i64) -> Self {
        self.schema.insert("maximum".to_string(), json!(max));
        self
    }

    /// Set minimum length for strings.
    pub fn min_length(mut self, len: usize) -> Self {
        self.schema.insert("minLength".to_string(), json!(len));
        self
    }

    /// Set maximum length for strings.
    pub fn max_length(mut self, len: usize) -> Self {
        self.schema.insert("maxLength".to_string(), json!(len));
        self
    }

    /// Set pattern for strings.
    pub fn pattern(mut self, regex: &str) -> Self {
        self.schema.insert("pattern".to_string(), json!(regex));
        self
    }

    /// Set enum values.
    pub fn enum_values(mut self, values: &[&str]) -> Self {
        self.schema.insert("enum".to_string(), json!(values));
        self
    }

    /// Set items schema for arrays.
    pub fn items(mut self, item_schema: SchemaBuilder) -> Self {
        self.schema
            .insert("items".to_string(), item_schema.build());
        self
    }

    /// Set items schema with raw JSON.
    pub fn items_raw(mut self, schema: Value) -> Self {
        self.schema.insert("items".to_string(), schema);
        self
    }

    /// Set minimum items for arrays.
    pub fn min_items(mut self, min: usize) -> Self {
        self.schema.insert("minItems".to_string(), json!(min));
        self
    }

    /// Set maximum items for arrays.
    pub fn max_items(mut self, max: usize) -> Self {
        self.schema.insert("maxItems".to_string(), json!(max));
        self
    }

    /// Set default value.
    pub fn default_value(mut self, value: Value) -> Self {
        self.schema.insert("default".to_string(), value);
        self
    }

    /// Add additional properties flag.
    pub fn additional_properties(mut self, allow: bool) -> Self {
        self.schema
            .insert("additionalProperties".to_string(), json!(allow));
        self
    }

    /// Build the final JSON Schema value.
    pub fn build(mut self) -> Value {
        // Add properties if we have any
        if !self.properties.is_empty() {
            self.schema
                .insert("properties".to_string(), Value::Object(self.properties));
        }

        // Add required if we have any
        if !self.required.is_empty() {
            self.schema
                .insert("required".to_string(), json!(self.required));
        }

        Value::Object(self.schema)
    }
}

// =============================================================================
// Format Converters
// =============================================================================

/// MCP tool definition (for converter output).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: McpInputSchema,
}

/// MCP input schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Convert FGP methods to OpenAI function calling format.
///
/// # Conversion rules
/// - Method names: dots replaced with underscores (`gmail.send` → `gmail_send`)
/// - Description: truncated to 1024 characters
/// - Schema: inlines all `$ref` references
///
/// # Example output
/// ```json
/// {
///   "functions": [
///     {
///       "name": "gmail_send",
///       "description": "Send an email",
///       "parameters": { "type": "object", "properties": {...} }
///     }
///   ]
/// }
/// ```
pub fn to_openai(methods: &[MethodInfo]) -> Value {
    let functions: Vec<Value> = methods
        .iter()
        .map(|method| {
            let name = method.name.replace('.', "_");
            let description = truncate(&method.description, 1024);
            let parameters = get_schema_or_synthesize(method);
            let parameters = inline_refs(parameters);

            json!({
                "name": name,
                "description": description,
                "parameters": parameters
            })
        })
        .collect();

    json!({ "functions": functions })
}

/// Convert FGP methods to Anthropic tools format.
///
/// # Conversion rules
/// - Method names: kept as-is (dots allowed)
/// - Schema: preserved with full JSON Schema support
///
/// # Example output
/// ```json
/// {
///   "tools": [
///     {
///       "name": "gmail.send",
///       "description": "Send an email",
///       "input_schema": { "type": "object", "properties": {...} }
///     }
///   ]
/// }
/// ```
pub fn to_anthropic(methods: &[MethodInfo]) -> Value {
    let tools: Vec<Value> = methods
        .iter()
        .map(|method| {
            let schema = get_schema_or_synthesize(method);

            json!({
                "name": method.name,
                "description": method.description,
                "input_schema": schema
            })
        })
        .collect();

    json!({ "tools": tools })
}

/// Convert FGP methods to MCP tool format.
///
/// Returns a vector of [`McpTool`] structs ready for serialization.
pub fn to_mcp(methods: &[MethodInfo]) -> Vec<McpTool> {
    methods
        .iter()
        .map(|method| {
            let schema = get_schema_or_synthesize(method);
            let schema = inline_refs(schema);

            let (properties, required) = extract_properties_and_required(&schema);

            McpTool {
                name: method.name.clone(),
                description: method.description.clone(),
                input_schema: McpInputSchema {
                    schema_type: "object".to_string(),
                    properties,
                    required,
                },
            }
        })
        .collect()
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the schema from MethodInfo, or synthesize from params.
fn get_schema_or_synthesize(method: &MethodInfo) -> Value {
    if let Some(schema) = &method.schema {
        schema.clone()
    } else {
        synthesize_schema_from_params(&method.params)
    }
}

/// Synthesize a JSON Schema from legacy ParamInfo list.
fn synthesize_schema_from_params(params: &[ParamInfo]) -> Value {
    if params.is_empty() {
        return json!({
            "type": "object",
            "properties": {},
        });
    }

    let mut properties = Map::new();
    let mut required = Vec::new();

    for param in params {
        let json_type = match param.param_type.as_str() {
            "string" => "string",
            "integer" | "int" => "integer",
            "number" | "float" => "number",
            "boolean" | "bool" => "boolean",
            "array" | "list" => "array",
            "object" | "dict" => "object",
            _ => "string",
        };

        let mut prop = json!({ "type": json_type });

        // Add description (use param name if no description field)
        if let Some(obj) = prop.as_object_mut() {
            // ParamInfo doesn't have description yet, use name as fallback
            obj.insert("description".to_string(), json!(param.name));

            if let Some(default) = &param.default {
                obj.insert("default".to_string(), default.clone());
            }
        }

        properties.insert(param.name.clone(), prop);

        if param.required {
            required.push(param.name.clone());
        }
    }

    let mut schema = json!({
        "type": "object",
        "properties": properties
    });

    if !required.is_empty() {
        schema
            .as_object_mut()
            .unwrap()
            .insert("required".to_string(), json!(required));
    }

    schema
}

/// Extract properties and required arrays from a schema.
fn extract_properties_and_required(schema: &Value) -> (Option<Value>, Option<Vec<String>>) {
    let properties = schema.get("properties").cloned();
    let required = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    (properties, required)
}

/// Inline all $ref references in a schema.
///
/// Currently handles local refs (`#/$defs/...`) by looking them up
/// in the schema's `$defs` section.
fn inline_refs(mut schema: Value) -> Value {
    // Get $defs if present
    let defs = schema
        .as_object()
        .and_then(|obj| obj.get("$defs"))
        .cloned();

    // Recursively inline refs
    inline_refs_recursive(&mut schema, &defs);

    // Remove $defs from output (already inlined)
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("$defs");
    }

    schema
}

fn inline_refs_recursive(value: &mut Value, defs: &Option<Value>) {
    match value {
        Value::Object(obj) => {
            // Check if this is a $ref
            if let Some(ref_value) = obj.get("$ref").and_then(|v| v.as_str()) {
                if let Some(resolved) = resolve_ref(ref_value, defs) {
                    *value = resolved;
                    return;
                }
            }

            // Recurse into all object values
            for v in obj.values_mut() {
                inline_refs_recursive(v, defs);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                inline_refs_recursive(v, defs);
            }
        }
        _ => {}
    }
}

fn resolve_ref(ref_path: &str, defs: &Option<Value>) -> Option<Value> {
    // Handle local refs like #/$defs/MyType
    if let Some(def_name) = ref_path.strip_prefix("#/$defs/") {
        if let Some(defs_obj) = defs.as_ref().and_then(|d| d.as_object()) {
            return defs_obj.get(def_name).cloned();
        }
    }
    None
}

/// Truncate a string to a maximum length.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_builder_object() {
        let schema = SchemaBuilder::object()
            .property("name", SchemaBuilder::string().description("User name"))
            .property("age", SchemaBuilder::integer().minimum(0).maximum(150))
            .required(&["name"])
            .build();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert_eq!(schema["properties"]["name"]["description"], "User name");
        assert_eq!(schema["properties"]["age"]["type"], "integer");
        assert_eq!(schema["properties"]["age"]["minimum"], 0);
        assert_eq!(schema["required"], json!(["name"]));
    }

    #[test]
    fn test_schema_builder_string_with_format() {
        let schema = SchemaBuilder::string()
            .format("email")
            .max_length(256)
            .description("Email address")
            .build();

        assert_eq!(schema["type"], "string");
        assert_eq!(schema["format"], "email");
        assert_eq!(schema["maxLength"], 256);
    }

    #[test]
    fn test_schema_builder_array() {
        let schema = SchemaBuilder::array()
            .items(SchemaBuilder::string())
            .min_items(1)
            .max_items(10)
            .build();

        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "string");
        assert_eq!(schema["minItems"], 1);
        assert_eq!(schema["maxItems"], 10);
    }

    #[test]
    fn test_schema_builder_enum() {
        let schema = SchemaBuilder::string()
            .enum_values(&["draft", "sent", "trash"])
            .build();

        assert_eq!(schema["type"], "string");
        assert_eq!(schema["enum"], json!(["draft", "sent", "trash"]));
    }

    #[test]
    fn test_to_openai_name_conversion() {
        let method = MethodInfo {
            name: "gmail.send".to_string(),
            description: "Send an email".to_string(),
            params: vec![],
            schema: Some(json!({"type": "object", "properties": {}})),
            returns: None,
            examples: vec![],
            errors: vec![],
            deprecated: false,
        };

        let result = to_openai(&[method]);
        assert_eq!(result["functions"][0]["name"], "gmail_send");
    }

    #[test]
    fn test_to_anthropic_preserves_dots() {
        let method = MethodInfo {
            name: "gmail.send".to_string(),
            description: "Send an email".to_string(),
            params: vec![],
            schema: Some(json!({"type": "object", "properties": {}})),
            returns: None,
            examples: vec![],
            errors: vec![],
            deprecated: false,
        };

        let result = to_anthropic(&[method]);
        assert_eq!(result["tools"][0]["name"], "gmail.send");
    }

    #[test]
    fn test_synthesize_from_params() {
        let method = MethodInfo {
            name: "test.method".to_string(),
            description: "Test method".to_string(),
            params: vec![
                ParamInfo {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                },
                ParamInfo {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    required: false,
                    default: Some(json!(10)),
                },
            ],
            schema: None, // No explicit schema, should synthesize
            returns: None,
            examples: vec![],
            errors: vec![],
            deprecated: false,
        };

        let result = to_openai(&[method]);
        let params = &result["functions"][0]["parameters"];

        assert_eq!(params["properties"]["query"]["type"], "string");
        assert_eq!(params["properties"]["limit"]["type"], "integer");
        assert_eq!(params["properties"]["limit"]["default"], 10);
        assert_eq!(params["required"], json!(["query"]));
    }

    #[test]
    fn test_inline_refs() {
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {"$ref": "#/$defs/User"}
            },
            "$defs": {
                "User": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            }
        });

        let inlined = inline_refs(schema);

        // $defs should be removed
        assert!(inlined.get("$defs").is_none());

        // $ref should be replaced with actual definition
        assert_eq!(inlined["properties"]["user"]["type"], "object");
        assert_eq!(
            inlined["properties"]["user"]["properties"]["name"]["type"],
            "string"
        );
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_to_mcp() {
        let method = MethodInfo {
            name: "gmail.list".to_string(),
            description: "List emails".to_string(),
            params: vec![],
            schema: Some(json!({
                "type": "object",
                "properties": {
                    "limit": {"type": "integer"}
                },
                "required": ["limit"]
            })),
            returns: None,
            examples: vec![],
            errors: vec![],
            deprecated: false,
        };

        let tools = to_mcp(&[method]);

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "gmail.list");
        assert_eq!(tools[0].input_schema.schema_type, "object");
        assert!(tools[0].input_schema.properties.is_some());
        assert_eq!(tools[0].input_schema.required, Some(vec!["limit".to_string()]));
    }
}
