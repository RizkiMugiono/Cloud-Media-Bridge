mod advice;
mod market;
mod billing;
mod lifecycle;
mod planning;
mod cloud_operator;
mod crypto;
mod media;
mod offline;
mod provider_api;
mod provider_http;
mod remote_worker;
mod provider_ops;
mod runtime;
mod worker_client;

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
        "billing_policy" => billing::policy(),
        "provider_sources" => market::sources(arguments["provider"].as_str()),
        "market_refresh" => market::refresh(get_string(arguments,"provider")?),
        "market_quote_check" => market::check(&arguments["quote"]),
        "cloud_review" => planning::review(&arguments["manifest"]),
        "lifecycle_advice" => lifecycle::advise(&arguments["plan"], &arguments["receipt"], &arguments["observed"]),
        "billing_estimate" => billing::estimate(&arguments["quote"]),
        "media_tasks_list" => media::catalog(),
        "runtimes_list" => runtime::registry(),
        "cloud_session_prepare" => cloud_operator::prepare(
            get_string(arguments,"directory")?,get_string(arguments,"manifest")?
        ),
        "cloud_session_status" => cloud_operator::status(get_string(arguments,"directory")?),
        "offline_fixture_demo" => offline::demo(
            get_string(arguments, "directory")?, get_string(arguments, "kind")?
        ),
        "offline_fixture_status" => offline::status(get_string(arguments, "directory")?),
        "offline_fixture_verify" => offline::verify(get_string(arguments, "directory")?),
        "offline_worker_smoke" => worker_client::smoke(
            get_string(arguments,"directory")?,
            arguments["port"].as_u64().ok_or("Port must be an integer")? as u16,
            get_string(arguments,"kind")?
        ),
        "provider_operation_preview" => provider_ops::preview(
            get_string(arguments,"provider")?,
            get_string(arguments,"image")?,
            get_string(arguments,"gpu")?,
            arguments["disk_gb"].as_u64().ok_or("Missing integer disk_gb")?,
            arguments["offer_id"].as_u64()
        ),
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
        "doctor" | "providers_list" | "deployment_policy" | "billing_policy" | "media_tasks_list" | "runtimes_list" => object.is_empty(),
        "provider_sources" => has_only(&["provider"]) && arguments.get("provider").is_none_or(Value::is_string),
        "market_refresh" => has_only(&["provider"]) && arguments["provider"].is_string(),
        "market_quote_check" => has_only(&["quote"]) && arguments["quote"].is_object(),
        "cloud_review" => has_only(&["manifest"]) && arguments["manifest"].is_object(),
        "lifecycle_advice" => has_only(&["plan","receipt","observed"]) && ["plan","receipt","observed"].iter().all(|k| arguments[k].is_object()),
        "billing_estimate" => has_only(&["quote"]) && arguments["quote"].is_object(),
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
        "offline_worker_smoke" => {
            has_only(&["directory","port","kind","confirm_fixture_only"])
                && arguments["confirm_fixture_only"] == true
                && arguments["directory"].as_str().is_some_and(|v| !v.is_empty() && v.len()<=1024)
                && arguments["port"].as_u64().is_some_and(|v| (1..=65535).contains(&v))
                && arguments["kind"].as_str().is_some_and(|v| offline::supported_kinds().contains(&v))
        }
        "provider_operation_preview" => {
            has_only(&["provider","image","gpu","disk_gb","offer_id"])
                && arguments["provider"].is_string()
                && arguments["image"].is_string()
                && arguments["gpu"].is_string()
                && arguments["disk_gb"].as_u64().is_some()
                && arguments.get("offer_id").is_none_or(|v|v.as_u64().is_some())
        }
        "cloud_session_prepare" => {
            has_only(&["directory","manifest","confirm_no_cloud_charge"])
                && arguments["confirm_no_cloud_charge"]==true
                && arguments["directory"].as_str().is_some_and(|v| !v.is_empty()&&v.len()<=1024)
                && arguments["manifest"].as_str().is_some_and(|v| !v.is_empty()&&v.len()<=1024)
        },
        "cloud_session_status" => {
            has_only(&["directory"])
                && arguments["directory"].as_str().is_some_and(|v| !v.is_empty()&&v.len()<=1024)
        },
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
        "billing_policy",
        "billing_estimate",
        "provider_sources",
        "market_refresh",
        "market_quote_check",
        "cloud_review",
        "lifecycle_advice",
        "media_tasks_list",
        "runtimes_list",
        "runtime_resolve",
        "offline_fixture_demo",
        "offline_fixture_status",
        "offline_fixture_verify",
        "provider_operation_preview",
        "offline_worker_smoke",
        "cloud_session_prepare",
        "cloud_session_status",
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
        {"name":"deployment_policy","description":"Read approved deployment design (not enforced): two runtime routes, parallel GPU adapter work, per-deployment consent, explicit user-selected lifecycle options and implementation status.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"billing_policy","description":"Read GPU billing granularity and independent user lifecycle choice; no cloud actions.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"lifecycle_advice","description":"Evaluate a prepared plan and caller-reported consent and observations. Returns advice only.",
         "inputSchema":{"type":"object","properties":{"plan":{"type":"object"},"receipt":{"type":"object"},"observed":{"type":"object"}},"required":["plan","receipt","observed"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"provider_sources","description":"Get official current pricing, docs and upstream release links. No prices are embedded; agent must consult live sources.",
         "inputSchema":{"type":"object","properties":{"provider":{"type":"string"}},"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"market_refresh","description":"Read a provider catalog using a fixed GET endpoint and locally configured credentials when supported. No GPU rental; otherwise return official sources for agent research.",
         "inputSchema":{"type":"object","properties":{"provider":{"type":"string"}},"required":["provider"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"openWorldHint":true}},
        {"name":"market_quote_check","description":"Check caller quote source and timestamp freshness. Never authenticates a quote or approves spending.",
         "inputSchema":{"type":"object","properties":{"quote":{"type":"object"}},"required":["quote"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"cloud_review","description":"Review a proposed GPU plan, compare quoted costs and explain stop choices. No approval, filesystem writes or cloud operations.",
         "inputSchema":{"type":"object","properties":{"manifest":{"type":"object"}},"required":["manifest"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true}},
        {"name":"billing_estimate","description":"Calculate an offline, caller-quoted usage-metered GPU compute estimate by actual billing increment/minimum, or mark monthly/yearly/prepaid terms not estimable. Does not guarantee invoice amounts.",
         "inputSchema":{"type":"object","properties":{"quote":{"type":"object"}},
             "required":["quote"],"additionalProperties":false},
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
        {"name":"provider_operation_preview","description":"Pure offline preview of pinned OCI GPU deployment and provider-specific safe stop/idle API request shapes. No credentials, API calls or cost.",
         "inputSchema":{"type":"object","properties":{
            "provider":{"type":"string"},"image":{"type":"string"},
            "gpu":{"type":"string"},"disk_gb":{"type":"integer","minimum":10,"maximum":2000},
            "offer_id":{"type":"integer","minimum":1}
           },"required":["provider","image","gpu","disk_gb"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"offline_worker_smoke","description":"Interoperate with a local loopback-only fixture HTTP worker and retrieve/verify mock media bytes. Requires CMB_WORKER_TOKEN in environment; cannot connect to external hosts or run real AI.",
         "inputSchema":{"type":"object","properties":{
            "directory":{"type":"string"},"port":{"type":"integer","minimum":1,"maximum":65535},
            "kind":{"type":"string","enum":["image","video","audio","speech","music","three_d","spatial","document","multimodal"]},
            "confirm_fixture_only":{"type":"boolean","const":true}},
            "required":["directory","port","kind","confirm_fixture_only"],"additionalProperties":false},
         "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}},
        {"name":"cloud_session_prepare","description":"Write a local, cost-free, immutable deployment plan journal from a manifest. No cloud API calls, no GPU, and no user approval. Requires explicit confirm_no_cloud_charge=true.",
         "inputSchema":{"type":"object","properties":{
             "directory":{"type":"string","minLength":1,"maxLength":1024},
             "manifest":{"type":"string","minLength":1,"maxLength":1024},
             "confirm_no_cloud_charge":{"type":"boolean","const":true}},
             "required":["directory","manifest","confirm_no_cloud_charge"],"additionalProperties":false},
         "annotations":{"readOnlyHint":false,"destructiveHint":false,"openWorldHint":false}},
        {"name":"cloud_session_status","description":"Read a previously prepared local deployment plan journal. Does not access GPU provider or approve spending.",
         "inputSchema":{"type":"object","properties":{"directory":{"type":"string"}},
             "required":["directory"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"openWorldHint":false}},
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
        [cmd, sub] if cmd == "market" && sub == "sources" => print_json(market::sources(None)),
        [cmd, sub, provider] if cmd == "market" && sub == "sources" => print_json(market::sources(Some(provider))),
        [cmd, sub, provider] if cmd == "market" && sub == "refresh" => print_json(market::refresh(provider)),
        [cmd, sub, file] if cmd == "market" && sub == "check" => print_json(planning::load_manifest(file, 16384).and_then(|v| market::check(&v))),
        [cmd] if cmd == "policy" => print_json(deployment_policy()),
        [cmd] if cmd == "billing" => print_json(billing::policy()),
        [cmd, sub, file] if cmd == "billing" && sub == "estimate" => {
            print_json(fs::read(file).map_err(|_|"Cannot read local billing quote".to_string())
                .and_then(|bytes| if bytes.len()>16*1024 {Err("Billing quote exceeds 16 KiB".to_string())} else {Ok(bytes)})
                .and_then(|bytes|serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_|"Invalid quote JSON".to_string()))
                .and_then(|quote|billing::estimate(&quote)))
        },
        [cmd] if cmd == "runtimes" => print_json(runtime::registry()),
        [cmd, sub, file] if cmd == "cloud" && sub == "review" => {
            print_json(planning::load_manifest(file, 16384).and_then(|v| planning::review(&v)))
        }
        [cmd, sub, file] if cmd == "cloud" && sub == "advice" => {
            print_json(planning::load_manifest(file, 65536).and_then(|v| lifecycle::advise(&v["plan"], &v["receipt"], &v["observed"])))
        }
        [cmd, sub, dir, manifest] if cmd == "cloud" && sub == "prepare" => {
            print_json(cloud_operator::prepare(dir,manifest))
        }
        [cmd, sub, dir] if cmd == "cloud" && sub == "status" => {
            print_json(cloud_operator::status(dir))
        }
        [cmd, sub, ..] if cmd == "cloud" && sub == "terminate" => {
            print_json(Err("Termination is not implemented; separate destructive authorization required".into()))
        }
        [cmd, sub, dir] if cmd == "cloud" && sub == "deploy" => {
            print_json(cloud_operator::deploy(dir))
        }
        [cmd, sub, dir] if cmd == "cloud" && sub == "stop" => {
            print_json(cloud_operator::stop(dir))
        }
        [cmd, sub, dir, inputs] if cmd == "cloud" && sub == "generate" => {
            print_json(cloud_operator::generate(dir,inputs))
        }
        [cmd, provider, image, gpu, disk] if cmd == "provider-preview" => {
            print_json(disk.parse::<u64>().map_err(|_|"Invalid disk_gb".to_string())
                .and_then(|gb| provider_ops::preview(provider,image,gpu,gb,None)))
        }
        [cmd, provider, image, gpu, disk, offer] if cmd == "provider-preview" => {
            print_json(disk.parse::<u64>().map_err(|_|"Invalid disk_gb".to_string())
                .and_then(|gb| offer.parse::<u64>().map_err(|_|"Invalid offer_id".to_string())
                    .and_then(|id|provider_ops::preview(provider,image,gpu,gb,Some(id)))))
        },
        [cmd, sub, dir, port, kind] if cmd == "offline" && sub == "worker-smoke" => {
            print_json(port.parse::<u16>().map_err(|_|"Invalid fixture worker port".to_string())
                .and_then(|p| worker_client::smoke(dir,p,kind)))
        }
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
  cloud-media-bridge provider-preview <provider> <image@sha256> <gpu> <disk_gb> [offer_id]
  cloud-media-bridge billing
  cloud-media-bridge billing estimate <quote.json>
  cloud-media-bridge market sources [provider]
  cloud-media-bridge market refresh <provider>
  cloud-media-bridge market check <quote.json>
  cloud-media-bridge cloud review <deployment-manifest.json> (read-only comparison)
  cloud-media-bridge cloud advice <plan-receipt-observation.json> (read-only)
  cloud-media-bridge cloud prepare <new-directory> <deployment-manifest.json>
  cloud-media-bridge cloud status <directory>
  cloud-media-bridge cloud deploy <directory>       (production release gate disabled)
  cloud-media-bridge cloud stop <directory>         (production release gate disabled; owned resources only)
  cloud-media-bridge cloud generate <directory> <inputs.json> (production release gate disabled; approved lifecycle only)
  cloud-media-bridge offline worker-smoke <output-directory> <loopback-port> <media-kind>
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
Real provider API adapters for RunPod Pods and Vast.ai Instances now exist behind a human-operated CLI and verified-runtime release gate. No bundled runtime has the necessary verified evidence yet, so live paid deployment remains blocked. Offline fixture generation is not AI inference.");
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
            "explicit_user_selected_policy_required"
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
