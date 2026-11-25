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
    load_json(path, limit, "manifest")
}

pub(crate) fn load_json(path: &str, limit: usize, label: &str) -> Result<Value, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|_| format!("Cannot open {label}"))?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("Cannot read {label}"))?;
    if bytes.len() > limit {
        return Err(format!("Input exceeds {limit} bytes"));
    }
    serde_json::from_slice(&bytes).map_err(|_| format!("Invalid {label} JSON"))
}

fn valid_digest(value:&Value,allowed:&[usize])->bool{
    value.as_str().is_some_and(|s|allowed.contains(&s.len())&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)))
}
fn validate_recipe(recipe:&Value)->Result<(),String>{
    let object=recipe.as_object().ok_or("Runtime recipe must be an object")?;
    if object.len()!=4||object.keys().any(|k|!["upstream_url","revision_sha","recipe_sha256","base_image"].contains(&k.as_str()))
        ||!recipe["upstream_url"].as_str().is_some_and(crate::provider_profile::https)
        ||!valid_digest(&recipe["revision_sha"],&[40,64])||!valid_digest(&recipe["recipe_sha256"],&[64])
        ||!recipe["base_image"].as_str().is_some_and(crate::provider_ops::pinned_image){
        return Err("Recipe requires official HTTPS reference, immutable revision, full recipe SHA-256 and pinned base image; no inline executable code".into());
    }
    Ok(())
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
    let provider=raw["provider"].as_str().ok_or("Missing provider")?;
    let profile=&raw["provider_profile"];
    crate::provider_profile::resolve(provider,profile)?;
    let image=raw["image"].as_str();
    let recipe=&raw["runtime_recipe"];
    if !recipe.is_null(){validate_recipe(recipe)?;}
    if raw["image"].is_null()&&recipe.is_null(){return Err("Provide a pinned runtime image or reviewed upstream recipe".into());}
    if !raw["image"].is_null()&&!image.is_some_and(crate::provider_ops::pinned_image){return Err("Invalid pinned image".into());}
    let gpu=raw["gpu"].as_str().ok_or("Missing GPU")?;
    let disk=raw["disk_gb"].as_u64().ok_or("Missing disk allocation")?;
    if !crate::provider_ops::gpu_identity(gpu)||!(10..=2000).contains(&disk){
        return Err("Exact GPU and bounded disk allocation required".into());
    }
    if profile.is_null(){
        if let Some(image)=image{
            crate::provider_ops::preview(provider,image,gpu,disk,raw["offer_id"].as_u64())?;
        }else if provider=="vast-instances"&&!raw["offer_id"].as_u64().is_some_and(|n|n>0){
            return Err("Vast requires exact offer ID".into());
        }
        crate::handoff::validate_parameters(provider,&raw["provider_parameters"])?;
    }else{
        if !raw["provider_parameters"].is_null(){
            return Err("External provider parameters are constructed by the agent from official documentation, not embedded executable instructions".into());
        }
        if !profile["lifecycle_actions"].as_array().is_some_and(|v|v.contains(&raw["stop_policy"]["action"])){
            return Err("Selected lifecycle action not declared by external profile".into());
        }
    }
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
        "provider_profile",
        "runtime_release_sha256",
        "runtime_recipe",
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

