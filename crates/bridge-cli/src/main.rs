use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL: &str = "2025-11-25";
const USAGE: &str = "Usage: cloud-media-bridge <mcp|doctor|--version>";

fn doctor() -> String {
    format!(
        "Cloud-Media-Bridge {VERSION} on {} ({}). Local MCP stdio available.",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

#[derive(PartialEq)]
enum Phase {
    New,
    Negotiated,
    Ready,
}

fn error(id: &Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

fn handle(frame: &[u8], phase: &mut Phase) -> Option<Value> {
    let Ok(request) = serde_json::from_slice::<Value>(frame) else {
        return Some(error(&Value::Null, -32700, "Parse error"));
    };
    let id = &request["id"];
    let method = request["method"].as_str();
    if request["jsonrpc"] != "2.0"
        || method.is_none()
        || request.get("id").is_some() && !(id.is_string() || id.is_i64() || id.is_u64())
    {
        return Some(error(&Value::Null, -32600, "Invalid request"));
    }
    let params = &request["params"];
    if id.is_null() {
        if method == Some("notifications/initialized")
            && *phase == Phase::Negotiated
            && (request.get("params").is_none() || params.is_object())
        {
            *phase = Phase::Ready;
        }
        return None;
    }
    let invalid = || Some(error(id, -32602, "Invalid parameters"));
    if request.get("params").is_some() && !params.is_object() {
        return invalid();
    }
    let result = match method? {
        "ping" => json!({}),
        "initialize" if *phase == Phase::New => {
            if !params["protocolVersion"].is_string()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
                || !params["capabilities"].is_object()
            {
                return invalid();
            }
            *phase = Phase::Negotiated;
            json!({"protocolVersion":PROTOCOL,"capabilities":{"tools":{}},"serverInfo":{"name":"cloud-media-bridge","version":VERSION}})
        }
        "initialize" => return Some(error(id, -32600, "Already initialized")),
        _ if *phase != Phase::Ready => return Some(error(id, -32002, "Session not initialized")),
        "tools/list" => {
            if params.get("cursor").is_some() {
                return invalid();
            }
            json!({"tools":[{"name":"doctor","description":"Report local bridge version, OS and architecture.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true}}]})
        }
        "tools/call" => {
            if params["name"] != "doctor"
                || params
                    .get("arguments")
                    .is_some_and(|args| !args.as_object().is_some_and(|map| map.is_empty()))
            {
                return invalid();
            }
            json!({"content":[{"type":"text","text":doctor()}]})
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn serve(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    const MAX_FRAME: u64 = 1024 * 1024;
    let mut phase = Phase::New;
    let mut frame = Vec::new();
    loop {
        frame.clear();
        let bytes = input
            .by_ref()
            .take(MAX_FRAME + 1)
            .read_until(b'\n', &mut frame)?;
        if bytes == 0 {
            return Ok(());
        }
        if bytes as u64 > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "MCP frame exceeds 1 MiB",
            ));
        }
        if let Some(reply) = handle(&frame, &mut phase) {
            serde_json::to_writer(&mut output, &reply)?;
            writeln!(output)?;
            output.flush()?;
        }
    }
}

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "mcp" => serve(io::stdin().lock(), io::stdout().lock()),
        [cmd] if cmd == "doctor" => {
            println!("{}", doctor());
            Ok(())
        }
        [cmd] if cmd == "--version" => {
            println!("{VERSION}");
            Ok(())
        }
        [cmd] if cmd == "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2)
        }
    }
}
