//! Local advisory plan snapshots. No provider credentials or cloud transport.
use crate::crypto::{secure_nonce, sha256};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const JOURNAL: &str = "cloud-resource.json";
const VERSION: u64 = 1;

fn safe_scope(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
}

fn root_dir(directory: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(directory);
    if path.as_os_str().is_empty() {
        return Err("Cloud session directory is required".into());
    }
    fs::create_dir_all(&path)
        .map_err(|_| "Unable to create cloud session directory".to_string())?;
    let path = fs::canonicalize(path).map_err(|_| "Invalid cloud session directory".to_string())?;
    if !path.is_dir() {
        return Err("Cloud session must be a directory".into());
    }
    Ok(path)
}

struct Lock(PathBuf);
impl Lock {
    fn acquire(root: &Path) -> Result<Self, String> {
        let path = root.join(".cloud.lock");
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(&path).map_err(|_| {
            "Cloud session locked; reconcile the provider before recovering a crash".to_string()
        })?;
        Ok(Self(path))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn save(root: &Path, value: &Value) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "Could not serialize journal".to_string())?;
    if bytes.len() > 128 * 1024 {
        return Err("Cloud journal too large".into());
    }
    let tmp = root.join(format!(".journal-{}.tmp", secure_nonce()?));
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&tmp)
        .map_err(|_| "Cannot create private journal temp file".to_string())?;
    file.write_all(&bytes)
        .map_err(|_| "Cannot save cloud journal".to_string())?;
    file.sync_all()
        .map_err(|_| "Cannot sync cloud journal".to_string())?;
    drop(file);
    fs::rename(&tmp, root.join(JOURNAL)).map_err(|_| "Cannot commit cloud journal".to_string())
}

fn read(root: &Path) -> Result<Value, String> {
    let path = root.join(JOURNAL);
    let metadata = fs::symlink_metadata(&path).map_err(|_| "Cloud journal missing".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 128 * 1024 {
        return Err("Invalid or linked cloud journal refused".into());
    }
    let data = fs::read(&path).map_err(|_| "Cannot read cloud journal".to_string())?;
    let state: Value =
        serde_json::from_slice(&data).map_err(|_| "Invalid cloud journal JSON".to_string())?;
    if state["schema_version"] != VERSION || state["simulation_only"] != false {
        return Err("Not a real cloud resource journal".into());
    }
    if state["plan_sha256"] != sha256(state["plan"].to_string().as_bytes()) {
        return Err("Cloud plan digest does not match; refuse mutation".into());
    }
    Ok(state)
}

pub(crate) fn load_manifest(path: &str, limit: usize) -> Result<Value, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|_| "Cannot open manifest")?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read manifest")?;
    if bytes.len() > limit {
        return Err(format!("Input exceeds {limit} bytes"));
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid manifest JSON".into())
}

pub(crate) fn review(raw: &Value) -> Result<Value, String> {
    if raw["schema_version"] != 1 || raw.to_string().len() > 16384 {
        return Err("Expected a version 1 manifest under 16 KiB".into());
    }
    for key in [
        "account",
        "workspace",
        "runtime_entry_id",
        "runtime_family",
        "task",
    ] {
        if !raw[key].as_str().is_some_and(safe_scope) {
            return Err(format!("Invalid {key}"));
        }
    }
    let model = raw["model_id"].as_str().ok_or("Missing model ID")?;
    if crate::model_id(model)?.as_str() != model {
        return Err("Expected exact model ID".into());
    }
    let sha = raw["model_sha"].as_str().ok_or("Missing model SHA")?;
    if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Expected immutable model SHA".into());
    }
    crate::provider_ops::preview(
        raw["provider"].as_str().ok_or("Missing provider")?,
        raw["image"].as_str().ok_or("Missing pinned image")?,
        raw["gpu"].as_str().ok_or("Missing GPU")?,
        raw["disk_gb"].as_u64().ok_or("Missing disk allocation")?,
        raw["offer_id"].as_u64(),
    )?;
    let fields = [
        "schema_version",
        "account",
        "workspace",
        "provider",
        "model_id",
        "model_sha",
        "task",
        "runtime_family",
        "runtime_entry_id",
        "image",
        "gpu",
        "disk_gb",
        "offer_id",
        "region",
        "quantity",
        "billing_quote",
        "alternatives",
        "stop_policy",
        "max_elapsed_ms",
        "max_total_micro_usd",
        "alternatives_unavailable_reason",
    ];
    for key in raw.as_object().ok_or("Expected manifest object")?.keys() {
        if key != "request_id" && !fields.contains(&key.as_str()) {
            return Err(format!("Unrecognized plan field {key}"));
        }
    }
    let mut plan = json!({});
    for key in fields {
        plan[key] = raw[key].clone();
    }
    if let Some(id) = raw.get("request_id") {
        plan["request_id"] = id.clone();
    }
    let advice = crate::advice::evaluate(&plan)?;
    Ok(
        json!({"plan":plan,"plan_sha256":sha256(plan.to_string().as_bytes()),
        "advice":advice,"bridge_can_execute":false,"approval_granted":false}),
    )
}

pub(crate) fn prepare(directory: &str, manifest: &str) -> Result<Value, String> {
    let review = review(&load_manifest(manifest, 16384)?)?;
    let root = root_dir(directory)?;
    let _lock = Lock::acquire(&root)?;
    if root.join(JOURNAL).exists() {
        return Err("Cloud session already exists".into());
    }
    let mut plan = review["plan"].clone();
    plan["request_id"] = json!(secure_nonce()?);
    let digest = sha256(plan.to_string().as_bytes());
    let state = json!({"schema_version":VERSION,"simulation_only":false,"status":"awaiting_human_approval",
        "plan":plan,"plan_sha256":digest,"advice":review["advice"],
        "approval":{"status":"external_user_decision_required","consumed":false},
        "bridge_can_execute":false});
    save(&root, &state)?;
    Ok(state)
}

pub(crate) fn status(directory: &str) -> Result<Value, String> {
    let mut state = read(Path::new(directory))?;
    state["bridge_can_execute"] = json!(false);
    Ok(state)
}
