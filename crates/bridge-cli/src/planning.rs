//! Read-only plan validation; no authorization or cloud actions.
use crate::crypto::sha256;
use serde_json::{json, Value};
use std::fs;

fn safe_scope(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
}

pub(crate) fn load_manifest(path: &str, limit: usize) -> Result<Value, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|_| "Cannot open manifest")?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read manifest")?;
    if bytes.len() > limit {
        return Err(format!("Input exceeds {limit} bytes"));
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid manifest JSON".into())
}

pub(crate) fn review(raw: &Value) -> Result<Value, String> {
    if raw["schema_version"] != 1 || raw.to_string().len() > 16384 {
        return Err("Expected a version 1 manifest under 16 KiB".into());
    }
    for key in [
        "account",
        "workspace",
        "runtime_entry_id",
        "runtime_family",
        "task",
    ] {
        if !raw[key].as_str().is_some_and(safe_scope) {
            return Err(format!("Invalid {key}"));
        }
    }
    let model = raw["model_id"].as_str().ok_or("Missing model ID")?;
    if crate::model_id(model)?.as_str() != model {
        return Err("Expected exact model ID".into());
    }
    let sha = raw["model_sha"].as_str().ok_or("Missing model SHA")?;
    if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Expected immutable model SHA".into());
    }
    crate::provider_ops::preview(
        raw["provider"].as_str().ok_or("Missing provider")?,
        raw["image"].as_str().ok_or("Missing pinned image")?,
        raw["gpu"].as_str().ok_or("Missing GPU")?,
        raw["disk_gb"].as_u64().ok_or("Missing disk allocation")?,
        raw["offer_id"].as_u64(),
    )?;
    crate::handoff::validate_parameters(raw["provider"].as_str().ok_or("Missing provider")?, &raw["provider_parameters"])?;
    if let Some(v)=raw.get("runtime_release_sha256").filter(|v|!v.is_null()) {
        if !v.as_str().is_some_and(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))){return Err("Invalid runtime release SHA-256".into());}
    }
    let fields = [
        "schema_version",
        "account",
        "workspace",
        "provider",
        "model_id",
        "model_sha",
        "task",
        "runtime_family",
        "runtime_entry_id",
        "image",
        "gpu",
        "disk_gb",
        "offer_id",
        "region",
        "quantity",
        "billing_quote",
        "alternatives",
        "stop_policy",
        "max_elapsed_ms",
        "max_total_micro_usd",
        "alternatives_unavailable_reason",
        "compatibility_reason",
        "provider_parameters",
        "runtime_release_sha256",
    ];
    for key in raw.as_object().ok_or("Expected manifest object")?.keys() {
        if key != "request_id" && key != "nonce" && !fields.contains(&key.as_str()) {
            return Err(format!("Unrecognized plan field {key}"));
        }
    }
    let mut plan = json!({});
    for key in fields {
        plan[key] = raw[key].clone();
    }
    for key in ["request_id", "nonce"] {
        if let Some(id) = raw.get(key) {
            if !id.as_str().is_some_and(safe_scope) {
                return Err(format!("Invalid {key}"));
            }
            plan[key] = id.clone();
        }
    }
    let advice = crate::advice::evaluate(&plan)?;
    Ok(
        json!({"plan":plan,"plan_sha256":sha256(plan.to_string().as_bytes()),
        "advice":advice,"bridge_can_execute":false,"approval_granted":false}),
    )
}

