//! MCP stdio server: newline-delimited JSON-RPC 2.0.
//! All tools are read-only — safe for agents to auto-approve.

pub mod tools;

use anyhow::Result;
use prefixgraph_core::Store;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;

pub const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn serve_stdio(root: &Path) -> Result<()> {
    let store = Store::open_existing(root)?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_msg(
                    &mut out,
                    &json!({"jsonrpc": "2.0", "id": null,
                            "error": {"code": -32700, "message": format!("parse error: {e}")}}),
                )?;
                continue;
            }
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // Notifications (no id) get no response.
        if id.is_none() || id == Some(Value::Null) {
            continue;
        }
        let id = id.unwrap();

        let response = match method {
            "initialize" => {
                let requested = msg
                    .pointer("/params/protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or(PROTOCOL_VERSION);
                json!({"jsonrpc": "2.0", "id": id, "result": {
                    "protocolVersion": requested,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "prefixgraph", "version": env!("CARGO_PKG_VERSION")}
                }})
            }
            "ping" => json!({"jsonrpc": "2.0", "id": id, "result": {}}),
            "tools/list" => {
                json!({"jsonrpc": "2.0", "id": id, "result": {"tools": tools::definitions()}})
            }
            "tools/call" => {
                let name = msg
                    .pointer("/params/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = msg
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or(json!({}));
                match tools::dispatch(&store, name, &args) {
                    Ok(result) => {
                        let text = serde_json::to_string_pretty(&result)
                            .unwrap_or_else(|_| result.to_string());
                        json!({"jsonrpc": "2.0", "id": id, "result": {
                            "content": [{"type": "text", "text": text}],
                            "isError": false
                        }})
                    }
                    Err(e) => json!({"jsonrpc": "2.0", "id": id, "result": {
                        "content": [{"type": "text", "text": format!("error: {e}")}],
                        "isError": true
                    }}),
                }
            }
            other => json!({"jsonrpc": "2.0", "id": id,
                            "error": {"code": -32601, "message": format!("method not found: {other}")}}),
        };
        write_msg(&mut out, &response)?;
    }
    Ok(())
}

fn write_msg<W: Write>(out: &mut W, msg: &Value) -> Result<()> {
    // Newline-delimited JSON: the message itself must not contain raw newlines.
    let s = serde_json::to_string(msg)?;
    out.write_all(s.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}
