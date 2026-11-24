//! Validate a local, pinned GPU-smoke build bundle without contacting a GPU.
use crate::crypto::sha256;
use serde_json::{json,Value};
use std::{fs,path::Path};
fn read(path:&Path,limit:u64)->Result<Vec<u8>,String>{
    let m=fs::symlink_metadata(path).map_err(|_|"Missing bundle file")?;
    if !m.is_file()||m.file_type().is_symlink()||m.len()>limit{return Err("Bundle requires bounded regular files".into());}
    fs::read(path).map_err(|_|"Cannot read bundle file".into())
}
fn digest(v:&str)->bool{v.len()==64&&v.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
fn image(v:&str)->bool{v.rsplit_once("@sha256:").is_some_and(|(n,h)|!n.is_empty()&&n.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_./:".contains(&b))&&digest(h))}
fn validate(release:&Value,worker:&[u8],lock:&[u8])->Result<(),String>{
    if release["schema_version"]!=1||release["purpose"]!="gpu_smoke"||!release["base_image"].as_str().is_some_and(image)
        ||release["worker_sha256"]!=sha256(worker)||release["dependency_lock_sha256"]!=sha256(lock){
        return Err("Release identity, image or bundle digest does not match".into());
    }
    let model=release["model_id"].as_str().ok_or("Missing model ID")?;
    if crate::model_id(model)?.as_str()!=model || !release["model_sha"].as_str().is_some_and(|s|matches!(s.len(),40|64)&&s.bytes().all(|b|b.is_ascii_hexdigit())){
        return Err("Exact immutable model identity required".into());
    }
    match (release["runtime_family"].as_str(),release["task"].as_str()){
        (Some("diffusers"),Some("text-to-image"|"image-to-image"))|(Some("transformers"),Some("text-to-audio"|"text-to-speech"|"automatic-speech-recognition"))=>(),
        _=>return Err("No in-repository GPU smoke handler for this exact family/task; use a separately reviewed upstream runtime".into()),
    }
    let s=std::str::from_utf8(lock).map_err(|_|"Lock must be UTF-8")?;
    // pip enforces hashes for every dependency during the offline build. Reject
    // URL, index, editable, recursive and local-file directives here.
    for line in s.lines().map(str::trim).filter(|s|!s.is_empty()&&!s.starts_with('#')){
        if line.contains("://")||line.contains(" @ ")||line.starts_with('-')&&!line.starts_with("--hash=sha256:"){
            return Err("Lock contains a network or unsupported directive".into());
        }
    }
    if !s.contains("--hash=sha256:"){return Err("Hash-pinned dependency lock required".into());}
    Ok(())
}
pub(crate) fn verify(dir:&str)->Result<Value,String>{
    let root=Path::new(dir);
    let release=read(&root.join("runtime-release.json"),16384)?;
    let manifest:Value=serde_json::from_slice(&release).map_err(|_|"Invalid release JSON")?;
    let worker=read(&root.join("media_worker.py"),1024*1024)?;
    let lock=read(&root.join("requirements.lock"),4*1024*1024)?;
    validate(&manifest,&worker,&lock)?;
    Ok(json!({"status":"local_bundle_integrity_validated","runtime_release_sha256":sha256(&release),
        "release":manifest,"gpu_used":false,"image_built":false,"model_tested":false,"production_ready":false,
        "next":"Build with a reviewed wheelhouse and pinned base, inspect image, then obtain separate signed GPU smoke authorization."}))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn exact_bundle_hashes_and_supported_handler_required(){
        let worker=b"worker";let lock=b"example==1 --hash=sha256:abc";
        let mut r=json!({"schema_version":1,"purpose":"gpu_smoke","base_image":format!("example/base@sha256:{}","a".repeat(64)),"model_id":"example/model","model_sha":"a".repeat(40),"runtime_family":"diffusers","task":"text-to-image","worker_sha256":sha256(worker),"dependency_lock_sha256":sha256(lock)});
        assert!(validate(&r,worker,lock).is_ok());
        assert!(validate(&r,b"changed",lock).is_err());
        r["task"]=json!("text-to-video");assert!(validate(&r,worker,lock).is_err());
    }
}
