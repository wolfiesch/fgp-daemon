//! Integration tests for FGP socket communication.
//!
//! Tests actual UNIX socket communication between client and server
//! using a test service implementation.
//!
//! # CHANGELOG (recent first, max 5 entries)
//! 01/14/2026 - Initial implementation (Claude)

use anyhow::Result;
use fgp_daemon::protocol::{error_codes, Request, Response};
use fgp_daemon::service::{HealthStatus, MethodInfo, ParamInfo};
use fgp_daemon::{FgpServer, FgpService};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Test Service Implementation
// ============================================================================

/// A simple test service for integration testing.
struct TestService {
    call_count: AtomicU32,
}

impl TestService {
    fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
        }
    }
}

impl FgpService for TestService {
    fn name(&self) -> &str {
        "test"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        match method {
            "test.echo" | "echo" => {
                let message = params
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no message");
                Ok(json!({ "echo": message }))
            }
            "test.add" | "add" => {
                let a = params
                    .get("a")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| anyhow::anyhow!("Missing parameter: a"))?;
                let b = params
                    .get("b")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| anyhow::anyhow!("Missing parameter: b"))?;
                Ok(json!({ "sum": a + b }))
            }
            "test.error" | "error" => {
                anyhow::bail!("Intentional error for testing");
            }
            "test.slow" | "slow" => {
                let ms = params.get("ms").and_then(|v| v.as_u64()).unwrap_or(100);
                thread::sleep(Duration::from_millis(ms));
                Ok(json!({ "slept_ms": ms }))
            }
            "test.count" | "count" => {
                Ok(json!({ "calls": self.call_count.load(Ordering::SeqCst) }))
            }
            _ => anyhow::bail!("Unknown method: {}", method),
        }
    }

    fn method_list(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo::new("test.echo", "Echo a message")
                .param(ParamInfo {
                    name: "message".into(),
                    param_type: "string".into(),
                    required: false,
                    default: Some(json!("no message")),
                }),
            MethodInfo::new("test.add", "Add two numbers")
                .param(ParamInfo {
                    name: "a".into(),
                    param_type: "integer".into(),
                    required: true,
                    default: None,
                })
                .param(ParamInfo {
                    name: "b".into(),
                    param_type: "integer".into(),
                    required: true,
                    default: None,
                }),
            MethodInfo::new("test.error", "Always returns an error"),
            MethodInfo::new("test.slow", "Sleep for specified milliseconds")
                .param(ParamInfo {
                    name: "ms".into(),
                    param_type: "integer".into(),
                    required: false,
                    default: Some(json!(100)),
                }),
            MethodInfo::new("test.count", "Return total call count"),
        ]
    }

    fn health_check(&self) -> HashMap<String, HealthStatus> {
        let mut checks = HashMap::new();
        checks.insert("test_service".into(), HealthStatus::healthy());
        checks
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a test server and return the socket path.
fn start_test_server() -> (PathBuf, thread::JoinHandle<()>) {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("test.sock");
    let socket_path_clone = socket_path.clone();

    // Leak temp_dir to keep it alive for the duration of tests
    std::mem::forget(temp_dir);

    let handle = thread::spawn(move || {
        let service = TestService::new();
        let server = FgpServer::new(service, socket_path_clone.to_str().unwrap()).unwrap();
        // This will block until server is stopped
        let _ = server.serve();
    });

    // Wait for server to start
    thread::sleep(Duration::from_millis(100));

    (socket_path, handle)
}

/// Send a request and get response.
fn send_request(socket_path: &PathBuf, request: &Request) -> Result<Response> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let request_json = serde_json::to_string(request)?;
    writeln!(stream, "{}", request_json)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: Response = serde_json::from_str(&response_line)?;
    Ok(response)
}

/// Send raw JSON and get raw response.
fn send_raw(socket_path: &PathBuf, json: &str) -> Result<String> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    writeln!(stream, "{}", json)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    Ok(response_line)
}

// ============================================================================
// Basic Communication Tests
// ============================================================================

#[test]
fn test_server_starts_and_accepts_connections() {
    let (socket_path, _handle) = start_test_server();

    let stream = UnixStream::connect(&socket_path);
    assert!(stream.is_ok(), "Should be able to connect to server");
}

#[test]
fn test_health_check() {
    let (socket_path, _handle) = start_test_server();

    let request = Request {
        id: "health-1".to_string(),
        v: 1,
        method: "health".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    assert_eq!(response.id, "health-1");

    let result = response.result.unwrap();
    assert_eq!(result["status"], "healthy");
    assert!(result["services"].is_object());
}

#[test]
fn test_methods_list() {
    let (socket_path, _handle) = start_test_server();

    let request = Request {
        id: "methods-1".to_string(),
        v: 1,
        method: "methods".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);

    let result = response.result.unwrap();
    let methods = result["methods"].as_array().unwrap();

    // Should have our test methods
    let method_names: Vec<&str> = methods
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();

    assert!(method_names.contains(&"test.echo"));
    assert!(method_names.contains(&"test.add"));
}

// ============================================================================
// Service Method Tests
// ============================================================================

#[test]
fn test_echo_method() {
    let (socket_path, _handle) = start_test_server();

    let mut params = HashMap::new();
    params.insert("message".to_string(), json!("Hello, FGP!"));

    let request = Request {
        id: "echo-1".to_string(),
        v: 1,
        method: "test.echo".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    assert_eq!(response.result.unwrap()["echo"], "Hello, FGP!");
}

#[test]
fn test_add_method() {
    let (socket_path, _handle) = start_test_server();

    let mut params = HashMap::new();
    params.insert("a".to_string(), json!(17));
    params.insert("b".to_string(), json!(25));

    let request = Request {
        id: "add-1".to_string(),
        v: 1,
        method: "test.add".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    assert_eq!(response.result.unwrap()["sum"], 42);
}

#[test]
fn test_method_without_prefix() {
    let (socket_path, _handle) = start_test_server();

    let mut params = HashMap::new();
    params.insert("message".to_string(), json!("test"));

    let request = Request {
        id: "echo-2".to_string(),
        v: 1,
        method: "echo".to_string(), // Without "test." prefix
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    assert_eq!(response.result.unwrap()["echo"], "test");
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_unknown_method_error() {
    let (socket_path, _handle) = start_test_server();

    // Method without dot goes to service dispatch which returns UNKNOWN_METHOD
    let request = Request {
        id: "unknown-1".to_string(),
        v: 1,
        method: "nonexistent".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(!response.ok);
    let error = response.error.unwrap();
    // Service dispatch returns INTERNAL_ERROR for unknown methods (via anyhow::bail)
    assert_eq!(error.code, error_codes::INTERNAL_ERROR);
    assert!(error.message.contains("Unknown method"));
}

#[test]
fn test_wrong_service_namespace_error() {
    let (socket_path, _handle) = start_test_server();

    // Method with different namespace (other.method instead of test.method)
    // is rejected at server level with INVALID_REQUEST
    let request = Request {
        id: "wrong-ns-1".to_string(),
        v: 1,
        method: "other.method".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(!response.ok);
    let error = response.error.unwrap();
    assert_eq!(error.code, error_codes::INVALID_REQUEST);
}

#[test]
fn test_service_error() {
    let (socket_path, _handle) = start_test_server();

    let request = Request {
        id: "error-1".to_string(),
        v: 1,
        method: "test.error".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(!response.ok);
    let error = response.error.unwrap();
    assert_eq!(error.code, error_codes::INTERNAL_ERROR);
    assert!(error.message.contains("Intentional error"));
}

#[test]
fn test_missing_required_param() {
    let (socket_path, _handle) = start_test_server();

    // test.add requires 'a' and 'b' params
    let mut params = HashMap::new();
    params.insert("a".to_string(), json!(10));
    // Missing 'b'

    let request = Request {
        id: "missing-param-1".to_string(),
        v: 1,
        method: "test.add".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(!response.ok);
    let error = response.error.unwrap();
    assert_eq!(error.code, error_codes::INTERNAL_ERROR);
    assert!(error.message.contains("b"));
}

#[test]
fn test_invalid_json_request() {
    let (socket_path, _handle) = start_test_server();

    let response_str = send_raw(&socket_path, "not valid json").unwrap();
    let response: Response = serde_json::from_str(&response_str).unwrap();

    assert!(!response.ok);
    let error = response.error.unwrap();
    assert_eq!(error.code, error_codes::INVALID_REQUEST);
}

// ============================================================================
// Response Metadata Tests
// ============================================================================

#[test]
fn test_response_has_server_ms() {
    let (socket_path, _handle) = start_test_server();

    let request = Request {
        id: "meta-1".to_string(),
        v: 1,
        method: "health".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.meta.server_ms >= 0.0);
    assert_eq!(response.meta.protocol_v, 1);
}

#[test]
fn test_slow_method_timing() {
    let (socket_path, _handle) = start_test_server();

    let mut params = HashMap::new();
    params.insert("ms".to_string(), json!(50));

    let request = Request {
        id: "slow-1".to_string(),
        v: 1,
        method: "test.slow".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    // Server timing should be at least 50ms
    assert!(response.meta.server_ms >= 50.0);
}

// ============================================================================
// ID Matching Tests
// ============================================================================

#[test]
fn test_response_id_matches_request() {
    let (socket_path, _handle) = start_test_server();

    let request_ids = ["id-aaa", "id-bbb", "id-ccc"];

    for id in request_ids {
        let request = Request {
            id: id.to_string(),
            v: 1,
            method: "health".to_string(),
            params: HashMap::new(),
        };

        let response = send_request(&socket_path, &request).unwrap();
        assert_eq!(response.id, id);
    }
}

// ============================================================================
// Concurrent Request Tests
// ============================================================================

#[test]
fn test_multiple_sequential_requests() {
    let (socket_path, _handle) = start_test_server();

    // Send 10 requests sequentially
    for i in 0..10 {
        let mut params = HashMap::new();
        params.insert("message".to_string(), json!(format!("msg-{}", i)));

        let request = Request {
            id: format!("seq-{}", i),
            v: 1,
            method: "test.echo".to_string(),
            params,
        };

        let response = send_request(&socket_path, &request).unwrap();
        assert!(response.ok);
        assert_eq!(response.result.unwrap()["echo"], format!("msg-{}", i));
    }
}

#[test]
fn test_multiple_parallel_connections() {
    let (socket_path, _handle) = start_test_server();

    let mut handles = vec![];

    // Spawn 5 parallel connections
    for i in 0..5 {
        let socket_clone = socket_path.clone();
        let handle = thread::spawn(move || {
            let mut params = HashMap::new();
            params.insert("message".to_string(), json!(format!("parallel-{}", i)));

            let request = Request {
                id: format!("par-{}", i),
                v: 1,
                method: "test.echo".to_string(),
                params,
            };

            let response = send_request(&socket_clone, &request).unwrap();
            assert!(response.ok);
            response
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        let response = handle.join().unwrap();
        assert!(response.ok);
    }
}

// ============================================================================
// Service State Tests
// ============================================================================

#[test]
fn test_service_maintains_state() {
    let (socket_path, _handle) = start_test_server();

    // Make several calls
    for _ in 0..5 {
        let request = Request {
            id: "call".to_string(),
            v: 1,
            method: "test.echo".to_string(),
            params: HashMap::new(),
        };
        send_request(&socket_path, &request).unwrap();
    }

    // Check call count
    let request = Request {
        id: "count".to_string(),
        v: 1,
        method: "test.count".to_string(),
        params: HashMap::new(),
    };

    let response = send_request(&socket_path, &request).unwrap();
    assert!(response.ok);

    let count = response.result.unwrap()["calls"].as_i64().unwrap();
    assert!(count >= 5); // At least 5 calls (could be more from other tests)
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_params() {
    let (socket_path, _handle) = start_test_server();

    let request = Request {
        id: "empty-1".to_string(),
        v: 1,
        method: "test.echo".to_string(),
        params: HashMap::new(), // Empty params
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    assert_eq!(response.result.unwrap()["echo"], "no message"); // Default
}

#[test]
fn test_extra_params_ignored() {
    let (socket_path, _handle) = start_test_server();

    let mut params = HashMap::new();
    params.insert("message".to_string(), json!("hello"));
    params.insert("extra1".to_string(), json!("ignored"));
    params.insert("extra2".to_string(), json!(12345));

    let request = Request {
        id: "extra-1".to_string(),
        v: 1,
        method: "test.echo".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    assert_eq!(response.result.unwrap()["echo"], "hello");
}

#[test]
fn test_large_message() {
    let (socket_path, _handle) = start_test_server();

    let large_message = "x".repeat(100_000); // 100KB message

    let mut params = HashMap::new();
    params.insert("message".to_string(), json!(large_message));

    let request = Request {
        id: "large-1".to_string(),
        v: 1,
        method: "test.echo".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    assert_eq!(
        response.result.unwrap()["echo"].as_str().unwrap().len(),
        100_000
    );
}

#[test]
fn test_unicode_in_params() {
    let (socket_path, _handle) = start_test_server();

    let mut params = HashMap::new();
    params.insert("message".to_string(), json!("Hello 世界 🌍 مرحبا"));

    let request = Request {
        id: "unicode-1".to_string(),
        v: 1,
        method: "test.echo".to_string(),
        params,
    };

    let response = send_request(&socket_path, &request).unwrap();

    assert!(response.ok);
    let result = response.result.unwrap();
    let echo = result["echo"].as_str().unwrap();
    assert!(echo.contains("世界"));
    assert!(echo.contains("🌍"));
    assert!(echo.contains("مرحبا"));
}
