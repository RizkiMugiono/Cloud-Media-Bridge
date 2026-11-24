//! Plan-bound, non-mutating provider request drafts and capability checks.
use serde_json::{json,Value};
fn label(v:&Value)->bool{
    v.as_str().is_some_and(|s|!s.is_empty()&&s.len()<=128&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_./ ".contains(&b)))
}
fn num(v:&Value,max:u64)->bool{v.as_u64().is_some_and(|n|n>0&&n<=max)}
pub(crate) fn validate_parameters(id:&str,v:&Value)->Result<(),String>{
    if v.is_null(){return Ok(());}
    let fields=v.as_object().ok_or("provider_parameters must be an object")?;
    let keys:&[&str]=match id{
        "runpod-pods"=>&["cloud"],
        "runpod-serverless"=>&["template_id","gpu_pool","min_workers","max_workers","idle_seconds","queue_delay_seconds"],
        "modal"=>&["min_workers","max_workers","idle_seconds","cpu_cores","memory_mib","compute_region_evidence"],
        "vast-instances"=>&["offer_region_evidence"],
        "vast-serverless"=>&["template_hash","gpu_filter_evidence","cold_workers","max_workers"],
        "lambda-cloud"=>&["instance_type","ssh_key_name","base_image_id","installer_digest"],
        "generic-https"=>&["worker_origin_ref","ownership_scope"],
        "generic-ssh"=>&["ssh_profile","host_key_fingerprint","installer_digest","ownership_scope"],
        "kubernetes-gpu"=>&["context","namespace","gpu_resource","gpu_node_label","gpu_node_value","zone_node_label"],
        _=>return Err("Unknown provider".into()),
    };
    for (k,v) in fields{
        if !keys.contains(&k.as_str()){return Err(format!("Unknown or unsafe provider parameter {k}"));}
        if matches!(k.as_str(),"min_workers"|"cold_workers"){
            if v.as_u64().is_none_or(|n|n>64){return Err(format!("Invalid {k}"));}
        }else if matches!(k.as_str(),"max_workers"|"idle_seconds"|"queue_delay_seconds"|"cpu_cores"|"memory_mib"){
            let max=match k.as_str(){"memory_mib"=>4_194_304,"idle_seconds"=>86_400,_=>1024};
            if !num(v,max){return Err(format!("Invalid {k}"));}
        }else if !label(v){return Err(format!("Invalid {k}"));}
    }
    if id=="runpod-pods"&&!matches!(v["cloud"].as_str(),Some("SECURE"|"COMMUNITY")){return Err("Select exact RunPod cloud tier".into());}
    Ok(())
}
fn require(p:&Value,keys:&[&str],gaps:&mut Vec<String>){
    for k in keys{if p[*k].is_null(){gaps.push(format!("Specify provider parameter {k}"));}}
}
pub(crate) fn review(raw:&Value)->Result<Value,String>{
    let reviewed=crate::planning::review(raw)?;
    let plan=&reviewed["plan"];
    let id=plan["provider"].as_str().ok_or("Missing provider")?;
    let p=&plan["provider_parameters"];
    let gpu=plan["gpu"].as_str().ok_or("Missing GPU")?;
    let region=plan["region"].as_str().unwrap_or("");
    let qty=plan["quantity"].as_u64();
    let action=plan["stop_policy"]["action"].as_str().unwrap_or("");
    let mode=plan["stop_policy"]["mode"].as_str().unwrap_or("");
    let name=format!("cmb-{}",plan["request_id"].as_str().unwrap_or("review-only"));
    let mut gaps=Vec::new();
    if !label(&plan["region"])||!num(&plan["quantity"],64){gaps.push("Specify exact region and GPU count".into());}
    if plan["request_id"].is_null(){gaps.push("Prepare unique deployment plan before authorization".into());}
    let mut env=json!({"CMB_DEPLOYMENT_REQUEST_ID":plan["request_id"],"CMB_MODEL_ID":plan["model_id"],
        "CMB_MODEL_SHA":plan["model_sha"],"CMB_TASK":plan["task"],"CMB_RUNTIME_FAMILY":plan["runtime_family"]});
    if !plan["runtime_release_sha256"].is_null(){env["CMB_RUNTIME_RELEASE_SHA256"]=plan["runtime_release_sha256"].clone();}
    let profile=&plan["provider_profile"];
    // Recipe-only proposals let the agent use current upstream runtimes on
    // VM/SSH/SDK providers or build a pinned image before an OCI deployment.
    // Do not emit an executable native request with a missing image.
    let draft=if plan["image"].is_null(){
        gaps.push("Review the exact upstream recipe, base image and dependencies; if this provider requires OCI, build and pin the resulting image then prepare a fresh plan".into());
        json!({"transport":"official_agent_handoff","native_interface":profile["transport"],
            "runtime_recipe":plan["runtime_recipe"],"model":{"id":plan["model_id"],"revision_sha":plan["model_sha"],"task":plan["task"],"family":plan["runtime_family"]},
            "allocation":{"gpu":gpu,"quantity":qty,"region":region,"disk_gb":plan["disk_gb"],"offer_id":plan["offer_id"],"account":plan["account"],"workspace":plan["workspace"]},
            "provider_parameters":p,"resource_name":name,"selected_lifecycle":plan["stop_policy"],
            "steps":["Inspect the pinned upstream recipe and dependencies without running untrusted model code","Use provider-compatible native runtime; pin any generated image and renew approval when plan changes","Verify exact provider operation and obtain explicit approval before paid dispatch"]})
    }else{match id{
        "runpod-pods"=>{
            require(p,&["cloud"],&mut gaps);
            if !matches!(action,"stop"|"terminate"){gaps.push("Pod does not imply suspend or scale-to-zero".into());}
            json!({"transport":"rest_draft","method":"POST","url":"https://api.runpod.io/v2/pods",
                "body":{"name":name,"image":plan["image"],"gpu":{"id":gpu,"count":qty},"dataCenterIds":[region],
                    "cloud":p["cloud"],"disk":plan["disk_gb"],"startSsh":false,"globalNetworking":false,"env":env},
                "secret_env_names":["CMB_WORKER_TOKEN"],"credential_env":"RUNPOD_API_KEY"})
        },
        "runpod-serverless"=>{
            require(p,&["template_id","gpu_pool","min_workers","max_workers","idle_seconds","queue_delay_seconds"],&mut gaps);
            if action=="scale_to_zero"&&p["min_workers"]!=0{gaps.push("Scale-to-zero requires zero minimum workers".into());}
            if !matches!(action,"scale_to_zero"|"terminate"){gaps.push("Serverless does not support Pod stop".into());}
            gaps.push("Verify exact GPU mapping and queue-protocol template digest".into());
            json!({"transport":"rest_draft","method":"POST","url":"https://api.runpod.io/v2/serverless",
                "body":{"name":name,"type":"QUEUE","templateId":p["template_id"],"gpu":{"pools":[p["gpu_pool"]],"count":qty},
                "workers":{"min":p["min_workers"],"max":p["max_workers"],"idleTimeout":p["idle_seconds"]},
                "scaling":{"type":"QUEUE_DELAY","queueDelay":p["queue_delay_seconds"]},"dataCenterIds":[region]},
                "credential_env":"RUNPOD_API_KEY"})
        },
        "modal"=>{
            require(p,&["min_workers","max_workers","idle_seconds","cpu_cores","memory_mib","compute_region_evidence"],&mut gaps);
            if gpu.contains(' '){gaps.push("Use exact Modal GPU identifier".into());}
            if action=="scale_to_zero"&&p["min_workers"]!=0{gaps.push("Scale-to-zero requires zero minimum containers".into());}
            if !matches!(action,"scale_to_zero"|"terminate"){gaps.push("Modal Server does not imply instance stop".into());}
            gaps.push("Ingress region is not compute-placement proof; verify actual constraints".into());
            let mut args=vec![json!("server"),json!("deploy"),json!("--image"),plan["image"].clone(),
                json!("--port"),json!("8000"),json!("--gpu"),json!(format!("{gpu}:{}",qty.unwrap_or(0))),
                json!("--app-name"),json!(name),json!("--env"),plan["workspace"].clone(),json!("--region"),json!(region),
                json!("--secret-from-env"),json!("CMB_WORKER_TOKEN")];
            for (flag,key) in [("--min-containers","min_workers"),("--max-containers","max_workers"),("--scaledown-window","idle_seconds"),("--cpu","cpu_cores"),("--memory","memory_mib")]{
                if let Some(n)=p[key].as_u64(){args.extend([json!(flag),json!(n.to_string())]);}
            }
            for (k,v) in env.as_object().ok_or("Invalid model environment")?{if let Some(s)=v.as_str(){args.extend([json!("--env-var"),json!(format!("{k}={s}"))]);}}
            json!({"transport":"official_cli_draft","executable":"modal","arguments":args,
                "require_authenticated_proxy":true,"never_pass_yes_or_unauthenticated":true})
        },
        "vast-instances"=>{
            require(p,&["offer_region_evidence"],&mut gaps);
            if !matches!(action,"stop"|"terminate"){gaps.push("Instance lifecycle is not serverless scaling".into());}
            json!({"transport":"rest_draft","method":"PUT","url":format!("https://cloud.vast.ai/api/v0/asks/{}/",plan["offer_id"].as_u64().ok_or("Exact Vast offer ID required")?),
                "body":{"image":plan["image"],"disk":plan["disk_gb"],"label":name,"env":env},
                "credential_env":"VAST_API_KEY","secret_env_names":["CMB_WORKER_TOKEN"],
                "preflight":"Read exact offer and verify price, region, GPU, count and rental type"})
        },
        "vast-serverless"=>{
            require(p,&["template_hash","gpu_filter_evidence","cold_workers","max_workers"],&mut gaps);
            if !matches!(action,"scale_to_zero"|"terminate"){gaps.push("Select provider-native serverless lifecycle".into());}
            gaps.push("Verify current endpoint/workergroup schema, template contract and GPU filter".into());
            json!({"transport":"official_agent_handoff","steps":["Validate exact template and GPU filter","Create uniquely named endpoint","Create workergroup using template and limits","Verify worker identity and autoscaling"],
                "endpoint_name":name,"template_hash":p["template_hash"],"cold_workers":p["cold_workers"],"max_workers":p["max_workers"]})
        },
        "lambda-cloud"=>{
            require(p,&["instance_type","ssh_key_name","base_image_id","installer_digest"],&mut gaps);
            if action!="terminate"{gaps.push("Lambda termination is distinct from stop; separate destructive consent required".into());}
            gaps.push("Verify actual instance type GPU count, VM image and pinned installer".into());
            json!({"transport":"rest_draft","method":"POST","url":"https://cloud.lambda.com/api/v1/instance-operations/launch",
                "body":{"name":name,"region_name":region,"instance_type_name":p["instance_type"],"ssh_key_names":[p["ssh_key_name"]],
                    "image":{"id":p["base_image_id"]},"workspace_id":plan["workspace"]},
                "credential_env":"LAMBDA_API_KEY","approved_installer_digest":p["installer_digest"],"worker_image":plan["image"]})
        },
        "generic-https"=>{
            require(p,&["worker_origin_ref","ownership_scope"],&mut gaps);
            gaps.push("Existing HTTPS worker is not permission to manage underlying GPU".into());
            json!({"transport":"existing_https","worker_origin_ref":p["worker_origin_ref"],"steps":["Resolve trusted configured origin","Authenticate worker and deployment/model identity","Use approved input digest and verify output"]})
        },
        "generic-ssh"=>{
            require(p,&["ssh_profile","host_key_fingerprint","installer_digest","ownership_scope"],&mut gaps);
            gaps.push("Existing SSH host is not permission to power off or destroy it".into());
            json!({"transport":"trusted_ssh_profile","ssh_profile":p["ssh_profile"],"host_key_fingerprint":p["host_key_fingerprint"],
                "installer_digest":p["installer_digest"],"image":plan["image"],"environment":env})
        },
        "kubernetes-gpu"=>{
            require(p,&["context","namespace","gpu_resource","gpu_node_label","gpu_node_value","zone_node_label"],&mut gaps);
            if p["gpu_node_label"]==p["zone_node_label"]{gaps.push("Distinct GPU and zone selector keys required".into());}
            gaps.push("Verify node labels, GPU device plugin, admission/RBAC and separate node scale-down".into());
            let mut selector=json!({});if let Some(k)=p["gpu_node_label"].as_str(){selector[k]=p["gpu_node_value"].clone();}
            if let Some(k)=p["zone_node_label"].as_str(){selector[k]=json!(region);}
            let mut limits=json!({});if let Some(k)=p["gpu_resource"].as_str(){limits[k]=json!(qty);}
            json!({"transport":"kubernetes_manifest_draft","context":p["context"],
                "manifest":{"apiVersion":"v1","kind":"Pod","metadata":{"name":name,"namespace":p["namespace"],"labels":{"cmb-owner":plan["request_id"]}},
                "spec":{"restartPolicy":"Never","automountServiceAccountToken":false,"nodeSelector":selector,
                "containers":[{"name":"media-worker","image":plan["image"],"resources":{"limits":limits},
                    "env":env.as_object().ok_or("Invalid model environment")?.iter().map(|(k,v)|json!({"name":k,"value":v})).collect::<Vec<_>>(),
                    "securityContext":{"allowPrivilegeEscalation":false}}]}},"secret_injection_requires_trusted_operator":true})
        },
        _ if !profile.is_null()=>{
            gaps.push("Agent must verify the user-selected provider and official native request schema, exact offer, runtime and actual lifecycle capability".into());
            json!({"transport":"official_agent_handoff","native_interface":profile["transport"],
                "allocation":{"gpu":gpu,"quantity":qty,"region":region,"disk_gb":plan["disk_gb"],"offer_id":plan["offer_id"],"account":plan["account"],"workspace":plan["workspace"]},
                "runtime":{"image":plan["image"],"recipe":plan["runtime_recipe"],"model_id":plan["model_id"],"model_sha":plan["model_sha"],"task":plan["task"],"family":plan["runtime_family"]},
                "environment":env,"resource_name":name,"selected_lifecycle":plan["stop_policy"],
                "steps":["Read current official API or CLI contract","Construct exact provider-native operation and show it to user","After host approval dispatch with user-owned credentials","Record exact resource and verify allocation, identity, inference and cleanup"]})
        },
        _=>return Err("Unknown provider".into()),
    }};
    if p["min_workers"].as_u64().zip(p["max_workers"].as_u64()).is_some_and(|(a,b)|a>b){gaps.push("Minimum workers exceeds maximum".into());}
    Ok(json!({"plan_sha256":reviewed["plan_sha256"],"provider":id,"selected_gpu":gpu,"region":region,"quantity":qty,
        "create_draft":draft,"selected_lifecycle":{"mode":mode,"action":action},"provider_review_items":gaps,
        "cost_review_items":reviewed["advice"]["review_items"],"official_sources":crate::provider_profile::sources(id,profile)?,
        "steps":["Recheck current quote, exact allocation and runtime","Obtain explicit user approval through agent host or provider UI","Bind consent to the exact plan and native operation","Dispatch only reviewed native operation","Record resource ID and verify ownership/health","Use separately authorized media operation","Verify artifacts and reconcile lifecycle and invoice"],
        "provider_operation_sent":false,"approval_granted":false,"production_ready":false}))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn all_provider_modes_return_nonexecuting_handoff(){
        let sample:Value=serde_json::from_str(include_str!("../../../examples/cloud-deployment-manifest.NOT-EXECUTABLE.json")).unwrap();
        for id in ["modal","runpod-pods","runpod-serverless","vast-instances","vast-serverless","lambda-cloud","generic-https","generic-ssh","kubernetes-gpu"]{
            let mut p=sample.clone();p["provider"]=json!(id);p["billing_quote"]["provider"]=json!(id);
            p["provider_parameters"]=Value::Null;p["offer_id"]=json!(123);p["quantity"]=json!(2);
            let h=review(&p).unwrap();assert_eq!(h["provider"],id);assert_eq!(h["quantity"],2);
            assert_eq!(h["provider_operation_sent"],false);assert_eq!(h["approval_granted"],false);assert_eq!(h["production_ready"],false);
            if id=="runpod-pods"{assert_eq!(h["create_draft"]["body"]["gpu"]["count"],2);}
            if id=="modal"{assert!(!h["create_draft"]["arguments"].as_array().unwrap().contains(&json!("--unauthenticated")));}
        }
        assert!(validate_parameters("runpod-pods",&json!({"token":"secret"})).is_err());
        assert!(validate_parameters("modal",&json!({"max_workers":0})).is_err());
    }
}
