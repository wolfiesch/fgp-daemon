//! Example: Simple echo daemon.
//!
//! This is a minimal FGP daemon that echoes back any parameters it receives.
//!
//! # Run the daemon
//! ```bash
//! cargo run --example echo_daemon
//! ```
//!
//! # Test with the client
//! ```bash
//! echo '{"id":"1","v":1,"method":"echo","params":{"message":"hello"}}' | nc -U ~/.fgp/services/echo/daemon.sock
//! ```

use anyhow::Result;
use fgp_daemon::{FgpServer, FgpService};
use fgp_daemon::service::{MethodInfo, ParamInfo};
use serde_json::Value;
use std::collections::HashMap;

/// Simple echo service.
struct EchoService;

impl FgpService for EchoService {
    fn name(&self) -> &str {
        "echo"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
        match method {
            "echo" => {
                // Echo back the params
                Ok(serde_json::json!({
                    "echo": params,
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }))
            }
            "ping" => {
                Ok(serde_json::json!({"pong": true}))
            }
            "error" => {
                // Intentionally return an error for testing
                anyhow::bail!("Intentional error for testing")
            }
            _ => {
                anyhow::bail!("Unknown method: {}", method)
            }
        }
    }

    fn method_list(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo {
                name: "echo".into(),
                description: "Echo back the provided parameters".into(),
                params: vec![
                    ParamInfo {
                        name: "message".into(),
                        param_type: "string".into(),
                        required: false,
                        default: None,
                    },
                ],
            },
            MethodInfo {
                name: "ping".into(),
                description: "Simple ping/pong health check".into(),
                params: vec![],
            },
            MethodInfo {
                name: "error".into(),
                description: "Returns an error (for testing error handling)".into(),
                params: vec![],
            },
        ]
    }

    fn on_start(&self) -> Result<()> {
        println!("Echo service starting...");
        Ok(())
    }

    fn on_stop(&self) -> Result<()> {
        println!("Echo service stopping...");
        Ok(())
    }
}

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("fgp_daemon=debug")
        .init();

    println!("Starting echo daemon...");
    println!("Socket: ~/.fgp/services/echo/daemon.sock");
    println!();
    println!("Test with:");
    println!("  echo '{{\"id\":\"1\",\"v\":1,\"method\":\"health\",\"params\":{{}}}}' | nc -U ~/.fgp/services/echo/daemon.sock");
    println!("  echo '{{\"id\":\"2\",\"v\":1,\"method\":\"echo\",\"params\":{{\"message\":\"hello\"}}}}' | nc -U ~/.fgp/services/echo/daemon.sock");
    println!();

    let server = FgpServer::new(EchoService, "~/.fgp/services/echo/daemon.sock")?;
    server.serve()?;

    Ok(())
}
