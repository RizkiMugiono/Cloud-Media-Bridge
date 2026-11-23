use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL: &str = "2025-11-25";
const MAX_FRAME: u64 = 1024 * 1024;
const PROVIDER_CATALOG: &str = include_str!("../../../config/providers.json");

fn doctor() -> String {
    format!(
        "Cloud-Media-Bridge {VERSION} on {} ({}). Local MCP stdio available.",
        env::consts::OS,
        env::consts::ARCH
    )
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}

fn model_id(input: &str) -> Result<String, String> {
    let trimmed = input.trim().trim_end_matches('/');
    let path = if let Some(rest) = trimmed.strip_prefix("https://huggingface.co/") {
        rest
    } else if trimmed.contains("://") {
        return Err("Only huggingface.co HTTPS model URLs are supported".into());
    } else {
        trimmed
    };
    let mut parts = path.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if parts.next().is_some() || !valid_segment(owner) || !valid_segment(name) {
        return Err("Expected owner/model or https://huggingface.co/owner/model".into());
    }
    Ok(format!("{owner}/{name}"))
}

fn percent_encode(input: &str) -> String {
    let mut result = String::new();
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            result.push(b as char);
        } else {
            result.push_str(&format!("%{b:02X}"));
        }
    }
    result
}

fn get_json(url: &str) -> Result<Value, String> {
    // URLs are constructed internally from fixed HTTPS origins, not taken from model metadata.
    if !url.starts_with("https://huggingface.co/api/models") {
        return Err("Unapproved outbound HTTP destination".into());
    }
    let output = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--max-redirs",
            "0",
            "--connect-timeout",
            "8",
            "--max-time",
            "20",
            "--max-filesize",
            "2097152",
            url,
        ])
        .output()
        .map_err(|_| "curl is required for Hugging Face queries".to_string())?;
    if !output.status.success() {
        return Err("Hugging Face request failed or access is restricted".into());
    }
    if output.stdout.len() > 2_097_152 {
        return Err("Hugging Face response exceeds the safe size limit".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "Invalid Hub JSON response".into())
}

fn search_models(query: &str, limit: usize) -> Result<Value, String> {
    if query.trim().is_empty() || query.len() > 200 || !(1..=50).contains(&limit) {
        return Err("Search requires 1-200 characters and a limit of 1-50".into());
    }
    let url = format!(
        "https://huggingface.co/api/models?search={}&limit={limit}",
        percent_encode(query.trim())
    );
    let models = get_json(&url)?;
    let entries = models
        .as_array()
        .ok_or("Hub search did not return a model array")?
        .iter()
        .map(|m| {
            json!({
                "id": m.get("id").or_else(|| m.get("modelId")),
                "sha": m.get("sha"),
                "task": m.get("pipeline_tag"),
                "library": m.get("library_name"),
                "gated": m.get("gated"),
                "private": m.get("private"),
                "last_modified": m.get("lastModified")
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({"source": "huggingface", "count": entries.len(), "models": entries,
              "discovery_is_not_execution_support": true}))
}

fn inspect_model(input: &str) -> Result<Value, String> {
    let id = model_id(input)?;
    let info = get_json(&format!("https://huggingface.co/api/models/{id}"))?;
    let sha = info["sha"]
        .as_str()
        .filter(|s| (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("Hub did not provide an immutable commit SHA")?;
    let siblings = info["siblings"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(json!({
        "source": "huggingface",
        "id": id,
        "url": format!("https://huggingface.co/{id}"),
        "revision_sha": sha,
        "task": info["pipeline_tag"],
        "library": info["library_name"],
        "gated": info["gated"],
        "private": info["private"],
        "license": info["cardData"]["license"],
        "files_count": siblings,
        "execution_status": "not_verified",
        "warning": "Model discovery does not prove that a compatible serving runtime exists."
    }))
}

fn providers_config() -> Result<Value, String> {
    let data = match env::var("CMB_PROVIDER_CONFIG") {
        Ok(path) => fs::read_to_string(path)
            .map_err(|_| "Cannot read CMB_PROVIDER_CONFIG file".to_string())?,
        Err(_) => PROVIDER_CATALOG.to_string(),
    };
    let config: Value =
        serde_json::from_str(&data).map_err(|_| "Invalid provider JSON configuration".to_string())?;
    if config["providers"].as_array().is_none() {
        return Err("Provider JSON requires a providers array".into());
    }
    Ok(config)
}

fn providers_list() -> Result<Value, String> {
    providers_config()
}

fn provider_info(id: &str) -> Result<Value, String> {
    if !valid_segment(id) {
        return Err("Invalid provider identifier".into());
    }
    providers_config()?["providers"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == id).cloned())
        .ok_or_else(|| format!("Unknown provider: {id}"))
}

fn plan_generation(model: &str, provider: &str) -> Result<Value, String> {
    let selected = provider_info(provider)?;
    let inspected = inspect_model(model)?;
    let task = inspected["task"].as_str().unwrap_or("unknown");
    let media_task = matches!(
        task,
        "text-to-image" | "image-to-image" | "text-to-video" | "image-to-video"
            | "text-to-audio" | "audio-to-audio" | "text-to-speech"
            | "automatic-speech-recognition" | "audio-classification"
            | "image-classification" | "image-to-text" | "object-detection"
            | "image-segmentation" | "depth-estimation" | "image-feature-extraction"
            | "text-to-3d" | "image-to-3d"
    );
    let verdict = if !media_task {
        "unsupported_or_unclassified_task"
    } else if selected["integration"] == "planned" {
        "provider_adapter_not_implemented"
    } else {
        "requires_compatible_serving_image_or_pipeline"
    };
    Ok(json!({
        "model": inspected,
        "provider": selected,
        "plan_status": verdict,
        "safe_to_deploy": false,
        "costing": "not_estimated",
        "user_selection_required": true,
        "user_resource_approval_required": true,
        "action": "read_only_plan",
        "note": "No GPU is provisioned; deployment requires a compatible, verified runtime and separate human approval."
    }))
}

fn execute_readonly(name: &str, arguments: &Value) -> Result<Value, String> {
    fn get_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
        arguments[key]
            .as_str()
            .ok_or_else(|| format!("Missing string argument: {key}"))
    }
    match name {
        "doctor" => Ok(json!({"message": doctor()})),
        "providers_list" => providers_list(),
        "providers_show" => provider_info(get_string(arguments, "provider")?),
        "models_search" => search_models(
            get_string(arguments, "query")?,
            arguments["limit"].as_u64().unwrap_or(10) as usize,
        ),
        "models_inspect" => inspect_model(get_string(arguments, "model")?),
        "plan_generation" => plan_generation(get_string(arguments, "model")?, get_string(arguments, "provider")?),
        _ => Err("Unknown MCP tool".into()),
    }
}

fn error(id: &Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

#[derive(PartialEq)]
enum Phase {
    New,
    Negotiated,
    Ready,
}

fn handle(frame: &[u8], phase: &mut Phase) -> Option<Value> {
    let request = match serde_json::from_slice::<Value>(frame) {
        Ok(value) => value,
        Err(_) => return Some(error(&Value::Null, -32700, "Parse error")),
    };
    let id = &request["id"];
    let method = request["method"].as_str();
    if request["jsonrpc"] != "2.0"
        || method.is_none()
        || (request.get("id").is_some() && !(id.is_string() || id.is_i64() || id.is_u64()))
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
    if request.get("params").is_some() && !params.is_object() {
        return Some(error(id, -32602, "Invalid parameters"));
    }
    let result = match method? {
        "ping" => json!({}),
        "initialize" if *phase == Phase::New => {
            if !params["protocolVersion"].is_string()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
                || !params["capabilities"].is_object()
            {
                return Some(error(id, -32602, "Invalid parameters"));
            }
            *phase = Phase::Negotiated;
            json!({"protocolVersion":PROTOCOL, "capabilities":{"tools":{}},
                   "serverInfo":{"name":"cloud-media-bridge","version":VERSION}})
        }
        "initialize" => return Some(error(id, -32600, "Already initialized")),
        _ if *phase != Phase::Ready => return Some(error(id, -32002, "Session not initialized")),
        "tools/list" => json!({"tools": tool_definitions()}),
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return Some(error(id, -32602, "Invalid parameters"));
            };
            let empty = json!({});
            let arguments = params.get("arguments").unwrap_or(&empty);
            if !arguments.is_object() || !tool_names().contains(&name) || !valid_tool_args(name, arguments) {
                return Some(error(id, -32602, "Invalid parameters"));
            }
            match execute_readonly(name, arguments) {
                Ok(value) => {
                    let text = if name == "doctor" { doctor() } else { value.to_string() };
                    json!({"content":[{"type":"text","text":text}], "structuredContent": value})
                },
                Err(message) => json!({"content":[{"type":"text","text":message}],"isError":true}),
            }
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0", "id":id, "result":result}))
}

fn valid_tool_args(name: &str, arguments: &Value) -> bool {
    let Some(object) = arguments.as_object() else { return false; };
    let has_only = |keys: &[&str]| object.keys().all(|key| keys.contains(&key.as_str()));
    match name {
        "doctor" | "providers_list" => object.is_empty(),
        "providers_show" => has_only(&["provider"]) && arguments["provider"].is_string(),
        "models_search" => {
            has_only(&["query", "limit"]) && arguments["query"].is_string()
                && arguments.get("limit").is_none_or(|v| {
                    v.as_u64().is_some_and(|n| (1..=50).contains(&n))
                })
        }
        "models_inspect" => has_only(&["model"]) && arguments["model"].is_string(),
        "plan_generation" => {
            has_only(&["model", "provider"]) && arguments["model"].is_string()
                && arguments["provider"].is_string()
        }
        _ => false,
    }
}

fn tool_names() -> &'static [&'static str] {
    &[
        "doctor",
        "providers_list",
        "providers_show",
        "models_search",
        "models_inspect",
        "plan_generation",
    ]
}

fn tool_definitions() -> Value {
    json!([
        {"name":"doctor","description":"Report local bridge version and operating system.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"providers_list","description":"List configured GPU provider kinds and implementation status.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"providers_show","description":"Inspect a provider's configuration and readiness.",
         "inputSchema":{"type":"object","properties":{"provider":{"type":"string"}},
                        "required":["provider"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"models_search","description":"Search the live Hugging Face Hub; model discovery is not an execution guarantee.",
         "inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":50}},
                        "required":["query"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"models_inspect","description":"Resolve an accessible Hugging Face model URL to metadata and immutable revision SHA.",
         "inputSchema":{"type":"object","properties":{"model":{"type":"string"}},
                        "required":["model"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"plan_generation","description":"Make a read-only plan for a model and a user-selected GPU provider; never deploy.",
         "inputSchema":{"type":"object","properties":{"model":{"type":"string"},"provider":{"type":"string"}},
                        "required":["model","provider"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}}
    ])
}

fn serve(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
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

fn print_json(value: Result<Value, String>) -> i32 {
    match value {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
            0
        }
        Err(message) => {
            eprintln!("Cloud-Media-Bridge: {message}");
            1
        }
    }
}

fn run_cli(args: &[String]) -> i32 {
    match args {
        [cmd] if cmd == "doctor" => {
            println!("{}", doctor());
            0
        }
        [cmd] if cmd == "--version" => {
            println!("{VERSION}");
            0
        }
        [cmd] if cmd == "providers" => print_json(providers_list()),
        [cmd, id] if cmd == "providers" => print_json(provider_info(id)),
        [cmd, action, query] if cmd == "models" && action == "search" => {
            print_json(search_models(query, 10))
        }
        [cmd, action, query, limit] if cmd == "models" && action == "search" => {
            print_json(
                limit
                    .parse::<usize>()
                    .map_err(|_| "Limit must be an integer".to_string())
                    .and_then(|n| search_models(query, n)),
            )
        }
        [cmd, action, model] if cmd == "models" && action == "inspect" => {
            print_json(inspect_model(model))
        }
        [cmd, model, provider] if cmd == "plan" => {
            print_json(plan_generation(model, provider))
        }
        [cmd] if cmd == "--help" => {
            println!("Usage:
  cloud-media-bridge doctor
  cloud-media-bridge mcp
  cloud-media-bridge providers [provider-id]
  cloud-media-bridge models search <query> [limit]
  cloud-media-bridge models inspect <owner/model|https://huggingface.co/owner/model>
  cloud-media-bridge plan <model> <provider-id>

This version performs only read-only model discovery and planning.
No deployment, inference or billing operation is implemented.");
            0
        }
        _ => {
            eprintln!("Use --help for supported commands");
            2
        }
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "mcp" {
        serve(io::stdin().lock(), io::stdout().lock())
    } else {
        std::process::exit(run_cli(&args));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_model_ids() {
        assert_eq!(
            model_id("https://huggingface.co/example/media-model").unwrap(),
            "example/media-model"
        );
        assert_eq!(model_id("example/media-model").unwrap(), "example/media-model");
        for value in [
            "http://huggingface.co/a/b",
            "https://evil.com/a/b",
            "https://huggingface.co/a/b/tree/main",
            "https://huggingface.co/a/../../token",
            "https://huggingface.co/a/%2F",
            "localhost/a/b",
            "a/b?x=1",
        ] {
            assert!(model_id(value).is_err(), "{value}");
        }
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(percent_encode("video & sound"), "video%20%26%20sound");
    }

    #[test]
    fn provider_catalog_is_parseable() {
        let p: Value = serde_json::from_str(PROVIDER_CATALOG).unwrap();
        assert!(p["providers"].as_array().unwrap().len() >= 7);
        assert_eq!(p["providers"][0]["id"], "modal");
        assert!(provider_info("not-a-provider").is_err());
    }

    #[test]
    fn unknown_mcp_tool_returns_error() {
        assert!(execute_readonly("no_such_tool", &json!({})).is_err());
    }
}
