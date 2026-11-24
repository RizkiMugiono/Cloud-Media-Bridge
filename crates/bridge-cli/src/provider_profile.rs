//! User-selected provider profiles. Data only: no URL fetch, shell execution,
//! credential lookup, or assertion of actual provider compatibility.
use serde_json::{json,Value};

fn id(s:&str)->bool{
    !s.is_empty()&&s.len()<=80&&s.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-')
        &&s.as_bytes()[0].is_ascii_alphanumeric()&&s.as_bytes()[s.len()-1].is_ascii_alphanumeric()
}
fn label(v:&Value)->bool{
    v.as_str().is_some_and(|s|!s.trim().is_empty()&&s.len()<=128&&!s.chars().any(char::is_control))
}
fn https(s:&str)->bool{
    if s.len()>512||s.bytes().any(|b|!b.is_ascii_graphic()||b==b'\\')||s.contains(['@','?','#']){return false;}
    let Some(rest)=s.strip_prefix("https://")else{return false;};
    let host=rest.split('/').next().unwrap_or("");
    let labels:Vec<_>=host.split('.').collect();
    labels.len()>=2&&labels.last().is_some_and(|x|x.bytes().any(|b|b.is_ascii_alphabetic()))
        &&labels.iter().all(|x|!x.is_empty()&&x.len()<=63&&x.as_bytes()[0].is_ascii_alphanumeric()
            &&x.as_bytes()[x.len()-1].is_ascii_alphanumeric()
            &&x.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'))
        &&!host.ends_with(".local")&&!host.ends_with(".localhost")&&!host.ends_with(".internal")
}
pub(crate) fn validate(profile:&Value)->Result<Value,String>{
    let fields=profile.as_object().ok_or("Provider profile must be an object")?;
    let allowed=["schema_version","id","name","pricing","documentation","transport","lifecycle_actions"];
    if fields.keys().any(|k|!allowed.contains(&k.as_str()))||profile["schema_version"]!=1{
        return Err("Unsupported provider profile field or version".into());
    }
    let key=profile["id"].as_str().filter(|s|id(s)).ok_or("Use a lowercase provider identifier")?;
    if crate::provider_info(key).is_ok(){return Err("Cannot override a built-in provider; use a distinct provider ID".into());}
    if !label(&profile["name"]){return Err("Provider display name required".into());}
    for k in ["pricing","documentation"]{
        let urls=profile[k].as_array().filter(|v|!v.is_empty()&&v.len()<=8).ok_or("Provide one to eight official URLs per source group")?;
        if !urls.iter().all(|u|u.as_str().is_some_and(https)){return Err("Only plain public HTTPS source URLs without credentials, query or fragment are accepted".into());}
    }
    if !matches!(profile["transport"].as_str(),Some("api"|"cli"|"ssh"|"kubernetes"|"https"|"sdk")){
        return Err("Specify the native provider interface".into());
    }
    let actions=profile["lifecycle_actions"].as_array().filter(|v|!v.is_empty()&&v.len()<=4).ok_or("Declare provider lifecycle capabilities")?;
    if !actions.iter().all(|v|matches!(v.as_str(),Some("stop"|"terminate"|"suspend"|"scale_to_zero"))){
        return Err("Unknown lifecycle capability".into());
    }
    Ok(profile.clone())
}
pub(crate) fn resolve(provider:&str,profile:&Value)->Result<Value,String>{
    if profile.is_null(){return crate::provider_info(provider);}
    let v=validate(profile)?;
    if v["id"]!=provider{return Err("Profile ID does not match selected provider".into());}
    Ok(json!({"id":provider,"name":v["name"],"profile":v,"capabilities":{"discover":true,"plan":true,"execute":false,"deploy":false},
        "native_execution":"agent_host","profile_source":"caller_supplied_unverified"}))
}
pub(crate) fn sources(provider:&str,profile:&Value)->Result<Value,String>{
    if profile.is_null(){return crate::market::sources(Some(provider));}
    let v=resolve(provider,profile)?;
    Ok(json!({"pricing":v["profile"]["pricing"],"documentation":v["profile"]["documentation"],
        "transport":v["profile"]["transport"],"method":"agent_read_official_sources",
        "source_authenticity_verified":false,"credential_accessed":false,"network_request_sent":false}))
}
pub(crate) fn review(profile:&Value)->Result<Value,String>{
    let v=validate(profile)?;
    Ok(json!({"profile":v,"status":"profile_structure_validated",
        "source_authenticity_verified":false,"provider_operation_sent":false,"approval_granted":false,
        "next":"Agent verifies official sources and uses current provider-native tools after explicit user approval."}))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn external_profile_accepts_new_provider_without_registration_and_rejects_overrides(){
        let mut p:Value=serde_json::from_str(include_str!("../../../examples/external-provider.example.json")).unwrap();
        assert!(review(&p).is_ok());assert!(resolve("new-gpu-cloud",&p).is_ok());
        assert!(resolve("different",&p).is_err());p["id"]=json!("modal");assert!(validate(&p).is_err());
        p["id"]=json!("new-gpu-cloud");p["pricing"]=json!(["https://user:pass@example.com/"]);assert!(validate(&p).is_err());
        p["pricing"]=json!(["https://127.0.0.1/"]);assert!(validate(&p).is_err());
        p["pricing"]=json!(["https://example.com/pricing"]);p["command"]=json!("anything");assert!(validate(&p).is_err());
    }
}
