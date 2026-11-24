//! Provider REST adapter contract and exact resource-state reconciliation.
//! The production transport is intentionally disabled until a model-specific
//! runtime image, cloud identity and user-approved lifecycle are verified.
//! Offline unit tests exercise the exact same request and response machinery.
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub(crate) struct Operation {
    pub(crate) method: &'static str,
    pub(crate) url: String,
    pub(crate) body: Option<Value>,
    pub(crate) credential_env: &'static str,
}

pub(crate) trait Transport {
    fn send(&mut self, op: &Operation) -> Result<Value, String>;
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

fn image_pinned(image: &str) -> bool {
    let Some((name, sha)) = image.rsplit_once("@sha256:") else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 240
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./:".contains(&b))
        && sha.len() == 64
        && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

fn model_identifier(value: &str) -> bool {
    let (Some(owner), Some(model)) = (value.split('/').next(), value.split('/').nth(1)) else {
        return false;
    };
    value.split('/').count() == 2
        && identifier(owner)
        && identifier(model)
}

fn revision(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64)
        && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn safe_label(id: &str) -> String {
    format!("cmb-{id}")
}

pub(crate) struct Deployment<'a> {
    pub(crate) provider: &'a str,
    pub(crate) request_id: &'a str,
    pub(crate) image: &'a str,
    pub(crate) gpu: &'a str,
    pub(crate) disk_gb: u64,
    pub(crate) offer_id: Option<u64>,
    pub(crate) model_id: &'a str,
    pub(crate) model_sha: &'a str,
    pub(crate) task: &'a str,
    pub(crate) family: &'a str,
    pub(crate) worker_token: &'a str,
}

impl Deployment<'_> {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !identifier(self.request_id)
            || !image_pinned(self.image)
            || !model_identifier(self.model_id)
            || !revision(self.model_sha)
            || !identifier(self.gpu)
            || !identifier(self.task)
            || !identifier(self.family)
            || !(10..=2000).contains(&self.disk_gb)
        {
            return Err("Invalid model SHA, identity, pinned image or bounded GPU request".into());
        }
        // Token goes only to the user's provider as worker ENV, never the MCP
        // response, resource ledger, audit log or a command-line argument.
        if self.worker_token.len() < 24
            || self.worker_token.len() > 256
            || !self.worker_token.bytes().all(|b| b.is_ascii_graphic() && b != b'\\' && b != b'"')
        {
            return Err("CMB_WORKER_TOKEN must be a secure, printable 24-256 character secret".into());
        }
        Ok(())
    }
}

fn env_vars(plan: &Deployment<'_>) -> Value {
    json!({
        "CMB_DEPLOYMENT_REQUEST_ID": plan.request_id,
        "CMB_MODEL_ID": plan.model_id,
        "CMB_MODEL_SHA": plan.model_sha,
        "CMB_TASK": plan.task,
        "CMB_RUNTIME_FAMILY": plan.family,
        "CMB_WORKER_TOKEN": plan.worker_token
    })
}

pub(crate) fn create_operation(plan: &Deployment<'_>) -> Result<Operation, String> {
    plan.validate()?;
    match plan.provider {
        "runpod-pods" => Ok(Operation {
            method: "POST",
            url: "https://api.runpod.io/v2/pods".into(),
            body: Some(json!({
                "name": safe_label(plan.request_id),
                "image": plan.image,
                "gpu": {"id":plan.gpu,"count":1},
                "disk":plan.disk_gb,
                "globalNetworking":false,
                "env":env_vars(plan)
            })),
            credential_env: "RUNPOD_API_KEY"
        }),
        "vast-instances" => {
            let offer = plan.offer_id.filter(|id| *id > 0)
                .ok_or("A specific selected Vast.ai GPU offer ID is required")?;
            // Vast uses a distinct, already selected GPU offer rather than
            // RunPod's name-based GPU allocation. No offer is invented here.
            Ok(Operation {
                method: "PUT",
                url: format!("https://cloud.vast.ai/api/v0/asks/{offer}/"),
                body: Some(json!({
                    "image":plan.image,
                    "disk":plan.disk_gb,
                    "label":safe_label(plan.request_id),
                    "env":env_vars(plan)
                })),
                credential_env: "VAST_API_KEY"
            })
        }
        _ => Err("This provider has no verified non-destructive GPU provisioning adapter. Live deployment is disabled".into())
    }
}

fn resource_id(provider: &str, data: &Value) -> Result<String, String> {
    if data.get("error").is_some() || data["success"] == false {
        return Err("Provider reported failed resource creation; reconcile the account before retrying".into());
    }
    let value = match provider {
        "runpod-pods" => data["id"].as_str().or_else(||data["pod"]["id"].as_str())
            .map(str::to_string),
        "vast-instances" => data["new_contract"].as_u64().map(|v|v.to_string())
            .or_else(||data["new_contract"].as_str().map(str::to_string)),
        _ => None
    };
    let id = value.ok_or("Provider did not return a recognized new resource id; manual reconciliation required")?;
    if !identifier(&id) {
        return Err("Provider returned an unsafe resource identifier".into());
    }
    Ok(id)
}

pub(crate) fn create<T: Transport>(backend: &mut T, plan: &Deployment<'_>) -> Result<String, String> {
    let req = create_operation(plan)?;
    resource_id(plan.provider, &backend.send(&req)?)
}

pub(crate) fn inspect_operation(provider: &str, id: &str) -> Result<Operation, String> {
    if !identifier(id) {
        return Err("Invalid resource id".into());
    }
    match provider {
        "runpod-pods" => Ok(Operation {
            method: "GET",
            url: format!("https://api.runpod.io/v2/pods/{id}"),
            body: None,
            credential_env: "RUNPOD_API_KEY"
        }),
        "vast-instances" => Ok(Operation {
            method: "GET",
            url: format!("https://cloud.vast.ai/api/v0/instances/{id}/"),
            body: None,
            credential_env: "VAST_API_KEY"
        }),
        _ => Err("No verified resource state API for this provider".into())
    }
}

pub(crate) fn stop_operation(provider: &str, id: &str) -> Result<Operation, String> {
    if !identifier(id) {
        return Err("Invalid resource id".into());
    }
    match provider {
        "runpod-pods" => Ok(Operation {
            method: "POST",
            url: format!("https://api.runpod.io/v2/pods/{id}/action"),
            body: Some(json!({"action":"stop"})),
            credential_env: "RUNPOD_API_KEY"
        }),
        "vast-instances" => Ok(Operation {
            method: "PUT",
            url: format!("https://cloud.vast.ai/api/v0/instances/{id}/"),
            body: Some(json!({"state":"stopped"})),
            credential_env: "VAST_API_KEY"
        }),
        _ => Err("Stop not supported: destruction is not an acceptable substitute".into())
    }
}

pub(crate) fn stopped(provider: &str, response: &Value) -> bool {
    let item = match provider {
        "runpod-pods" => response.get("pod").unwrap_or(response),
        "vast-instances" => response.get("instance").unwrap_or(response),
        _ => return false
    };
    match provider {
        "runpod-pods" => item["status"]=="EXITED"
            || (item["desiredStatus"]=="EXITED" && item["runtimeStatus"]=="stopped"),
        "vast-instances" => {
            item["actual_status"] == "stopped"
        },
        _ => false
    }
}

/// Reconcile only the resource recorded as created by this deployment. If
/// anything is uncertain, do not destroy it and do not report success.
pub(crate) fn stop_owned<T: Transport>(
    backend: &mut T,
    provider: &str,
    resource_id: &str,
    owner_request_id: &str,
    record: &Value,
) -> Result<Value, String> {
    if !identifier(owner_request_id)
        || record["owner_request_id"] != owner_request_id
        || record["id"] != resource_id
        || record["provider"] != provider
        || record["created_by_bridge"] != true
        || record["active_jobs"].as_u64().unwrap_or(u64::MAX) != 0
        || record["active_artifact_transfers"].as_u64().unwrap_or(u64::MAX) != 0
    {
        return Err("Resource ownership or active-job/artifact state is not confirmed; stop forbidden".into());
    }
    let before = backend.send(&inspect_operation(provider, resource_id)?)?;
    if stopped(provider, &before) {
        return Ok(json!({"resource_id":resource_id,"state":"STOP_CONFIRMED","already_stopped":true,
            "storage_or_network_charges_may_remain":true}));
    }
    let request = stop_operation(provider, resource_id)?;
    let response = backend.send(&request)?;
    if response.get("error").is_some() || response["success"] == false {
        return Err("Provider refused resource stop; resource may still be billable".into());
    }
    // A successful stop HTTP response is not itself stopped-state evidence.
    let after = backend.send(&inspect_operation(provider, resource_id)?)?;
    if !stopped(provider, &after) {
        return Ok(json!({"resource_id":resource_id,"state":"STOP_UNCERTAIN",
            "manual_reconciliation_required":true,"storage_or_network_charges_may_remain":true}));
    }
    Ok(json!({"resource_id":resource_id,"state":"STOP_CONFIRMED","already_stopped":false,
        "storage_or_network_charges_may_remain":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Replay {
        replies: Vec<Value>,
        sent: Vec<Operation>
    }
    impl Transport for Replay {
        fn send(&mut self, op: &Operation) -> Result<Value,String> {
            self.sent.push(op.clone());
            if self.replies.is_empty() {return Err("No fake provider response".into());}
            Ok(self.replies.remove(0))
        }
    }
    fn plan(provider: &'static str) -> Deployment<'static> {
        Deployment {
            provider,request_id:"req001",image:"ghcr.io/example/worker@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            gpu:"H100",disk_gb:50,offer_id:Some(123),model_id:"example/media",
            model_sha:"0123456789abcdef0123456789abcdef01234567",
            task:"text-to-image",family:"diffusers",
            worker_token:"example-secret-for-offline-tests-123456"
        }
    }
    fn owned(provider:&str,id:&str)->Value {
        json!({"created_by_bridge":true,"owner_request_id":"req001",
            "provider":provider,"id":id,"active_jobs":0,"active_artifact_transfers":0})
    }

    #[test]
    fn runpod_and_vast_create_requests_have_exact_pinned_identity_and_private_worker_token() {
        let pod=create_operation(&plan("runpod-pods")).unwrap();
        assert_eq!(pod.credential_env,"RUNPOD_API_KEY");
        assert_eq!(pod.method,"POST");
        assert_eq!(pod.url,"https://api.runpod.io/v2/pods");
        assert_eq!(pod.body.as_ref().unwrap()["env"]["CMB_MODEL_SHA"],plan("runpod-pods").model_sha);
        assert_eq!(pod.body.as_ref().unwrap()["globalNetworking"],false);
        let vast=create_operation(&plan("vast-instances")).unwrap();
        assert_eq!(vast.method,"PUT");
        assert_eq!(vast.url,"https://cloud.vast.ai/api/v0/asks/123/");
        assert_eq!(vast.body.as_ref().unwrap()["label"],"cmb-req001");
        for other in ["modal","runpod-serverless","vast-serverless","lambda-cloud","generic-https","generic-ssh","kubernetes-gpu"] {
            assert!(create_operation(&plan(other)).is_err());
        }
    }

    #[test]
    fn new_resource_id_is_parsed_from_actual_provider_response_shape() {
        let mut pod=Replay{replies:vec![json!({"id":"pod123"})],sent:vec![]};
        assert_eq!(create(&mut pod,&plan("runpod-pods")).unwrap(),"pod123");
        let mut vast=Replay{replies:vec![json!({"success":true,"new_contract":12345})],sent:vec![]};
        assert_eq!(create(&mut vast,&plan("vast-instances")).unwrap(),"12345");
        let mut broken=Replay{replies:vec![json!({"success":false,"error":"capacity"})],sent:vec![]};
        assert!(create(&mut broken,&plan("vast-instances")).is_err());
    }

    #[test]
    fn stop_checks_exact_ownership_and_last_job_before_sending_requests() {
        let mut fake=Replay{replies:vec![],sent:vec![]};
        let mut r=owned("runpod-pods","p01");
        r["active_jobs"]=json!(1);
        assert!(stop_owned(&mut fake,"runpod-pods","p01","req001",&r).is_err());
        r["active_jobs"]=json!(0);
        r["owner_request_id"]=json!("different-user");
        assert!(stop_owned(&mut fake,"runpod-pods","p01","req001",&r).is_err());
        assert!(fake.sent.is_empty());
    }

    #[test]
    fn successful_stop_must_be_confirmed_by_provider_observation() {
        let mut fake=Replay{replies:vec![
            json!({"desiredStatus":"RUNNING"}),
            json!({"success":true}),
            json!({"status":"EXITED","runtimeStatus":"stopped"})
        ],sent:vec![]};
        let result=stop_owned(&mut fake,"runpod-pods","p01","req001",&owned("runpod-pods","p01")).unwrap();
        assert_eq!(result["state"],"STOP_CONFIRMED");
        assert_eq!(fake.sent.len(),3);
        assert_eq!(fake.sent[1].body.as_ref().unwrap()["action"],"stop");
        assert_eq!(fake.sent[1].method,"POST");
    }

    #[test]
    fn an_accepted_stop_is_not_falsely_reported_as_confirmed() {
        let mut fake=Replay{replies:vec![
            json!({"actual_status":"running"}),
            json!({"success":true}),
            json!({"actual_status":"running"})
        ],sent:vec![]};
        let result=stop_owned(&mut fake,"vast-instances","123","req001",&owned("vast-instances","123")).unwrap();
        assert_eq!(result["state"],"STOP_UNCERTAIN");
        assert_eq!(result["manual_reconciliation_required"],true);
        assert!(!stopped("vast-instances",&json!({"intended_status":"stopped","actual_status":"running"})));
    }

    #[test]
    fn desired_stop_is_not_actual_stopped_evidence() {
        assert!(!stopped("runpod-pods",&json!({
            "desiredStatus":"EXITED","runtimeStatus":"running","status":"RUNNING"
        })));
        assert!(stopped("runpod-pods",&json!({
            "desiredStatus":"EXITED","runtimeStatus":"stopped"
        })));
        assert!(!stopped("vast-instances",&json!({
            "intended_status":"stopped","actual_status":"running"
        })));
    }

    #[test]
    fn provider_errors_are_never_interpreted_as_success_or_termination() {
        let mut fake=Replay{replies:vec![json!({"actual_status":"running"}),json!({"success":false})],sent:vec![]};
        assert!(stop_owned(&mut fake,"vast-instances","123","req001",&owned("vast-instances","123")).is_err());
        assert_eq!(fake.sent.len(),2);
        assert_ne!(fake.sent[1].method,"DELETE");
        assert!(stop_operation("lambda-cloud","123").is_err());
    }
}
