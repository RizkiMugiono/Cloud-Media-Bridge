use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

const BINARY: &str = env!("CARGO_BIN_EXE_cloud-media-bridge");
const INIT: &str = r#"{"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"offline-test","version":"1"}}}"#;
const READY: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const LIST: &str = r#"{"jsonrpc":"2.0","id":9,"method":"tools/list"}"#;

fn exchange(lines: &[&str]) -> Vec<Value> {
    let mut child = Command::new(BINARY)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for line in lines {
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let reply: Value = serde_json::from_str(line).unwrap();
            assert_eq!(reply["jsonrpc"], "2.0");
            reply
        })
        .collect()
}

#[test]
fn h01_list_and_call_local_doctor() {
    let replies = exchange(&[
        INIT,
        READY,
        LIST,
        r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"doctor","arguments":{}}}"#,
    ]);
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["id"], "init");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(replies[0]["result"]["capabilities"]["tools"], json!({}));
    assert_eq!(replies[1]["id"], 9);
    let tools = replies[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 27);
    assert_eq!(tools[0]["name"], "doctor");
    assert_eq!(tools[0]["inputSchema"]["type"], "object");
    assert_eq!(replies[2]["id"], 10);
    let doctor = Command::new(BINARY).arg("doctor").output().unwrap();
    assert!(doctor.status.success());
    let text = String::from_utf8(doctor.stdout).unwrap();
    assert!(text.contains(std::env::consts::OS));
    assert_eq!(replies[2]["result"]["content"][0]["text"], text.trim_end());
}

#[test]
fn h02_invalid_requests_leave_session_usable() {
    let replies = exchange(&[
        "{",
        LIST,
        INIT,
        LIST,
        READY,
        r#"{"jsonrpc":"2.0","id":4,"method":"unknown"}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"doctor","arguments":{"approved":true}}}"#,
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"generate"}}"#,
        LIST,
    ]);
    assert_eq!(replies.len(), 8);
    for (index, code) in [
        (0, -32700),
        (1, -32002),
        (3, -32002),
        (4, -32601),
        (5, -32602),
        (6, -32602),
    ] {
        assert_eq!(replies[index]["error"]["code"], code);
    }
    assert_eq!(replies[7]["result"]["tools"][0]["name"], "doctor");
    assert!(replies[7]["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "models_search"));
}

#[test]
fn provider_listing_is_read_only_and_does_not_need_credentials() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"providers_list","arguments":{}}}"#,
    ]);
    let data = &replies[1]["result"]["structuredContent"];
    let providers = data["providers"].as_array().unwrap();
    assert!(providers.len() >= 7);
    assert!(providers.iter().any(|provider| provider["id"] == "modal"));
    assert!(providers.iter().any(|provider| provider["id"] == "runpod-serverless"));
    assert!(providers.iter().any(|provider| provider["id"] == "vast-instances"));
}

#[test]
fn invalid_model_url_and_missing_plan_args_fail_safely() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":"models_inspect","arguments":{"model":"http://169.254.169.254/secret"}}}"#,
        r#"{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"plan_generation","arguments":{"model":"author/model"}}}"#,
        r#"{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{"name":"models_search","arguments":{"query":"test","limit":9999}}}"#,
    ]);
    assert_eq!(replies[1]["result"]["isError"], true);
    assert_eq!(replies[2]["error"]["code"], -32602);
    assert_eq!(replies[3]["error"]["code"], -32602);
}

#[test]
fn deployment_policy_mcp_is_read_only_and_user_decisions_are_visible() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":40,"method":"tools/call","params":{"name":"deployment_policy","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"deployment_policy","arguments":{"approve":true}}}"#,
    ]);
    let policy = &replies[1]["result"]["structuredContent"];
    assert_eq!(policy["implementation_status"], "documented_contract_not_enforced");
    assert_eq!(policy["provider_development"]["strategy"], "parallel_workstreams");
    assert_eq!(policy["approval"]["grants_reusable_for_deployment"], false);
    assert_eq!(
        policy["resource_lifecycle"]["pod_vm_instance"]["auto_destroy"],
        false
    );
    assert_eq!(replies[2]["error"]["code"], -32602);
}

#[test]
fn media_task_catalog_covers_all_modality_families_without_enabling_gpu() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":50,"method":"tools/call","params":{"name":"media_tasks_list","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":51,"method":"tools/call","params":{"name":"media_tasks_list","arguments":{"deploy":true}}}"#,
    ]);
    let catalog = &replies[1]["result"]["structuredContent"];
    assert_eq!(catalog["implementation_status"], "classification_only_no_gpu_runtime");
    for category in ["image", "video", "audio", "speech", "music", "three_d", "spatial", "document", "multimodal"] {
        assert!(catalog["categories"].as_array().unwrap().iter().any(|c| c == category));
    }
    assert_eq!(replies[2]["error"]["code"], -32602);
}

#[test]
fn hub_search_requires_query_or_task_and_never_accepts_approval_flag() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":52,"method":"tools/call","params":{"name":"models_search","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":53,"method":"tools/call","params":{"name":"models_search","arguments":{"task":"audio-to-audio","approved":true}}}"#,
        r#"{"jsonrpc":"2.0","id":54,"method":"tools/call","params":{"name":"models_search","arguments":{"task":"audio-to-audio","limit":0}}}"#,
    ]);
    assert_eq!(replies[1]["error"]["code"], -32602);
    assert_eq!(replies[2]["error"]["code"], -32602);
    assert_eq!(replies[3]["error"]["code"], -32602);
}

#[test]
fn runtime_registry_is_read_only_and_lists_both_delivery_routes() {
    let replies = exchange(&[
        INIT,
        READY,
        r#"{"jsonrpc":"2.0","id":60,"method":"tools/call","params":{"name":"runtimes_list","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":61,"method":"tools/call","params":{"name":"runtimes_list","arguments":{"approved":true}}}"#,
        r#"{"jsonrpc":"2.0","id":62,"method":"tools/call","params":{"name":"runtime_resolve","arguments":{"provider":"modal"}}}"#,
    ]);
    let reg = &replies[1]["result"]["structuredContent"];
    assert_eq!(reg["implementation_status"], "read_only_candidates_no_deployment");
    let entries = reg["entries"].as_array().unwrap();
    assert!(entries.len() >= 16);
    assert!(entries.iter().any(|entry| entry["family"] == "diffusers"));
    assert!(entries.iter().any(|entry| entry["family"] == "transformers"));
    assert!(entries.iter().any(|entry| entry["family"] == "audiocraft"));
    assert!(entries.iter().all(|entry| entry["execution_enabled"] == false));
    assert_eq!(replies[2]["error"]["code"], -32602);
    assert_eq!(replies[3]["error"]["code"], -32602);
}

#[test]
fn runtime_registry_cli_never_calls_cloud_providers() {
    let output = Command::new(BINARY).arg("runtimes").output().unwrap();
    assert!(output.status.success());
    let registry: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(registry["implementation_status"], "read_only_candidates_no_deployment");
    let entries = registry["entries"].as_array().unwrap();
    assert!(entries.iter().any(|entry| entry["delivery_route"] == "verified_prebuilt_oci_image"));
    assert!(entries.iter().any(|entry| entry["delivery_route"] == "vetted_runtime_recipe_on_base_image"));
}

fn new_fixture_directory() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let epoch=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir()
        .join(format!("cmb-mcp-e2e-{}-{epoch}",std::process::id()))
        .to_string_lossy()
        .into_owned()
}

#[test]
fn mcp_offline_e2e_runs_mock_provider_worker_artifact_sha_and_owned_resource_stop() {
    let dir=new_fixture_directory();
    let demo=json!({"jsonrpc":"2.0","id":70,"method":"tools/call",
        "params":{"name":"offline_fixture_demo","arguments":{
           "directory":dir,"kind":"music","confirm_fixture_only":true
        }}});
    let status=json!({"jsonrpc":"2.0","id":71,"method":"tools/call",
        "params":{"name":"offline_fixture_status","arguments":{"directory":dir}}});
    let verify=json!({"jsonrpc":"2.0","id":72,"method":"tools/call",
        "params":{"name":"offline_fixture_verify","arguments":{"directory":dir}}});
    let d=demo.to_string();let s=status.to_string();let v=verify.to_string();
    let replies=exchange(&[INIT,READY,&d,&s,&v]);
    let result=&replies[1]["result"]["structuredContent"];
    assert_eq!(result["simulation_only"],true);
    assert_eq!(result["real_ai_inference"],false);
    assert_eq!(result["auto_stop_confirmed"],true);
    assert_eq!(result["result"]["artifacts"][0]["kind"],"music");
    let state=&replies[2]["result"]["structuredContent"];
    assert_eq!(state["journal"]["approval"]["origin"],"test_fixture_only");
    assert_eq!(state["journal"]["approval"]["consumed"],true);
    assert_eq!(state["provider_observation"]["stop_calls"],1);
    assert_eq!(replies[3]["result"]["structuredContent"]["artifact_verified"],true);
    let _=std::fs::remove_dir_all(dir);
}

#[test]
fn mcp_offline_demo_does_not_accept_unapproved_or_unknown_actions() {
    let dir=new_fixture_directory();
    let cases=[
        json!({"jsonrpc":"2.0","id":80,"method":"tools/call",
               "params":{"name":"offline_fixture_demo","arguments":{"directory":dir,"kind":"image"}}}),
        json!({"jsonrpc":"2.0","id":81,"method":"tools/call",
               "params":{"name":"offline_fixture_demo","arguments":{"directory":dir,"kind":"image","confirm_fixture_only":false}}}),
        json!({"jsonrpc":"2.0","id":82,"method":"tools/call",
               "params":{"name":"offline_fixture_demo","arguments":{"directory":dir,"kind":"exec-shell","confirm_fixture_only":true}}}),
    ];
    let strings=cases.iter().map(Value::to_string).collect::<Vec<_>>();
    let replies=exchange(&[INIT,READY,&strings[0],&strings[1],&strings[2]]);
    for reply in replies.iter().skip(1) {
        assert_eq!(reply["error"]["code"],-32602);
    }
    assert!(!std::path::Path::new(&dir).join("journal.json").exists());
}

#[test]
fn piped_cli_cannot_issue_fake_human_approval() {
    let dir=new_fixture_directory();
    let prepare=Command::new(BINARY).args(["offline","prepare",&dir,"image"]).output().unwrap();
    assert!(prepare.status.success());
    let approve=Command::new(BINARY).args(["offline","approve",&dir]).output().unwrap();
    assert!(!approve.status.success());
    let run=Command::new(BINARY).args(["offline","run",&dir]).output().unwrap();
    assert!(!run.status.success());
    assert!(String::from_utf8(run.stderr).unwrap().contains("approval"));
    let _=std::fs::remove_dir_all(dir);
}

#[test]
fn provider_operation_previews_cover_safe_stop_without_network_side_effects() {
    let image=format!("ghcr.io/example/fixture@sha256:{}", "a".repeat(64));
    let request=json!({"jsonrpc":"2.0","id":90,"method":"tools/call",
        "params":{"name":"provider_operation_preview","arguments":{
            "provider":"runpod-pods","image":image,"gpu":"NVIDIA RTX 4090","disk_gb":50
        }}});
    let line=request.to_string();
    let replies=exchange(&[INIT,READY,&line]);
    let p=&replies[1]["result"]["structuredContent"];
    assert_eq!(p["status"],"request_contract_preview_only");
    assert_eq!(p["proposed_stop_or_idle_policy"]["body"]["action"],"stop");
    assert_eq!(p["network_request_sent"],false);
    assert_eq!(p["execution_enabled"],false);
}

#[test]
fn worker_smoke_requires_explicit_fixture_confirmation_and_valid_port() {
    let path=new_fixture_directory();
    let case=json!({"jsonrpc":"2.0","id":95,"method":"tools/call",
        "params":{"name":"offline_worker_smoke","arguments":{
            "directory":path,"port":0,"kind":"image","confirm_fixture_only":true
        }}});
    let reply=exchange(&[INIT,READY,&case.to_string()]);
    assert_eq!(reply[1]["error"]["code"],-32602);
}

#[test]
fn agent_can_prepare_and_inspect_cloud_plan_but_cannot_approve_or_deploy_via_mcp() {
    use std::path::PathBuf;
    let root=std::env::temp_dir().join(format!("cmb-mcp-cloud-{}",std::process::id()));
    let _=std::fs::remove_dir_all(&root);
    let example=PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cloud-deployment-manifest.NOT-EXECUTABLE.json");
    let directory=root.to_string_lossy().to_string();
    let manifest=example.to_string_lossy().to_string();
    // JSON string inputs come from locally generated safe paths, not a model card.
    let prepare=json!({"jsonrpc":"2.0","id":70,"method":"tools/call",
        "params":{"name":"cloud_session_prepare","arguments":{
            "directory":directory,"manifest":manifest,"confirm_no_cloud_charge":true
        }}});
    let status=json!({"jsonrpc":"2.0","id":71,"method":"tools/call",
        "params":{"name":"cloud_session_status","arguments":{"directory":directory}}});
    let forbid=json!({"jsonrpc":"2.0","id":72,"method":"tools/call",
        "params":{"name":"cloud_session_prepare","arguments":{
            "directory":directory,"manifest":manifest,"approved":true
        }}});
    let forbid_deploy=json!({"jsonrpc":"2.0","id":73,"method":"tools/call",
        "params":{"name":"cloud_deploy","arguments":{"directory":directory}}});
    let messages=[INIT.to_string(),READY.to_string(),prepare.to_string(),status.to_string(),
                  forbid.to_string(),forbid_deploy.to_string()];
    let refs=messages.iter().map(String::as_str).collect::<Vec<_>>();
    let replies=exchange(&refs);
    assert_eq!(replies[1]["result"]["structuredContent"]["status"],"awaiting_human_approval");
    assert_eq!(replies[2]["result"]["structuredContent"]["status"],"awaiting_human_approval");
    assert_eq!(replies[2]["result"]["structuredContent"]["approval"]["consumed"],false);
    assert_eq!(replies[2]["result"]["structuredContent"]["stop_policy"]["mode"],"after_job");
    assert_eq!(replies[3]["error"]["code"],-32602);
    assert_eq!(replies[4]["error"]["code"],-32602);
    let _=std::fs::remove_dir_all(root);
}

#[test]
fn billing_quote_mcp_estimates_five_seconds_without_provisioning_gpu() {
    let quote=json!({"schema_version":1,"billing_model":"per_hour",
        "billing_increment_ms":3_600_000,
        "minimum_billable_ms":3_600_000,
        "billable_elapsed_ms":5_000,
        "quoted_micro_usd_per_hour":1_000_000
    });
    let policy=json!({"jsonrpc":"2.0","id":80,"method":"tools/call",
        "params":{"name":"billing_policy","arguments":{}}});
    let request=json!({"jsonrpc":"2.0","id":81,"method":"tools/call",
        "params":{"name":"billing_estimate","arguments":{"quote":quote}}});
    let invalid=json!({"jsonrpc":"2.0","id":82,"method":"tools/call",
        "params":{"name":"billing_estimate","arguments":{"quote":quote,"approved":true}}});
    let replies=exchange(&[INIT,READY,&policy.to_string(),&request.to_string(),&invalid.to_string()]);
    let policy=&replies[1]["result"]["structuredContent"];
    assert_eq!(policy["default_lifecycle"]["wait_until_end_of_paid_period"],false);
    let estimate=&replies[2]["result"]["structuredContent"];
    assert_eq!(estimate["effective_billable_ms"],3_600_000);
    assert_eq!(estimate["estimated_incremental_compute_micro_usd"],1_000_000);
    assert_eq!(estimate["lifecycle_decision"],"not_selected_by_billing");
    assert_eq!(estimate["requires_user_lifecycle_selection"],true);
    assert_eq!(replies[3]["error"]["code"],-32602);
}

#[test]
fn review_and_lifecycle_tools_never_issue_deployment_authorization() {
    let sample: Value = serde_json::from_str(include_str!("../../../examples/cloud-deployment-manifest.NOT-EXECUTABLE.json")).unwrap();
    let review = json!({"jsonrpc":"2.0","id":81,"method":"tools/call",
        "params":{"name":"cloud_review","arguments":{"manifest":sample}}});
    let mut plan = sample;
    plan["request_id"] = json!("one-deployment");
    let advice = json!({"jsonrpc":"2.0","id":82,"method":"tools/call",
        "params":{"name":"lifecycle_advice","arguments":{"plan":plan,"receipt":{},"observed":{}}}});
    let replies=exchange(&[INIT,READY,&review.to_string(),&advice.to_string()]);
    let reviewed=&replies[1]["result"]["structuredContent"];
    assert_eq!(reviewed["approval_granted"],false);
    assert_eq!(reviewed["advice"]["selected_estimate"]["estimated_total_micro_usd"],1889);
    let next=&replies[2]["result"]["structuredContent"];
    assert_eq!(next["bridge_can_execute"],false);
    assert_eq!(next["recommendation"],"ask_user_for_fresh_plan_bound_consent");
}

#[test]
fn live_commands_remain_blocked_even_with_legacy_opt_in() {
    for args in [vec!["cloud","deploy","unused"],vec!["cloud","generate","unused","inputs"],vec!["cloud","stop","unused"]] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_cloud-media-bridge"))
            .args(args).env("CMB_ENABLE_LIVE_GPU","I_ACCEPT_GPU_CHARGES").output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Live GPU execution disabled"));
    }
}

#[test]
fn market_tools_expose_sources_and_never_authorize_gpu() {
    let sources=json!({"jsonrpc":"2.0","id":100,"method":"tools/call","params":{"name":"provider_sources","arguments":{}}});
    let refresh=json!({"jsonrpc":"2.0","id":101,"method":"tools/call","params":{"name":"market_refresh","arguments":{"provider":"modal"}}});
    let check=json!({"jsonrpc":"2.0","id":102,"method":"tools/call","params":{"name":"market_quote_check","arguments":{"quote":{"provider":"modal","pricing_source":"https://modal.com/pricing","retrieved_at_unix_ms":1}}}});
    let bad=json!({"jsonrpc":"2.0","id":103,"method":"tools/call","params":{"name":"market_refresh","arguments":{"provider":"modal","url":"https://evil.invalid"}}});
    let replies=exchange(&[INIT,READY,&sources.to_string(),&refresh.to_string(),&check.to_string(),&bad.to_string()]);
    assert_eq!(replies[1]["result"]["structuredContent"]["providers"].as_object().unwrap().len(),9);
    assert_eq!(replies[2]["result"]["structuredContent"]["gpu_rented"],false);
    assert_eq!(replies[2]["result"]["structuredContent"]["agent_read_only_command"]["arguments"],json!(["billing","rates","--json"]));
    assert_eq!(replies[2]["result"]["structuredContent"]["approval_granted"],false);
    assert_eq!(replies[3]["result"]["structuredContent"]["status"],"refresh_required");
    assert_eq!(replies[4]["error"]["code"],-32602);
}

#[test]
fn retired_custom_authorization_and_watchdog_commands_are_not_exposed(){
    for args in [vec!["authorization","challenge","unused","unused"],vec!["cloud","watch","unused","--local-only"]] {
        let p=std::process::Command::new(env!("CARGO_BIN_EXE_cloud-media-bridge")).args(args).output().unwrap();
        assert!(!p.status.success());
        assert!(String::from_utf8_lossy(&p.stderr).contains("Use --help"));
    }
}

#[test]
fn external_provider_uses_same_plan_cost_and_handoff_without_registration(){
    let profile:Value=serde_json::from_str(include_str!("../../../examples/external-provider.example.json")).unwrap();
    let mut plan:Value=serde_json::from_str(include_str!("../../../examples/cloud-deployment-manifest.NOT-EXECUTABLE.json")).unwrap();
    plan["provider"]=profile["id"].clone();
    plan["provider_profile"]=profile.clone();
    plan["provider_parameters"]=Value::Null;
    plan["billing_quote"]["provider"]=profile["id"].clone();
    plan["billing_quote"]["pricing_source"]=profile["pricing"][0].clone();
    plan["billing_quote"]["retrieved_at_unix_ms"]=json!(1);
    let prof=json!({"jsonrpc":"2.0","id":201,"method":"tools/call","params":{"name":"provider_profile_review","arguments":{"profile":profile}}});
    let review=json!({"jsonrpc":"2.0","id":202,"method":"tools/call","params":{"name":"cloud_review","arguments":{"manifest":plan}}});
    let draft=json!({"jsonrpc":"2.0","id":203,"method":"tools/call","params":{"name":"provider_handoff","arguments":{"manifest":plan}}});
    let replies=exchange(&[INIT,READY,&prof.to_string(),&review.to_string(),&draft.to_string()]);
    assert_eq!(replies[1]["result"]["structuredContent"]["status"],"profile_structure_validated");
    let p=&replies[2]["result"]["structuredContent"];
    assert_eq!(p["advice"]["selected_estimate"]["estimated_total_micro_usd"],1889);
    assert_eq!(p["advice"]["selected_estimate"]["quote_freshness"]["source_recognized"],true);
    let d=&replies[3]["result"]["structuredContent"];
    assert_eq!(d["create_draft"]["transport"],"official_agent_handoff");
    assert_eq!(d["provider_operation_sent"],false);
    assert_eq!(d["approval_granted"],false);
    assert_eq!(d["plan_sha256"],p["plan_sha256"]);
}

#[test]
fn legacy_mcp_negotiates_each_supported_handshake_revision(){
    for version in ["2024-11-05","2025-03-26","2025-06-18","2025-11-25"]{
        let init=INIT.replace("2025-11-25",version);
        let replies=exchange(&[&init,READY,LIST]);
        assert_eq!(replies[0]["result"]["protocolVersion"],version);
        assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(),27);
        assert!(replies[1]["result"].get("ttlMs").is_none());
        assert!(replies[1]["result"].get("cacheScope").is_none());
    }
    let unknown=INIT.replace("2025-11-25","unsupported");
    assert_eq!(exchange(&[&unknown])[0]["result"]["protocolVersion"],"2025-11-25");
}
#[test]
fn modern_stdio_discovery_and_tool_calls_do_not_need_initialize(){
    let meta=json!({"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}});
    let discover=json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":meta}});
    let list=json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":meta}});
    let call=json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"_meta":meta,"name":"doctor","arguments":{}}});
    let bad=json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}});
    let replies=exchange(&[&discover.to_string(),&list.to_string(),&call.to_string(),&bad.to_string()]);
    assert_eq!(replies[0]["result"]["supportedVersions"],json!(["2026-07-28"]));
    assert_eq!(replies[0]["result"]["resultType"],"complete");
    assert_eq!(replies[0]["result"]["ttlMs"],0);
    assert_eq!(replies[0]["result"]["cacheScope"],"private");
    assert_eq!(replies[0]["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],"cloud-media-bridge");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(),27);
    assert_eq!(replies[1]["result"]["ttlMs"],0);
    assert_eq!(replies[1]["result"]["cacheScope"],"private");
    assert!(replies[2]["result"]["structuredContent"]["message"].is_string());
    assert!(replies[2]["result"].get("ttlMs").is_none());
    assert!(replies[2]["result"].get("cacheScope").is_none());
    assert_eq!(replies[3]["error"]["code"],-32022);
    let pinned=exchange(&[&call.to_string()]);
    assert_eq!(pinned[0]["result"]["resultType"],"complete");
    let direct=exchange(&[&list.to_string()]);
    assert_eq!(direct[0]["result"]["resultType"],"complete");
    assert_eq!(direct[0]["result"]["ttlMs"],0);
    assert_eq!(direct[0]["result"]["cacheScope"],"private");
    assert_eq!(direct[0]["result"]["tools"].as_array().unwrap().len(),27);
}
#[test]
fn cli_tool_dispatch_matches_mcp_and_never_exposes_paid_commands(){
    let root=std::env::temp_dir().join(format!("cmb-cli-tools-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path=root.join("args.json");
    std::fs::write(&path,"{}").unwrap();
    let output=Command::new(BINARY).args(["tools","list"]).output().unwrap();
    assert!(output.status.success());
    let list:Value=serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(list,exchange(&[INIT,READY,LIST])[1]["result"]["tools"]);
    let output=Command::new(BINARY).args(["tools","call","providers_list"]).arg(&path).output().unwrap();
    assert!(output.status.success());
    let cli:Value=serde_json::from_slice(&output.stdout).unwrap();
    let rpc=r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"providers_list","arguments":{}}}"#;
    assert_eq!(cli,exchange(&[INIT,READY,rpc])[1]["result"]["structuredContent"]);
    let bad=Command::new(BINARY).args(["tools","call","cloud_deploy"]).arg(&path).output().unwrap();
    assert!(!bad.status.success());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_recipe_plan_prepare_and_lifecycle_stay_bound_to_same_digest(){
    let mut plan:Value=serde_json::from_str(include_str!("../../../examples/external-deployment-manifest.NOT-EXECUTABLE.json")).unwrap();
    let root=std::env::temp_dir().join(format!("cmb-external-plan-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let manifest=root.join("manifest.json");
    std::fs::write(&manifest,plan.to_string()).unwrap();
    let session=root.join("session");
    let output=Command::new(BINARY).args(["cloud","prepare"]).arg(&session).arg(&manifest).output().unwrap();
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let prepared:Value=serde_json::from_slice(&output.stdout).unwrap();
    plan=prepared["plan"].clone();
    let receipt=json!({"request_id":plan["request_id"],"plan_sha256":prepared["plan_sha256"],"end_user_approved":true,"resource_id":"r1"});
    let obs=json!({"provider":plan["provider"],"account":plan["account"],"workspace":plan["workspace"],"owner_request_id":plan["request_id"],"resource_id":"r1",
        "verified_owned":true,"shared":false,"provider_state":"running","active_jobs":0,"active_transfers":0,"elapsed_ms":100,"exposure_micro_usd":20,"supported_action":"stop","final_job_terminal":true});
    let draft=json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"provider_handoff","arguments":{"manifest":plan}}});
    let lifecycle=json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"lifecycle_action","arguments":{"plan":plan,"receipt":receipt,"observed":obs}}});
    let replies=exchange(&[INIT,READY,&draft.to_string(),&lifecycle.to_string()]);
    let d=&replies[1]["result"]["structuredContent"];
    assert_eq!(d["plan_sha256"],prepared["plan_sha256"]);
    assert_eq!(d["create_draft"]["runtime_recipe"],plan["runtime_recipe"]);
    assert_eq!(d["provider_operation_sent"],false);
    let l=&replies[2]["result"]["structuredContent"];
    assert_eq!(l["action_draft"]["transport"],"official_agent_handoff");
    assert_eq!(l["approval_granted"],false);
    let mut changed=plan.clone();changed["provider_profile"]["name"]=json!("changed profile");
    let altered=json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"lifecycle_action","arguments":{"plan":changed,"receipt":receipt,"observed":obs}}});
    let reply=exchange(&[INIT,READY,&altered.to_string()]);
    assert_eq!(reply[1]["result"]["structuredContent"]["advice"]["recommendation"],"ask_user_for_fresh_plan_bound_consent");
    assert!(reply[1]["result"]["structuredContent"]["action_draft"].is_null());
    std::fs::remove_dir_all(root).unwrap();
}
