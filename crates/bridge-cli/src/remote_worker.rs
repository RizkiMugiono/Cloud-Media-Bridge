//! Offline worker response contract tests.
use serde_json::{json,Value};
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

#[cfg(test)]
mod tests {
    use super::*;
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
