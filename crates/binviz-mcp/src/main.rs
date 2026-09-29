//! binviz as an MCP (Model Context Protocol) server.
//!
//! An agent loads a binary once and then explores it for as long as it likes —
//! search, inspect addresses, disassemble, ask where the bytes go, map it out
//! with notes — without re-parsing anything. Speaks JSON-RPC 2.0 over stdio:
//!
//! ```text
//! claude mcp add binviz -- /path/to/binviz-mcp
//! ```
//!
//! Notes are saved next to the binary (`<file>.binviz-notes.json`) in the same
//! format the web UI imports and exports, so an agent's work shows up there.

mod crash;
mod diff;
mod folders;
mod notes;
mod queue;
mod tools;

use std::io::{BufRead, Write};

use serde_json::{Value, json};

/// Protocol revisions we speak; the newest one the client asks for wins.
const PROTOCOLS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    let mut server = tools::Server::default();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&mut server, &msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            // One message per line: serde_json never emits raw newlines.
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Answers a request; notifications and responses get no reply.
fn handle(server: &mut tools::Server, msg: &Value) -> Option<Value> {
    let method = msg.get("method")?.as_str()?;
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = PROTOCOLS.iter().find(|&&p| p == asked).unwrap_or(&PROTOCOLS[1]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "binviz", "title": "binviz", "version": env!("CARGO_PKG_VERSION") },
                "instructions": tools::INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            // A bug in one tool must not take the session (and its loaded binaries) down.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| server.call(&name, &args)))
                .unwrap_or_else(|panic| {
                    let what = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown panic".into());
                    Err(format!("internal error in {name}: {what}"))
                });
            let (text, is_error) = match outcome {
                Ok(text) => (text, false),
                Err(text) => (text, true),
            };
            Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error(id, code, &message),
    })
}
