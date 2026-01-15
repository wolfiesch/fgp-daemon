//! Error code tests for FGP protocol.
//!
//! Tests that all standard error codes are correctly defined and
//! can be used in error responses.
//!
//! # CHANGELOG (recent first, max 5 entries)
//! 01/14/2026 - Initial implementation (Claude)

use fgp_daemon::protocol::{error_codes, ErrorInfo, Response, ResponseMeta};
use serde_json::json;

// ============================================================================
// Standard Error Code Constants
// ============================================================================

#[test]
fn test_error_code_invalid_request() {
    assert_eq!(error_codes::INVALID_REQUEST, "INVALID_REQUEST");
}

#[test]
fn test_error_code_unknown_method() {
    assert_eq!(error_codes::UNKNOWN_METHOD, "UNKNOWN_METHOD");
}

#[test]
fn test_error_code_invalid_params() {
    assert_eq!(error_codes::INVALID_PARAMS, "INVALID_PARAMS");
}

#[test]
fn test_error_code_internal_error() {
    assert_eq!(error_codes::INTERNAL_ERROR, "INTERNAL_ERROR");
}

#[test]
fn test_error_code_not_found() {
    assert_eq!(error_codes::NOT_FOUND, "NOT_FOUND");
}

#[test]
fn test_error_code_unauthorized() {
    assert_eq!(error_codes::UNAUTHORIZED, "UNAUTHORIZED");
}

#[test]
fn test_error_code_timeout() {
    assert_eq!(error_codes::TIMEOUT, "TIMEOUT");
}

#[test]
fn test_error_code_service_unavailable() {
    assert_eq!(error_codes::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE");
}

// ============================================================================
// Error Response Construction
// ============================================================================

#[test]
fn test_invalid_request_response() {
    let response = Response {
        id: "err-1".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::INVALID_REQUEST.to_string(),
            message: "Request JSON is malformed".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 0.1,
            protocol_v: 1,
        },
    };

    assert!(!response.ok);
    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "INVALID_REQUEST");
}

#[test]
fn test_unknown_method_response() {
    let method = "nonexistent.method";
    let response = Response {
        id: "err-2".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::UNKNOWN_METHOD.to_string(),
            message: format!("Method '{}' not found", method),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 0.2,
            protocol_v: 1,
        },
    };

    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "UNKNOWN_METHOD");
    assert!(err.message.contains(method));
}

#[test]
fn test_invalid_params_response() {
    let response = Response {
        id: "err-3".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::INVALID_PARAMS.to_string(),
            message: "Missing required parameter: repo".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 0.3,
            protocol_v: 1,
        },
    };

    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "INVALID_PARAMS");
    assert!(err.message.contains("repo"));
}

#[test]
fn test_internal_error_response() {
    let response = Response {
        id: "err-4".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::INTERNAL_ERROR.to_string(),
            message: "Database connection failed".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 100.0,
            protocol_v: 1,
        },
    };

    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "INTERNAL_ERROR");
}

#[test]
fn test_not_found_response() {
    let response = Response {
        id: "err-5".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::NOT_FOUND.to_string(),
            message: "Resource with ID 'abc123' not found".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 5.0,
            protocol_v: 1,
        },
    };

    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "NOT_FOUND");
}

#[test]
fn test_unauthorized_response() {
    let response = Response {
        id: "err-6".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::UNAUTHORIZED.to_string(),
            message: "Invalid or expired token".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 1.0,
            protocol_v: 1,
        },
    };

    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "UNAUTHORIZED");
}

#[test]
fn test_timeout_response() {
    let response = Response {
        id: "err-7".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::TIMEOUT.to_string(),
            message: "Request timed out after 30000ms".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 30000.0,
            protocol_v: 1,
        },
    };

    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "TIMEOUT");
}

#[test]
fn test_service_unavailable_response() {
    let response = Response {
        id: "err-8".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::SERVICE_UNAVAILABLE.to_string(),
            message: "Service is temporarily unavailable".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 0.5,
            protocol_v: 1,
        },
    };

    let err = response.error.as_ref().unwrap();
    assert_eq!(err.code, "SERVICE_UNAVAILABLE");
}

// ============================================================================
// Error Info Serialization
// ============================================================================

#[test]
fn test_error_info_serialization() {
    let error = ErrorInfo {
        code: error_codes::INTERNAL_ERROR.to_string(),
        message: "Something went wrong".to_string(),
        details: None,
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["code"], "INTERNAL_ERROR");
    assert_eq!(parsed["message"], "Something went wrong");
}

#[test]
fn test_error_info_deserialization() {
    let json = r#"{"code":"UNKNOWN_METHOD","message":"Method not found"}"#;
    let error: ErrorInfo = serde_json::from_str(json).unwrap();

    assert_eq!(error.code, "UNKNOWN_METHOD");
    assert_eq!(error.message, "Method not found");
}

#[test]
fn test_error_response_full_serialization() {
    let response = Response {
        id: "full-err".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::INVALID_PARAMS.to_string(),
            message: "Expected string, got number".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 1.5,
            protocol_v: 1,
        },
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Verify structure
    assert_eq!(parsed["ok"], false);
    assert!(parsed["result"].is_null());
    assert_eq!(parsed["error"]["code"], "INVALID_PARAMS");
    assert_eq!(parsed["meta"]["protocol_v"], 1);
}

// ============================================================================
// Error Message Edge Cases
// ============================================================================

#[test]
fn test_error_message_with_special_characters() {
    let error = ErrorInfo {
        code: error_codes::INVALID_REQUEST.to_string(),
        message: r#"Invalid JSON: unexpected token '}' at position 42"#.to_string(),
        details: None,
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: ErrorInfo = serde_json::from_str(&json).unwrap();

    assert!(parsed.message.contains("'}'"));
}

#[test]
fn test_error_message_with_newlines() {
    let error = ErrorInfo {
        code: error_codes::INTERNAL_ERROR.to_string(),
        message: "Error on line 1\nCaused by: error on line 2".to_string(),
        details: None,
    };

    let json = serde_json::to_string(&error).unwrap();

    // JSON should escape newlines
    assert!(!json.contains('\n') || json.contains("\\n"));

    let parsed: ErrorInfo = serde_json::from_str(&json).unwrap();
    assert!(parsed.message.contains('\n'));
}

#[test]
fn test_error_message_unicode() {
    let error = ErrorInfo {
        code: error_codes::NOT_FOUND.to_string(),
        message: "文件未找到 (File not found) 🔍".to_string(),
        details: None,
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: ErrorInfo = serde_json::from_str(&json).unwrap();

    assert!(parsed.message.contains("文件未找到"));
    assert!(parsed.message.contains("🔍"));
}

#[test]
fn test_error_message_empty() {
    let error = ErrorInfo {
        code: error_codes::INTERNAL_ERROR.to_string(),
        message: String::new(),
        details: None,
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: ErrorInfo = serde_json::from_str(&json).unwrap();

    assert!(parsed.message.is_empty());
}

#[test]
fn test_error_message_very_long() {
    let long_message = "x".repeat(10000);

    let error = ErrorInfo {
        code: error_codes::INTERNAL_ERROR.to_string(),
        message: long_message.clone(),
        details: None,
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: ErrorInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.message.len(), 10000);
}

// ============================================================================
// Custom Error Codes
// ============================================================================

#[test]
fn test_custom_error_code() {
    // Services can define custom error codes
    let custom_code = "RATE_LIMITED";

    let error = ErrorInfo {
        code: custom_code.to_string(),
        message: "Too many requests. Retry after 60 seconds.".to_string(),
        details: None,
    };

    let json = serde_json::to_string(&error).unwrap();
    let parsed: ErrorInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.code, "RATE_LIMITED");
}

#[test]
fn test_service_specific_error_code() {
    // Example: GitHub-specific error
    let error = ErrorInfo {
        code: "GITHUB_API_ERROR".to_string(),
        message: "GraphQL query failed: Bad credentials".to_string(),
        details: None,
    };

    assert!(error.code.starts_with("GITHUB"));
}

// ============================================================================
// Response State Validation
// ============================================================================

#[test]
fn test_success_response_has_no_error() {
    let response = Response {
        id: "success-1".to_string(),
        ok: true,
        result: Some(json!({"status": "ok"})),
        error: None,
        meta: ResponseMeta {
            server_ms: 5.0,
            protocol_v: 1,
        },
    };

    assert!(response.ok);
    assert!(response.result.is_some());
    assert!(response.error.is_none());
}

#[test]
fn test_error_response_has_no_result() {
    let response = Response {
        id: "error-1".to_string(),
        ok: false,
        result: None,
        error: Some(ErrorInfo {
            code: error_codes::INTERNAL_ERROR.to_string(),
            message: "Failed".to_string(),
            details: None,
        }),
        meta: ResponseMeta {
            server_ms: 5.0,
            protocol_v: 1,
        },
    };

    assert!(!response.ok);
    assert!(response.result.is_none());
    assert!(response.error.is_some());
}

// ============================================================================
// Error Code Naming Convention
// ============================================================================

#[test]
fn test_error_codes_are_uppercase() {
    assert!(error_codes::INVALID_REQUEST
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
    assert!(error_codes::UNKNOWN_METHOD
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
    assert!(error_codes::INVALID_PARAMS
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
    assert!(error_codes::INTERNAL_ERROR
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
    assert!(error_codes::NOT_FOUND
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
    assert!(error_codes::UNAUTHORIZED
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
    assert!(error_codes::TIMEOUT
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
    assert!(error_codes::SERVICE_UNAVAILABLE
        .chars()
        .all(|c| c.is_uppercase() || c == '_'));
}

#[test]
fn test_error_codes_use_underscore_separator() {
    // Multi-word error codes use underscore
    assert!(error_codes::INVALID_REQUEST.contains('_'));
    assert!(error_codes::UNKNOWN_METHOD.contains('_'));
    assert!(error_codes::INVALID_PARAMS.contains('_'));
    assert!(error_codes::INTERNAL_ERROR.contains('_'));
    assert!(error_codes::NOT_FOUND.contains('_'));
    assert!(error_codes::SERVICE_UNAVAILABLE.contains('_'));
}
