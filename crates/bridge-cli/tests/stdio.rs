use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

const BINARY: &str = env!("CARGO_BIN_EXE_cloud-media-bridge");
const INIT: &str = r#"{"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"offline-test","version":"1"}}}"#;
const READY: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const LIST: &str = r#"{"jsonrpc":"2.0","id":9,"method":"tools/list"}"#;

fn exchange(lines: &[&str]) -> Vec<Value> {
    let mut child = Command::new(BINARY)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for line in lines {
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let reply: Value = serde_json::from_str(line).unwrap();
            assert_eq!(reply["jsonrpc"], "2.0");
            reply
        })
        .collect()
}

#[test]
fn h01_list_and_call_local_doctor() {
    let replies = exchange(&[
        INIT,
        READY,
        LIST,
        r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"doctor","arguments":{}}}"#,
    ]);
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["id"], "init");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(replies[0]["result"]["capabilities"]["tools"], json!({}));
    assert_eq!(replies[1]["id"], 9);
    let tools = replies[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    assert_eq!(tools[0]["name"], "doctor");
    assert_eq!(tools[0]["inputSchema"]["type"], "object");
    assert_eq!(replies[2]["id"], 10);
    let doctor = Command::new(BINARY).arg("doctor").output().unwrap();
    assert!(doctor.status.success());
    let text = String::from_utf8(doctor.stdout).unwrap();
    assert!(text.contains(std::env::consts::OS));
    assert_eq!(replies[2]["result"]["content"][0]["text"], text.trim_end());
}

#[test]
fn h02_invalid_requests_leave_session_usable() {
    let replies = exchange(&[
        "{",
        LIST,
        INIT,
        LIST,
        READY,
        r#"{"jsonrpc":"2.0","id":4,"method":"unknown"}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"doctor","arguments":{"approved":true}}}"#,
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"generate"}}"#,
        LIST,
    ]);
    assert_eq!(replies.len(), 8);
    for (index, code) in [
        (0, -32700),
        (1, -32002),
        (3, -32002),
        (4, -32601),
        (5, -32602),
        (6, -32602),
    ] {
        assert_eq!(replies[index]["error"]["code"], code);
    }
    assert_eq!(replies[7]["result"]["tools"][0]["name"], "doctor");
    assert!(replies[7]["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "models_search"));
}

#[test]
fn provider_listing_is_read_only_and_does_not_need_credentials() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"providers_list","arguments":{}}}"#,
    ]);
    let data = &replies[1]["result"]["structuredContent"];
    let providers = data["providers"].as_array().unwrap();
    assert!(providers.len() >= 7);
    assert!(providers.iter().any(|provider| provider["id"] == "modal"));
    assert!(providers.iter().any(|provider| provider["id"] == "runpod-serverless"));
    assert!(providers.iter().any(|provider| provider["id"] == "vast-instances"));
}

#[test]
fn invalid_model_url_and_missing_plan_args_fail_safely() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":"models_inspect","arguments":{"model":"http://169.254.169.254/secret"}}}"#,
        r#"{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"plan_generation","arguments":{"model":"author/model"}}}"#,
        r#"{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{"name":"models_search","arguments":{"query":"test","limit":9999}}}"#,
    ]);
    assert_eq!(replies[1]["result"]["isError"], true);
    assert_eq!(replies[2]["error"]["code"], -32602);
    assert_eq!(replies[3]["error"]["code"], -32602);
}
