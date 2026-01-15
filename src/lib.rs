//! # fgp-daemon
//!
//! Fast Gateway Protocol SDK for building low-latency daemon services.
//!
//! FGP daemons use UNIX sockets with NDJSON framing to achieve 10-30ms response times,
//! compared to 200-500ms for stdio-based protocols like MCP.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use fgp_daemon::{FgpServer, FgpService, Request, Response};
//! use std::collections::HashMap;
//! use serde_json::Value;
//! use anyhow::Result;
//!
//! struct MyService;
//!
//! impl FgpService for MyService {
//!     fn name(&self) -> &str { "my-service" }
//!     fn version(&self) -> &str { "1.0.0" }
//!
//!     fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
//!         match method {
//!             "echo" => Ok(serde_json::json!({"echo": params})),
//!             _ => anyhow::bail!("Unknown method: {}", method),
//!         }
//!     }
//! }
//!
//! fn main() -> Result<()> {
//!     let server = FgpServer::new(MyService, "~/.fgp/services/my-service/daemon.sock")?;
//!     server.serve()
//! }
//! ```
//!
//! ## Protocol Overview
//!
//! FGP uses NDJSON (newline-delimited JSON) over UNIX sockets:
//!
//! **Request:**
//! ```json
//! {"id":"uuid","v":1,"method":"service.action","params":{}}
//! ```
//!
//! **Response:**
//! ```json
//! {"id":"uuid","ok":true,"result":{},"error":null,"meta":{"server_ms":12}}
//! ```

pub mod client;
pub mod lifecycle;
pub mod logging;
pub mod protocol;
pub mod schema;
pub mod server;
pub mod service;

#[cfg(feature = "python")]
pub mod python;

// Re-exports for convenience
pub use client::FgpClient;
pub use schema::{to_anthropic, to_mcp, to_openai, McpTool, SchemaBuilder};
pub use lifecycle::{
    cleanup_socket, daemonize, fgp_services_dir, is_service_running, service_pid_path,
    service_socket_path, start_service, start_service_with_timeout, stop_service, write_pid_file,
};
pub use protocol::{ErrorInfo, Request, Response, ResponseMeta};
pub use server::FgpServer;
pub use service::FgpService;

#[cfg(feature = "python")]
pub use python::PythonModule;

/// Protocol version constant
pub const PROTOCOL_VERSION: u8 = 1;

/// Default socket base path
pub const DEFAULT_SOCKET_BASE: &str = "~/.fgp/services";
