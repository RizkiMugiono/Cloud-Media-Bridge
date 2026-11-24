//! Authenticated provider HTTPS transport.
//! API keys are read from the user's environment and sent to curl's stdin
//! config. They never appear in process argv, CLI/MCP JSON, or error messages.
use crate::provider_api::{Operation, Transport};
use serde_json::Value;
use std::env;
use std::io::Write;
use std::process::{Command, Stdio};

const MAX_RESPONSE: usize = 2 * 1024 * 1024;

fn allowed_operation(op: &Operation) -> bool {
    if !matches!(op.method, "GET"|"POST"|"PUT")
        || op.url.contains(['?', '#', '@', '\r','\n'])
        || op.url.len() > 240
    {
        return false;
    }
    match op.credential_env {
        "RUNPOD_API_KEY" => {
            let base = "https://api.runpod.io/v2/pods";
            op.url == base || op.url.strip_prefix(&(base.to_string()+"/"))
                .is_some_and(|path| !path.is_empty() && path.split('/').all(|part|
                    !part.is_empty() && part.len()<=128 && part.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'||c==b'_'))
                    && path.split('/').count() <= 2)
        }
        "VAST_API_KEY" => {
            let base = "https://cloud.vast.ai/api/v0/";
            let Some(path) = op.url.strip_prefix(base) else {return false};
            let parts=path.split('/').collect::<Vec<_>>();
            match parts.as_slice() {
                ["asks",id,""]|["instances",id,""] =>
                    !id.is_empty() && id.len()<=20 && id.bytes().all(|c| c.is_ascii_digit()),
                _=>false
            }
        }
        _ => false,
    }
}

fn escape_curl_config(value: &str) -> Result<String,String> {
    if value.len()>MAX_RESPONSE || value.chars().any(|c| c.is_control()) {
        return Err("Unsafe content for curl configuration".into());
    }
    Ok(value.replace('\\',"\\\\").replace('"',"\\\""))
}

fn build_config(op: &Operation, token: &str) -> Result<String,String> {
    if !allowed_operation(op) {
        return Err("Refusing an unapproved provider endpoint or HTTP method".into());
    }
    if token.len()<16 || token.len()>1024
        || token.chars().any(|c| c.is_whitespace() || c.is_control() || c=='"')
    {
        return Err("Invalid user-owned provider credential".into());
    }
    let mut cfg=format!(
        "header = \"Authorization: Bearer {}\"\nheader = \"Accept: application/json\"\n",
        escape_curl_config(token)?
    );
    if let Some(body)=&op.body {
        let payload=serde_json::to_string(body).map_err(|_|"Cannot serialize provider JSON".to_string())?;
        cfg.push_str("header = \"Content-Type: application/json\"\n");
        cfg.push_str(&format!("data = \"{}\"\n",escape_curl_config(&payload)?));
    }
    Ok(cfg)
}

#[derive(Default)]
pub(crate) struct CurlTransport;
impl Transport for CurlTransport {
    fn send(&mut self, op: &Operation) -> Result<Value,String> {
        let token=env::var(op.credential_env)
            .map_err(|_|format!("Missing {} in local environment",op.credential_env))?;
        let cfg=build_config(op,&token)?;
        let mut child=Command::new("curl")
            .args(["--silent","--show-error","--fail","--proto","=https",
                "--max-redirs","0","--connect-timeout","10","--max-time","45",
                "--max-filesize","2097152","--noproxy","",
                "--request",op.method,"--config","-","--url",&op.url])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn().map_err(|_|"curl is required for provider HTTPS calls".to_string())?;
        if let Some(mut input)=child.stdin.take() {
            input.write_all(cfg.as_bytes())
                .map_err(|_|"Failed to send private provider request".to_string())?;
        }
        let output=child.wait_with_output()
            .map_err(|_|"Provider network request did not complete".to_string())?;
        if !output.status.success() {
            return Err("Provider API returned an HTTP/transport failure; reconcile resource state before any retry".into());
        }
        if output.stdout.len()>MAX_RESPONSE {
            return Err("Provider API response exceeds safe size".into());
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|_|"Provider API returned invalid JSON; reconcile before retry".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn op() -> Operation {
        Operation{method:"POST",url:"https://api.runpod.io/v2/pods".into(),
            body:Some(json!({"image":"ghcr.io/example/test@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"})),
            credential_env:"RUNPOD_API_KEY"}
    }
    #[test]
    fn allowlist_rejects_credentials_in_urls_and_alternate_hosts() {
        let mut o=op();
        assert!(allowed_operation(&o));
        o.url="https://evil.test/v2/pods".into();
        assert!(!allowed_operation(&o));
        o.url="https://api.runpod.io.evil.test/v2/pods".into();
        assert!(!allowed_operation(&o));
        o.url="https://api.runpod.io/v2/pods?api_key=secret".into();
        assert!(!allowed_operation(&o));
        o.url="https://api.runpod.io/v2/pods/../../secrets".into();
        assert!(!allowed_operation(&o));
    }
    #[test]
    fn secure_curl_config_keeps_token_off_argv() {
        let cfg=build_config(&op(),"offline-token-with-enough-entropy-abc123").unwrap();
        assert!(cfg.contains("Authorization: Bearer offline-token"));
        assert!(cfg.contains("Content-Type: application/json"));
        assert!(build_config(&op(),"bad\nheader:evil").is_err());
        assert!(build_config(&op(),"bad\"extra").is_err());
    }
    #[test]
    fn only_vast_instance_and_ask_ids_are_accepted() {
        let mut o=op();
        o.credential_env="VAST_API_KEY";
        o.method="PUT";
        o.url="https://cloud.vast.ai/api/v0/instances/123/".into();
        assert!(allowed_operation(&o));
        o.url="https://cloud.vast.ai/api/v0/asks/456/".into();
        assert!(allowed_operation(&o));
        o.url="https://cloud.vast.ai/api/v0/auth/apikeys/".into();
        assert!(!allowed_operation(&o));
    }
}
