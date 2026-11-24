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
// Intentionally restricted, platform-specific lock format: exact versions and
// one or more full SHA-256 wheel digests per logical requirement.
fn validate_lock(lock:&[u8])->Result<(),String>{
    let s=std::str::from_utf8(lock).map_err(|_|"Lock must be UTF-8")?;
    let mut names=std::collections::BTreeSet::new();
    let mut logical=String::new();
    for raw in s.lines(){
        let line=raw.trim();
        if line.is_empty()||line.starts_with('#'){continue;}
        if let Some(part)=line.strip_suffix('\\'){
            logical.push_str(part);logical.push(' ');continue;
        }
        logical.push_str(line);
        let mut tokens=logical.split_whitespace();
        let package=tokens.next().ok_or("Empty requirement")?;
        let (name,version)=package.split_once("==").ok_or("Each dependency needs an exact version")?;
        if name.is_empty()||version.is_empty()||!name.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_.".contains(&b))
            ||!version.bytes().all(|b|b.is_ascii_alphanumeric()||b"._+-!".contains(&b)){
            return Err("Unsupported package syntax; use a platform-specific exact lock".into());
        }
        let normalized=name.to_ascii_lowercase().replace(['_','.'],"-");
        if !names.insert(normalized){return Err("Duplicate package in lock".into());}
        let mut count=0;
        for token in tokens{
            let hash=token.strip_prefix("--hash=sha256:").ok_or("Only exact wheel hash options are allowed")?;
            if !digest(hash){return Err("Expected complete lowercase SHA-256 wheel hash".into());}
            count+=1;
        }
        if count==0{return Err("Each dependency must have a wheel hash".into());}
        logical.clear();
    }
    if !logical.is_empty()||names.is_empty(){return Err("Empty or incomplete dependency lock".into());}
    Ok(())
}
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
    validate_lock(lock)?;
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
    #[test]fn lock_rejects_partial_hashes_urls_unpinned_dependencies_and_directives(){
        let hash="a".repeat(64);
        assert!(validate_lock(format!("demo==1.0 --hash=sha256:{hash}\nother==2.0 --hash=sha256:{hash}").as_bytes()).is_ok());
        for s in ["demo==1 --hash=sha256:abc","demo>=1","demo==1","-r other.txt","demo @ https://example.invalid/a.whl"]{
            assert!(validate_lock(s.as_bytes()).is_err(),"{s}");
        }
        assert!(validate_lock(format!("demo==1 --hash=sha256:{hash}\nDemo==2 --hash=sha256:{hash}").as_bytes()).is_err());
    }
    #[test]fn exact_bundle_hashes_and_supported_handler_required(){
        let worker=b"worker";let lock=format!("example==1 --hash=sha256:{}","a".repeat(64));let lock=lock.as_bytes();
        let mut r=json!({"schema_version":1,"purpose":"gpu_smoke","base_image":format!("example/base@sha256:{}","a".repeat(64)),"model_id":"example/model","model_sha":"a".repeat(40),"runtime_family":"diffusers","task":"text-to-image","worker_sha256":sha256(worker),"dependency_lock_sha256":sha256(lock)});
        assert!(validate(&r,worker,lock).is_ok());
        assert!(validate(&r,b"changed",lock).is_err());
        r["task"]=json!("text-to-video");assert!(validate(&r,worker,lock).is_err());
    }
}
