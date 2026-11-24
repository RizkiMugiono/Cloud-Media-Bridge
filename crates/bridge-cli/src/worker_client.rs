//! Local loopback-only Rust -> HTTP fixture worker interoperability test.
//! Explicitly refuses public URLs, remote providers, model execution and real GPU.
use crate::crypto::sha256;
use serde_json::{json, Value};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn request(port:u16,method:&str,path:&str,token:Option<&str>,body:Option<&[u8]>)
    -> Result<Vec<u8>,String>
{
    if port==0 || !path.starts_with('/') || path.contains("..") || path.contains('?') {
        return Err("Invalid loopback fixture worker request path".into());
    }
    let url=format!("http://127.0.0.1:{port}{path}");
    let mut cmd=Command::new("curl");
    cmd.args(["--silent","--show-error","--fail","--proto","=http",
        "--noproxy","*","--max-redirs","0","--connect-timeout","3",
        "--max-time","15","--max-filesize","33554432","--request",method]);
    if let Some(token)=token {
        if token.len()<16 || token.contains(['\r','\n']) {
            return Err("Invalid fixture token".into());
        }
        cmd.args(["--header",&format!("Authorization: Bearer {token}")]);
    }
    if body.is_some() {
        cmd.args(["--header","Content-Type: application/json","--data-binary","@-"]);
    }
    cmd.arg(&url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if body.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child=cmd.spawn().map_err(|_|"curl is required for the local fixture-worker HTTP test".to_string())?;
    if let Some(body)=body {
        let Some(mut stdin)=child.stdin.take() else {return Err("Worker request stdin unavailable".into());};
        stdin.write_all(body).map_err(|e|format!("Worker input error: {e}"))?;
    }
    let output=child.wait_with_output().map_err(|e|format!("Worker request failed: {e}"))?;
    if !output.status.success() {
        return Err("Local fixture worker returned an HTTP or transport error".into());
    }
    if output.stdout.len()>32*1024*1024 {
        return Err("Worker response exceeded fixture size limit".into());
    }
    Ok(output.stdout)
}
fn safe_artifact_id(value:&str)->bool{
    value.len()==32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(crate) fn smoke(directory:&str,port:u16,kind:&str)->Result<Value,String>{
    if !crate::offline::supported_kinds().contains(&kind) || port==0 {
        return Err("Invalid fixture kind or port".into());
    }
    let token=env::var("CMB_WORKER_TOKEN").map_err(|_|"CMB_WORKER_TOKEN must be set for loopback fixture test".to_string())?;
    let health: Value=serde_json::from_slice(&request(port,"GET","/health",None,None)?)
        .map_err(|_|"Invalid fixture worker health JSON".to_string())?;
    // Never accidentally connect to a real production worker through this tool.
    if health["simulation_only"]!=true || health["status"]!="ok" {
        return Err("Refusing to send fixture request to non-fixture worker".into());
    }
    let task=health["task"].as_str().ok_or("Fixture worker has no task")?;
    let model=json!({"id":"fixture/offline-model","revision_sha":"0000000000000000000000000000000000000000"});
    let nonce=crate::crypto::secure_nonce()?;
    let payload=json!({"schema_version":1,"request_id":nonce,
        "task":task,"model":model,
        "inputs":[{"kind":"text","text":"Offline worker integration fixture, not AI"}],
        "fixture_kind":kind});
    let response:Value=serde_json::from_slice(
        &request(port,"POST","/v1/media/generate",Some(&token),Some(payload.to_string().as_bytes()))?
    ).map_err(|_|"Fixture worker returned invalid JSON".to_string())?;
    if response["simulation_only"]!=true || response["status"]!="completed"
        || response["request_id"]!=payload["request_id"] {
        return Err("Worker response does not match the mock request".into());
    }
    let artifact=&response["artifacts"][0];
    if artifact["kind"]!=kind {
        return Err("Worker returned wrong fixture kind".into());
    }
    let id=artifact["asset_ref"].as_str().ok_or("Missing artifact handle")?;
    if !safe_artifact_id(id) {
        return Err("Invalid artifact handle".into());
    }
    let expected=artifact["sha256"].as_str().ok_or("Missing artifact digest")?;
    if expected.len()!=64 || !expected.bytes().all(|b|b.is_ascii_hexdigit()) {
        return Err("Invalid worker artifact SHA-256".into());
    }
    let bytes=request(port,"GET",&format!("/v1/media/artifacts/{id}"),Some(&token),None)?;
    if bytes.len() as u64 !=artifact["size_bytes"].as_u64().unwrap_or(u64::MAX)
        || sha256(&bytes)!=expected {
        return Err("Worker artifact failed size/SHA-256 verification".into());
    }
    let root=Path::new(directory);
    fs::create_dir_all(root).map_err(|e|format!("Cannot prepare fixture artifact directory: {e}"))?;
    let path=root.join(format!("http-fixture-{kind}.bin"));
    let mut options=OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file=options.open(&path).map_err(|e|format!("Refusing to overwrite fixture output: {e}"))?;
    file.write_all(&bytes).map_err(|e|e.to_string())?;
    file.sync_all().map_err(|e|e.to_string())?;
    Ok(json!({
        "worker_protocol":"http_loopback_fixture",
        "simulation_only":true,
        "real_ai_inference":false,
        "provider_gpu_used":false,
        "worker_health_verified":true,
        "artifact_verified":true,
        "kind":kind,
        "media_type":artifact["media_type"],
        "bytes":bytes.len(),
        "sha256":expected,
        "saved_file":path.to_string_lossy()
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_worker_artifact_identifiers_are_rejected() {
        for bad in ["../etc/passwd","file://secrets","", "a".repeat(40).as_str()] {
            assert!(!safe_artifact_id(bad));
        }
        assert!(safe_artifact_id("0123456789abcdef0123456789abcdef"));
    }
    #[test]
    fn worker_is_restricted_to_valid_fixture_port_and_kind() {
        assert!(smoke("",0,"image").is_err());
        assert!(smoke("",8080,"exec").is_err());
    }
}
