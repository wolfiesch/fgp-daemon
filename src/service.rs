//! FGP service trait definition.
//!
//! Implement [`FgpService`] to create your daemon's business logic.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

/// Trait for FGP daemon services.
///
/// Implement this trait to define your daemon's methods and behavior.
///
/// # Required Methods
///
/// Every FGP daemon must implement these methods:
/// - `health` - Returns service status (handled by [`FgpServer`](crate::FgpServer) by default)
/// - `stop` - Graceful shutdown (handled by [`FgpServer`](crate::FgpServer) by default)
/// - `methods` - List available methods (handled by [`FgpServer`](crate::FgpServer) by default)
///
/// # Example
///
/// ```rust
/// use fgp_daemon::FgpService;
/// use std::collections::HashMap;
/// use serde_json::Value;
/// use anyhow::Result;
///
/// struct GmailService {
///     // Your service state here
/// }
///
/// impl FgpService for GmailService {
///     fn name(&self) -> &str { "gmail" }
///     fn version(&self) -> &str { "1.0.0" }
///
///     fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
///         match method {
///             "gmail.list" => {
///                 let limit = params.get("limit")
///                     .and_then(|v| v.as_u64())
///                     .unwrap_or(10);
///                 // Fetch emails...
///                 Ok(serde_json::json!({"emails": [], "count": 0}))
///             }
///             _ => anyhow::bail!("Unknown method: {}", method),
///         }
///     }
///
///     fn method_list(&self) -> Vec<MethodInfo> {
///         vec![
///             MethodInfo {
///                 name: "gmail.list".into(),
///                 description: "List emails from inbox".into(),
///                 params: vec![
///                     ParamInfo { name: "limit".into(), param_type: "integer".into(), required: false },
///                 ],
///             },
///         ]
///     }
/// }
/// ```
pub trait FgpService: Send + Sync {
    /// Service name (used in socket path and logging).
    fn name(&self) -> &str;

    /// Service version (semver format recommended).
    fn version(&self) -> &str;

    /// Dispatch a method call to the appropriate handler.
    ///
    /// This is the main entry point for all method calls. The server will call this
    /// method for every incoming request, passing the method name and parameters.
    ///
    /// # Arguments
    /// * `method` - The method name (e.g., "gmail.list", "echo")
    /// * `params` - Method parameters as key-value pairs
    ///
    /// # Returns
    /// * `Ok(Value)` - Success result to send back to client
    /// * `Err(_)` - Error to send back to client
    fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value>;

    /// List of methods this service provides.
    ///
    /// Used by the `methods` standard method to advertise available methods.
    /// Override this to provide method documentation.
    fn method_list(&self) -> Vec<MethodInfo> {
        vec![]
    }

    /// Called when the daemon starts.
    ///
    /// Override to perform initialization (e.g., open database connections).
    fn on_start(&self) -> Result<()> {
        Ok(())
    }

    /// Called when the daemon is stopping.
    ///
    /// Override to perform cleanup (e.g., close connections, flush caches).
    fn on_stop(&self) -> Result<()> {
        Ok(())
    }

    /// Custom health check.
    ///
    /// Override to add service-specific health information.
    /// The default implementation returns an empty map.
    fn health_check(&self) -> HashMap<String, HealthStatus> {
        HashMap::new()
    }
}

/// Method information for the `methods` response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MethodInfo {
    /// Method name (e.g., "gmail.list")
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Parameter definitions
    #[serde(default)]
    pub params: Vec<ParamInfo>,
}

/// Parameter information for method documentation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParamInfo {
    /// Parameter name
    pub name: String,
    /// Parameter type (e.g., "string", "integer", "boolean", "object")
    #[serde(rename = "type")]
    pub param_type: String,
    /// Whether this parameter is required
    #[serde(default)]
    pub required: bool,
    /// Default value (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// Health status for a dependency.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthStatus {
    /// Whether the dependency is healthy
    pub ok: bool,
    /// Latency in milliseconds (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    /// Additional status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl HealthStatus {
    /// Create a healthy status.
    pub fn healthy() -> Self {
        Self {
            ok: true,
            latency_ms: None,
            message: None,
        }
    }

    /// Create a healthy status with latency.
    pub fn healthy_with_latency(latency_ms: f64) -> Self {
        Self {
            ok: true,
            latency_ms: Some(latency_ms),
            message: None,
        }
    }

    /// Create an unhealthy status.
    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            latency_ms: None,
            message: Some(message.into()),
        }
    }
}
