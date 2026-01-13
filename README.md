# fgp-daemon

Rust SDK for building [Fast Gateway Protocol (FGP)](https://github.com/wolfiesch/fgp) daemons.

FGP daemons use UNIX sockets with NDJSON framing to achieve **10-30ms response times**, compared to 200-500ms for stdio-based protocols like MCP.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
fgp-daemon = "0.1"
```

Create a daemon:

```rust
use fgp_daemon::{FgpServer, FgpService};
use std::collections::HashMap;
use serde_json::Value;
use anyhow::Result;

struct MyService;

impl FgpService for MyService {
    fn name(&self) -> &str { "my-service" }
    fn version(&self) -> &str { "1.0.0" }

    fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
        match method {
            "echo" => Ok(serde_json::json!({"echo": params})),
            _ => anyhow::bail!("Unknown method: {}", method),
        }
    }
}

fn main() -> Result<()> {
    let server = FgpServer::new(MyService, "~/.fgp/services/my-service/daemon.sock")?;
    server.serve()
}
```

## Features

- **Fast**: 10-30ms response times via persistent UNIX sockets
- **Simple**: NDJSON protocol (one JSON object per line)
- **Built-in methods**: `health`, `stop`, `methods` handled automatically
- **Lifecycle management**: PID files, socket cleanup, daemonization
- **Thin client**: Call any FGP daemon from Rust

## Protocol

Request:
```json
{"id":"uuid","v":1,"method":"service.action","params":{}}
```

Response:
```json
{"id":"uuid","ok":true,"result":{},"error":null,"meta":{"server_ms":12}}
```

See [FGP-PROTOCOL.md](https://github.com/wolfiesch/fgp/blob/main/FGP-PROTOCOL.md) for the full specification.

## Examples

Run the echo daemon example:

```bash
cargo run --example echo_daemon
```

Test with netcat:

```bash
echo '{"id":"1","v":1,"method":"health","params":{}}' | nc -U ~/.fgp/services/echo/daemon.sock
```

## Client Usage

```rust
use fgp_daemon::FgpClient;

let client = FgpClient::new("~/.fgp/services/gmail/daemon.sock")?;

// Call a method
let response = client.call("gmail.list", serde_json::json!({"limit": 10}))?;

// Built-in convenience methods
let health = client.health()?;
let methods = client.methods()?;
client.stop()?;
```

## License

MIT
