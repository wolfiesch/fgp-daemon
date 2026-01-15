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
/// use fgp_daemon::service::{MethodInfo, ParamInfo};
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
///             MethodInfo::new("gmail.list", "List emails from inbox")
///                 .param(ParamInfo {
///                     name: "limit".into(),
///                     param_type: "integer".into(),
///                     required: false,
///                     default: None,
///                 }),
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
///
/// Supports both legacy `params` array and full JSON Schema via `schema` field.
/// When both are present, `schema` takes precedence.
///
/// # Example with Schema Builder
///
/// ```rust
/// use fgp_daemon::service::MethodInfo;
/// use fgp_daemon::schema::SchemaBuilder;
///
/// let method = MethodInfo::new("gmail.send", "Send an email")
///     .schema(SchemaBuilder::object()
///         .property("to", SchemaBuilder::string().format("email"))
///         .property("subject", SchemaBuilder::string())
///         .required(&["to", "subject"])
///         .build())
///     .errors(&["UNAUTHORIZED", "INVALID_RECIPIENT"]);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MethodInfo {
    /// Method name (e.g., "gmail.list")
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Parameter definitions (legacy, use `schema` for new code)
    #[serde(default)]
    pub params: Vec<ParamInfo>,

    /// Full JSON Schema for parameters (takes precedence over `params`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,

    /// JSON Schema for successful response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<Value>,

    /// Usage examples
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<MethodExample>,

    /// Possible error codes this method may return
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,

    /// Whether this method is deprecated
    #[serde(default)]
    pub deprecated: bool,
}

impl MethodInfo {
    /// Create a new method info with name and description.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            params: vec![],
            schema: None,
            returns: None,
            examples: vec![],
            errors: vec![],
            deprecated: false,
        }
    }

    /// Set the full JSON Schema for parameters.
    pub fn schema(mut self, schema: Value) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Set the JSON Schema for the return value.
    pub fn returns(mut self, schema: Value) -> Self {
        self.returns = Some(schema);
        self
    }

    /// Add a usage example.
    pub fn example(mut self, description: impl Into<String>, params: Value) -> Self {
        self.examples.push(MethodExample {
            description: description.into(),
            params,
            result: None,
        });
        self
    }

    /// Add a usage example with expected result.
    pub fn example_with_result(
        mut self,
        description: impl Into<String>,
        params: Value,
        result: Value,
    ) -> Self {
        self.examples.push(MethodExample {
            description: description.into(),
            params,
            result: Some(result),
        });
        self
    }

    /// Set possible error codes.
    pub fn errors(mut self, codes: &[&str]) -> Self {
        self.errors = codes.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Mark this method as deprecated.
    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self
    }

    /// Add legacy param info (for backward compatibility).
    pub fn param(mut self, param: ParamInfo) -> Self {
        self.params.push(param);
        self
    }
}

/// Usage example for a method.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MethodExample {
    /// Description of what this example demonstrates
    pub description: String,
    /// Example parameters
    pub params: Value,
    /// Expected result (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
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
