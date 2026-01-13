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

pub mod protocol;
pub mod server;
pub mod service;
pub mod lifecycle;
pub mod client;

// Re-exports for convenience
pub use protocol::{Request, Response, ErrorInfo, ResponseMeta};
pub use server::FgpServer;
pub use service::FgpService;
pub use client::FgpClient;
pub use lifecycle::{daemonize, write_pid_file, cleanup_socket};

/// Protocol version constant
pub const PROTOCOL_VERSION: u8 = 1;

/// Default socket base path
pub const DEFAULT_SOCKET_BASE: &str = "~/.fgp/services";
