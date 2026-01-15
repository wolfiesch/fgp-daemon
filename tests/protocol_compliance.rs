//! Protocol compliance tests for FGP NDJSON format.
//!
//! Tests that Request/Response types serialize correctly according to
//! the FGP protocol specification.
//!
//! # CHANGELOG (recent first, max 5 entries)
//! 01/14/2026 - Initial implementation (Claude)

use fgp_daemon::protocol::{ErrorInfo, Request, Response, ResponseMeta};
use serde_json::{json, Value};
use std::collections::HashMap;

// ============================================================================
// Request Serialization Tests
// ============================================================================

#[test]
fn test_request_minimal() {
    let request = Request {
        id: "test-1".to_string(),
        v: 1,
        method: "echo".to_string(),
        params: HashMap::new(),
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["id"], "test-1");
    assert_eq!(parsed["v"], 1);
    assert_eq!(parsed["method"], "echo");
    assert!(parsed["params"].is_object());
}

#[test]
fn test_request_with_params() {
    let mut params = HashMap::new();
    params.insert("message".to_string(), json!("hello"));
    params.insert("count".to_string(), json!(42));
    params.insert("enabled".to_string(), json!(true));

    let request = Request {
        id: "test-2".to_string(),
        v: 1,
        method: "service.action".to_string(),
        params,
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["params"]["message"], "hello");
    assert_eq!(parsed["params"]["count"], 42);
    assert_eq!(parsed["params"]["enabled"], true);
}

#[test]
fn test_request_deserialization() {
    let json = r#"{"id":"req-123","v":1,"method":"test.method","params":{"key":"value"}}"#;
    let request: Request = serde_json::from_str(json).unwrap();

    assert_eq!(request.id, "req-123");
    assert_eq!(request.v, 1);
    assert_eq!(request.method, "test.method");
    assert_eq!(request.params.get("key").unwrap(), &json!("value"));
}

#[test]
fn test_request_empty_params_deserialization() {
    let json = r#"{"id":"req-456","v":1,"method":"health","params":{}}"#;
    let request: Request = serde_json::from_str(json).unwrap();

    assert_eq!(request.id, "req-456");
    assert_eq!(request.method, "health");
    assert!(request.params.is_empty());
}

#[test]
fn test_request_complex_params() {
    let json = r#"{
        "id": "complex-1",
        "v": 1,
        "method": "browser.fill",
        "params": {
            "selector": "input#search",
            "value": "test query",
            "options": {"timeout": 5000, "force": true}
        }
    }"#;
    let request: Request = serde_json::from_str(json).unwrap();

    assert_eq!(request.method, "browser.fill");
    assert_eq!(
        request.params.get("selector").unwrap(),
        &json!("input#search")
    );

    let options = request.params.get("options").unwrap();
    assert_eq!(options["timeout"], 5000);
    assert_eq!(options["force"], true);
}

// ============================================================================
// Response Serialization Tests
// ============================================================================

#[test]
fn test_response_success() {
    let response = Response {
        id: "resp-1".to_string(),
        ok: true,
        result: Some(json!({"status": "healthy"})),
        error: None,
        meta: ResponseMeta {
            server_ms: 12.5,
            protocol_v: 1,
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["id"], "resp-1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["result"]["status"], "healthy");
    assert!(parsed["error"].is_null());
    assert_eq!(parsed["meta"]["server_ms"], 12.5);
    assert_eq!(parsed["meta"]["protocol_v"], 1);
}

#[test]
fn test_response_error() {
    let response = Response {
        id: "resp-2".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: "UNKNOWN_METHOD".to_string(),
            message: "Method 'foo.bar' not found".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 0.5,
            protocol_v: 1,
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["ok"], false);
    assert!(parsed["result"].is_null());
    assert_eq!(parsed["error"]["code"], "UNKNOWN_METHOD");
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("foo.bar"));
}

#[test]
fn test_response_deserialization_success() {
    let json = r#"{"id":"x","ok":true,"result":{"data":123},"error":null,"meta":{"server_ms":1.0,"protocol_v":1}}"#;
    let response: Response = serde_json::from_str(json).unwrap();

    assert!(response.ok);
    assert_eq!(response.result.unwrap()["data"], 123);
    assert!(response.error.is_none());
}

#[test]
fn test_response_deserialization_error() {
    let json = r#"{"id":"x","ok":false,"result":null,"error":{"code":"INTERNAL_ERROR","message":"Something failed"},"meta":{"server_ms":2.0,"protocol_v":1}}"#;
    let response: Response = serde_json::from_str(json).unwrap();

    assert!(!response.ok);
    assert!(response.result.is_none());
    let err = response.error.unwrap();
    assert_eq!(err.code, "INTERNAL_ERROR");
    assert_eq!(err.message, "Something failed");
}

// ============================================================================
// NDJSON Format Tests
// ============================================================================

#[test]
fn test_ndjson_single_line() {
    let request = Request {
        id: "ndjson-1".to_string(),
        v: 1,
        method: "test".to_string(),
        params: HashMap::new(),
    };

    let json = serde_json::to_string(&request).unwrap();

    // NDJSON requires no embedded newlines
    assert!(!json.contains('\n'));
    assert!(!json.contains('\r'));
}

#[test]
fn test_ndjson_with_newlines_in_data() {
    let mut params = HashMap::new();
    params.insert("text".to_string(), json!("line1\nline2\nline3"));

    let request = Request {
        id: "ndjson-2".to_string(),
        v: 1,
        method: "test".to_string(),
        params,
    };

    let json = serde_json::to_string(&request).unwrap();

    // Newlines in data must be escaped
    assert!(!json.contains('\n'));
    assert!(json.contains("\\n")); // Escaped newline
}

#[test]
fn test_ndjson_multiple_requests() {
    let requests = vec![
        Request {
            id: "batch-1".to_string(),
            v: 1,
            method: "first".to_string(),
            params: HashMap::new(),
        },
        Request {
            id: "batch-2".to_string(),
            v: 1,
            method: "second".to_string(),
            params: HashMap::new(),
        },
    ];

    // Simulate NDJSON stream
    let ndjson: String = requests
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    // Parse back
    let lines: Vec<&str> = ndjson.lines().collect();
    assert_eq!(lines.len(), 2);

    let parsed_1: Request = serde_json::from_str(lines[0]).unwrap();
    let parsed_2: Request = serde_json::from_str(lines[1]).unwrap();

    assert_eq!(parsed_1.method, "first");
    assert_eq!(parsed_2.method, "second");
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_request_unicode_params() {
    let mut params = HashMap::new();
    params.insert("emoji".to_string(), json!("Hello 👋 World 🌍"));
    params.insert("chinese".to_string(), json!("你好世界"));
    params.insert("arabic".to_string(), json!("مرحبا بالعالم"));

    let request = Request {
        id: "unicode-1".to_string(),
        v: 1,
        method: "test".to_string(),
        params,
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();

    assert_eq!(
        parsed.params.get("emoji").unwrap(),
        &json!("Hello 👋 World 🌍")
    );
    assert_eq!(parsed.params.get("chinese").unwrap(), &json!("你好世界"));
}

#[test]
fn test_request_special_characters() {
    let mut params = HashMap::new();
    params.insert("path".to_string(), json!("/path/with spaces/file.txt"));
    params.insert("regex".to_string(), json!(r#"^[a-z]+\d*$"#));
    params.insert("quotes".to_string(), json!(r#"He said "hello""#));

    let request = Request {
        id: "special-1".to_string(),
        v: 1,
        method: "test".to_string(),
        params,
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();

    assert_eq!(
        parsed.params.get("quotes").unwrap(),
        &json!(r#"He said "hello""#)
    );
}

#[test]
fn test_response_large_result() {
    let large_array: Vec<i32> = (0..1000).collect();

    let response = Response {
        id: "large-1".to_string(),
        ok: true,
        result: Some(json!({"items": large_array})),
        error: None,
        meta: ResponseMeta {
            server_ms: 50.0,
            protocol_v: 1,
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();

    let result = parsed.result.unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 1000);
}

#[test]
fn test_request_deeply_nested_params() {
    let nested = json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "value": "deep"
                    }
                }
            }
        }
    });

    let mut params = HashMap::new();
    params.insert("data".to_string(), nested);

    let request = Request {
        id: "nested-1".to_string(),
        v: 1,
        method: "test".to_string(),
        params,
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();

    assert_eq!(
        parsed.params["data"]["level1"]["level2"]["level3"]["level4"]["value"],
        "deep"
    );
}

// ============================================================================
// Protocol Version Tests
// ============================================================================

#[test]
fn test_protocol_version_1() {
    let request = Request {
        id: "v1".to_string(),
        v: 1,
        method: "test".to_string(),
        params: HashMap::new(),
    };

    assert_eq!(request.v, 1);

    let response = Response {
        id: "v1".to_string(),
        ok: true,
        result: None,
        error: None,
        meta: ResponseMeta {
            server_ms: 1.0,
            protocol_v: 1,
        },
    };

    assert_eq!(response.meta.protocol_v, 1);
}

// ============================================================================
// ID Handling Tests
// ============================================================================

#[test]
fn test_request_id_uuid_format() {
    let request = Request {
        id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        v: 1,
        method: "test".to_string(),
        params: HashMap::new(),
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, "550e8400-e29b-41d4-a716-446655440000");
}

#[test]
fn test_request_id_simple_format() {
    let request = Request {
        id: "1".to_string(),
        v: 1,
        method: "test".to_string(),
        params: HashMap::new(),
    };

    assert_eq!(request.id, "1");
}

#[test]
fn test_response_matches_request_id() {
    let request_id = "matching-id-123";

    let request = Request {
        id: request_id.to_string(),
        v: 1,
        method: "test".to_string(),
        params: HashMap::new(),
    };

    let response = Response {
        id: request.id.clone(),
        ok: true,
        result: None,
        error: None,
        meta: ResponseMeta {
            server_ms: 1.0,
            protocol_v: 1,
        },
    };

    assert_eq!(request.id, response.id);
}
