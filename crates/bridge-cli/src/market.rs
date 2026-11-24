//! Live read-only offer discovery; never create, reserve or mutate a resource.
//! Prices are not embedded. Missing or changed provider schemas fail closed.
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCES: &str = include_str!("../../../config/live-sources.json");
const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_ROWS: usize = 100;
const MAX_AGE_MS: u64 = 15 * 60 * 1000;

fn now_ms() -> Result<u64, String> {
    SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock precedes Unix epoch".to_string())?
        .as_millis().try_into().map_err(|_| "System clock overflow".into())
}

pub(crate) fn sources(provider: Option<&str>) -> Result<Value, String> {
    let data: Value = serde_json::from_str(SOURCES).map_err(|_| "Invalid live source catalog")?;
    match provider {
        None => Ok(data),
        Some(id) => data["providers"].get(id).cloned().ok_or_else(|| "Unknown provider source".into()),
    }
}

// Fixed GET-only endpoints; callers cannot supply a URL, credential name or method.
fn endpoint(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "runpod-pods" => Some(("https://api.runpod.io/v2/catalog/gpus?include=AVAILABILITY&product=POD", "RUNPOD_API_KEY")),
        "runpod-serverless" => Some(("https://api.runpod.io/v2/catalog/gpus?include=AVAILABILITY&product=SERVERLESS", "RUNPOD_API_KEY")),
        "vast-instances" => Some(("https://cloud.vast.ai/api/v0/bundles/?q=%7B%22limit%22%3A100%2C%22type%22%3A%22on-demand%22%2C%22rentable%22%3A%7B%22eq%22%3Atrue%7D%2C%22rented%22%3A%7B%22eq%22%3Afalse%7D%7D", "VAST_API_KEY")),
        "lambda-cloud" => Some(("https://cloud.lambda.com/api/v1/instance-types", "LAMBDA_API_KEY")),
        _ => None,
    }
}

fn token_config(token: &str) -> Result<String, String> {
    if !(8..=1024).contains(&token.len()) || !token.bytes().all(|b| b.is_ascii_graphic() && b != b'"' && b != b'\\') {
        return Err("Invalid local provider credential".into());
    }
    Ok(format!("header = \"Authorization: Bearer {token}\"\nheader = \"Accept: application/json\"\n"))
}

fn get(url: &str, token: &str) -> Result<Value, String> {
    let config = token_config(token)?;
    let mut child = Command::new("curl")
        .args(["--disable", "--silent", "--fail", "--proto", "=https", "--max-redirs", "0",
            "--connect-timeout", "10", "--max-time", "30", "--max-filesize", "2097152",
            "--request", "GET", "--config", "-", "--url", url])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|_| "curl unavailable")?;
    if let Some(mut input) = child.stdin.take() {
        input.write_all(config.as_bytes()).map_err(|_| "Cannot send catalog request")?;
    }
    let output = child.wait_with_output().map_err(|_| "Catalog request failed")?;
    if !output.status.success() || output.stdout.len() > MAX_BYTES {
        return Err("Catalog HTTP request failed or exceeded size limit".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "Catalog response is not supported JSON".into())
}

fn pick(row: &Value, keys: &[&str]) -> Value {
    let mut out = json!({});
    for key in keys {
        if let Some(v) = row.get(*key) { out[*key] = v.clone(); }
    }
    out
}

fn normalize(id: &str, body: &Value) -> Result<Vec<Value>, String> {
    if body.get("error").is_some() || body.get("errors").is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty())) {
        return Err("Provider reported an error".into());
    }
    let mut offers = Vec::new();
    match id {
        "lambda-cloud" => {
            let rows = body["data"].as_object().ok_or("Lambda catalog schema changed")?;
            for (name, row) in rows.iter().take(MAX_ROWS) {
                let instance = &row["instance_type"];
                let price = instance["price_cents_per_hour"].as_u64().and_then(|n| n.checked_mul(10000));
                let quantity = instance["specs"]["gpus"].as_u64();
                if price.is_none() || quantity.is_none_or(|n| n == 0) { continue; }
                let regions = row["regions_with_capacity_available"].as_array().ok_or("Lambda region schema changed")?;
                offers.push(json!({"offer_id":name,"gpu":instance["gpu_description"],"quantity":quantity,
                    "quoted_micro_usd_per_hour":price,"rate_basis":"whole_instance",
                    "available_regions":regions.iter().map(|v| pick(v,&["name","description"])).collect::<Vec<_>>(),
                    "specs":pick(&instance["specs"], &["gpus","vcpus","memory_gib","storage_gib"])}));
            }
        }
        "vast-instances" => {
            let rows = body["offers"].as_array().ok_or("Vast catalog schema changed")?;
            for row in rows.iter().take(MAX_ROWS) {
                if row["id"].as_u64().is_none() || row["dph_total"].as_f64().is_none_or(|n| !n.is_finite() || n < 0.0) { continue; }
                offers.push(pick(row,&["id","gpu_name","num_gpus","gpu_ram","gpu_total_ram","geolocation",
                    "dph_total","dph_base","storage_cost","storage_total_cost","inet_up_cost","inet_down_cost",
                    "rentable","rented","reliability","verification","cuda_max_good","duration"]));
            }
        }
        "runpod-pods" | "runpod-serverless" => {
            let rows = body["gpus"].as_array().or_else(|| body["data"].as_array()).or_else(|| body.as_array())
                .ok_or("RunPod catalog schema changed")?;
            for row in rows.iter().take(MAX_ROWS) {
                if row["id"].as_str().is_none() { continue; }
                let mut offer = pick(row,&["id","name","memory","secure","community","availability","pool","manufacturer","maxCount"]);
                // Provider price objects are product-specific; do not silently assume unit or quantity.
                offer["price"] = pick(&row["price"],&["secure","community","flex","active"]);
                if let Some(regions) = row["dataCenters"].as_array() {
                    offer["dataCenters"] = json!(regions.iter().map(|v|pick(v,&["id","region","availability"])).collect::<Vec<_>>());
                }
                offers.push(offer);
            }
        }
        _ => return Err("No structured catalog adapter for provider".into()),
    }
    Ok(offers)
}

pub(crate) fn refresh(id: &str) -> Result<Value, String> {
    let refs = sources(Some(id))?;
    let mut out = json!({"provider":id,"sources":refs,"approval_granted":false,
        "can_deploy":false,"gpu_rented":false,"quote_ready":false,
        "note":"Catalog data is not an executable quote. Verify billing terms, exact allocation, availability, units, account-specific pricing and all ancillary charges."});
    let Some((url, env_name)) = endpoint(id) else {
        if id == "modal" {
            out["status"] = json!("agent_run_read_only_modal_billing_cli_or_read_official_sources");
            out["agent_read_only_command"] = out["sources"]["read_only_cli"].clone();
            return Ok(out);
        }
        out["status"] = json!("read_official_sources_with_agent_or_user");
        return Ok(out);
    };
    let Ok(token) = std::env::var(env_name) else {
        out["status"] = json!("credential_unavailable_use_official_sources");
        out["credential_env"] = json!(env_name);
        return Ok(out);
    };
    out["endpoint"] = json!(url);
    match get(url, &token).and_then(|body| normalize(id, &body)) {
        Ok(offers) => {
            out["status"] = json!(if offers.is_empty() { "no_recognized_offers_recheck_source" } else { "live_catalog_observed" });
            out["retrieved_at_unix_ms"] = json!(now_ms()?);
            out["offers"] = json!(offers);
            out["max_rows"] = json!(MAX_ROWS);
            out["complete_inventory"] = json!(false);
        }
        Err(_) => { out["status"] = json!("catalog_unavailable_or_schema_changed_recheck_official_source"); }
    }
    Ok(out)
}

fn check_at(quote: &Value, now: u64) -> Result<Value, String> {
    let provider = quote["provider"].as_str().ok_or("Quote requires provider")?;
    let refs = sources(Some(provider))?;
    check_refs(quote,now,&refs,endpoint(provider).map(|(url,_)|url))
}
fn check_refs(quote:&Value,now:u64,refs:&Value,api:Option<&str>)->Result<Value,String>{
    let url = quote["pricing_source"].as_str().unwrap_or("");
    let recognized = refs["pricing"].as_array().is_some_and(|links| links.iter().any(|v| v.as_str() == Some(url)))
        || api.is_some_and(|u|u==url);
    let at = quote["retrieved_at_unix_ms"].as_u64();
    let age = at.and_then(|t| now.checked_sub(t));
    let expiry = quote["expires_at_unix_ms"].as_u64();
    let status = if !recognized {"source_requires_review"}
        else if age.is_none() {"missing_or_future_timestamp"}
        else if age.is_some_and(|v| v > MAX_AGE_MS) || expiry.is_some_and(|v| now >= v) {"refresh_required"}
        else {"freshness_window_met_caller_assertion_only"};
    Ok(json!({"status":status,"source_recognized":recognized,"age_ms":age,"max_age_ms":MAX_AGE_MS,
        "expires_at_unix_ms":expiry,"proof_of_price":false,"approval_granted":false,
        "note":"Timestamp and URL are caller assertions, not independent authentication or a price lock. Recheck exact quote before deployment."}))
}

pub(crate) fn check(quote: &Value) -> Result<Value, String> { check_at(quote, now_ms()?) }
pub(crate) fn check_profile(quote:&Value,profile:&Value)->Result<Value,String>{
    if profile.is_null(){return check(quote);}
    let provider=quote["provider"].as_str().ok_or("Quote requires provider")?;
    let refs=crate::provider_profile::sources(provider,profile)?;
    check_refs(quote,now_ms()?,&refs,None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sources_cover_all_nine_modes_without_embedded_rates() {
        let data = sources(None).unwrap();
        assert_eq!(data["providers"].as_object().unwrap().len(),9);
        for id in ["modal","runpod-pods","runpod-serverless","vast-instances","vast-serverless","lambda-cloud","generic-https","generic-ssh","kubernetes-gpu"] {
            assert!(sources(Some(id)).is_ok());
        }
        assert!(sources(Some("https://evil.invalid")).is_err());
        assert!(endpoint("modal").is_none());
        assert!(token_config("bad\nheader: test").is_err());
    }
    #[test]
    fn provider_catalogs_preserve_rate_basis_and_unknowns() {
        let l=normalize("lambda-cloud",&json!({"data":{"gpu_example":{"instance_type":{
            "gpu_description":"GPU","price_cents_per_hour":123,"specs":{"gpus":2}},
            "regions_with_capacity_available":[{"name":"us"}]}}})).unwrap();
        assert_eq!(l[0]["quoted_micro_usd_per_hour"],1230000);
        assert_eq!(l[0]["quantity"],2);
        assert_eq!(l[0]["rate_basis"],"whole_instance");
        let v=normalize("vast-instances",&json!({"offers":[{"id":1,"dph_total":0.5,"gpu_name":"GPU","secret":"omit"}]})).unwrap();
        assert_eq!(v[0]["dph_total"],0.5);
        assert!(v[0].get("secret").is_none());
        let r=normalize("runpod-pods",&json!({"gpus":[{"id":"gpu","price":{"secure":0.7},"memory":24}]})).unwrap();
        assert_eq!(r[0]["price"]["secure"],0.7);
        assert!(r[0].get("quoted_micro_usd_per_hour").is_none());
        assert!(normalize("lambda-cloud",&json!({"changed":true})).is_err());
    }
    #[test]
    fn old_future_and_unrecognized_quotes_are_not_fresh() {
        let mut q=json!({"provider":"modal","pricing_source":"https://modal.com/pricing","retrieved_at_unix_ms":1000});
        assert_eq!(check_at(&q,2000).unwrap()["status"],"freshness_window_met_caller_assertion_only");
        assert_eq!(check_at(&q,MAX_AGE_MS+1001).unwrap()["status"],"refresh_required");
        assert_eq!(check_at(&q,999).unwrap()["status"],"missing_or_future_timestamp");
        q["expires_at_unix_ms"]=json!(1500);
        assert_eq!(check_at(&q,2000).unwrap()["status"],"refresh_required");
        q["pricing_source"]=json!("https://unknown.invalid");
        assert_eq!(check_at(&q,2000).unwrap()["status"],"source_requires_review");
    }
}
