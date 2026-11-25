//! Pure, offline request-contract previews for nine GPU provider classes.
//! No credential access, curl, subprocesses or live cloud operations.
use serde_json::{json, Value};

pub(crate) fn pinned_image(s: &str) -> bool {
    let Some((name, digest)) = s.rsplit_once("@sha256:") else {return false;};
    !name.is_empty() && name.len() <= 250
        && !name.bytes().any(|b| b.is_ascii_whitespace() || b < 32 || b==127)
        && !name.contains("://")
        && digest.len()==64
        && digest.bytes().all(|b| b.is_ascii_hexdigit())
}

fn safe_identity(s:&str) -> bool {
    !s.is_empty() && s.len()<=96
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_. ".contains(&b))
}
pub(crate) fn gpu_identity(s:&str) -> bool {
    safe_identity(s)
}
pub(crate) fn preview(provider:&str,image:&str,gpu:&str,disk:u64,offer_id:Option<u64>) -> Result<Value,String> {
    if !pinned_image(image) {
        return Err("An OCI image pinned by a full SHA-256 digest is required".into());
    }
    if !gpu_identity(gpu) || !(10..=2000).contains(&disk) {
        return Err("GPU identifier and disk allocation must be explicit and bounded".into());
    }
    let name="cmb-pending-human-approval";
    let (request,stop,mode,notes): (Value,Value,&str,&str)=match provider {
        "runpod-pods" => (
            json!({"method":"POST","url":"https://api.runpod.io/v2/pods",
                "body":{"name":name,"image":image,"gpu":{"id":gpu,"count":1},"disk":disk,
                    "globalNetworking":false},
                "credential_env":"RUNPOD_API_KEY"}),
            json!({"method":"POST","url_template":"https://api.runpod.io/v2/pods/{owned_id}/action",
                "body":{"action":"stop"},"requires_owned_resource":true,
                "verify_via":"GET https://api.runpod.io/v2/pods/{owned_id}","observed_stopped_status":"EXITED"}),
            "gpu-pod","Provider supports stopping compute while retaining disk; resource charges may persist"
        ),
        "runpod-serverless" => (
            json!({"method":"POST","url":"https://api.runpod.io/v2/serverless",
                "body":{"name":name,"templateId":null,"gpu":{"pools":[gpu],"count":1},
                    "workers":{"min":0,"max":1,"idleTimeout":5},
                    "scaling":{"type":"QUEUE_DELAY","queueDelay":4},
                    "type":"QUEUE","flashboot":"OFF"},
                "credential_env":"RUNPOD_API_KEY"}),
            json!({"lifecycle":"provider_worker_scale_to_zero","workers_min":0,
                "verification_required":true,"not_a_pod_stop":true}),
            "serverless","Serverless requires a verified queue template ID; the image belongs to the template, not endpoint create. Verify actual idle worker count"
        ),
        "vast-instances" => {
            let offer=offer_id.filter(|n|*n>0).ok_or("Vast.ai instance creation requires a specific real offer ID")?;
            (
                json!({"method":"PUT","url":format!("https://cloud.vast.ai/api/v0/asks/{offer}/"),
                    "body":{"image":image,"disk":disk,"label":name},
                    "credential_env":"VAST_API_KEY"}),
                json!({"method":"PUT","url_template":"https://cloud.vast.ai/api/v0/instances/{owned_id}/",
                    "body":{"state":"stopped"},"requires_owned_resource":true,
                    "verify_via":"GET https://cloud.vast.ai/api/v0/instances/{owned_id}/"}),
                "gpu-instance","Vast stopped instance may continue incurring disk charges"
            )
        },
        "modal" => (
            json!({"transport":"official_agent_handoff","image":image,"gpu":gpu,"disk_gb":disk_gb,
                "native_modes":["modal run (reviewed one-shot App)","modal deploy (reviewed Function/App)","modal server deploy (verified HTTP server image only)"],
                "requires_mode_selection":true,"environment":"user-owned Modal auth",
                "additional_gpu_flags":"verify selected workload and installed Modal CLI before execution"}),
            json!({"lifecycle":"serverless_scale_to_zero","idle_workers":0,
                "verify_app_ownership_and_worker_state":true}),
            "serverless","Modal CLI arguments and selected GPU flags require live-version validation; not executable from this preview"
        ),
        "vast-serverless" => (
            json!({"provider_mode":"vast_serverless",
                "request_builder_status":"not_implemented_until_endpoint_and_worker_schema_verified",
                "image_digest":image}),
            json!({"lifecycle":"provider_serverless_worker_autoscaling",
                "verification_required":true}),
            "serverless","Separate endpoint API and worker contract; never reuse Vast instance creation without verification"
        ),
        "lambda-cloud" => (
            json!({"method":"POST","url":"https://cloud.lambda.com/api/v1/instance-operations/launch",
                "request_builder_status":"requires_region_instance_type_ssh_key_cloud_init_and_verified_runtime",
                "image_digest":image,"credential_env":"LAMBDA_API_KEY"}),
            json!({"lifecycle":"STOP_UNSUPPORTED_ACTION_REQUIRED",
                "reason":"No verified non-destructive stop API; termination is NOT automatic",
                "auto_terminate":false}),
            "gpu-vm","Cannot fulfill automatic non-destructive stop; unattended deployment must be blocked"
        ),
        "generic-https" => (
            json!({"mode":"existing_user_owned_authenticated_worker",
                "request_builder_status":"requires_explicit_pinned_https_origin_and_worker_protocol",
                "image_digest":image}),
            json!({"lifecycle":"externally_managed","auto_stop":false}),
            "external-worker","Connecting to an existing worker is not permission to stop the host"
        ),
        "generic-ssh" => (
            json!({"mode":"existing_user_owned_ssh_host",
                "request_builder_status":"requires_separate_vetted_host_and_worker_installer",
                "image_digest":image}),
            json!({"lifecycle":"externally_managed","auto_stop":false}),
            "existing-host","Never stop preexisting unowned hardware"
        ),
        "kubernetes-gpu" => (
            json!({"kind":"Job","apiVersion":"batch/v1",
                "metadata":{"generateName":"cmb-reviewed-"},
                "spec":{"template":{"spec":{"restartPolicy":"Never","containers":[{
                    "name":"media-worker","image":image,
                    "resources":{"limits":{"nvidia.com/gpu":1}}
                }]}},"backoffLimit":0}}),
            json!({"lifecycle":"only_owned_workload_cleanup_with_independent_cluster_node_scale_verification",
                "auto_delete_cluster":false}),
            "gpu-cluster","GPU node scale-down depends on cluster/autoscaler; Job deletion is distinct from GPU shutdown"
        ),
        _ => return Err("Unknown provider ID".into())
    };
    Ok(json!({
        "provider":provider,"compute_mode":mode,"image_digest_valid":true,
        "gpu_requested":gpu,"disk_gb":disk,
        "proposed_create_operation":request,"proposed_stop_or_idle_policy":stop,
        "safety_note":notes,"model_sha_required":true,"verified_runtime_required":true,
        "fresh_human_deployment_approval_required":true,
        "requires_durable_resource_ownership_record":true,
        "status":"request_contract_preview_only",
        "credential_accessed":false,"network_request_sent":false,
        "execution_enabled":false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image()->String{format!("ghcr.io/example/verified-worker@sha256:{}", "a".repeat(64))}
    #[test]
    fn all_nine_providers_have_safe_offline_contracts() {
        for id in [
            "modal","runpod-serverless","runpod-pods","vast-serverless",
            "vast-instances","lambda-cloud","generic-https","generic-ssh","kubernetes-gpu"
        ] {
            let p=preview(id,&image(),"NVIDIA RTX 4090",50,Some(123)).unwrap();
            assert_eq!(p["status"],"request_contract_preview_only");
            assert_eq!(p["credential_accessed"],false);
            assert_eq!(p["network_request_sent"],false);
            assert_eq!(p["execution_enabled"],false);
            assert_eq!(p["fresh_human_deployment_approval_required"],true);
        }
    }
    #[test]
    fn runpod_and_vast_use_official_stop_not_termination() {
        let p=preview("runpod-pods",&image(),"NVIDIA RTX 4090",50,None).unwrap();
        assert_eq!(p["proposed_create_operation"]["url"],"https://api.runpod.io/v2/pods");
        assert_eq!(p["proposed_stop_or_idle_policy"]["body"]["action"],"stop");
        let v=preview("vast-instances",&image(),"NVIDIA RTX 4090",50,Some(123)).unwrap();
        assert_eq!(v["proposed_create_operation"]["method"],"PUT");
        assert_eq!(v["proposed_stop_or_idle_policy"]["body"]["state"],"stopped");
        assert!(preview("vast-instances",&image(),"NVIDIA RTX 4090",50,None).is_err());
    }
    #[test]
    fn serverless_defaults_to_zero_workers_and_lambda_refuses_implicit_termination() {
        let s=preview("runpod-serverless",&image(),"ADA_24",50,None).unwrap();
        assert_eq!(s["proposed_create_operation"]["body"]["workers"]["min"],0);
        let body=&s["proposed_create_operation"]["body"];
        assert_eq!(body["workers"]["idleTimeout"],5);
        assert_eq!(body["scaling"]["queueDelay"],4);
        assert_eq!(body["type"],"QUEUE");
        assert_eq!(body["flashboot"],"OFF");
        assert!(body.get("templateId").is_some_and(Value::is_null));
        assert!(body.get("image").is_none());
        let l=preview("lambda-cloud",&image(),"gpu_1x_h100",50,None).unwrap();
        assert_eq!(l["proposed_stop_or_idle_policy"]["auto_terminate"],false);
        assert_eq!(l["proposed_stop_or_idle_policy"]["lifecycle"],"STOP_UNSUPPORTED_ACTION_REQUIRED");
    }
    #[test]
    fn reject_unknown_image_digest_shell_injection_and_invalid_disk() {
        assert!(preview("modal","python:latest","H100",50,None).is_err());
        assert!(preview("modal",&image(),"H100; rm -rf /",50,None).is_err());
        assert!(preview("modal",&image(),"H100",0,None).is_err());
        assert!(preview("nonexistent",&image(),"H100",50,None).is_err());
    }
}
