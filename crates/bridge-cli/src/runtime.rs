//! Read-only resolver for pre-existing inference runtimes.
//! It proposes *candidates*, never permission to run a GPU or untrusted model code.
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::BTreeSet;

const BUNDLED_REGISTRY: &str = include_str!("../../../config/runtime-registry.json");

pub(crate) fn registry() -> Result<Value, String> {
    let value: Value = serde_json::from_str(BUNDLED_REGISTRY)
        .map_err(|_| "Invalid bundled runtime registry JSON".to_string())?;
    validate_registry(&value)?;
    Ok(value)
}

fn validate_registry(value: &Value) -> Result<(), String> {
    if value["schema_version"] != 1
        || value["implementation_status"] != "read_only_candidates_no_deployment"
    {
        return Err("Runtime registry must remain explicitly read-only".into());
    }
    let rows = value["entries"].as_array().ok_or("Runtime entries must be an array")?;
    let mut ids = BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().ok_or("Runtime entry missing id")?;
        let route = row["delivery_route"].as_str().ok_or("Runtime entry missing route")?;
        if id.is_empty() || !ids.insert(id) {
            return Err("Runtime identifiers must be nonempty and unique".into());
        }
        if !matches!(
            route,
            "verified_prebuilt_oci_image" | "vetted_runtime_recipe_on_base_image"
        ) {
            return Err("Unrecognized runtime delivery route".into());
        }
        if row["execution_enabled"] != false {
            return Err("Registry may not enable a GPU runtime".into());
        }
        if row["upstream_url"].as_str().is_none_or(|url| !url.starts_with("https://")) {
            return Err("Runtime provenance must be an HTTPS URL".into());
        }
        if row["verification"]["status"].as_str().is_none() {
            return Err("Runtime verification status is required".into());
        }
        if !row["match"].is_object() || !row["delivery"].is_object() {
            return Err("Runtime match and delivery metadata are required".into());
        }
    }
    Ok(())
}

fn entry_list(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn contains_ci(values: &[String], needle: &str) -> bool {
    values.iter().any(|v| v.eq_ignore_ascii_case(needle))
}

fn sha256_digest(value: &str) -> bool {
    let Some((prefix, hash)) = value.rsplit_once("@sha256:") else { return false; };
    !prefix.is_empty() && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn checksum(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hash| {
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

fn model_sha_valid(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn verification_scope(row: &Value, model: &Value, provider: &Value) -> bool {
    let check = &row["verification"];
    let Some(sha) = model["revision_sha"].as_str() else { return false; };
    let Some(provider_id) = provider["id"].as_str() else { return false; };
    if !model_sha_valid(sha)
        || check["status"] != "verified"
        || check["model_tested"] != true
        || check["provider_tested"] != true
        || check["reviewer_approved"] != true
        || check["tested_model_revision"] != sha
        || !entry_list(check, "tested_provider_ids").contains(&provider_id.to_string())
        || !check["evidence_url"].as_str().is_some_and(|v| v.starts_with("https://"))
        || !check["evidence_commit"].as_str().is_some_and(model_sha_valid)
        || !check["worker_implementation_verified"].as_bool().unwrap_or(false)
    {
        return false;
    }

    let Some(worker_digest) = row["delivery"]["worker_implementation_digest"].as_str() else {
        return false;
    };
    if !checksum(worker_digest) {
        return false;
    }
    match row["delivery_route"].as_str() {
        Some("verified_prebuilt_oci_image") => row["delivery"]["oci_image_digest"]
            .as_str()
            .is_some_and(sha256_digest),
        Some("vetted_runtime_recipe_on_base_image") => {
            row["delivery"]["base_image_digest"].as_str().is_some_and(sha256_digest)
                && row["delivery"]["pinned_install_manifest"].as_str().is_some_and(checksum)
        }
        _ => false,
    }
}

fn file_present(files: &[String], required: &[String]) -> bool {
    required.is_empty()
        || required.iter().any(|f| {
            files.iter().any(|name| {
                name.eq_ignore_ascii_case(f)
                    || name.rsplit('/').next().is_some_and(|base| base.eq_ignore_ascii_case(f))
            })
        })
}

fn tags_have(tags: &[String], needle: &str) -> bool {
    tags.iter().any(|v| v.eq_ignore_ascii_case(needle))
}

fn category_known(model: &Value) -> bool {
    matches!(
        model["media"]["classification_status"].as_str(),
        Some("known_media_task" | "inferred_media_task_requires_review")
    )
}

fn match_row(row: &Value, model: &Value) -> Option<(i32, Vec<String>)> {
    let criteria = &row["match"];
    let model_id = model["id"].as_str().unwrap_or("");
    let allowed_models = entry_list(criteria, "model_id_allowlist");
    let exact_model_match = !model_id.is_empty() && allowed_models.iter().any(|id| id == model_id);
    // Reviewed model-specific runtimes can provide an exact match even if a
    // new media pipeline tag is not yet in the general task taxonomy.
    // Pure text-only models remain outside the project scope.
    if !category_known(model) && !exact_model_match {
        return None;
    }
    let library = model["library"].as_str().unwrap_or("");
    let known_library = criteria["library_name"].as_str().unwrap_or("");
    let lib_match = !known_library.is_empty() && known_library.eq_ignore_ascii_case(library);
    let tasks = entry_list(criteria, "pipeline_tags");
    let task = model["task"].as_str().unwrap_or("");
    let task_match = !task.is_empty() && contains_ci(&tasks, task);
    let arches = entry_list(criteria, "architectures");
    let current_arches = entry_list(&model["runtime_hints"], "architectures");
    let arch_match = !arches.is_empty()
        && current_arches.iter().any(|arch| contains_ci(&arches, arch));
    let files = entry_list(&model["runtime_hints"], "file_names");
    let signals = entry_list(criteria, "required_files");
    let files_match = file_present(&files, &signals);
    let needs_workflow = criteria["requires_reviewed_workflow"] == true;
    let tag_source = row["family"].as_str().unwrap_or("");
    let known_tags = entry_list(&model["runtime_hints"], "tags");
    let tag_match = !tag_source.is_empty() && tags_have(&known_tags, tag_source);

    // Never match an arbitrary pipeline on a task name alone. A framework,
    // architecture, explicit family tag, or recognizable checkpoint must corroborate it.
    let corroborated = exact_model_match || lib_match || arch_match
        || (task_match && files_match && !signals.is_empty())
        || (task_match && tag_match);
    if !corroborated || (!task_match && !arch_match && !exact_model_match) {
        return None;
    }
    if needs_workflow && !lib_match && !tag_match && !exact_model_match {
        return None;
    }
    if !files_match && !signals.is_empty() {
        return None;
    }
    // Reject claiming a framework match if the Hub explicitly names another.
    if !library.is_empty() && !known_library.is_empty() && !lib_match {
        return None;
    }

    let mut score = 0;
    let mut reasons = Vec::new();
    if exact_model_match {
        score += 100;
        reasons.push("reviewed_exact_model_id_match".to_string());
    }
    if lib_match {
        score += 30;
        reasons.push("hub_library_match".to_string());
    }
    if task_match {
        score += 20;
        reasons.push("pipeline_tag_match".to_string());
    }
    if arch_match {
        score += 40;
        reasons.push("architecture_match".to_string());
    }
    if !signals.is_empty() && files_match {
        score += 10;
        reasons.push("model_file_signal".to_string());
    }
    if tag_match {
        score += 5;
        reasons.push("family_tag_match".to_string());
    }
    if needs_workflow {
        reasons.push("reviewed_workflow_still_required".to_string());
    }
    Some((score, reasons))
}

fn access_blocks(model: &Value) -> Vec<String> {
    let mut blocked = Vec::new();
    if model["gated"] != false && !model["gated"].is_null() {
        blocked.push("gated_model_requires_user_access".to_string());
    }
    if model["private"] == true {
        blocked.push("private_model_requires_user_access".to_string());
    }
    if model["license"].is_null() || model["license"].as_str() == Some("unknown") {
        blocked.push("model_license_must_be_reviewed".to_string());
    }
    if model["runtime_hints"]["remote_code_mapping_present"] == true {
        blocked.push("untrusted_remote_code_requires_independent_review".to_string());
    }
    blocked
}

fn rank(a: &Value, b: &Value) -> Ordering {
    let av = a["verification_status"] == "verified_for_exact_model_provider";
    let bv = b["verification_status"] == "verified_for_exact_model_provider";
    bv.cmp(&av)
        .then_with(|| b["match_score"].as_i64().cmp(&a["match_score"].as_i64()))
        .then_with(|| {
            (b["delivery_route"] == "verified_prebuilt_oci_image")
                .cmp(&(a["delivery_route"] == "verified_prebuilt_oci_image"))
        })
        .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
}

pub(crate) fn resolve(model: &Value, provider: &Value) -> Result<Value, String> {
    let registry = registry()?;
    resolve_with_registry(model, provider, &registry)
}

fn resolve_with_registry(model: &Value, provider: &Value, config: &Value) -> Result<Value, String> {
    validate_registry(config)?;
    let id = model["id"].as_str().ok_or("Model id is required")?;
    let sha = model["revision_sha"].as_str().ok_or("Immutable revision SHA is required")?;
    if id.is_empty() || !model_sha_valid(sha) {
        return Err("Model must have an immutable, valid revision SHA".into());
    }
    let provider_id = provider["id"].as_str().ok_or("GPU provider id is required")?;
    let decision = model["media"]["classification_status"].as_str().unwrap_or("unclassified_requires_review");
    let reasons = access_blocks(model);
    let mut candidates = Vec::new();
    if decision != "non_media_task_out_of_scope" {
        for row in config["entries"].as_array().ok_or("Missing runtime entries")? {
            let Some((score, basis)) = match_row(row, model) else { continue; };
            let verified = verification_scope(row, model, provider);
            candidates.push(json!({
                "id": row["id"],
                "family": row["family"],
                "upstream_url": row["upstream_url"],
                "provenance": row["provenance"],
                "delivery_route": row["delivery_route"],
                "match_score": score,
                "match_basis": basis,
                "verification_status": if verified {
                    "verified_for_exact_model_provider"
                } else {
                    "candidate_unverified"
                },
                "worker_test_evidence": if verified { row["verification"]["evidence_url"].clone() } else { Value::Null },
                "loadable_inference_service": false,
                "model_deployment_allowed": false,
                "requires_human_approval": true
            }));
        }
    }
    candidates.sort_by(rank);
    // No automatic selection without exact-model, provider, worker and runtime evidence.
    let preferred = candidates.first().filter(|c| {
        reasons.is_empty()
            && c["verification_status"] == "verified_for_exact_model_provider"
    }).map(|c| c["id"].clone());
    let has_verified_evidence = candidates.iter().any(|c| {
        c["verification_status"] == "verified_for_exact_model_provider"
    });
    let status = if decision == "non_media_task_out_of_scope" {
        "non_media_task_out_of_scope"
    } else if candidates.is_empty() {
        "runtime_unknown_requires_review"
    } else if has_verified_evidence && !reasons.is_empty() {
        "verified_runtime_requires_access_or_safety_review"
    } else if preferred.is_none() {
        "unverified_runtime_candidates_found"
    } else {
        "verified_runtime_candidate_but_deployment_unavailable"
    };

    Ok(json!({
        "source": "bundled_versioned_runtime_registry",
        "registry_status": "read_only_candidates_no_deployment",
        "model_id": id,
        "revision_sha": sha,
        "provider_id": provider_id,
        "status": status,
        "candidate_count": candidates.len(),
        "candidates": candidates,
        "preferred_verified_candidate_id": preferred,
        "access_and_safety_requirements": reasons,
        "execution_enabled": false,
        "safe_to_deploy": false,
        "needs_new_human_deployment_approval": true,
        "next_step": "Verify the exact model, runtime image or installer, GPU provider, worker implementation and human deployment approval before enabling execution.",
        "note": "Official Hugging Face or upstream source is evidence of provenance, not evidence this specific model/worker/provider combination was tested."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(task: &str, library: &str, files: &[&str]) -> Value {
        json!({
            "id": "example/media",
            "revision_sha": "0123456789abcdef0123456789abcdef01234567",
            "task": task,
            "library": library,
            "license": "apache-2.0",
            "gated": false,
            "private": false,
            "media": {"classification_status":"known_media_task"},
            "runtime_hints": {
                "file_names": files,
                "architectures": [],
                "tags": [],
                "remote_code_mapping_present": false
            }
        })
    }

    fn provider() -> Value {
        json!({"id":"modal","capabilities":{"execute":false,"deploy":false}})
    }

    #[test]
    fn bundled_registry_is_valid_and_contains_both_routes() {
        let registry = registry().unwrap();
        let rows = registry["entries"].as_array().unwrap();
        assert!(rows.len() >= 16);
        for route in ["verified_prebuilt_oci_image", "vetted_runtime_recipe_on_base_image"] {
            assert!(rows.iter().any(|row| row["delivery_route"] == route));
        }
        assert!(rows.iter().all(|r| r["execution_enabled"] == false));
    }

    #[test]
    fn diffusers_uses_checkpoint_and_library_evidence_not_task_alone() {
        let detected = resolve(&model("text-to-image","diffusers",&["model_index.json"]), &provider()).unwrap();
        assert_eq!(detected["status"], "unverified_runtime_candidates_found");
        assert_eq!(detected["candidate_count"], 2);
        assert_eq!(detected["safe_to_deploy"], false);
        assert!(detected["preferred_verified_candidate_id"].is_null());
        let no_evidence = resolve(&model("text-to-image","", &[]), &provider()).unwrap();
        assert_eq!(no_evidence["status"], "runtime_unknown_requires_review");
        assert_eq!(no_evidence["candidate_count"], 0);
    }

    #[test]
    fn transformers_and_all_media_tasks_are_not_hardwired_to_images() {
        let asr = resolve(&model("automatic-speech-recognition","transformers",&["config.json"]), &provider()).unwrap();
        assert_eq!(asr["candidate_count"],2);
        assert_eq!(asr["candidates"][0]["family"],"transformers");
        let music = resolve(&model("text-to-music","audiocraft",&[]), &provider()).unwrap();
        assert_eq!(music["candidate_count"],2);
        let video = resolve(&model("text-to-video","diffusers",&["model_index.json"]), &provider()).unwrap();
        assert_eq!(video["candidate_count"],2);
    }

    #[test]
    fn exact_model_upstream_recipe_can_match_a_new_media_architecture() {
        let mut reg = registry().unwrap();
        let special = reg["entries"].as_array_mut().unwrap().iter_mut()
            .find(|row| row["id"] == "official-model-runtime-oci").unwrap();
        special["match"]["model_id_allowlist"] = json!(["example/media"]);

        let mut model = model("text-to-hologram","new-runtime",&["custom.safetensors"]);
        model["media"]["classification_status"] = json!("unclassified_requires_review");
        let candidate = resolve_with_registry(&model, &provider(), &reg).unwrap();
        assert_eq!(candidate["candidate_count"], 1);
        assert_eq!(candidate["candidates"][0]["family"], "specialized");
        assert_eq!(candidate["candidates"][0]["verification_status"], "candidate_unverified");
        assert_eq!(candidate["candidates"][0]["match_score"], 100);
        assert_eq!(candidate["safe_to_deploy"], false);

        model["id"] = json!("different/model");
        let unrecognized = resolve_with_registry(&model, &provider(), &reg).unwrap();
        assert_eq!(unrecognized["candidate_count"], 0);

        // A model ID allowlist never overrides a known pure text-only task.
        model["id"] = json!("example/media");
        model["media"]["classification_status"] = json!("non_media_task_out_of_scope");
        let excluded = resolve_with_registry(&model, &provider(), &reg).unwrap();
        assert_eq!(excluded["status"], "non_media_task_out_of_scope");
        assert_eq!(excluded["candidate_count"], 0);
    }

    #[test]
    fn unknown_and_text_only_models_fail_closed() {
        let mut v=model("experimental-hologram","mystery",&["unknown.data"]);
        v["media"]["classification_status"]=json!("unclassified_requires_review");
        let decision=resolve(&v,&provider()).unwrap();
        assert_eq!(decision["candidate_count"],0);
        assert_eq!(decision["safe_to_deploy"],false);
        v["media"]["classification_status"]=json!("non_media_task_out_of_scope");
        assert_eq!(resolve(&v,&provider()).unwrap()["status"],"non_media_task_out_of_scope");
    }

    #[test]
    fn gated_and_remote_code_are_explicitly_blocked() {
        let mut v=model("text-to-image","diffusers",&["model_index.json"]);
        v["gated"]=json!("manual");
        v["runtime_hints"]["remote_code_mapping_present"]=json!(true);
        let out=resolve(&v,&provider()).unwrap();
        let blockers=out["access_and_safety_requirements"].as_array().unwrap();
        assert!(blockers.contains(&json!("gated_model_requires_user_access")));
        assert!(blockers.contains(&json!("untrusted_remote_code_requires_independent_review")));
        assert_eq!(out["safe_to_deploy"],false);
    }

    #[test]
    fn fake_verified_label_never_passes_without_model_and_worker_evidence() {
        let mut reg=registry().unwrap();
        reg["entries"][0]["verification"]["status"]=json!("verified");
        let output=resolve_with_registry(
            &model("text-to-image","diffusers",&["model_index.json"]),
            &provider(),&reg
        ).unwrap();
        assert_eq!(output["status"],"unverified_runtime_candidates_found");
    }

    #[test]
    fn synthetic_exact_evidence_prefers_pinned_image_over_installer() {
        let mut reg=registry().unwrap();
        let sha="0123456789abcdef0123456789abcdef01234567";
        for row in reg["entries"].as_array_mut().unwrap() {
            if row["family"]!="diffusers" || !row["id"].as_str().unwrap().starts_with("hf-diffusers-image") {
                continue;
            }
            row["verification"]=json!({
                "status":"verified","model_tested":true,"provider_tested":true,
                "reviewer_approved":true,"tested_model_revision":sha,
                "tested_provider_ids":["modal"],"evidence_url":"https://example.org/test-evidence",
                "evidence_commit":sha,"worker_implementation_verified":true
            });
            row["delivery"]["worker_implementation_digest"]=json!(format!("sha256:{}", "a".repeat(64)));
            if row["delivery_route"]=="verified_prebuilt_oci_image" {
                row["delivery"]["oci_image_digest"]=json!(format!("docker.io/org/runtime@sha256:{}", "b".repeat(64)));
            } else {
                row["delivery"]["base_image_digest"]=json!(format!("docker.io/org/base@sha256:{}", "c".repeat(64)));
                row["delivery"]["pinned_install_manifest"]=json!(format!("sha256:{}", "d".repeat(64)));
            }
        }
        let result=resolve_with_registry(
            &model("text-to-image","diffusers",&["model_index.json"]),
            &provider(),&reg
        ).unwrap();
        assert_eq!(result["candidates"][0]["verification_status"],"verified_for_exact_model_provider");
        assert_eq!(result["candidates"][0]["delivery_route"],"verified_prebuilt_oci_image");
        assert_eq!(result["safe_to_deploy"],false);
        assert_eq!(result["execution_enabled"],false);
        assert_eq!(result["status"],"verified_runtime_candidate_but_deployment_unavailable");

        // A published gated model still cannot be chosen for deployment
        // without independent user-access review, even when model-specific
        // runtime evidence exists.
        let mut gated = model("text-to-image","diffusers",&["model_index.json"]);
        gated["gated"] = json!("manual");
        let gated_result = resolve_with_registry(&gated, &provider(), &reg).unwrap();
        assert_eq!(gated_result["status"], "verified_runtime_requires_access_or_safety_review");
        assert!(gated_result["preferred_verified_candidate_id"].is_null());
        assert_eq!(gated_result["safe_to_deploy"], false);
    }

    #[test]
    fn missing_model_revision_and_modified_registry_are_rejected() {
        let mut v=model("text-to-image","diffusers",&["model_index.json"]);
        v["revision_sha"]=json!("main");
        assert!(resolve(&v,&provider()).is_err());
        let mut reg=registry().unwrap();
        reg["entries"][0]["execution_enabled"]=json!(true);
        assert!(resolve_with_registry(&model("text-to-image","diffusers",&["model_index.json"]), &provider(), &reg).is_err());
    }
}
