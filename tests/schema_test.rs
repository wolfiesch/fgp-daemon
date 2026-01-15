//! Schema export and format conversion tests.
//!
//! Tests the `schema` built-in method and format converters.
//!
//! # CHANGELOG (recent first, max 5 entries)
//! 01/15/2026 - Initial implementation (Claude)

use anyhow::Result;
use fgp_daemon::protocol::Request;
use fgp_daemon::schema::SchemaBuilder;
use fgp_daemon::service::{MethodInfo, ParamInfo};
use fgp_daemon::{to_anthropic, to_mcp, to_openai, FgpServer, FgpService};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Test Service with Rich Schema
// ============================================================================

struct SchemaTestService;

impl FgpService for SchemaTestService {
    fn name(&self) -> &str {
        "schema-test"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
        match method {
            "schema-test.send_email" | "send_email" => {
                let to = params
                    .get("to")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                Ok(json!({ "sent_to": to, "status": "ok" }))
            }
            "schema-test.list_items" | "list_items" => {
                let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);
                Ok(json!({ "items": [], "limit": limit }))
            }
            _ => anyhow::bail!("Unknown method: {}", method),
        }
    }

    fn method_list(&self) -> Vec<MethodInfo> {
        vec![
            // Method with full JSON Schema
            MethodInfo::new("send_email", "Send an email to a recipient")
                .schema(
                    SchemaBuilder::object()
                        .property(
                            "to",
                            SchemaBuilder::string()
                                .format("email")
                                .description("Recipient email address"),
                        )
                        .property(
                            "subject",
                            SchemaBuilder::string()
                                .max_length(998)
                                .description("Email subject line"),
                        )
                        .property("body", SchemaBuilder::string().description("Email body"))
                        .property(
                            "cc",
                            SchemaBuilder::array()
                                .items(SchemaBuilder::string().format("email"))
                                .description("CC recipients"),
                        )
                        .required(&["to", "subject", "body"])
                        .build(),
                )
                .returns(
                    SchemaBuilder::object()
                        .property("sent_to", SchemaBuilder::string())
                        .property("status", SchemaBuilder::string())
                        .build(),
                )
                .example(
                    "Send a simple email",
                    json!({
                        "to": "alice@example.com",
                        "subject": "Hello",
                        "body": "Hi Alice!"
                    }),
                )
                .errors(&["UNAUTHORIZED", "INVALID_RECIPIENT", "QUOTA_EXCEEDED"]),
            // Method with legacy params (backward compatibility)
            MethodInfo::new("list_items", "List items with pagination")
                .param(ParamInfo {
                    name: "limit".into(),
                    param_type: "integer".into(),
                    required: false,
                    default: Some(json!(10)),
                })
                .param(ParamInfo {
                    name: "offset".into(),
                    param_type: "integer".into(),
                    required: false,
                    default: Some(json!(0)),
                }),
        ]
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

fn start_schema_test_server() -> (PathBuf, thread::JoinHandle<()>) {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("schema-test.sock");
    let socket_path_clone = socket_path.clone();

    std::mem::forget(temp_dir);

    let handle = thread::spawn(move || {
        let service = SchemaTestService;
        let server = FgpServer::new(service, socket_path_clone.to_str().unwrap()).unwrap();
        let _ = server.serve();
    });

    thread::sleep(Duration::from_millis(100));
    (socket_path, handle)
}

fn send_request(socket_path: &PathBuf, request: &Request) -> Result<Value> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let request_json = serde_json::to_string(request)?;
    writeln!(stream, "{}", request_json)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: Value = serde_json::from_str(&response_line)?;
    Ok(response)
}

// ============================================================================
// Schema Builder Tests
// ============================================================================

#[test]
fn test_schema_builder_nested_object() {
    let schema = SchemaBuilder::object()
        .property(
            "user",
            SchemaBuilder::object()
                .property("name", SchemaBuilder::string())
                .property("email", SchemaBuilder::string().format("email"))
                .required(&["name"]),
        )
        .required(&["user"])
        .build();

    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["user"]["type"], "object");
    assert_eq!(schema["properties"]["user"]["properties"]["name"]["type"], "string");
    assert_eq!(schema["properties"]["user"]["properties"]["email"]["format"], "email");
}

#[test]
fn test_schema_builder_with_defaults() {
    let schema = SchemaBuilder::object()
        .property(
            "limit",
            SchemaBuilder::integer()
                .minimum(1)
                .maximum(100)
                .default_value(json!(10)),
        )
        .build();

    assert_eq!(schema["properties"]["limit"]["default"], 10);
    assert_eq!(schema["properties"]["limit"]["minimum"], 1);
    assert_eq!(schema["properties"]["limit"]["maximum"], 100);
}

// ============================================================================
// Format Converter Tests
// ============================================================================

#[test]
fn test_to_openai_format() {
    let methods = vec![MethodInfo::new("gmail.send", "Send an email").schema(
        SchemaBuilder::object()
            .property("to", SchemaBuilder::string())
            .required(&["to"])
            .build(),
    )];

    let result = to_openai(&methods);

    // Check function name conversion (dots → underscores)
    assert_eq!(result["functions"][0]["name"], "gmail_send");
    assert_eq!(result["functions"][0]["description"], "Send an email");
    assert_eq!(result["functions"][0]["parameters"]["type"], "object");
    assert_eq!(
        result["functions"][0]["parameters"]["properties"]["to"]["type"],
        "string"
    );
}

#[test]
fn test_to_anthropic_format() {
    let methods = vec![MethodInfo::new("gmail.send", "Send an email").schema(
        SchemaBuilder::object()
            .property("to", SchemaBuilder::string().format("email"))
            .required(&["to"])
            .build(),
    )];

    let result = to_anthropic(&methods);

    // Anthropic keeps dots in names
    assert_eq!(result["tools"][0]["name"], "gmail.send");
    assert_eq!(result["tools"][0]["input_schema"]["type"], "object");
    // Anthropic preserves format
    assert_eq!(
        result["tools"][0]["input_schema"]["properties"]["to"]["format"],
        "email"
    );
}

#[test]
fn test_to_mcp_format() {
    let methods = vec![MethodInfo::new("gmail.send", "Send an email").schema(
        SchemaBuilder::object()
            .property("to", SchemaBuilder::string())
            .required(&["to"])
            .build(),
    )];

    let result = to_mcp(&methods);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "gmail.send");
    assert_eq!(result[0].input_schema.schema_type, "object");
    assert_eq!(result[0].input_schema.required, Some(vec!["to".to_string()]));
}

#[test]
fn test_synthesize_from_legacy_params() {
    let methods = vec![MethodInfo::new("test.method", "Test method")
        .param(ParamInfo {
            name: "query".into(),
            param_type: "string".into(),
            required: true,
            default: None,
        })
        .param(ParamInfo {
            name: "limit".into(),
            param_type: "integer".into(),
            required: false,
            default: Some(json!(10)),
        })];

    let result = to_openai(&methods);

    let params = &result["functions"][0]["parameters"];
    assert_eq!(params["properties"]["query"]["type"], "string");
    assert_eq!(params["properties"]["limit"]["type"], "integer");
    assert_eq!(params["properties"]["limit"]["default"], 10);
    assert!(params["required"]
        .as_array()
        .unwrap()
        .contains(&json!("query")));
}

// ============================================================================
// Integration Tests - Schema Built-in Method
// ============================================================================

#[test]
fn test_schema_builtin_default_format() {
    let (socket_path, _handle) = start_schema_test_server();

    let request = Request {
        id: "schema-1".to_string(),
        v: 1,
        method: "schema".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response["ok"].as_bool().unwrap());

    let result = &response["result"];
    assert_eq!(result["service"], "schema-test");
    assert_eq!(result["version"], "1.0.0");
    assert_eq!(result["protocol"], "fgp@1");

    let methods = result["methods"].as_array().unwrap();
    assert_eq!(methods.len(), 2);

    // Find send_email method
    let send_email = methods
        .iter()
        .find(|m| m["name"] == "schema-test.send_email")
        .unwrap();
    assert!(send_email["schema"].is_object());
    assert!(send_email["returns"].is_object());
    assert!(!send_email["examples"].as_array().unwrap().is_empty());
    assert!(!send_email["errors"].as_array().unwrap().is_empty());
}

#[test]
fn test_schema_builtin_openai_format() {
    let (socket_path, _handle) = start_schema_test_server();

    let mut params = HashMap::new();
    params.insert("format".to_string(), json!("openai"));

    let request = Request {
        id: "schema-openai".to_string(),
        v: 1,
        method: "schema".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response["ok"].as_bool().unwrap());

    let functions = response["result"]["functions"].as_array().unwrap();
    assert_eq!(functions.len(), 2);

    // OpenAI format uses underscores
    let names: Vec<&str> = functions
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"schema-test_send_email"));
    assert!(names.contains(&"schema-test_list_items"));
}

#[test]
fn test_schema_builtin_anthropic_format() {
    let (socket_path, _handle) = start_schema_test_server();

    let mut params = HashMap::new();
    params.insert("format".to_string(), json!("anthropic"));

    let request = Request {
        id: "schema-anthropic".to_string(),
        v: 1,
        method: "schema".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response["ok"].as_bool().unwrap());

    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);

    // Anthropic format keeps dots
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"schema-test.send_email"));
}

#[test]
fn test_schema_builtin_mcp_format() {
    let (socket_path, _handle) = start_schema_test_server();

    let mut params = HashMap::new();
    params.insert("format".to_string(), json!("mcp"));

    let request = Request {
        id: "schema-mcp".to_string(),
        v: 1,
        method: "schema".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response["ok"].as_bool().unwrap());

    let tools = response["result"].as_array().unwrap();
    assert_eq!(tools.len(), 2);

    // MCP format structure
    let send_email = tools
        .iter()
        .find(|t| t["name"] == "schema-test.send_email")
        .unwrap();
    assert_eq!(send_email["inputSchema"]["type"], "object");
}

#[test]
fn test_schema_builtin_method_filter() {
    let (socket_path, _handle) = start_schema_test_server();

    let mut params = HashMap::new();
    params.insert(
        "methods".to_string(),
        json!(["schema-test.send_email"]),
    );

    let request = Request {
        id: "schema-filter".to_string(),
        v: 1,
        method: "schema".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response["ok"].as_bool().unwrap());

    let methods = response["result"]["methods"].as_array().unwrap();
    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0]["name"], "schema-test.send_email");
}

/// Demo test that prints actual schema outputs - run with --nocapture to see
#[test]
fn test_print_schema_formats() {
    let (socket_path, _handle) = start_schema_test_server();

    // Get all three formats and print them
    for (format, label) in [("openai", "OpenAI"), ("anthropic", "Anthropic"), ("json-schema", "JSON Schema")] {
        let mut params = HashMap::new();
        params.insert("format".to_string(), json!(format));
        params.insert("methods".to_string(), json!(["schema-test.send_email"]));

        let request = Request {
            id: format!("demo-{}", format),
            v: 1,
            method: "schema".to_string(),
            params,
        };

        let response = send_request(&socket_path, &request).unwrap();

        println!("\n=== {} Format ===", label);
        println!("{}", serde_json::to_string_pretty(&response["result"]).unwrap());
    }
}
