use crate::error::{AppError, AppResult};
use crate::models::user::User;
use crate::AppState;
use actix_web::web;
use serde_json::Value;

/// Get sorted pipeline filters for a given model
/// Filters are sorted by priority
pub fn get_sorted_filters(
    model_id: &str,
    models: &std::collections::HashMap<String, Value>,
) -> Vec<Value> {
    let mut filters: Vec<Value> = models
        .values()
        .filter(|model| {
            // Check if model has pipeline type "filter"
            if let Some(pipeline) = model.get("pipeline") {
                if let Some(filter_type) = pipeline.get("type").and_then(|t| t.as_str()) {
                    if filter_type == "filter" {
                        // Check if this filter applies to our model
                        if let Some(pipelines) =
                            pipeline.get("pipelines").and_then(|p| p.as_array())
                        {
                            // "*" means apply to all models
                            if pipelines.iter().any(|p| p.as_str() == Some("*")) {
                                return true;
                            }

                            // Check if our model_id is in the list
                            if pipelines.iter().any(|p| p.as_str() == Some(model_id)) {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        })
        .cloned()
        .collect();

    // Sort by priority (ascending)
    filters.sort_by(|a, b| {
        let priority_a = a
            .get("pipeline")
            .and_then(|p| p.get("priority"))
            .and_then(|p| p.as_i64())
            .unwrap_or(0);
        let priority_b = b
            .get("pipeline")
            .and_then(|p| p.get("priority"))
            .and_then(|p| p.as_i64())
            .unwrap_or(0);
        priority_a.cmp(&priority_b)
    });

    filters
}

/// Process pipeline inlet filter (before chat completion)
/// This modifies the payload before it's sent to the LLM
pub async fn process_pipeline_inlet_filter(
    state: &web::Data<AppState>,
    mut payload: Value,
    user: &User,
    models: &std::collections::HashMap<String, Value>,
) -> AppResult<Value> {
    let model_id = payload
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| AppError::BadRequest("Model ID is required".to_string()))?;

    let mut sorted_filters = get_sorted_filters(model_id, models);

    // Add the model itself if it has a pipeline
    if let Some(model) = models.get(model_id) {
        if model.get("pipeline").is_some() {
            sorted_filters.push(model.clone());
        }
    }

    let config = state.config.read().unwrap();
    let client = reqwest::Client::new();

    // Create user object
    let user_obj = serde_json::json!({
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "role": user.role,
    });

    for filter in sorted_filters {
        // Get urlIdx from filter
        let url_idx = filter
            .get("urlIdx")
            .and_then(|i| i.as_u64())
            .map(|i| i as usize);

        if let Some(idx) = url_idx {
            if idx >= config.openai_api_base_urls.len() {
                tracing::warn!("Pipeline filter urlIdx {} out of bounds", idx);
                continue;
            }

            let url = &config.openai_api_base_urls[idx];
            let key = config
                .openai_api_keys
                .get(idx)
                .map(|s| s.as_str())
                .unwrap_or("");

            if key.is_empty() {
                tracing::warn!("Pipeline filter urlIdx {} has no API key", idx);
                continue;
            }

            let filter_id = filter
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("unknown");

            let request_data = serde_json::json!({
                "user": user_obj,
                "body": payload,
            });

            // Call the filter's inlet endpoint
            match client
                .post(format!("{}/{}/filter/inlet", url, filter_id))
                .header("Authorization", format!("Bearer {}", key))
                .header("Content-Type", "application/json")
                .json(&request_data)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    // Update payload with the filtered result
                    match response.json::<Value>().await {
                        Ok(filtered_payload) => {
                            payload = filtered_payload;
                            tracing::debug!(
                                "Pipeline inlet filter {} applied successfully",
                                filter_id
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to parse pipeline inlet filter {} response: {}",
                                filter_id,
                                e
                            );
                        }
                    }
                }
                Ok(response) => {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    tracing::error!(
                        "Pipeline inlet filter {} error: {} - {}",
                        filter_id,
                        status,
                        error_text
                    );

                    // Check if error response has detail
                    if let Ok(error_json) = serde_json::from_str::<Value>(&error_text) {
                        if let Some(detail) = error_json.get("detail") {
                            return Err(AppError::ExternalServiceError(format!(
                                "Pipeline filter error: {}",
                                detail
                            )));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Pipeline inlet filter {} connection error: {}",
                        filter_id,
                        e
                    );
                }
            }
        }
    }

    Ok(payload)
}

/// Process pipeline outlet filter (after chat completion)
/// This modifies the response after it's received from the LLM
pub async fn process_pipeline_outlet_filter(
    state: &web::Data<AppState>,
    mut payload: Value,
    user: &User,
    models: &std::collections::HashMap<String, Value>,
) -> AppResult<Value> {
    let model_id = payload
        .get("model")
        .and_then(|m| m.as_str())
        .ok_or_else(|| AppError::BadRequest("Model ID is required".to_string()))?;

    let mut sorted_filters = get_sorted_filters(model_id, models);

    // For outlet, prepend the model itself if it has a pipeline (reverse order)
    if let Some(model) = models.get(model_id) {
        if model.get("pipeline").is_some() {
            sorted_filters.insert(0, model.clone());
        }
    }

    let config = state.config.read().unwrap();
    let client = reqwest::Client::new();

    // Create user object
    let user_obj = serde_json::json!({
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "role": user.role,
    });

    for filter in sorted_filters {
        // Get urlIdx from filter
        let url_idx = filter
            .get("urlIdx")
            .and_then(|i| i.as_u64())
            .map(|i| i as usize);

        if let Some(idx) = url_idx {
            if idx >= config.openai_api_base_urls.len() {
                tracing::warn!("Pipeline filter urlIdx {} out of bounds", idx);
                continue;
            }

            let url = &config.openai_api_base_urls[idx];
            let key = config
                .openai_api_keys
                .get(idx)
                .map(|s| s.as_str())
                .unwrap_or("");

            if key.is_empty() {
                tracing::warn!("Pipeline filter urlIdx {} has no API key", idx);
                continue;
            }

            let filter_id = filter
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("unknown");

            let request_data = serde_json::json!({
                "user": user_obj,
                "body": payload,
            });

            // Call the filter's outlet endpoint
            match client
                .post(format!("{}/{}/filter/outlet", url, filter_id))
                .header("Authorization", format!("Bearer {}", key))
                .header("Content-Type", "application/json")
                .json(&request_data)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    // Update payload with the filtered result
                    match response.json::<Value>().await {
                        Ok(filtered_payload) => {
                            payload = filtered_payload;
                            tracing::debug!(
                                "Pipeline outlet filter {} applied successfully",
                                filter_id
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to parse pipeline outlet filter {} response: {}",
                                filter_id,
                                e
                            );
                        }
                    }
                }
                Ok(response) => {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    tracing::error!(
                        "Pipeline outlet filter {} error: {} - {}",
                        filter_id,
                        status,
                        error_text
                    );

                    // Check if error response has detail
                    if let Ok(error_json) = serde_json::from_str::<Value>(&error_text) {
                        if let Some(detail) = error_json.get("detail") {
                            // For outlet filters, log but don't fail the request
                            tracing::error!("Pipeline outlet filter error: {}", detail);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Pipeline outlet filter {} connection error: {}",
                        filter_id,
                        e
                    );
                }
            }
        }
    }

    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sorted_filters() {
        let mut models = std::collections::HashMap::new();

        // Model with filter pipeline
        models.insert(
            "filter1".to_string(),
            serde_json::json!({
                "id": "filter1",
                "pipeline": {
                    "type": "filter",
                    "priority": 10,
                    "pipelines": ["*"]
                }
            }),
        );

        models.insert(
            "filter2".to_string(),
            serde_json::json!({
                "id": "filter2",
                "pipeline": {
                    "type": "filter",
                    "priority": 5,
                    "pipelines": ["gpt-4"]
                }
            }),
        );

        models.insert(
            "filter3".to_string(),
            serde_json::json!({
                "id": "filter3",
                "pipeline": {
                    "type": "filter",
                    "priority": 20,
                    "pipelines": ["gpt-3.5-turbo"]
                }
            }),
        );

        // Test for gpt-4 model
        let filters = get_sorted_filters("gpt-4", &models);
        assert_eq!(filters.len(), 2); // filter2 and filter1 (matches *)

        // Should be sorted by priority: filter2 (5), filter1 (10)
        assert_eq!(
            filters[0].get("id").and_then(|id| id.as_str()),
            Some("filter2")
        );
        assert_eq!(
            filters[1].get("id").and_then(|id| id.as_str()),
            Some("filter1")
        );
    }
}
