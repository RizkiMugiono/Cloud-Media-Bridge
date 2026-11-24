mod crypto;
mod media;
mod offline;
mod runtime;

use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL: &str = "2025-11-25";
const MAX_FRAME: u64 = 1024 * 1024;
const PROVIDER_CATALOG: &str = include_str!("../../../config/providers.json");
const DEPLOYMENT_POLICY: &str = include_str!("../../../config/deployment-policy.json");

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

fn search_models(query: Option<&str>, task: Option<&str>, limit: usize) -> Result<Value, String> {
    let search = query.unwrap_or("").trim();
    let filter = task.unwrap_or("").trim();
    if (search.is_empty() && filter.is_empty())
        || search.len() > 200
        || filter.len() > 96
        || !filter.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !(1..=50).contains(&limit)
    {
        return Err("Search requires query or pipeline task, safe task characters and a limit of 1-50".into());
    }
    let mut url = format!("https://huggingface.co/api/models?limit={limit}");
    if !search.is_empty() {
        url.push_str(&format!("&search={}", percent_encode(search)));
    }
    if !filter.is_empty() {
        url.push_str(&format!("&pipeline_tag={}", percent_encode(filter)));
    }
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
                "last_modified": m.get("lastModified"),
                "media": media::classify(m["pipeline_tag"].as_str(), m.get("tags"))
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "source": "huggingface",
        "count": entries.len(),
        "models": entries,
        "query": if search.is_empty() { Value::Null } else { json!(search) },
        "pipeline_tag_filter": if filter.is_empty() { Value::Null } else { json!(filter) },
        "discovery_is_not_execution_support": true
    }))
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
    let file_names = info["siblings"].as_array().into_iter().flatten().take(200)
        .filter_map(|entry| entry["rfilename"].as_str())
        .filter(|name| name.len() <= 256)
        .collect::<Vec<_>>();
    let architectures = info["config"]["architectures"].as_array().into_iter().flatten()
        .take(16).filter_map(Value::as_str).filter(|name| name.len() <= 128)
        .collect::<Vec<_>>();
    let tags = info["tags"].as_array().into_iter().flatten().take(100)
        .filter_map(Value::as_str).filter(|tag| tag.len() <= 128)
        .collect::<Vec<_>>();
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
        "runtime_hints": {
            "architectures": architectures,
            "model_type": info["config"]["model_type"],
            "file_names": file_names,
            "tags": tags,
            "remote_code_mapping_present": !info["config"]["auto_map"].is_null(),
            "warning": "Upstream model metadata is untrusted; runtime hints do not grant executable compatibility."
        },
        "media": media::classify(info["pipeline_tag"].as_str(), info.get("tags")),
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

// This contract describes future behavior. No deployment/stop operation is implemented.
fn deployment_policy() -> Result<Value, String> {
    let policy: Value = serde_json::from_str(DEPLOYMENT_POLICY)
        .map_err(|_| "Invalid bundled deployment policy".to_string())?;
    if policy["implementation_status"] != "documented_contract_not_enforced" {
        return Err("Deployment policy must not imply enforcement without code".into());
    }
    Ok(policy)
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
    let classification = inspected["media"]["classification_status"].as_str().unwrap_or("unclassified_requires_review");
    let resolved = runtime::resolve(&inspected, &selected)?;
    let verdict = match classification {
        "non_media_task_out_of_scope" => "non_media_task_out_of_scope",
        "unclassified_requires_review" | "catalog_unavailable" => "unknown_media_task_requires_review",
        "inferred_media_task_requires_review" => "new_media_task_requires_verified_runtime",
        _ if selected["capabilities"]["execute"] == false => "provider_execution_not_implemented",
        _ => "verified_runtime_required",
    };
    Ok(json!({
        "model": inspected,
        "provider": selected,
        "media_classification": inspected["media"].clone(),
        "runtime_resolution": resolved,
        "plan_status": verdict,
        "safe_to_deploy": false,
        "costing": "not_estimated",
        "user_selection_required": true,
        "user_resource_approval_required": true,
        "deployment_approval_scope": "new_human_approval_for_every_deployment_request",
        "runtime_options": ["verified_prebuilt_oci_image", "vetted_runtime_recipe_on_base_image"],
        "auto_stop_implementation": "not_implemented",
        "action": "read_only_plan",
        "note": "No GPU is provisioned; deployment requires a compatible, verified runtime and separate human approval."
    }))
}

fn execute_tool(name: &str, arguments: &Value) -> Result<Value, String> {
    fn get_string<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
        arguments[key]
            .as_str()
            .ok_or_else(|| format!("Missing string argument: {key}"))
    }
    match name {
        "doctor" => Ok(json!({"message": doctor()})),
        "providers_list" => providers_list(),
        "deployment_policy" => deployment_policy(),
        "media_tasks_list" => media::catalog(),
        "runtimes_list" => runtime::registry(),
        "offline_fixture_demo" => offline::demo(
            get_string(arguments, "directory")?, get_string(arguments, "kind")?
        ),
        "offline_fixture_status" => offline::status(get_string(arguments, "directory")?),
        "offline_fixture_verify" => offline::verify(get_string(arguments, "directory")?),
        "runtime_resolve" => {
            let inspected = inspect_model(get_string(arguments, "model")?)?;
            let provider = provider_info(get_string(arguments, "provider")?)?;
            runtime::resolve(&inspected, &provider)
        },
        "providers_show" => provider_info(get_string(arguments, "provider")?),
        "models_search" => search_models(
            arguments["query"].as_str(),
            arguments["task"].as_str(),
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
            match execute_tool(name, arguments) {
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
        "doctor" | "providers_list" | "deployment_policy" | "media_tasks_list" | "runtimes_list" => object.is_empty(),
        "providers_show" => has_only(&["provider"]) && arguments["provider"].is_string(),
        "models_search" => {
            has_only(&["query", "task", "limit"])
                && arguments.get("query").is_none_or(Value::is_string)
                && arguments.get("task").is_none_or(Value::is_string)
                && (arguments["query"].as_str().is_some_and(|v| !v.trim().is_empty())
                    || arguments["task"].as_str().is_some_and(|v| !v.trim().is_empty()))
                && arguments.get("limit").is_none_or(|v| {
                    v.as_u64().is_some_and(|n| (1..=50).contains(&n))
                })
        }
        "models_inspect" => has_only(&["model"]) && arguments["model"].is_string(),
        "offline_fixture_demo" => {
            has_only(&["directory","kind","confirm_fixture_only"])
                && arguments["confirm_fixture_only"] == true
                && arguments["directory"].as_str().is_some_and(|v| !v.is_empty() && v.len() <= 1024)
                && arguments["kind"].as_str().is_some_and(|v| offline::supported_kinds().contains(&v))
        }
        "offline_fixture_status" | "offline_fixture_verify" => {
            has_only(&["directory"])
                && arguments["directory"].as_str().is_some_and(|v| !v.is_empty() && v.len() <= 1024)
        },
        "plan_generation" | "runtime_resolve" => {
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
        "deployment_policy",
        "media_tasks_list",
        "runtimes_list",
        "runtime_resolve",
        "offline_fixture_demo",
        "offline_fixture_status",
        "offline_fixture_verify",
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
        {"name":"deployment_policy","description":"Read approved deployment design (not enforced): two runtime routes, parallel GPU adapter work, per-deployment consent, owned resource auto-stop intent.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"media_tasks_list","description":"List categorized image, video, audio, voice, music, 3D, document, spatial and multimodal task hints. Discovery does not imply compatibility.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"runtimes_list","description":"List versioned upstream runtime families and unverified OCI or installer recipe candidates. Does not deploy.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"runtime_resolve","description":"Inspect an exact Hugging Face media model, compare it to trusted upstream runtime candidate metadata and a GPU provider. Read-only; execution unavailable.",
         "inputSchema":{"type":"object","properties":{"model":{"type":"string"},"provider":{"type":"string"}},
                        "required":["model","provider"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"offline_fixture_demo","description":"Create a fully offline simulated deployment and fixture-media artifact in a new user-specified local directory, then verify mock resource auto-stop. NOT real AI, model inference, cloud or GPU.",
         "inputSchema":{"type":"object","properties":{
             "directory":{"type":"string","minLength":1,"maxLength":1024},
             "kind":{"type":"string","enum":["image","video","audio","speech","music","three_d","spatial","document","multimodal"]},
             "confirm_fixture_only":{"type":"boolean","const":true}
           },"required":["directory","kind","confirm_fixture_only"],"additionalProperties":false},
         "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}},
        {"name":"offline_fixture_status","description":"Read an existing offline fixture deployment journal and simulated provider observation; no cloud activity.",
         "inputSchema":{"type":"object","properties":{"directory":{"type":"string"}},"required":["directory"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"offline_fixture_verify","description":"Verify a local fixture artifact's size and SHA-256; never claim real AI inference.",
         "inputSchema":{"type":"object","properties":{"directory":{"type":"string"}},"required":["directory"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"models_search","description":"Search all accessible Hugging Face models or filter by any Hub pipeline task; tags are classified without asserting inference compatibility.",
         "inputSchema":{"type":"object","properties":{"query":{"type":"string"},"task":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":50}},
                        "additionalProperties":false},
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
        [cmd] if cmd == "policy" => print_json(deployment_policy()),
        [cmd] if cmd == "runtimes" => print_json(runtime::registry()),
        [cmd, sub, dir, kind] if cmd == "offline" && sub == "prepare" => {
            print_json(offline::prepare(dir, kind))
        }
        [cmd, sub, dir, kind] if cmd == "offline" && sub == "demo" => {
            print_json(offline::demo(dir, kind))
        }
        [cmd, sub, dir] if cmd == "offline" && sub == "approve" => {
            print_json(offline::approve_interactively(dir))
        }
        [cmd, sub, dir] if cmd == "offline" && sub == "run" => print_json(offline::run(dir)),
        [cmd, sub, dir] if cmd == "offline" && sub == "reconcile" => print_json(offline::reconcile(dir)),
        [cmd, sub, dir] if cmd == "offline" && sub == "verify" => print_json(offline::verify(dir)),
        [cmd, sub, dir] if cmd == "offline" && sub == "status" => print_json(offline::status(dir)),
        [cmd, action, model, provider] if cmd == "runtimes" && action == "resolve" => {
            print_json(inspect_model(model).and_then(|inspected| {
                provider_info(provider).and_then(|selected| runtime::resolve(&inspected, &selected))
            }))
        },
        [cmd, id] if cmd == "providers" => print_json(provider_info(id)),
        [cmd, action] if cmd == "models" && action == "tasks" => print_json(media::catalog()),
        [cmd, action, query] if cmd == "models" && action == "search" => {
            print_json(search_models(Some(query), None, 10))
        }
        [cmd, action, task] if cmd == "models" && action == "search-task" => {
            print_json(search_models(None, Some(task), 10))
        }
        [cmd, action, query, limit] if cmd == "models" && action == "search" => {
            print_json(
                limit
                    .parse::<usize>()
                    .map_err(|_| "Limit must be an integer".to_string())
                    .and_then(|n| search_models(Some(query), None, n)),
            )
        }
        [cmd, action, task, limit] if cmd == "models" && action == "search-task" => {
            print_json(
                limit
                    .parse::<usize>()
                    .map_err(|_| "Limit must be an integer".to_string())
                    .and_then(|n| search_models(None, Some(task), n)),
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
  cloud-media-bridge policy
  cloud-media-bridge runtimes
  cloud-media-bridge runtimes resolve <model> <provider-id>
  cloud-media-bridge offline demo <new-directory> <media-kind>
  cloud-media-bridge offline prepare <new-directory> <media-kind>
  cloud-media-bridge offline approve <directory>      (human TTY required)
  cloud-media-bridge offline run <directory>
  cloud-media-bridge offline status <directory>
  cloud-media-bridge offline verify <directory>
  cloud-media-bridge offline reconcile <directory>
  cloud-media-bridge models tasks
  cloud-media-bridge models search <query> [limit]
  cloud-media-bridge models search-task <pipeline_tag> [limit]
  cloud-media-bridge models inspect <owner/model|https://huggingface.co/owner/model>
  cloud-media-bridge plan <model> <provider-id>

This version supports read-only model discovery/planning and an offline
deployment simulation with synthetic fixture-media artifacts and mock auto-stop.
No real provider deployment, AI model inference, or GPU billing is implemented.");
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
    fn deployment_policy_has_exact_user_decisions() {
        let policy = deployment_policy().unwrap();
        assert_eq!(
            policy["runtime_installation"]["allowed_routes"].as_array().unwrap().len(),
            2
        );
        assert_eq!(policy["provider_development"]["strategy"], "parallel_workstreams");
        assert_eq!(policy["provider_development"]["targets"].as_array().unwrap().len(), 9);
        assert_eq!(
            policy["approval"]["deployment"],
            "new_human_approval_for_every_deployment_request"
        );
        assert_eq!(policy["approval"]["grants_reusable_for_deployment"], false);
        assert_eq!(policy["resource_lifecycle"]["pod_vm_instance"]["auto_destroy"], false);
        assert_eq!(policy["resource_lifecycle"]["pod_vm_instance"]["auto_terminate"], false);
        assert_eq!(
            policy["resource_lifecycle"]["pod_vm_instance"]["after_last_job"],
            "automatically_stop_bridge_owned_resource_if_provider_supports_stop"
        );
        assert_eq!(policy["execution"]["current_capability"], "read_only");
    }

    #[test]
    fn provider_catalog_matches_parallel_workstreams_without_fake_execution() {
        let catalog: Value = serde_json::from_str(PROVIDER_CATALOG).unwrap();
        let policy = deployment_policy().unwrap();
        let providers = catalog["providers"].as_array().unwrap();
        let targets = policy["provider_development"]["targets"].as_array().unwrap();
        assert_eq!(providers.len(), targets.len());
        for provider in providers {
            assert!(targets.contains(&provider["id"]));
            assert_eq!(provider["development_track"], "parallel");
            assert_eq!(provider["capabilities"]["execute"], false);
            assert_eq!(provider["capabilities"]["deploy"], false);
            assert_eq!(provider["lifecycle_implementation"], "not_implemented");
            assert_eq!(
                provider["deployment_approval"],
                "fresh_human_consent_for_every_deployment"
            );
        }
    }

    #[test]
    fn unknown_mcp_tool_returns_error() {
        assert!(execute_tool("no_such_tool", &json!({})).is_err());
    }
}
