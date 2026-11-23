//! Read-only, data-driven media task discovery. Never grants executable runtime support.
use serde_json::{json, Value};
use std::collections::BTreeSet;

const MEDIA_TASKS: &str = include_str!("../../../config/media-tasks.json");
#[cfg(test)]
const MEDIA_REQUEST_SCHEMA: &str = include_str!("../../../schemas/media-request.schema.json");
#[cfg(test)]
const MEDIA_RESULT_SCHEMA: &str = include_str!("../../../schemas/media-result.schema.json");

pub(crate) fn catalog() -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(MEDIA_TASKS).map_err(|_| "Invalid media tasks catalog".to_string())?;
    if value["schema_version"] != 1
        || value["categories"].as_array().is_none()
        || value["tasks"].as_array().is_none()
        || value["classification_rules"].is_null()
    {
        return Err("Media tasks catalog has an invalid schema".into());
    }
    Ok(value)
}

fn tag_strings(tags: Option<&Value>) -> impl Iterator<Item = &str> {
    tags.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(100)
        .filter_map(Value::as_str)
        .filter(|s| s.len() <= 128)
}

fn contains_token(text: &str, keyword: &str) -> bool {
    let text = text.to_ascii_lowercase();
    // Prevent "mask" accidentally matching arbitrary "masked", but retain
    // common compound tags such as "gaussian-splat" and "point-cloud".
    let padded = format!("-{}-", text.replace([' ', '_', '/', ':', '.'], "-"));
    let target = format!("-{keyword}-");
    padded.contains(&target)
}

fn keyword_categories(config: &Value, text: &str, output: &mut BTreeSet<String>) {
    if let Some(categories) = config["media_keyword_categories"].as_object() {
        for (category, keywords) in categories {
            if let Some(keywords) = keywords.as_array() {
                if keywords
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|keyword| contains_token(text, keyword))
                {
                    output.insert(category.clone());
                }
            }
        }
    }
}

fn exact_task<'a>(catalog: &'a Value, tag: &str) -> Option<&'a Value> {
    catalog["tasks"]
        .as_array()?
        .iter()
        .find(|record| record["task"].as_str() == Some(tag))
}

fn is_non_media(config: &Value, task: &str) -> bool {
    config["non_media_pipeline_tags"]
        .as_array()
        .is_some_and(|tasks| tasks.iter().any(|known| known.as_str() == Some(task)))
}

/// Classification is a non-executable hint. An unknown pipeline tag is never
/// rejected from discovery and is never assumed to be a valid runtime.
pub(crate) fn classify(task: Option<&str>, tags: Option<&Value>) -> Value {
    let Ok(config) = catalog() else {
        return json!({
            "classification_status": "catalog_unavailable",
            "media_categories": [],
            "execution_status": "not_verified",
            "needs_runtime_review": true
        });
    };
    let declared = task.filter(|s| !s.is_empty()).unwrap_or("");
    let mut selected = if !declared.is_empty() {
        Some(declared)
    } else {
        None
    };
    let mut source = if selected.is_some() { "pipeline_tag" } else { "unknown" };

    // If the publisher did not provide a pipeline tag, look for an explicit
    // task tag before applying fuzzy hints.
    if selected.is_none() {
        for candidate in tag_strings(tags) {
            if exact_task(&config, candidate).is_some() {
                selected = Some(candidate);
                source = "hub_tags";
                break;
            }
        }
    }

    let selected = selected.unwrap_or("");
    let record = exact_task(&config, selected);
    let mut categories = BTreeSet::new();
    let mut inputs = json!([]);
    let mut outputs = json!([]);

    if let Some(record) = record {
        if let Some(values) = record["categories"].as_array() {
            for category in values.iter().filter_map(Value::as_str) {
                categories.insert(category.to_string());
            }
        }
        inputs = record["inputs"].clone();
        outputs = record["outputs"].clone();
    } else {
        keyword_categories(&config, selected, &mut categories);
    }
    // Tags such as "music" can specialize a broad "text-to-audio" task.
    // They do not confer any model loading or execution privileges.
    for tag in tag_strings(tags) {
        keyword_categories(&config, tag, &mut categories);
    }
    let status = if record.is_some() {
        "known_media_task"
    } else if !categories.is_empty() {
        "inferred_media_task_requires_review"
    } else if !selected.is_empty() && is_non_media(&config, selected) {
        "non_media_task_out_of_scope"
    } else {
        "unclassified_requires_review"
    };

    json!({
        "declared_task": if declared.is_empty() { Value::Null } else { json!(declared) },
        "matched_task": if selected.is_empty() { Value::Null } else { json!(selected) },
        "task_source": source,
        "media_categories": categories.into_iter().collect::<Vec<_>>(),
        "input_modalities": inputs,
        "output_modalities": outputs,
        "classification_status": status,
        "needs_runtime_review": record.is_none(),
        "execution_status": "not_verified",
        "note": "Task metadata is only a hint; a model requires an independently verified runtime recipe and a human-approved deployment."
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_media_job_contract_does_not_restrict_future_modality_names() {
        let request: Value = serde_json::from_str(MEDIA_REQUEST_SCHEMA).unwrap();
        let result: Value = serde_json::from_str(MEDIA_RESULT_SCHEMA).unwrap();
        assert_eq!(request["properties"]["task"]["type"], "string");
        assert!(request["properties"]["task"].get("enum").is_none());
        assert_eq!(request["$defs"]["typedInput"]["properties"]["kind"]["type"], "string");
        assert!(request["$defs"]["typedInput"]["properties"]["kind"].get("enum").is_none());
        assert_eq!(result["$defs"]["artifact"]["properties"]["kind"]["type"], "string");
        assert!(result["$defs"]["artifact"]["properties"]["kind"].get("enum").is_none());
        assert_eq!(result["properties"]["artifacts"]["type"], "array");
    }

    #[test]
    fn taxonomy_covers_all_media_families() {
        let cfg = catalog().unwrap();
        assert_eq!(cfg["categories"].as_array().unwrap().len(), 9);
        assert!(cfg["tasks"].as_array().unwrap().len() >= 50);
        for task in [
            "text-to-image",
            "image-text-to-video",
            "video-to-video",
            "text-to-audio",
            "text-to-speech",
            "voice-activity-detection",
            "text-to-music",
            "automatic-speech-recognition",
            "image-to-3d",
            "document-question-answering",
            "pose-estimation",
            "any-to-any",
        ] {
            assert_eq!(classify(Some(task), None)["classification_status"], "known_media_task");
        }
    }

    #[test]
    fn broad_audio_task_can_also_be_music() {
        let tags = json!(["diffusers", "music", "license:apache-2.0"]);
        let classification = classify(Some("text-to-audio"), Some(&tags));
        assert_eq!(classification["classification_status"], "known_media_task");
        assert!(classification["media_categories"].as_array().unwrap().contains(&json!("music")));
        assert!(classification["media_categories"].as_array().unwrap().contains(&json!("audio")));
        assert_eq!(classification["execution_status"], "not_verified");
    }

    #[test]
    fn future_and_untagged_models_are_not_dropped() {
        let inferred = classify(Some("text-to-holographic-video"), None);
        assert_eq!(
            inferred["classification_status"],
            "inferred_media_task_requires_review"
        );
        let unknown = classify(Some("new-research-task"), None);
        assert_eq!(unknown["classification_status"], "unclassified_requires_review");
        let missing = classify(None, None);
        assert_eq!(missing["classification_status"], "unclassified_requires_review");
        assert_eq!(unknown["execution_status"], "not_verified");
    }

    #[test]
    fn pure_text_model_is_out_of_scope_without_unsafe_fallback() {
        let text = classify(Some("text-generation"), None);
        assert_eq!(text["classification_status"], "non_media_task_out_of_scope");
        assert_eq!(text["execution_status"], "not_verified");
    }

    #[test]
    fn known_task_can_be_recovered_from_hub_tags() {
        let tags = json!(["pytorch", "image-to-video", "license:other"]);
        let item = classify(None, Some(&tags));
        assert_eq!(item["task_source"], "hub_tags");
        assert_eq!(item["classification_status"], "known_media_task");
    }
}
