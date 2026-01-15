//! FGP UNIX socket server implementation.
//!
//! The [`FgpServer`] handles socket creation, connection management, and request dispatch.

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
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
pub struct FgpServer<S: FgpService + 'static> {
    service: Arc<S>,
    socket_path: PathBuf,
    started_at: Arc<Instant>,
    started_at_iso: Arc<String>,
    running: Arc<AtomicBool>,
}

impl<S: FgpService + 'static> FgpServer<S> {
    /// Create a new FGP server.
    ///
    /// # Arguments
    /// * `service` - The service implementation
    /// * `socket_path` - Path to the UNIX socket (supports `~` expansion)
    pub fn new(service: S, socket_path: impl AsRef<Path>) -> Result<Self> {
        let socket_path = expand_path(socket_path.as_ref())?;
        let started_at_iso = Arc::new(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));

        // Create parent directory if needed
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            service: Arc::new(service),
            socket_path,
            started_at: Arc::new(Instant::now()),
            started_at_iso,
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
    /// Connections are handled concurrently using threads for parallel request processing.
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
            "FGP daemon started (concurrent mode)"
        );

        // Accept connections and spawn thread for each (concurrent)
        for stream in listener.incoming() {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            match stream {
                Ok(stream) => {
                    // Clone Arcs for the spawned thread
                    let service = Arc::clone(&self.service);
                    let started_at = Arc::clone(&self.started_at);
                    let started_at_iso = Arc::clone(&self.started_at_iso);
                    let running = Arc::clone(&self.running);

                    thread::spawn(move || {
                        if let Err(e) = Self::handle_connection_static(
                            stream,
                            &service,
                            &started_at,
                            &started_at_iso,
                            &running,
                        ) {
                            error!(error = %e, "Connection error");
                        }
                    });
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

    /// Handle a single client connection (instance method - calls static version).
    #[allow(dead_code)]
    fn handle_connection(&self, stream: UnixStream) -> Result<()> {
        Self::handle_connection_static(
            stream,
            &self.service,
            &self.started_at,
            &self.started_at_iso,
            &self.running,
        )
    }

    /// Handle a single client connection (static version for thread spawning).
    fn handle_connection_static(
        stream: UnixStream,
        service: &Arc<S>,
        started_at: &Arc<Instant>,
        started_at_iso: &Arc<String>,
        running: &Arc<AtomicBool>,
    ) -> Result<()> {
        let writer_stream = stream.try_clone()?;
        let mut reader = BufReader::new(&stream);
        let mut writer = writer_stream;

        // Read NDJSON requests (one line at a time)
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                return Ok(()); // Client disconnected
            }

            if line.trim().is_empty() {
                continue;
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
                    continue;
                }
            };

            if request.v != crate::PROTOCOL_VERSION {
                let response = Response::error(
                    &request.id,
                    error_codes::INVALID_REQUEST,
                    format!(
                        "Unsupported protocol version: {} (expected {})",
                        request.v,
                        crate::PROTOCOL_VERSION
                    ),
                    start.elapsed().as_secs_f64() * 1000.0,
                );
                let response_line = response.to_ndjson_line()?;
                writer.write_all(response_line.as_bytes())?;
                writer.flush()?;
                continue;
            }

            let method = request.method.as_str();
            let service_prefix = format!("{}.", service.name());
            let is_namespaced_for_service = method.starts_with(&service_prefix);
            let action = if is_namespaced_for_service {
                &method[service_prefix.len()..]
            } else {
                method
            };

            debug!(
                method = %request.method,
                id = %request.id,
                "Handling request"
            );

            // Dispatch to service or handle built-in methods. Built-ins may be called as either:
            // - "health" / "methods" / "stop" (preferred)
            // - "<service>.health" / "<service>.methods" / "<service>.stop" (accepted for compatibility)
            let response = match action {
                "health" if method == "health" || is_namespaced_for_service => {
                    Self::handle_health_static(&request.id, start, service, started_at, started_at_iso)
                }
                "stop" if method == "stop" || is_namespaced_for_service => {
                    running.store(false, Ordering::SeqCst);
                    Response::success(
                        &request.id,
                        serde_json::json!({"message": "Shutting down"}),
                        start.elapsed().as_secs_f64() * 1000.0,
                    )
                }
                "methods" if method == "methods" || is_namespaced_for_service => {
                    Self::handle_methods_static(&request.id, start, service)
                }
                _ => {
                    if method.contains('.') && !is_namespaced_for_service {
                        Response::error(
                            &request.id,
                            error_codes::INVALID_REQUEST,
                            format!(
                                "Method namespace must match service '{}': got '{}'",
                                service.name(),
                                method
                            ),
                            start.elapsed().as_secs_f64() * 1000.0,
                        )
                    } else {
                        // Normalize to fully-qualified method names for the service dispatch.
                        let dispatch_method = if is_namespaced_for_service {
                            request.method.clone()
                        } else if method.contains('.') {
                            // Already handled mismatch above, so this is unreachable.
                            request.method.clone()
                        } else {
                            format!("{}{}", service_prefix, method)
                        };

                        debug!(
                            request_method = %request.method,
                            dispatch_method = %dispatch_method,
                            id = %request.id,
                            "Dispatching request"
                        );

                        match service.dispatch(&dispatch_method, request.params) {
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
                        }
                    }
                }
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

            if !running.load(Ordering::SeqCst) {
                break;
            }
        }

        Ok(())
    }

    /// Handle the `health` built-in method (instance version).
    #[allow(dead_code)]
    fn handle_health(&self, id: &str, start: Instant) -> Response {
        Self::handle_health_static(id, start, &self.service, &self.started_at, &self.started_at_iso)
    }

    /// Handle the `health` built-in method (static version).
    fn handle_health_static(
        id: &str,
        start: Instant,
        service: &Arc<S>,
        started_at: &Arc<Instant>,
        started_at_iso: &Arc<String>,
    ) -> Response {
        let uptime = started_at.elapsed().as_secs();
        let services = service.health_check();

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
                "started_at": started_at_iso.as_str(),
                "version": service.version(),
                "uptime_seconds": uptime,
                "services": services,
            }),
            start.elapsed().as_secs_f64() * 1000.0,
        )
    }

    /// Handle the `methods` built-in method (instance version).
    #[allow(dead_code)]
    fn handle_methods(&self, id: &str, start: Instant) -> Response {
        Self::handle_methods_static(id, start, &self.service)
    }

    /// Handle the `methods` built-in method (static version).
    fn handle_methods_static(id: &str, start: Instant, service: &Arc<S>) -> Response {
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

        let service_prefix = format!("{}.", service.name());
        for mut method_info in service.method_list() {
            if !method_info.name.contains('.') {
                method_info.name = format!("{}{}", service_prefix, method_info.name);
            }
            methods.push(method_info);
        }

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
