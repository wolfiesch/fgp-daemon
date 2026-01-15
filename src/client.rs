//! FGP client for calling daemon methods.
//!
//! Provides a simple client for connecting to FGP daemons and making method calls.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::protocol::{Request, Response};

/// FGP client for calling daemon methods.
///
/// # Example
///
/// ```rust,no_run
/// use fgp_daemon::FgpClient;
///
/// let client = FgpClient::new("~/.fgp/services/gmail/daemon.sock")?;
///
/// // Call a method
/// let response = client.call("gmail.list", serde_json::json!({"limit": 10}))?;
/// println!("Response: {:?}", response);
///
/// // Simple health check
/// let health = client.health()?;
/// println!("Health: {:?}", health);
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// ## Auto-Start
///
/// Use `FgpClient::for_service()` to create a client that automatically starts
/// the daemon if it's not running:
///
/// ```rust,no_run
/// use fgp_daemon::FgpClient;
///
/// // This will auto-start the gmail daemon if needed
/// let client = FgpClient::for_service("gmail")?;
/// let response = client.call("gmail.inbox", serde_json::json!({}))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub struct FgpClient {
    socket_path: PathBuf,
    timeout: Duration,
    /// Service name for auto-start support
    auto_start_service: Option<String>,
}

impl FgpClient {
    /// Create a new FGP client.
    ///
    /// # Arguments
    /// * `socket_path` - Path to the daemon's UNIX socket (supports `~` expansion)
    pub fn new(socket_path: impl AsRef<Path>) -> Result<Self> {
        let socket_path = expand_path(socket_path.as_ref())?;
        Ok(Self {
            socket_path,
            timeout: Duration::from_secs(30),
            auto_start_service: None,
        })
    }

    /// Create a client for a named service with auto-start enabled.
    ///
    /// This is the recommended way to create a client. If the daemon is not running,
    /// it will be automatically started on the first call.
    ///
    /// # Arguments
    /// * `service_name` - Name of the service (e.g., "gmail", "browser", "calendar")
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use fgp_daemon::FgpClient;
    ///
    /// let client = FgpClient::for_service("gmail")?;
    /// // If gmail daemon isn't running, it will be auto-started
    /// let response = client.call("gmail.inbox", serde_json::json!({}))?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn for_service(service_name: &str) -> Result<Self> {
        let socket_path = crate::lifecycle::service_socket_path(service_name);
        Ok(Self {
            socket_path,
            timeout: Duration::from_secs(30),
            auto_start_service: Some(service_name.to_string()),
        })
    }

    /// Set the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable auto-start for a specific service.
    ///
    /// When auto-start is enabled and the daemon is not running, the client
    /// will attempt to start it automatically on the first call.
    ///
    /// # Arguments
    /// * `service_name` - Name of the service to auto-start
    pub fn with_auto_start(mut self, service_name: &str) -> Self {
        self.auto_start_service = Some(service_name.to_string());
        self
    }

    /// Disable auto-start.
    ///
    /// Calls will fail immediately if the daemon is not running.
    pub fn without_auto_start(mut self) -> Self {
        self.auto_start_service = None;
        self
    }

    /// Call a daemon method.
    ///
    /// # Arguments
    /// * `method` - Method name (e.g., "gmail.list")
    /// * `params` - Method parameters as JSON value
    pub fn call(&self, method: &str, params: serde_json::Value) -> Result<Response> {
        let params_map: HashMap<String, serde_json::Value> = match params {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            serde_json::Value::Null => HashMap::new(),
            _ => {
                let mut map = HashMap::new();
                map.insert("value".into(), params);
                map
            }
        };

        let request = Request::new(method, params_map);
        self.send_request(&request)
    }

    /// Call a method with raw params HashMap.
    pub fn call_raw(
        &self,
        method: &str,
        params: HashMap<String, serde_json::Value>,
    ) -> Result<Response> {
        let request = Request::new(method, params);
        self.send_request(&request)
    }

    /// Call the `health` method.
    pub fn health(&self) -> Result<Response> {
        self.call("health", serde_json::Value::Null)
    }

    /// Call the `methods` method.
    pub fn methods(&self) -> Result<Response> {
        self.call("methods", serde_json::Value::Null)
    }

    /// Call the `stop` method.
    pub fn stop(&self) -> Result<Response> {
        self.call("stop", serde_json::Value::Null)
    }

    /// Check if the daemon is running.
    pub fn is_running(&self) -> bool {
        self.health().is_ok()
    }

    /// Send a request and receive a response.
    fn send_request(&self, request: &Request) -> Result<Response> {
        // Try to connect to socket
        let stream = match UnixStream::connect(&self.socket_path) {
            Ok(stream) => stream,
            Err(e) => {
                // Connection failed - try auto-start if configured
                if let Some(ref service_name) = self.auto_start_service {
                    tracing::info!(
                        "Daemon not running, auto-starting service '{}'...",
                        service_name
                    );

                    // Start the service
                    crate::lifecycle::start_service(service_name)
                        .with_context(|| format!("Failed to auto-start service '{}'", service_name))?;

                    // Retry connection
                    UnixStream::connect(&self.socket_path)
                        .with_context(|| format!("Cannot connect to daemon at {:?} after auto-start", self.socket_path))?
                } else {
                    return Err(e).with_context(|| format!("Cannot connect to daemon at {:?}", self.socket_path));
                }
            }
        };

        self.send_request_on_stream(stream, request)
    }

    /// Send request on an already-connected stream.
    fn send_request_on_stream(&self, mut stream: UnixStream, request: &Request) -> Result<Response> {
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        // Send request
        let request_line = request.to_ndjson_line()?;
        stream.write_all(request_line.as_bytes())?;
        stream.flush()?;

        // Read response
        let mut reader = BufReader::new(&stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        Response::from_ndjson_line(&response_line)
    }
}

/// Expand `~` in path to home directory.
fn expand_path(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();
    let expanded = shellexpand::tilde(&path_str);
    Ok(PathBuf::from(expanded.as_ref()))
}

/// Convenience function to call a method on a daemon.
///
/// This does NOT auto-start the daemon. If the daemon is not running, the call
/// will fail. Use `call_auto_start()` if you want automatic daemon startup.
///
/// # Example
///
/// ```rust,no_run
/// use fgp_daemon::client::call;
///
/// // Fails if gmail daemon is not running
/// let response = call("gmail", "gmail.list", serde_json::json!({"limit": 5}))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn call(service_name: &str, method: &str, params: serde_json::Value) -> Result<Response> {
    let socket_path = crate::lifecycle::service_socket_path(service_name);
    let client = FgpClient::new(socket_path)?;
    client.call(method, params)
}

/// Call a method with auto-start enabled.
///
/// If the daemon is not running, it will be started automatically.
///
/// # Example
///
/// ```rust,no_run
/// use fgp_daemon::client::call_auto_start;
///
/// // Auto-starts gmail daemon if not running
/// let response = call_auto_start("gmail", "gmail.list", serde_json::json!({"limit": 5}))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn call_auto_start(service_name: &str, method: &str, params: serde_json::Value) -> Result<Response> {
    let client = FgpClient::for_service(service_name)?;
    client.call(method, params)
}

/// Check if a daemon service is running.
pub fn is_running(service_name: &str) -> bool {
    crate::lifecycle::is_service_running(service_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path() {
        let path = expand_path(Path::new("~/.fgp/test")).unwrap();
        assert!(!path.to_string_lossy().contains('~'));
    }
}
