//! Detached user signatures; the bridge never holds the signing key.
use crate::crypto::{secure_nonce, sha256};
use serde_json::{json,Value};
use std::{fs,io::Write,path::{Path,PathBuf},process::{Command,Stdio},time::{SystemTime,UNIX_EPOCH}};
const NS:&str="cloud-media-bridge-v1";
const AGE:u64=300_000;
fn now()->Result<u64,String>{
    SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"Invalid clock")?.as_millis().try_into().map_err(|_|"Clock overflow".into())
}
fn hex(v:&Value,n:usize)->bool{
    v.as_str().is_some_and(|s|s.len()==n&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)))
}
fn id(v:&Value)->bool{
    v.as_str().is_some_and(|s|!s.is_empty()&&s.len()<=128&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"-_.".contains(&b)))
}
fn scope(op:&Value,r:&Value,i:&Value)->bool{
    match op.as_str(){
        Some("deploy")=>r.is_null()&&i.is_null(),
        Some("generate")=>id(r)&&hex(i,64),
        Some("stop"|"terminate"|"suspend"|"scale_to_zero")=>id(r)&&i.is_null(),
        _=>false,
    }
}
fn principal()->Result<String,String>{
    let p=std::env::var("CMB_AUTH_PRINCIPAL").map_err(|_|"Independent operator must configure CMB_AUTH_PRINCIPAL")?;
    if p.is_empty()||p.len()>128||!p.bytes().all(|b|b.is_ascii_alphanumeric()||b"@-_.".contains(&b)){return Err("Invalid signer principal".into());}
    Ok(p)
}
fn payload(plan:&Value,req:&Value,p:&str,t:u64)->Result<Value,String>{
    if !hex(&plan["request_id"],48)||!hex(&plan["nonce"],48)||!scope(&req["operation"],&req["resource_id"],&req["input_sha256"]){return Err("Invalid plan or operation scope".into());}
    Ok(json!({"schema_version":1,"namespace":NS,"principal":p,"request_id":plan["request_id"],
        "plan_sha256":sha256(plan.to_string().as_bytes()),"grant_id":secure_nonce()?,
        "operation":req["operation"],"resource_id":req["resource_id"],"input_sha256":req["input_sha256"],
        "issued_at_unix_ms":t,"expires_at_unix_ms":t.checked_add(AGE).ok_or("Expiry overflow")?}))
}
fn validate(plan:&Value,g:&Value,req:&Value,p:&str,t:u64)->Result<(),String>{
    let issued=g["issued_at_unix_ms"].as_u64().ok_or("Missing issue time")?;
    let expires=g["expires_at_unix_ms"].as_u64().ok_or("Missing expiry")?;
    if g.as_object().is_none_or(|o|o.len()!=11)||g["schema_version"]!=1||g["namespace"]!=NS||g["principal"]!=p
       ||!hex(&plan["request_id"],48)||!hex(&plan["nonce"],48)||!hex(&g["grant_id"],48)
       ||g["request_id"]!=plan["request_id"]||g["plan_sha256"]!=sha256(plan.to_string().as_bytes())
       ||g["operation"]!=req["operation"]||g["resource_id"]!=req["resource_id"]||g["input_sha256"]!=req["input_sha256"]
       ||!scope(&g["operation"],&g["resource_id"],&g["input_sha256"])
       ||issued>t||t>=expires||expires.checked_sub(issued).is_none_or(|n|n==0||n>AGE){
        return Err("Grant expired or bound to a different plan, identity or operation".into());
    }Ok(())
}
fn trusted(key:&str,dir:bool)->Result<PathBuf,String>{
    let p=PathBuf::from(std::env::var(key).map_err(|_|format!("Configure trusted {key}"))?);
    let m=fs::symlink_metadata(&p).map_err(|_|format!("Cannot access {key}"))?;
    if !p.is_absolute()||m.file_type().is_symlink()||(dir&&!m.is_dir())||(!dir&&!m.is_file()){return Err(format!("{key} must be an absolute trusted local path"));}
    Ok(p)
}
fn verify_sig(g:&Value,sig:&Path,p:&str)->Result<(),String>{
    let keygen=trusted("CMB_AUTH_SSH_KEYGEN",false)?;
    let allowed=trusted("CMB_AUTH_ALLOWED_SIGNERS",false)?;
    let sm=fs::symlink_metadata(sig).map_err(|_|"Missing detached signature")?;
    if !sm.is_file()||sm.file_type().is_symlink()||sm.len()>16384{return Err("Invalid signature file".into());}
    let mut child=Command::new(keygen).args(["-Y","verify","-f"]).arg(allowed).args(["-I",p,"-n",NS,"-s"]).arg(sig)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_|"OpenSSH verifier unavailable")?;
    child.stdin.take().ok_or("Verifier stdin unavailable")?.write_all(g.to_string().as_bytes()).map_err(|_|"Cannot send signed bytes")?;
    if !child.wait().map_err(|_|"Verifier failed")?.success(){return Err("Detached signature invalid".into());}Ok(())
}
fn mark(root:&Path,g:&Value)->Result<(),String>{
    let (prefix,key)=if g["operation"]=="deploy"{("deploy",&g["request_id"])}else{("grant",&g["grant_id"])};
    if !hex(key,48){return Err("Invalid replay ID".into());}
    let mut opts=fs::OpenOptions::new();opts.write(true).create_new(true);
    #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;opts.mode(0o600);}
    let mut f=opts.open(root.join(format!("{prefix}-{}",key.as_str().ok_or("Missing ID")?))).map_err(|_|"Grant replay or ledger unavailable; reconcile before retry")?;
    f.write_all(sha256(g.to_string().as_bytes()).as_bytes()).map_err(|_|"Cannot persist marker")?;
    f.sync_all().map_err(|_|"Cannot sync marker")?;
    #[cfg(unix)]
    fs::File::open(root).and_then(|dir|dir.sync_all()).map_err(|_|"Cannot sync replay ledger directory; do not retry")?;
    Ok(())
}
fn verify(plan:&Value,g:&Value,sig:&Path,req:&Value,consume:bool)->Result<Value,String>{
    let p=principal()?;validate(plan,g,req,&p,now()?)?;verify_sig(g,sig,&p)?;validate(plan,g,req,&p,now()?)?;
    if consume{mark(&trusted("CMB_AUTH_LEDGER",true)?,g)?;}
    Ok(json!({"signature_verified":true,"grant_consumed":consume,"operation":g["operation"],"grant_id":g["grant_id"],"plan_sha256":g["plan_sha256"],"provider_operation_sent":false}))
}
pub(crate) fn challenge(dir:&str,req:&str)->Result<Value,String>{
    let plan=crate::cloud_operator::status(dir)?["plan"].clone();
    let g=payload(&plan,&crate::planning::load_manifest(req,4096)?,&principal()?,now()?)?;
    Ok(json!({"payload":g,"sign_exact_utf8":g.to_string(),"signature_namespace":NS,"approval_granted":false}))
}
pub(crate) fn verify_cli(dir:&str,grant:&str,sig:&str,req:&str,consume:bool)->Result<Value,String>{
    let plan=crate::cloud_operator::status(dir)?["plan"].clone();
    verify(&plan,&crate::planning::load_manifest(grant,8192)?,Path::new(sig),&crate::planning::load_manifest(req,4096)?,consume)
}
pub(crate) fn consume_for_plan(plan:&Value,op:&str,resource:Value,input:Value)->Result<Value,String>{
    let g=std::env::var("CMB_AUTH_GRANT_FILE").map_err(|_|"Independent signed grant required")?;
    let s=std::env::var("CMB_AUTH_SIGNATURE_FILE").map_err(|_|"Detached signature required")?;
    verify(plan,&crate::planning::load_manifest(&g,8192)?,Path::new(&s),&json!({"operation":op,"resource_id":resource,"input_sha256":input}),true)
}
#[cfg(test)]
mod tests{
    use super::*;
    fn plan()->Value{json!({"request_id":"a".repeat(48),"nonce":"b".repeat(48),"gpu":"example"})}
    #[test]fn binds_plan_scope_identity_and_expiry(){
        let p=plan();let op=json!({"operation":"deploy"});let g=payload(&p,&op,"owner",1000).unwrap();
        assert!(validate(&p,&g,&op,"owner",1001).is_ok());
        let mut changed=p.clone();changed["gpu"]=json!("other");
        assert!(validate(&changed,&g,&op,"owner",1001).is_err());
        assert!(validate(&p,&g,&op,"other",1001).is_err());
        assert!(validate(&p,&g,&op,"owner",1000+AGE).is_err());
        assert!(validate(&p,&g,&op,"owner",999).is_err());
        assert!(validate(&p,&g,&json!({"operation":"stop","resource_id":"r"}),"owner",1001).is_err());
        let job=json!({"operation":"generate","resource_id":"r","input_sha256":"a".repeat(64)});
        let j=payload(&p,&job,"owner",1000).unwrap();
        assert!(validate(&p,&j,&job,"owner",1001).is_ok());
        assert!(validate(&p,&j,&json!({"operation":"generate","resource_id":"r","input_sha256":"b".repeat(64)}),"owner",1001).is_err());
    }
    #[test]fn replay_is_refused_even_if_same_deployment_signed_again(){
        let root=std::env::temp_dir().join(format!("cmb-auth-{}",secure_nonce().unwrap()));fs::create_dir(&root).unwrap();
        let p=plan();let op=json!({"operation":"deploy"});let a=payload(&p,&op,"owner",1000).unwrap();let b=payload(&p,&op,"owner",1000).unwrap();
        assert_ne!(a["grant_id"],b["grant_id"]);
        assert!(mark(&root,&a).is_ok());assert!(mark(&root,&a).is_err());assert!(mark(&root,&b).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
