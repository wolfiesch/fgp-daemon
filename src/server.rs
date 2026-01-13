//! FGP UNIX socket server implementation.
//!
//! The [`FgpServer`] handles socket creation, connection management, and request dispatch.

use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use crate::protocol::{self, error_codes, Response};
use crate::service::{FgpService, MethodInfo};

/// FGP daemon server.
///
/// Listens on a UNIX socket and dispatches requests to the service.
///
/// # Example
///
/// ```rust,no_run
/// use fgp_daemon::{FgpServer, FgpService};
/// # use std::collections::HashMap;
/// # use serde_json::Value;
/// # use anyhow::Result;
/// #
/// # struct MyService;
/// # impl FgpService for MyService {
/// #     fn name(&self) -> &str { "test" }
/// #     fn version(&self) -> &str { "1.0.0" }
/// #     fn dispatch(&self, _: &str, _: HashMap<String, Value>) -> Result<Value> { Ok(Value::Null) }
/// # }
///
/// let server = FgpServer::new(MyService, "~/.fgp/services/test/daemon.sock")?;
/// server.serve()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub struct FgpServer<S: FgpService> {
    service: S,
    socket_path: PathBuf,
    started_at: Instant,
    running: Arc<AtomicBool>,
}

impl<S: FgpService> FgpServer<S> {
    /// Create a new FGP server.
    ///
    /// # Arguments
    /// * `service` - The service implementation
    /// * `socket_path` - Path to the UNIX socket (supports `~` expansion)
    pub fn new(service: S, socket_path: impl AsRef<Path>) -> Result<Self> {
        let socket_path = expand_path(socket_path.as_ref())?;

        // Create parent directory if needed
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            service,
            socket_path,
            started_at: Instant::now(),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Get the socket path.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Start serving requests (blocking).
    ///
    /// This method blocks until `stop()` is called or the process receives a signal.
    pub fn serve(&self) -> Result<()> {
        // Call service on_start hook
        self.service.on_start()?;

        // Clean up stale socket
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;

        // Set permissions to owner-only (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        self.running.store(true, Ordering::SeqCst);

        info!(
            service = self.service.name(),
            version = self.service.version(),
            socket = %self.socket_path.display(),
            "FGP daemon started"
        );

        // Accept connections sequentially (single-threaded)
        for stream in listener.incoming() {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            match stream {
                Ok(stream) => {
                    if let Err(e) = self.handle_connection(stream) {
                        error!(error = %e, "Connection error");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Accept error");
                }
            }
        }

        // Call service on_stop hook
        let _ = self.service.on_stop();

        // Cleanup
        let _ = std::fs::remove_file(&self.socket_path);

        info!(service = self.service.name(), "FGP daemon stopped");
        Ok(())
    }

    /// Stop the server gracefully.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Handle a single client connection.
    fn handle_connection(&self, stream: UnixStream) -> Result<()> {
        let writer_stream = stream.try_clone()?;
        let mut reader = BufReader::new(&stream);
        let mut writer = writer_stream;

        // Read NDJSON request (one line)
        let mut line = String::new();
        reader.read_line(&mut line)?;

        if line.trim().is_empty() {
            return Ok(()); // Client disconnected
        }

        let start = Instant::now();

        // Parse request
        let request = match protocol::Request::from_ndjson_line(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = Response::error(
                    "null",
                    error_codes::INVALID_REQUEST,
                    format!("Failed to parse request: {}", e),
                    start.elapsed().as_secs_f64() * 1000.0,
                );
                let response_line = response.to_ndjson_line()?;
                writer.write_all(response_line.as_bytes())?;
                writer.flush()?;
                return Ok(());
            }
        };

        debug!(
            method = %request.method,
            id = %request.id,
            "Handling request"
        );

        // Dispatch to service or handle built-in methods
        let response = match request.method.as_str() {
            "health" => self.handle_health(&request.id, start),
            "stop" => {
                self.stop();
                Response::success(
                    &request.id,
                    serde_json::json!({"message": "Shutting down"}),
                    start.elapsed().as_secs_f64() * 1000.0,
                )
            }
            "methods" => self.handle_methods(&request.id, start),
            _ => match self.service.dispatch(&request.method, request.params) {
                Ok(result) => Response::success(
                    &request.id,
                    result,
                    start.elapsed().as_secs_f64() * 1000.0,
                ),
                Err(e) => Response::error(
                    &request.id,
                    error_codes::INTERNAL_ERROR,
                    e.to_string(),
                    start.elapsed().as_secs_f64() * 1000.0,
                ),
            },
        };

        // Send NDJSON response
        let response_line = response.to_ndjson_line()?;
        writer.write_all(response_line.as_bytes())?;
        writer.flush()?;

        debug!(
            method = %request.method,
            id = %request.id,
            server_ms = response.meta.server_ms,
            "Request complete"
        );

        Ok(())
    }

    /// Handle the `health` built-in method.
    fn handle_health(&self, id: &str, start: Instant) -> Response {
        let uptime = self.started_at.elapsed().as_secs();
        let services = self.service.health_check();

        // Determine overall status
        let status = if services.values().all(|s| s.ok) {
            "healthy"
        } else if services.values().any(|s| s.ok) {
            "degraded"
        } else if services.is_empty() {
            "healthy"
        } else {
            "unhealthy"
        };

        Response::success(
            id,
            serde_json::json!({
                "status": status,
                "pid": std::process::id(),
                "started_at": chrono_now_iso(),
                "version": self.service.version(),
                "uptime_seconds": uptime,
                "services": services,
            }),
            start.elapsed().as_secs_f64() * 1000.0,
        )
    }

    /// Handle the `methods` built-in method.
    fn handle_methods(&self, id: &str, start: Instant) -> Response {
        let mut methods: Vec<MethodInfo> = vec![
            MethodInfo {
                name: "health".into(),
                description: "Returns daemon health and status".into(),
                params: vec![],
            },
            MethodInfo {
                name: "stop".into(),
                description: "Gracefully shuts down the daemon".into(),
                params: vec![],
            },
            MethodInfo {
                name: "methods".into(),
                description: "Lists available methods".into(),
                params: vec![],
            },
        ];

        methods.extend(self.service.method_list());

        Response::success(
            id,
            serde_json::json!({"methods": methods}),
            start.elapsed().as_secs_f64() * 1000.0,
        )
    }
}

/// Expand `~` in path to home directory.
fn expand_path(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy();
    let expanded = shellexpand::tilde(&path_str);
    Ok(PathBuf::from(expanded.as_ref()))
}

/// Get current time as ISO 8601 string (without chrono dependency).
fn chrono_now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple ISO format without full chrono
    format!("{}Z", secs)
}
