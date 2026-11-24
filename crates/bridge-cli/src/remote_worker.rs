//! Operator-only authenticated HTTPS media worker client.
//! Does not accept an arbitrary URL from MCP arguments. The worker origin
//! must be explicitly configured as CMB_WORKER_HTTPS_ORIGIN by the user.
use crate::crypto::sha256;
use serde_json::{json,Value};
use std::env;
use std::fs::{self,OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command,Stdio};

const MAX_RESULT: usize=32*1024*1024;
fn valid_origin(value:&str)->bool{
    let Some(host)=value.strip_prefix("https://") else {return false};
    if host.is_empty()||host.len()>240||host.contains(['@','/','?','#','\\','%','\r','\n'])
        || host.starts_with('-')||host.ends_with('-')
        || host.parse::<std::net::IpAddr>().is_ok()
    {return false;}
    let without_port=host.strip_suffix(":443").unwrap_or(host);
    if without_port.contains(':') {return false;}
    let labels=without_port.split('.').collect::<Vec<_>>();
    labels.len()>=2 && labels.iter().all(|part|!part.is_empty() && part.len()<=63
        && !part.starts_with('-') && !part.ends_with('-')
        && part.bytes().all(|c| c.is_ascii_alphanumeric()||c==b'-'))
}
fn valid_token(token:&str)->bool{
    (24..=256).contains(&token.len()) && token.bytes().all(|c|c.is_ascii_graphic()&&c!=b'"'&&c!=b'\\')
}
fn cfg_escape(v:&str)->String{v.replace('\\',"\\\\").replace('"',"\\\"")}
fn request(origin:&str,token:&str,method:&str,path:&str,body:Option<&[u8]>)->Result<Vec<u8>,String>{
    if !valid_origin(origin) || !valid_token(token)
        || !matches!(method,"GET"|"POST") || !path.starts_with('/')
        || path.contains(['?','#','\r','\n']) || path.contains("..")
    {return Err("Unsafe worker HTTPS origin, request or token".into());}
    let mut config=format!("header = \"Authorization: Bearer {}\"\n",cfg_escape(token));
    if let Some(bytes)=body {
        if bytes.len()>1_048_576 {return Err("Media worker JSON request exceeds 1 MiB".into());}
        let payload=std::str::from_utf8(bytes).map_err(|_|"Media JSON is not UTF-8".to_string())?;
        if payload.chars().any(|c|c.is_control()) {return Err("Control character in worker JSON".into());}
        config.push_str("header = \"Content-Type: application/json\"\n");
        config.push_str(&format!("data = \"{}\"\n",cfg_escape(payload)));
    }
    let url=format!("{origin}{path}");
    let mut child=Command::new("curl")
        .args(["--disable","--silent","--show-error","--fail","--proto","=https",
            "--max-redirs","0","--connect-timeout","10","--max-time","120",
            "--max-filesize","33554432","--request",method,
            "--config","-","--url",&url])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(|_|"curl required for authenticated media worker".to_string())?;
    if let Some(mut input)=child.stdin.take() {
        input.write_all(config.as_bytes()).map_err(|_|"Worker request could not be sent".to_string())?;
    }
    let output=child.wait_with_output().map_err(|_|"Worker request failed".to_string())?;
    if !output.status.success(){return Err("Worker returned HTTP/transport failure; job status is uncertain".into());}
    if output.stdout.len()>MAX_RESULT{return Err("Worker response exceeds artifact size limit".into());}
    Ok(output.stdout)
}
fn artifact_id(v:&str)->bool{
    v.len()==32&&v.bytes().all(|b|b.is_ascii_hexdigit())
}
fn artifact_ext(kind:&str,mime:&str)->Option<&'static str>{
    match (kind,mime){
        ("image","image/png")=>Some("png"),
        ("image","image/jpeg")=>Some("jpg"),
        ("image","image/webp")=>Some("webp"),
        ("video","video/mp4")=>Some("mp4"),
        ("audio"|"music"|"speech","audio/wav")=>Some("wav"),
        ("audio"|"music"|"speech","audio/mpeg")=>Some("mp3"),
        ("text"|"document"|"multimodal"|"spatial","application/json")=>Some("json"),
        ("three_d","model/obj")=>Some("obj"),
        _=>None
    }
}
pub(crate) fn validate_worker_result(body:&Value,request:&Value)->Result<Vec<Value>,String>{
    if body["simulation_only"]!=false||body["status"]!="completed"
        || body["request_id"]!=request["request_id"] || body["schema_version"]!=1
    {return Err("Worker returned mismatched, incomplete or simulated result".into());}
    let artifacts=body["artifacts"].as_array().ok_or("Missing worker artifact list")?;
    if artifacts.is_empty()||artifacts.len()>16 {return Err("Invalid worker artifact count".into());}
    let mut size=0u64;
    for artifact in artifacts {
        let id=artifact["asset_ref"].as_str().ok_or("Missing artifact id")?;
        let kind=artifact["kind"].as_str().ok_or("Missing artifact kind")?;
        let mime=artifact["media_type"].as_str().ok_or("Missing artifact MIME")?;
        let sha=artifact["sha256"].as_str().ok_or("Missing artifact digest")?;
        let n=artifact["size_bytes"].as_u64().ok_or("Missing artifact length")?;
        if !artifact_id(id)
            || artifact_ext(kind,mime).is_none()
            || sha.len()!=64||!sha.bytes().all(|c|c.is_ascii_hexdigit())
            || n==0||n>MAX_RESULT as u64
        {return Err("Untrusted worker artifact descriptor".into());}
        size+=n;
        if size>MAX_RESULT as u64{return Err("Total result exceeds size policy".into());}
    }
    Ok(artifacts.clone())
}

fn job_id_valid(id:&str)->bool{
    !id.is_empty()&&id.len()<=128&&id.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_".contains(&b))
}
fn verified_worker(plan:&Value)->Result<(String,String,Value),String>{
    let origin=env::var("CMB_WORKER_HTTPS_ORIGIN").map_err(|_|"Configure trusted HTTPS worker origin")?;
    let token=env::var("CMB_WORKER_TOKEN").map_err(|_|"Configure trusted worker token")?;
    let health:Value=serde_json::from_slice(&request(&origin,&token,"GET","/health",None)?)
        .map_err(|_|"Invalid worker health JSON")?;
    for (actual,expected) in [("task","task"),("model_id","model_id"),("revision_sha","model_sha"),
        ("family","runtime_family"),("deployment_request_id","request_id")]{
        if plan[expected].as_str().is_none_or(str::is_empty)||health[actual]!=plan[expected]{
            return Err("Worker identity does not match exact deployment plan".into());
        }
    }
    if health["status"]!="ok"||health["simulation_only"]!=false||
        !health["worker_instance_id"].as_str().is_some_and(artifact_id){
        return Err("Worker is not a real identified healthy service".into());
    }
    Ok((origin,token,health))
}
pub(crate) fn instance(plan:&Value)->Result<String,String>{
    let (_,_,health)=verified_worker(plan)?;
    Ok(health["worker_instance_id"].as_str().ok_or("Missing worker identity")?.to_string())
}
fn status_view(data:&Value,plan:&Value,job_id:&str,expected_instance:&str)->Result<Value,String>{
    if data["schema_version"]!=1||data["simulation_only"]!=false||data["request_id"]!=job_id||
        data["deployment_request_id"]!=plan["request_id"]||data["worker_instance_id"]!=expected_instance{
        return Err("Worker job identity changed or invalid; no retry".into());
    }
    let status=data["status"].as_str().ok_or("Missing job status")?;
    if !matches!(status,"running"|"completed"|"failed"|"unknown"){return Err("Unrecognized job status".into());}
    Ok(json!({"request_id":job_id,"worker_instance_id":expected_instance,"status":status,
        "active_jobs":data["active_jobs"].as_u64(),"artifact_transfer_verified":false,
        "worker_ledger_durable":false,"retry_authorized":false,"provider_mutation_sent":false}))
}
pub(crate) fn inspect_job(plan:&Value,job_id:&str,expected_instance:&str)->Result<Value,String>{
    if !job_id_valid(job_id)||!artifact_id(expected_instance){return Err("Missing original worker/job identity".into());}
    let (origin,token,health)=verified_worker(plan)?;
    if health["worker_instance_id"]!=expected_instance{return Err("Worker restarted; job state unknown; never retry blindly".into());}
    let data:Value=serde_json::from_slice(&request(&origin,&token,"GET",&format!("/v1/media/jobs/{job_id}"),None)?)
        .map_err(|_|"Invalid worker job response")?;
    status_view(&data,plan,job_id,expected_instance)
}

/// This function is only called by a local, separately-approved operator
/// deployment session. It never accepts a model or host supplied by an MCP
/// tool call, and downloaded bytes are independently SHA-256 verified.
pub(crate) fn generate(outdir:&Path,plan:&Value,job_id:&str,worker_instance_id:&str,inputs:&Value)
    ->Result<Value,String>
{
    let (origin,token,health)=verified_worker(plan)?;
    if health["worker_instance_id"]!=worker_instance_id||!job_id_valid(job_id){
        return Err("Worker instance changed or job ID invalid; do not send inference".into());
    }
    let model_id=plan["model_id"].as_str().ok_or("Missing model ID")?;
    let revision=plan["model_sha"].as_str().ok_or("Missing model SHA")?;
    let task=plan["task"].as_str().ok_or("Missing task")?;
    let input_items=inputs["inputs"].as_array().ok_or("inputs must be an array")?;
    if input_items.len()>16{return Err("Too many media inputs".into());}
    let params=inputs.get("parameters").cloned().unwrap_or(json!({}));
    if !params.is_object(){return Err("Media parameters must be an object".into());}
    let nonce=job_id;
    let payload=json!({
        "schema_version":1,"request_id":nonce,"task":task,
        "model":{"id":model_id,"revision_sha":revision},
        "inputs":input_items,"parameters":params
    });
    let answer:Value=serde_json::from_slice(
        &request(&origin,&token,"POST","/v1/media/generate",Some(payload.to_string().as_bytes()))?
    ).map_err(|_|"Invalid worker JSON response".to_string())?;
    let artifacts=validate_worker_result(&answer,&payload)?;
    fs::create_dir_all(outdir).map_err(|_|"Cannot create private output directory".to_string())?;
    let mut written=Vec::new();
    for (i,asset) in artifacts.iter().enumerate() {
        let id=asset["asset_ref"].as_str().ok_or("Missing asset handle")?;
        let kind=asset["kind"].as_str().ok_or("Missing asset kind")?;
        let mime=asset["media_type"].as_str().ok_or("Missing asset MIME")?;
        let extension=artifact_ext(kind,mime).ok_or("Unapproved artifact MIME")?;
        let bytes=request(&origin,&token,"GET",&format!("/v1/media/artifacts/{id}"),None)?;
        if bytes.len() as u64 != asset["size_bytes"].as_u64().unwrap_or(u64::MAX)
            || sha256(&bytes)!=asset["sha256"].as_str().unwrap_or("")
        {return Err("Worker artifact SHA-256/byte length mismatch".into());}
        let path=outdir.join(format!("verified-media-{nonce}-{i}.{extension}"));
        let mut opts=OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]{
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file=opts.open(&path).map_err(|_|"Refusing overwrite of existing media".to_string())?;
        file.write_all(&bytes).map_err(|_|"Cannot persist artifact".to_string())?;
        file.sync_all().map_err(|_|"Cannot sync artifact".to_string())?;
        written.push(json!({"kind":kind,"mime":mime,"bytes":bytes.len(),
            "sha256":asset["sha256"],"path":path.to_string_lossy()}));
    }
    Ok(json!({"status":"ARTIFACT_VERIFIED","simulation_only":false,
        "model_id":model_id,"revision_sha":revision,"task":task,
        "worker_origin":origin,"results":written}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_observation_is_bound_to_original_worker_and_never_authorizes_retry(){
        let plan=json!({"request_id":"deployment"});
        let instance="a".repeat(32);
        let mut data=json!({"schema_version":1,"simulation_only":false,"request_id":"job","deployment_request_id":"deployment",
            "worker_instance_id":instance,"status":"completed","active_jobs":0,"result":{"private":"secret"}});
        let v=status_view(&data,&plan,"job",&instance).unwrap();
        assert_eq!(v["retry_authorized"],false);
        assert_eq!(v["artifact_transfer_verified"],false);
        assert!(!v.to_string().contains("secret"));
        data["worker_instance_id"]=json!("b".repeat(32));
        assert!(status_view(&data,&plan,"job",&instance).is_err());
        assert!(!job_id_valid("../other"));
    }
    #[test]
    fn remote_worker_requires_https_hostname_and_never_local_metadata_address() {
        for bad in ["http://example.com","https://127.0.0.1","https://169.254.169.254",
            "https://localhost","https://host/path","https://user:secret@host.example",
            "https://example.org:8080","https://evil.example?x=1"] {
            assert!(!valid_origin(bad),"{bad}");
        }
        assert!(valid_origin("https://pod-8000.proxy.runpod.net"));
        assert!(valid_origin("https://media.example.org"));
    }
    #[test]
    fn refuse_simulated_results_and_mismatched_or_unsafe_artifacts() {
        let input=json!({"request_id":"req1"});
        let mut response=json!({"schema_version":1,"request_id":"req1",
            "status":"completed","simulation_only":false,
            "artifacts":[{"asset_ref":"0123456789abcdef0123456789abcdef",
                "kind":"image","media_type":"image/png","sha256":"a".repeat(64),"size_bytes":42}]});
        assert_eq!(validate_worker_result(&response,&input).unwrap().len(),1);
        response["simulation_only"]=json!(true);
        assert!(validate_worker_result(&response,&input).is_err());
        response["simulation_only"]=json!(false);
        response["artifacts"][0]["asset_ref"]=json!("../secrets");
        assert!(validate_worker_result(&response,&input).is_err());
    }
}
