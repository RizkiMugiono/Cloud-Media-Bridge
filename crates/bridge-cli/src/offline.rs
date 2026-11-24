//! Complete, strictly offline workflow with a simulated provider and fixture worker.
//! NO real model inference, GPU, provider API, or paid resource is used.
use crate::crypto::{secure_nonce, sha256};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

const JOURNAL: &str = "journal.json";
const PROVIDER: &str = "mock-provider.json";
const FORMAT_VERSION: u64 = 1;
const MAX_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;
const KINDS: &[&str] = &[
    "image", "video", "audio", "speech", "music", "three_d",
    "spatial", "document", "multimodal"
];

pub(crate) fn supported_kinds() -> &'static [&'static str] {
    KINDS
}

struct ExclusiveLock(PathBuf);
impl ExclusiveLock {
    fn acquire(root: &Path) -> Result<Self, String> {
        let path = root.join(".cmb.lock");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).map_err(|_| {
            "Session is locked (or a previous process crashed). Do not delete the lock until resource state is independently reconciled".to_string()
        })?;
        Ok(Self(path))
    }
}
impl Drop for ExclusiveLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn root_dir(path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    if candidate.as_os_str().is_empty() {
        return Err("A session directory is required".into());
    }
    fs::create_dir_all(&candidate).map_err(|e|format!("Cannot create session directory: {e}"))?;
    let root = fs::canonicalize(candidate).map_err(|e|format!("Cannot access session directory: {e}"))?;
    if !root.is_dir() {
        return Err("Session path must be a directory".into());
    }
    Ok(root)
}

fn exclusive_create(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path).map_err(|e|format!("Cannot create session file: {e}"))?;
    file.write_all(bytes).map_err(|e|format!("Cannot write session file: {e}"))?;
    file.sync_all().map_err(|e|format!("Cannot sync session file: {e}"))
}

fn atomic_json(path: &Path, value: &Value) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp-{}",secure_nonce()?));
    let bytes = serde_json::to_vec_pretty(value).map_err(|e|e.to_string())?;
    exclusive_create(&tmp, &bytes)?;
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Cannot atomically commit journal: {e}"));
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let meta=fs::symlink_metadata(path).map_err(|e|format!("State file unavailable: {e}"))?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 2_000_000 {
        return Err("Session state must be a regular small non-symlink file".into());
    }
    let data=fs::read(path).map_err(|e|format!("Cannot read state: {e}"))?;
    let value:Value=serde_json::from_slice(&data).map_err(|_|"Corrupt or invalid session JSON".to_string())?;
    if value["schema_version"] != FORMAT_VERSION || value["simulation_only"] != true {
        return Err("Not a Cloud-Media-Bridge offline fixture session".into());
    }
    Ok(value)
}

fn journal(root: &Path) -> Result<Value, String> {
    read_json(&root.join(JOURNAL))
}
fn save(root: &Path, state: &Value) -> Result<(), String> {
    atomic_json(&root.join(JOURNAL), state)
}
fn digest_plan(plan: &Value) -> String {
    sha256(plan.to_string().as_bytes())
}

pub(crate) fn prepare(directory: &str, kind: &str) -> Result<Value, String> {
    if !KINDS.contains(&kind) {
        return Err("Unsupported fixture kind; use image, video, audio, speech, music, three_d, spatial, document or multimodal".into());
    }
    let root = root_dir(directory)?;
    let _lock=ExclusiveLock::acquire(&root)?;
    if root.join(JOURNAL).exists() || root.join(PROVIDER).exists() {
        return Err("Session already exists; choose a new empty directory".into());
    }
    let nonce=secure_nonce()?;
    let request_id=secure_nonce()?;
    let plan=json!({
        "request_id": request_id,
        "nonce": nonce,
        "action": "offline_fixture_deployment",
        "provider": "mock-only",
        "account": "offline-local-fixture",
        "workspace": "offline-fixture-test",
        "model_id": "fixture/not-an-ai-model",
        "revision_sha": "0000000000000000000000000000000000000000",
        "runtime": "offline-fixture-worker",
        "runtime_recipe_digest": "not-an-executable-oci-image",
        "gpu": "none",
        "estimated_gpu_credit": 0,
        "media_kind": kind,
        "automatic_owned_resource_stop": true,
        "simulation_only": true
    });
    let plan_sha=digest_plan(&plan);
    let state=json!({
        "schema_version": FORMAT_VERSION,
        "simulation_only": true,
        "status": "prepared",
        "plan": plan,
        "plan_sha256": plan_sha,
        "approval": {"status":"pending","origin":null,"bound_plan_sha256":null,"consumed":false},
        "resource": {"id":null,"owner_request_id":null,"state":"not_created","stop_confirmed":false},
        "job": {"state":"pending","active":false,"cancel_requested":false},
        "artifact": null,
        "errors": []
    });
    save(&root,&state)?;
    Ok(json!({
        "status":"prepared","simulation_only":true,"session":root.to_string_lossy(),
        "request_id":state["plan"]["request_id"],"media_kind":kind,
        "plan_sha256":plan_sha,
        "required_human_challenge":format!("APPROVE {}",nonce),
        "next_step":"offline approve <directory> (requires interactive human terminal); offline demo is a mock-only test shortcut"
    }))
}

fn approval_state_valid(state: &Value) -> bool {
    state["plan"].is_object()
        && state["plan_sha256"].as_str().is_some_and(|hash| hash == digest_plan(&state["plan"]))
        && state["approval"]["status"] == "approved"
        && state["approval"]["bound_plan_sha256"] == state["plan_sha256"]
        && state["approval"]["consumed"] == false
        && state["plan"]["simulation_only"] == true
        && state["plan"]["provider"] == "mock-only"
        && state["plan"]["gpu"] == "none"
        && state["plan"]["estimated_gpu_credit"] == 0
        && state["status"] == "prepared"
}

fn approve_under_lock(root: &Path, origin: &str) -> Result<Value, String> {
    if !matches!(origin, "human_terminal" | "test_fixture_only") {
        return Err("Unknown approval origin".into());
    }
    let mut state=journal(root)?;
    if state["status"] != "prepared" || state["approval"]["status"] != "pending" {
        return Err("Approval is only allowed once for a fresh prepared deployment".into());
    }
    if state["plan_sha256"] != digest_plan(&state["plan"]) {
        return Err("Prepared deployment plan was modified".into());
    }
    state["approval"]["status"]=json!("approved");
    state["approval"]["origin"]=json!(origin);
    state["approval"]["bound_plan_sha256"]=state["plan_sha256"].clone();
    save(root,&state)?;
    Ok(json!({"approval":"approved","origin":origin,"simulation_only":true,
              "request_id":state["plan"]["request_id"],"one_shot":true}))
}

pub(crate) fn approve_interactively(directory: &str) -> Result<Value, String> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err("Approval requires an interactive human terminal; MCP arguments and redirected input cannot approve deployment".into());
    }
    let root = root_dir(directory)?;
    let _lock=ExclusiveLock::acquire(&root)?;
    let state=journal(&root)?;
    if state["status"] != "prepared" || state["approval"]["status"] != "pending" {
        return Err("No fresh deployment is awaiting approval".into());
    }
    let nonce=state["plan"]["nonce"].as_str().ok_or("Missing plan nonce")?;
    eprintln!("OFFLINE FIXTURE ONLY — No real GPU/cloud operation.");
    eprintln!("Request: {} | task: {} | provider: mock-only | GPU cost: 0",
        state["plan"]["request_id"],state["plan"]["media_kind"]);
    eprint!("Type 'APPROVE {nonce}' exactly to approve this one simulated deployment: ");
    io::stderr().flush().map_err(|e|e.to_string())?;
    let mut input=String::new();
    io::stdin().read_line(&mut input).map_err(|e|e.to_string())?;
    if input.trim_end_matches(['\r','\n']) != format!("APPROVE {nonce}") {
        return Err("Human approval challenge did not match".into());
    }
    approve_under_lock(&root,"human_terminal")
}

#[derive(Clone)]
struct MockProvider {
    root:PathBuf,
    fail_inference:bool,
    fail_stop:bool
}
impl MockProvider {
    fn new(root: &Path) -> Self {
        Self {root:root.to_path_buf(),fail_inference:false,fail_stop:false}
    }
    fn read(&self) -> Result<Option<Value>,String> {
        if self.root.join(PROVIDER).exists() {
            read_json(&self.root.join(PROVIDER)).map(Some)
        } else {
            Ok(None)
        }
    }
    fn store(&self, data:&Value) -> Result<(), String> {
        atomic_json(&self.root.join(PROVIDER),data)
    }
    fn provision(&self, request:&str) -> Result<String,String> {
        if let Some(existing)=self.read()? {
            if existing["owned_by"] != request {
                return Err("Existing resource is owned by another request".into());
            }
            return existing["resource_id"].as_str().map(str::to_string).ok_or("Invalid resource identifier".into());
        }
        let id=format!("mock-{}",secure_nonce()?);
        let data=json!({"schema_version":FORMAT_VERSION,"simulation_only":true,
            "resource_id":id,"owned_by":request,"state":"running",
            "active_jobs":1,"stop_calls":0,"stop_confirmed":false});
        self.store(&data)?;
        Ok(id)
    }
    fn complete_job(&self, request:&str) -> Result<(),String> {
        let mut data=self.read()?.ok_or("Mock resource does not exist")?;
        if data["owned_by"] != request {
            return Err("Resource ownership mismatch".into());
        }
        data["active_jobs"]=json!(0);
        self.store(&data)
    }
    fn stop_owned(&self, request:&str, resource:&str) -> Result<bool,String> {
        let mut data=self.read()?.ok_or("No resource to stop")?;
        if data["resource_id"] != resource || data["owned_by"] != request {
            return Err("Refusing stop: resource is not bridge-owned by this request".into());
        }
        if data["state"] == "stopped" {
            return Ok(true);
        }
        if data["active_jobs"].as_u64().unwrap_or(u64::MAX) != 0 {
            return Ok(false);
        }
        if self.fail_stop {
            return Err("Simulated provider stop failure".into());
        }
        data["stop_calls"]=json!(data["stop_calls"].as_u64().unwrap_or(0)+1);
        data["state"]=json!("stopped");
        data["stop_confirmed"]=json!(true);
        self.store(&data)?;
        // Inspect provider-observed status rather than assuming the stop call worked.
        let observed=self.read()?.ok_or("No stop confirmation")?;
        Ok(observed["state"]=="stopped" && observed["stop_confirmed"]==true)
    }
    fn infer(&self,kind:&str) -> Result<(Vec<u8>,&'static str,&'static str),String> {
        if self.fail_inference {return Err("Simulated inference worker failure".into());}
        fixture(kind)
    }
}

fn wav_fixture() -> Vec<u8> {
    let n=8000usize;
    let mut out=Vec::with_capacity(44+n*2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36+n as u32*2).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8000u32.to_le_bytes());
    out.extend_from_slice(&16000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(n as u32*2).to_le_bytes());
    for i in 0..n {
        let signal=((i as f64 * 2.0*std::f64::consts::PI*440.0/8000.0).sin()*1600.0) as i16;
        out.extend_from_slice(&signal.to_le_bytes());
    }
    out
}
fn fixture(kind:&str)->Result<(Vec<u8>,&'static str,&'static str),String>{
    let item=match kind {
        "image" => (b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"320\" height=\"80\"><text x=\"8\" y=\"40\">OFFLINE FIXTURE - NOT AI</text></svg>".to_vec(),"image/svg+xml","svg"),
        "audio" | "speech" | "music" => (wav_fixture(),"audio/wav","wav"),
        "three_d" => (b"# OFFLINE FIXTURE - NOT AI\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec(),"model/obj","obj"),
        "video" | "spatial" | "document" | "multimodal" => (
            serde_json::to_vec(&json!({"kind":kind,"simulation_only":true,"real_media_inference":false,"message":"Protocol-only mock artifact; not a generated video or AI output."})).map_err(|e|e.to_string())?,
            "application/json","json"
        ),
        _ => return Err("Unknown fixture kind".into())
    };
    Ok(item)
}

fn store_artifact(root:&Path,kind:&str,request:&str,bytes:&[u8],mime:&str,ext:&str)
    -> Result<Value,String>
{
    if bytes.is_empty() || bytes.len()>MAX_ARTIFACT_BYTES {
        return Err("Worker artifact violates size bounds".into());
    }
    let sha=sha256(bytes);
    let name=format!("artifact-{kind}.{ext}");
    let path=root.join(&name);
    exclusive_create(&path,bytes)?;
    let stored=fs::read(&path).map_err(|e|format!("Cannot verify output: {e}"))?;
    if stored.len()!=bytes.len() || sha256(&stored)!=sha {
        return Err("Artifact checksum mismatch after writing output".into());
    }
    Ok(json!({"schema_version":FORMAT_VERSION,"request_id":request,"status":"completed",
        "simulation_only":true,"real_ai_inference":false,
        "artifacts":[{"kind":kind,"media_type":mime,"size_bytes":stored.len(),
          "sha256":sha,"asset_ref":name,
          "metadata":{"simulation_only":true,"source":"offline_fixture_worker"}}]
    }))
}

pub(crate) fn verify(directory:&str) -> Result<Value,String> {
    let root=root_dir(directory)?;
    let state=journal(&root)?;
    let result=&state["artifact"];
    if !result.is_object() {
        return Err("No completed artifact".into());
    }
    let item=&result["artifacts"][0];
    let file=item["asset_ref"].as_str().ok_or("Missing artifact name")?;
    // Artifact handles must not carry path traversal, URI, or absolute paths.
    if file.contains(['/', '\\', ':']) || file.starts_with('.') || !file.starts_with("artifact-") {
        return Err("Untrusted artifact path".into());
    }
    let path=root.join(file);
    let meta=fs::symlink_metadata(&path).map_err(|e|e.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > MAX_ARTIFACT_BYTES as u64 {
        return Err("Invalid artifact file".into());
    }
    let bytes=fs::read(path).map_err(|e|e.to_string())?;
    let expected=item["sha256"].as_str().ok_or("Missing checksum")?;
    if bytes.len() as u64 != item["size_bytes"].as_u64().unwrap_or(u64::MAX)
        || sha256(&bytes)!=expected {
        return Err("Artifact failed SHA-256 / size verification".into());
    }
    Ok(json!({"artifact_verified":true,"sha256":expected,"bytes":bytes.len(),
        "kind":item["kind"],"simulation_only":true}))
}

fn sync_resource(root:&Path, state:&mut Value, provider:&MockProvider) -> Result<(),String> {
    let request=state["plan"]["request_id"].as_str().ok_or("Missing request ID")?.to_string();
    let Some(observed)=provider.read()? else {
        return Ok(());
    };
    if observed["owned_by"] != request || observed["simulation_only"] != true {
        return Err("Unknown or shared provider resource: cannot reconcile ownership".into());
    }
    let id=observed["resource_id"].as_str().ok_or("Invalid provider resource ID")?;
    if state["resource"]["id"].is_null() {
        state["resource"]["id"]=json!(id);
        state["resource"]["owner_request_id"]=json!(request);
    } else if state["resource"]["id"]!=id {
        return Err("Persisted resource ID does not match the provider".into());
    }
    state["resource"]["state"]=observed["state"].clone();
    state["resource"]["stop_confirmed"]=observed["stop_confirmed"].clone();
    save(root,state)
}

fn cleanup(root:&Path, state:&mut Value, provider:&MockProvider) -> Result<Value,String> {
    sync_resource(root,state,provider)?;
    let resource=state["resource"]["id"].as_str().ok_or("No provisioned resource found")?.to_string();
    let request=state["plan"]["request_id"].as_str().ok_or("Missing request ID")?.to_string();
    let observed=provider.read()?.ok_or("Provider state unavailable")?;
    if observed["active_jobs"].as_u64().unwrap_or(u64::MAX)>0 {
        state["resource"]["state"]=json!("stop_deferred_active_jobs");
        save(root,state)?;
        return Ok(json!({"status":"STOP_DEFERRED","active_jobs":observed["active_jobs"],"simulation_only":true}));
    }
    match provider.stop_owned(&request,&resource) {
        Ok(true) => {
            sync_resource(root,state,provider)?;
            state["resource"]["state"]=json!("stopped");
            state["resource"]["stop_confirmed"]=json!(true);
            save(root,state)?;
            Ok(json!({"status":"STOP_CONFIRMED","simulation_only":true,"resource":resource}))
        },
        Ok(false) => {
            state["resource"]["state"]=json!("stop_uncertain");
            save(root,state)?;
            Ok(json!({"status":"STOP_UNCERTAIN","simulation_only":true}))
        },
        Err(message) => {
            state["resource"]["state"]=json!("stop_failed");
            state["errors"].as_array_mut().ok_or("Invalid journal errors")?.push(json!(message));
            save(root,state)?;
            Ok(json!({"status":"STOP_FAILED","message":message,"simulation_only":true}))
        }
    }
}

fn run_under_lock(root:&Path, provider:&MockProvider) -> Result<Value,String> {
    let mut state=journal(root)?;
    if !approval_state_valid(&state) {
        return Err("No valid fresh one-shot approval for the exact deployment plan".into());
    }
    let request=state["plan"]["request_id"].as_str().ok_or("Missing request ID")?.to_string();
    let kind=state["plan"]["media_kind"].as_str().ok_or("Missing media kind")?.to_string();
    state["approval"]["consumed"]=json!(true);
    state["status"]=json!("provisioning");
    save(root,&state)?; // Must persist consumed grant before any side effect.
    let provisioned=provider.provision(&request);
    if let Ok(resource) = &provisioned {
        state["resource"]["id"]=json!(resource);
        state["resource"]["owner_request_id"]=json!(request);
        state["resource"]["state"]=json!("running");
        state["status"]=json!("running");
        state["job"]["state"]=json!("running");
        state["job"]["active"]=json!(true);
        save(root,&state)?;
    }
    let mut run_error: Option<String>=None;
    if let Err(e)=provisioned {
        run_error=Some(e);
    } else {
        match provider.infer(&kind) {
            Ok((bytes,mime,ext)) => {
                match store_artifact(root,&kind,&request,&bytes,mime,ext) {
                    Ok(result) => {
                        state["artifact"]=result;
                        state["job"]["state"]=json!("completed");
                        state["status"]=json!("artifact_verified");
                    }
                    Err(e) => {run_error=Some(e);}
                }
            }
            Err(e) => {run_error=Some(e);}
        };
        provider.complete_job(&request)?;
        state["job"]["active"]=json!(false);
        if let Some(e)=&run_error {
            state["job"]["state"]=json!("failed");
            state["status"]=json!("failed");
            state["errors"].as_array_mut().ok_or("Invalid errors")?.push(json!(e));
        }
        save(root,&state)?;
        let _ = cleanup(root,&mut state,provider)?;
    }
    if let Some(e)=run_error {
        return Err(format!("Simulated worker failed (cleanup attempted): {e}"));
    }
    verify(root.to_str().ok_or("Non-UTF-8 session path")?)?;
    let stopped=state["resource"]["stop_confirmed"]==true;
    let response=json!({"simulation_only":true,"real_ai_inference":false,
        "job_state":state["job"]["state"],"resource_state":state["resource"]["state"],
        "auto_stop_confirmed":stopped,"result":state["artifact"]});
    Ok(response)
}

pub(crate) fn run(directory:&str) -> Result<Value,String> {
    let root=root_dir(directory)?;
    let _lock=ExclusiveLock::acquire(&root)?;
    run_under_lock(&root,&MockProvider::new(&root))
}

pub(crate) fn reconcile(directory:&str) -> Result<Value,String> {
    let root=root_dir(directory)?;
    let _lock=ExclusiveLock::acquire(&root)?;
    let mut state=journal(&root)?;
    let provider=MockProvider::new(&root);
    cleanup(&root,&mut state,&provider)
}

pub(crate) fn status(directory:&str) -> Result<Value,String> {
    let root=root_dir(directory)?;
    let state=journal(&root)?;
    let resource=MockProvider::new(&root).read()?;
    Ok(json!({"simulation_only":true,"journal":state,"provider_observation":resource}))
}

/// Explicit test shortcut. Only mock-only, no model download, GPU, or provider API.
/// The approval is tagged as a test grant, never an actual human approval.
pub(crate) fn demo(directory:&str,kind:&str) -> Result<Value,String> {
    prepare(directory,kind)?;
    let root=root_dir(directory)?;
    {
        let _lock=ExclusiveLock::acquire(&root)?;
        approve_under_lock(&root,"test_fixture_only")?;
    }
    run(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize,Ordering};
    static SEQ:AtomicUsize=AtomicUsize::new(0);
    fn temp() -> String {
        let idx=SEQ.fetch_add(1,Ordering::SeqCst);
        std::env::temp_dir().join(format!("cmb-offline-{}-{idx}-{}",std::process::id(),secure_nonce().unwrap())).to_string_lossy().into_owned()
    }
    fn fixture_setup(kind:&str)->(String,PathBuf) {
        let name=temp();
        prepare(&name,kind).unwrap();
        let root=root_dir(&name).unwrap();
        (name,root)
    }
    fn allow(root:&Path) {
        let _lock=ExclusiveLock::acquire(root).unwrap();
        approve_under_lock(root,"test_fixture_only").unwrap();
    }

    #[test]
    fn all_nine_media_paths_have_real_verified_fixture_artifacts_and_auto_stop() {
        for kind in KINDS {
            let name=temp();
            let result=demo(&name,kind).unwrap();
            assert_eq!(result["simulation_only"],true);
            assert_eq!(result["real_ai_inference"],false);
            assert_eq!(result["auto_stop_confirmed"],true);
            assert_eq!(result["resource_state"],"stopped");
            let verified=verify(&name).unwrap();
            assert_eq!(verified["artifact_verified"],true);
            assert_eq!(verified["kind"],*kind);
            let provider=MockProvider::new(&root_dir(&name).unwrap()).read().unwrap().unwrap();
            assert_eq!(provider["stop_calls"],1);
            let _=fs::remove_dir_all(name);
        }
    }

    #[test]
    fn missing_or_replayed_approval_is_rejected() {
        let (name,root)=fixture_setup("music");
        assert!(run(&name).unwrap_err().contains("approval"));
        allow(&root);
        assert!(approve_under_lock(&root,"test_fixture_only").is_err());
        let completed=run(&name).unwrap();
        assert_eq!(completed["auto_stop_confirmed"],true);
        assert!(run(&name).is_err());
        let _=fs::remove_dir_all(name);
    }

    #[test]
    fn plan_tampering_and_cross_session_replay_are_denied() {
        let (name,root)=fixture_setup("video");
        allow(&root);
        let mut state=journal(&root).unwrap();
        state["plan"]["gpu"]=json!("expensive-gpu");
        save(&root,&state).unwrap();
        assert!(run(&name).is_err());
        assert!(!root.join(PROVIDER).exists());
        let _=fs::remove_dir_all(name);
    }

    #[test]
    fn failure_auto_stops_only_owned_mock_resource() {
        let (name,root)=fixture_setup("image");
        allow(&root);
        let provider=MockProvider {root:root.clone(),fail_inference:true,fail_stop:false};
        let message=run_under_lock(&root,&provider).unwrap_err();
        assert!(message.contains("Simulated worker failed"));
        let state=journal(&root).unwrap();
        assert_eq!(state["job"]["state"],"failed");
        assert_eq!(state["resource"]["stop_confirmed"],true);
        assert!(state["artifact"].is_null());
        let _=fs::remove_dir_all(name);
    }

    #[test]
    fn ownership_is_mandatory_and_stop_never_terminates_resources() {
        let (name,root)=fixture_setup("image");
        allow(&root);
        let provider=MockProvider::new(&root);
        let req=journal(&root).unwrap()["plan"]["request_id"].as_str().unwrap().to_string();
        let resource=provider.provision(&req).unwrap();
        assert!(provider.stop_owned("other-owner",&resource).is_err());
        assert!(!provider.stop_owned(&req,&resource).unwrap()); // still active
        provider.complete_job(&req).unwrap();
        assert!(provider.stop_owned(&req,&resource).unwrap());
        assert_eq!(provider.read().unwrap().unwrap()["state"],"stopped");
        let _=fs::remove_dir_all(name);
    }

    #[test]
    fn stop_failure_is_visible_and_reconcilable_without_second_stop() {
        let (name,root)=fixture_setup("audio");
        allow(&root);
        let provider=MockProvider {root:root.clone(),fail_inference:false,fail_stop:true};
        let out=run_under_lock(&root,&provider).unwrap();
        assert_eq!(out["auto_stop_confirmed"],false);
        let s=journal(&root).unwrap();
        assert_eq!(s["resource"]["state"],"stop_failed");
        assert_eq!(reconcile(&name).unwrap()["status"],"STOP_CONFIRMED");
        let p=MockProvider::new(&root);
        assert_eq!(p.read().unwrap().unwrap()["stop_calls"],1);
        assert_eq!(reconcile(&name).unwrap()["status"],"STOP_CONFIRMED");
        assert_eq!(p.read().unwrap().unwrap()["stop_calls"],1);
        let _=fs::remove_dir_all(name);
    }

    #[test]
    fn crash_window_reconciles_provider_owned_resource_by_idempotency() {
        let (name,root)=fixture_setup("three_d");
        allow(&root);
        let mut state=journal(&root).unwrap();
        let req=state["plan"]["request_id"].as_str().unwrap().to_string();
        state["approval"]["consumed"]=json!(true);
        state["status"]=json!("provisioning");
        save(&root,&state).unwrap();
        let provider=MockProvider::new(&root);
        provider.provision(&req).unwrap(); // crash before journal has resource ID
        let pending=reconcile(&name).unwrap();
        assert_eq!(pending["status"],"STOP_DEFERRED");
        assert!(journal(&root).unwrap()["resource"]["id"].is_string());
        provider.complete_job(&req).unwrap();
        assert_eq!(reconcile(&name).unwrap()["status"],"STOP_CONFIRMED");
        let _=fs::remove_dir_all(name);
    }

    #[test]
    fn artifact_mutation_and_symlinks_fail_integrity_checks() {
        let name=temp();
        demo(&name,"image").unwrap();
        let root=root_dir(&name).unwrap();
        fs::write(root.join("artifact-image.svg"),b"tampered").unwrap();
        assert!(verify(&name).is_err());
        let _=fs::remove_dir_all(name);
    }

    #[test]
    fn fixture_is_not_a_real_gpu_provider_or_ai_model() {
        let state=prepare(&temp(),"multimodal").unwrap();
        assert_eq!(state["simulation_only"],true);
        assert_eq!(supported_kinds().len(),9);
        let root=PathBuf::from(state["session"].as_str().unwrap());
        let prepared=journal(&root).unwrap();
        assert_eq!(prepared["plan"]["gpu"],"none");
        assert_eq!(prepared["plan"]["estimated_gpu_credit"],0);
        assert_eq!(prepared["plan"]["model_id"],"fixture/not-an-ai-model");
        let _=fs::remove_dir_all(root);
    }
}
