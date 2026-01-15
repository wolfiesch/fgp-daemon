//! FGP protocol types for NDJSON communication over UNIX socket.
//!
//! This module defines the core request/response types for the Fast Gateway Protocol.
//! All messages are serialized as single-line JSON (NDJSON format).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::PROTOCOL_VERSION;

/// NDJSON request from client to daemon.
///
/// # Example
/// ```json
/// {"id":"abc123","v":1,"method":"gmail.list","params":{"limit":10}}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Unique request ID (UUID recommended)
    pub id: String,
    /// Protocol version (currently 1)
    pub v: u8,
    /// Method name (e.g., "health", "gmail.list", "bundle")
    pub method: String,
    /// Method parameters (flexible key-value map)
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

/// NDJSON response from daemon to client.
///
/// # Example (success)
/// ```json
/// {"id":"abc123","ok":true,"result":{"emails":[]},"error":null,"meta":{"server_ms":12.5,"protocol_v":1}}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Request ID (echoed from request)
    pub id: String,
    /// Success flag
    pub ok: bool,
    /// Result data (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error information (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    /// Response metadata
    pub meta: ResponseMeta,
}

/// Error details in response.
///
/// Standard error codes:
/// - `INVALID_REQUEST`: Malformed request
/// - `UNKNOWN_METHOD`: Method not found
/// - `INVALID_PARAMS`: Invalid parameters
/// - `INTERNAL_ERROR`: Server-side error
/// - `NOT_FOUND`: Resource not found
/// - `UNAUTHORIZED`: Auth required or failed
/// - `TIMEOUT`: Operation timed out
/// - `SERVICE_UNAVAILABLE`: Dependency unavailable
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// Error code (UPPER_SNAKE_CASE)
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Additional error details (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Response metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMeta {
    /// Server execution time in milliseconds
    pub server_ms: f64,
    /// Protocol version
    pub protocol_v: u8,
}

impl Request {
    /// Create a new request with auto-generated UUID.
    pub fn new(method: impl Into<String>, params: HashMap<String, serde_json::Value>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            v: PROTOCOL_VERSION,
            method: method.into(),
            params,
        }
    }

    /// Create a simple request with no parameters.
    pub fn simple(method: impl Into<String>) -> Self {
        Self::new(method, HashMap::new())
    }

    /// Parse request from NDJSON line.
    pub fn from_ndjson_line(line: &str) -> Result<Self> {
        serde_json::from_str(line).context("Failed to parse request JSON")
    }

    /// Serialize request to NDJSON line.
    pub fn to_ndjson_line(&self) -> Result<String> {
        let json = serde_json::to_string(self)?;
        Ok(format!("{}\n", json))
    }
}

impl Response {
    /// Create a success response.
    pub fn success(id: impl Into<String>, result: serde_json::Value, server_ms: f64) -> Self {
        Self {
            id: id.into(),
            ok: true,
            result: Some(result),
            error: None,
            meta: ResponseMeta {
                server_ms,
                protocol_v: PROTOCOL_VERSION,
            },
        }
    }

    /// Create an error response.
    pub fn error(
        id: impl Into<String>,
        code: &str,
        message: impl Into<String>,
        server_ms: f64,
    ) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(ErrorInfo {
                code: code.to_string(),
                message: message.into(),
                details: None,
            }),
            meta: ResponseMeta {
                server_ms,
                protocol_v: PROTOCOL_VERSION,
            },
        }
    }

    /// Create an error response with details.
    pub fn error_with_details(
        id: impl Into<String>,
        code: &str,
        message: impl Into<String>,
        details: serde_json::Value,
        server_ms: f64,
    ) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(ErrorInfo {
                code: code.to_string(),
                message: message.into(),
                details: Some(details),
            }),
            meta: ResponseMeta {
                server_ms,
                protocol_v: PROTOCOL_VERSION,
            },
        }
    }

    /// Parse response from NDJSON line.
    pub fn from_ndjson_line(line: &str) -> Result<Self> {
        serde_json::from_str(line).context("Failed to parse response JSON")
    }

    /// Serialize response to NDJSON line.
    pub fn to_ndjson_line(&self) -> Result<String> {
        let json = serde_json::to_string(self)?;
        Ok(format!("{}\n", json))
    }
}

/// Standard error codes as constants.
pub mod error_codes {
    pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
    pub const UNKNOWN_METHOD: &str = "UNKNOWN_METHOD";
    pub const INVALID_PARAMS: &str = "INVALID_PARAMS";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    pub const NOT_FOUND: &str = "NOT_FOUND";
    pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
    pub const TIMEOUT: &str = "TIMEOUT";
    pub const SERVICE_UNAVAILABLE: &str = "SERVICE_UNAVAILABLE";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = Request::simple("health");
        let line = req.to_ndjson_line().unwrap();
        assert!(line.ends_with('\n'));
        assert!(line.contains("\"method\":\"health\""));
    }

    #[test]
    fn test_response_success() {
        let resp = Response::success("123", serde_json::json!({"status": "ok"}), 12.5);
        assert!(resp.ok);
        assert!(resp.error.is_none());
        assert_eq!(resp.meta.protocol_v, PROTOCOL_VERSION);
    }

    #[test]
    fn test_response_error() {
        let resp = Response::error("123", error_codes::NOT_FOUND, "User not found", 5.0);
        assert!(!resp.ok);
        assert!(resp.result.is_none());
        assert_eq!(resp.error.as_ref().unwrap().code, "NOT_FOUND");
    }
}
