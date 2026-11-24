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

pub(crate) const RUNPOD_ACCOUNT_QUERY:&str="query myself { myself { id } }";

pub(crate) fn account_operation(provider:&str)->Result<Operation,String>{
    match provider{
        "runpod-pods"|"runpod-serverless"=>Ok(Operation{
            method:"POST",url:"https://api.runpod.io/graphql".into(),
            body:Some(json!({"query":RUNPOD_ACCOUNT_QUERY})),credential_env:"RUNPOD_API_KEY"}),
        "vast-instances"|"vast-serverless"=>Ok(Operation{
            method:"GET",url:"https://cloud.vast.ai/api/v0/users/current".into(),
            body:None,credential_env:"VAST_API_KEY"}),
        _=>Err("No native account verification adapter for this provider".into()),
    }
}

/// Only the provider's account identifier is compared. Never return raw
/// identity responses or claim that account matching verifies a workspace.
pub(crate) fn verify_account<T:Transport>(backend:&mut T,provider:&str,expected:&str)->Result<Value,String>{
    if !identifier(expected){return Err("An exact provider account ID is required".into());}
    let response=backend.send(&account_operation(provider)?)?;
    if response.get("error").is_some()||response["success"]==false||
        response.get("errors").is_some_and(|e|!e.is_null()&&!e.as_array().is_some_and(Vec::is_empty)){
        return Err("Provider account lookup failed".into());
    }
    let raw=match provider{
        "runpod-pods"|"runpod-serverless"=>&response["data"]["myself"]["id"],
        "vast-instances"|"vast-serverless"=>&response["id"],
        _=>return Err("Unsupported account verification provider".into()),
    };
    let matched=raw.as_str().is_some_and(|s|s==expected)||raw.as_u64().is_some_and(|n|n.to_string()==expected);
    if !matched{return Err("Provider account ID does not match the approved plan".into());}
    Ok(json!({"provider":provider,"account_match":true,"workspace_verified":false,
        "provider_mutation_sent":false,"approval_granted":false}))
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
    pub(crate) region: &'a str,
    pub(crate) quantity: u64,
    pub(crate) cloud: &'a str,
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
            || self.gpu.is_empty() || self.gpu.len()>128 || !self.gpu.bytes().all(|b| b.is_ascii_alphanumeric()||b"-_. ".contains(&b))
            || !identifier(self.region) || !(1..=64).contains(&self.quantity)
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
        "runpod-pods" if !matches!(plan.cloud,"SECURE"|"COMMUNITY") => Err("RunPod requires an exact cloud tier".into()),
        "runpod-pods" => Ok(Operation {
            method: "POST",
            url: "https://api.runpod.io/v2/pods".into(),
            body: Some(json!({
                "name": safe_label(plan.request_id),
                "image": plan.image,
                "gpu": {"id":plan.gpu,"count":plan.quantity},
                "dataCenterIds":[plan.region],
                "cloud":plan.cloud,
                "startSsh":false,
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

fn item<'a>(provider:&str,response:&'a Value)->Option<&'a Value>{
    match provider {
        "runpod-pods"=>Some(response.get("pod").unwrap_or(response)),
        "vast-instances"=>Some(response.get("instances").or_else(||response.get("instance")).unwrap_or(response)),
        _=>None,
    }
}
fn provider_owned(provider:&str,response:&Value,id:&str,owner:&str)->bool{
    let Some(row)=item(provider,response) else{return false;};
    let same_id=row["id"].as_str().is_some_and(|s|s==id)||row["id"].as_u64().is_some_and(|n|n.to_string()==id);
    same_id && row[if provider=="runpod-pods"{"name"}else{"label"}]==safe_label(owner)
}
fn running(provider:&str,response:&Value)->bool{
    let Some(row)=item(provider,response) else{return false;};
    match provider{"runpod-pods"=>row["status"]=="RUNNING"||row["runtimeStatus"]=="running", "vast-instances"=>row["actual_status"]=="running",_=>false}
}
pub(crate) fn stopped(provider: &str, response: &Value) -> bool {
    let Some(item)=item(provider,response) else{return false;};
    match provider {
        "runpod-pods" => item["status"]=="EXITED"
            || (item["desiredStatus"]=="EXITED" && item["runtimeStatus"]=="stopped"),
        "vast-instances" => {
            item["actual_status"] == "stopped"
        },
        _ => false
    }
}

/// Provider-observed state only; a successful HTTP status or desired state
/// never proves actual GPU release. Do not expose the raw provider response.
pub(crate) fn observe_owned<T: Transport>(
    backend:&mut T, provider:&str, id:&str, owner:&str, record:&Value
)->Result<Value,String>{
    if !identifier(owner)||record["owner_request_id"]!=owner||record["id"]!=id
        ||record["provider"]!=provider||record["created_by_bridge"]!=true {
        return Err("Missing exact local resource ownership record".into());
    }
    let response=backend.send(&inspect_operation(provider,id)?)?;
    if response.get("error").is_some()||response["success"]==false||!provider_owned(provider,&response,id,owner){
        return Err("Provider-observed ownership is not established".into());
    }
    let state=match (running(provider,&response),stopped(provider,&response)){
        (true,false)=>"running",(false,true)=>"stopped",_=>"unknown",
    };
    Ok(json!({"resource_id":id,"provider":provider,"owner_request_id":owner,
        "provider_state":state,"provider_identity_matched":true,
        "billing_stopped_confirmed":false,"active_jobs_verified":false,
        "active_transfers_verified":false,"provider_mutation_sent":false}))
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
    if before.get("error").is_some()||before["success"]==false||!provider_owned(provider,&before,resource_id,owner_request_id){return Err("Provider ownership marker or resource identity missing/mismatched; stop forbidden".into());}
    if stopped(provider, &before) && running(provider,&before){return Err("Conflicting actual provider state; reconcile before stop".into());}
    if stopped(provider, &before) {
        return Ok(json!({"resource_id":resource_id,"state":"STOP_CONFIRMED","already_stopped":true,
            "storage_or_network_charges_may_remain":true}));
    }
    if !running(provider,&before){return Err("Provider actual resource state is uncertain; reconcile before stop".into());}
    let request = stop_operation(provider, resource_id)?;
    let response = backend.send(&request)?;
    if response.get("error").is_some() || response["success"] == false {
        return Err("Provider refused resource stop; resource may still be billable".into());
    }
    // A successful stop HTTP response is not itself stopped-state evidence.
    let after = backend.send(&inspect_operation(provider, resource_id)?)?;
    if after.get("error").is_some()||after["success"]==false||!provider_owned(provider,&after,resource_id,owner_request_id) || !stopped(provider, &after) || running(provider,&after) {
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
            gpu:"H100",disk_gb:50,region:"US-TEST-1",quantity:2,cloud:"SECURE",offer_id:Some(123),model_id:"example/media",
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
        assert_eq!(pod.method,"POST");
        assert_eq!(pod.url,"https://api.runpod.io/v2/pods");
        assert_eq!(pod.body.as_ref().unwrap()["env"]["CMB_MODEL_SHA"],plan("runpod-pods").model_sha);
        assert_eq!(pod.body.as_ref().unwrap()["globalNetworking"],false);
        assert_eq!(pod.body.as_ref().unwrap()["gpu"]["count"],2);
        assert_eq!(pod.body.as_ref().unwrap()["dataCenterIds"],json!(["US-TEST-1"]));
        assert_eq!(pod.body.as_ref().unwrap()["cloud"],"SECURE");
        let vast=create_operation(&plan("vast-instances")).unwrap();
        assert_eq!(vast.method,"PUT");
        assert_eq!(vast.url,"https://cloud.vast.ai/api/v0/asks/123/");
        assert_eq!(vast.body.as_ref().unwrap()["label"],"cmb-req001");
        for other in ["modal","runpod-serverless","vast-serverless","lambda-cloud","generic-https","generic-ssh","kubernetes-gpu"] {
            assert!(create_operation(&plan(other)).is_err());
        }
    }

    #[test]
    fn account_lookup_refuses_mismatch_errors_and_does_not_leak_identity(){
        let mut r=Replay{replies:vec![json!({"data":{"myself":{"id":"user_123","pubKey":"secret"}}})],sent:vec![]};
        let checked=verify_account(&mut r,"runpod-pods","user_123").unwrap();
        assert_eq!(checked["account_match"],true);
        assert_eq!(checked["workspace_verified"],false);
        assert!(!checked.to_string().contains("secret"));
        assert_eq!(r.sent[0].body.as_ref().unwrap()["query"],RUNPOD_ACCOUNT_QUERY);
        let mut v=Replay{replies:vec![json!({"id":123,"email":"private@example.invalid"}),json!({"id":456}),json!({"id":123,"error":"auth_error"})],sent:vec![]};
        assert!(verify_account(&mut v,"vast-instances","123").is_ok());
        assert!(verify_account(&mut v,"vast-instances","123").is_err());
        assert!(verify_account(&mut v,"vast-instances","123").is_err());
        assert!(v.sent.iter().all(|o|o.method=="GET"&&o.body.is_none()));
    }
    #[test]
    fn observation_is_get_only_redacted_and_never_proves_billing_or_job_state(){
        let record=json!({"owner_request_id":"req","id":"p01","provider":"runpod-pods","created_by_bridge":true});
        let mut fake=Replay{replies:vec![json!({"id":"p01","name":"cmb-req","status":"EXITED","env":{"SECRET":"private"}})],sent:vec![]};
        let observed=observe_owned(&mut fake,"runpod-pods","p01","req",&record).unwrap();
        assert_eq!(observed["provider_state"],"stopped");
        assert_eq!(observed["billing_stopped_confirmed"],false);
        assert_eq!(observed["active_jobs_verified"],false);
        assert!(!observed.to_string().contains("private"));
        assert_eq!(fake.sent.len(),1);
        assert_eq!(fake.sent[0].method,"GET");
        let mut conflict=Replay{replies:vec![json!({"id":"p01","name":"cmb-req","status":"EXITED","runtimeStatus":"running"})],sent:vec![]};
        assert_eq!(observe_owned(&mut conflict,"runpod-pods","p01","req",&record).unwrap()["provider_state"],"unknown");
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
            json!({"id":"p01","name":"cmb-req001","status":"RUNNING"}),
            json!({"success":true}),
            json!({"id":"p01","name":"cmb-req001","status":"EXITED","runtimeStatus":"stopped"})
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
            json!({"id":123,"label":"cmb-req001","actual_status":"running"}),
            json!({"success":true}),
            json!({"id":123,"label":"cmb-req001","actual_status":"running"})
        ],sent:vec![]};
        let result=stop_owned(&mut fake,"vast-instances","123","req001",&owned("vast-instances","123")).unwrap();
        assert_eq!(result["state"],"STOP_UNCERTAIN");
        assert_eq!(result["manual_reconciliation_required"],true);
        assert!(!stopped("vast-instances",&json!({"intended_status":"stopped","actual_status":"running"})));
    }

    #[test]
    fn provider_ownership_and_actual_state_are_required_before_mutation(){
        let mut fake=Replay{replies:vec![json!({"id":"p01","name":"other","status":"RUNNING"})],sent:vec![]};
        assert!(stop_owned(&mut fake,"runpod-pods","p01","req001",&owned("runpod-pods","p01")).is_err());
        assert_eq!(fake.sent.len(),1);
        let mut unknown=Replay{replies:vec![json!({"id":"p01","name":"cmb-req001","desiredStatus":"RUNNING"})],sent:vec![]};
        assert!(stop_owned(&mut unknown,"runpod-pods","p01","req001",&owned("runpod-pods","p01")).is_err());
        assert_eq!(unknown.sent.len(),1);
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
        let mut fake=Replay{replies:vec![json!({"id":123,"label":"cmb-req001","actual_status":"running"}),json!({"success":false})],sent:vec![]};
        assert!(stop_owned(&mut fake,"vast-instances","123","req001",&owned("vast-instances","123")).is_err());
        assert_eq!(fake.sent.len(),2);
        assert_ne!(fake.sent[1].method,"DELETE");
        assert!(stop_operation("lambda-cloud","123").is_err());
    }
}
