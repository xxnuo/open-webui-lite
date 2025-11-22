use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, warn};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineFilter {
    pub id: String,
    pub url_idx: usize,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRequest {
    pub user: UserInfo,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub role: String,
}

/// Process pipeline inlet filters
/// Inlet filters modify the request before it's sent to the model
#[allow(dead_code)]
pub async fn process_pipeline_inlet_filter(
    mut payload: Value,
    model_id: &str,
    user: &UserInfo,
    pipeline_filters: &[PipelineFilter],
    base_urls: &[String],
    api_keys: &[String],
) -> AppResult<Value> {
    debug!(
        "Processing {} inlet filters for model {}",
        pipeline_filters.len(),
        model_id
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    for filter in pipeline_filters {
        if filter.url_idx >= base_urls.len() {
            warn!(
                "Invalid url_idx {} for filter {}",
                filter.url_idx, filter.id
            );
            continue;
        }

        let base_url = &base_urls[filter.url_idx];
        let api_key = api_keys
            .get(filter.url_idx)
            .map(|s| s.as_str())
            .unwrap_or("");

        if api_key.is_empty() {
            warn!(
                "No API key for filter {} at url_idx {}",
                filter.id, filter.url_idx
            );
            continue;
        }

        let url = format!("{}/{}/filter/inlet", base_url, filter.id);

        let request_body = PipelineRequest {
            user: user.clone(),
            body: payload.clone(),
        };

        debug!("Calling inlet filter: {} at {}", filter.id, url);

        match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request_body)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<Value>().await {
                    Ok(modified_payload) => {
                        payload = modified_payload;
                        debug!("Inlet filter {} applied successfully", filter.id);
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse response from inlet filter {}: {}",
                            filter.id, e
                        );
                        return Err(AppError::ExternalServiceError(format!(
                            "Inlet filter {} returned invalid response",
                            filter.id
                        )));
                    }
                }
            }
            Ok(response) => {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                error!(
                    "Inlet filter {} failed: {} - {}",
                    filter.id, status, error_text
                );
                return Err(AppError::ExternalServiceError(format!(
                    "Inlet filter {} failed: {}",
                    filter.id, status
                )));
            }
            Err(e) => {
                error!("Failed to call inlet filter {}: {}", filter.id, e);
                return Err(AppError::ExternalServiceError(format!(
                    "Failed to call inlet filter {}: {}",
                    filter.id, e
                )));
            }
        }
    }

    Ok(payload)
}

/// Process pipeline outlet filters
/// Outlet filters modify the response after it's received from the model
#[allow(dead_code)]
pub async fn process_pipeline_outlet_filter(
    mut payload: Value,
    model_id: &str,
    user: &UserInfo,
    pipeline_filters: &[PipelineFilter],
    base_urls: &[String],
    api_keys: &[String],
) -> AppResult<Value> {
    debug!(
        "Processing {} outlet filters for model {}",
        pipeline_filters.len(),
        model_id
    );

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Outlet filters are processed in reverse order
    for filter in pipeline_filters.iter().rev() {
        if filter.url_idx >= base_urls.len() {
            warn!(
                "Invalid url_idx {} for filter {}",
                filter.url_idx, filter.id
            );
            continue;
        }

        let base_url = &base_urls[filter.url_idx];
        let api_key = api_keys
            .get(filter.url_idx)
            .map(|s| s.as_str())
            .unwrap_or("");

        if api_key.is_empty() {
            warn!(
                "No API key for filter {} at url_idx {}",
                filter.id, filter.url_idx
            );
            continue;
        }

        let url = format!("{}/{}/filter/outlet", base_url, filter.id);

        let request_body = PipelineRequest {
            user: user.clone(),
            body: payload.clone(),
        };

        debug!("Calling outlet filter: {} at {}", filter.id, url);

        match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&request_body)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<Value>().await {
                    Ok(modified_payload) => {
                        payload = modified_payload;
                        debug!("Outlet filter {} applied successfully", filter.id);
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse response from outlet filter {}: {}",
                            filter.id, e
                        );
                        // For outlet filters, we might want to continue even if one fails
                        warn!("Skipping outlet filter {} due to parse error", filter.id);
                    }
                }
            }
            Ok(response) => {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                warn!(
                    "Outlet filter {} failed: {} - {}",
                    filter.id, status, error_text
                );
                // Continue processing other filters even if one fails
            }
            Err(e) => {
                warn!("Failed to call outlet filter {}: {}", filter.id, e);
                // Continue processing other filters
            }
        }
    }

    Ok(payload)
}

/// Get sorted filters for a model based on priority
#[allow(dead_code)]
pub fn get_sorted_filters(model_id: &str, models: &HashMap<String, Value>) -> Vec<PipelineFilter> {
    let mut filters = Vec::new();

    // Get model configuration
    if let Some(model) = models.get(model_id) {
        // Check if model has filters
        if let Some(model_filters) = model.get("filters").and_then(|f| f.as_array()) {
            for filter_value in model_filters {
                if let Ok(filter) = serde_json::from_value::<PipelineFilter>(filter_value.clone()) {
                    filters.push(filter);
                }
            }
        }

        // Check if the model itself is a pipeline filter
        if let Some(pipeline) = model.get("pipeline") {
            if let Some(id) = model.get("id").and_then(|i| i.as_str()) {
                if let Some(url_idx) = model.get("urlIdx").and_then(|i| i.as_u64()) {
                    let priority = pipeline
                        .get("priority")
                        .and_then(|p| p.as_i64())
                        .unwrap_or(0) as i32;

                    filters.push(PipelineFilter {
                        id: id.to_string(),
                        url_idx: url_idx as usize,
                        priority,
                    });
                }
            }
        }
    }

    // Sort by priority (lower priority values are processed first)
    filters.sort_by_key(|f| f.priority);

    debug!("Found {} filters for model {}", filters.len(), model_id);
    filters
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_get_sorted_filters() {
        let mut models = HashMap::new();

        models.insert(
            "test-model".to_string(),
            json!({
                "id": "test-model",
                "filters": [
                    {
                        "id": "filter-1",
                        "url_idx": 0,
                        "priority": 10
                    },
                    {
                        "id": "filter-2",
                        "url_idx": 0,
                        "priority": 5
                    }
                ]
            }),
        );

        let filters = get_sorted_filters("test-model", &models);

        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].id, "filter-2"); // Lower priority comes first
        assert_eq!(filters[1].id, "filter-1");
    }

    #[test]
    fn test_get_sorted_filters_with_pipeline() {
        let mut models = HashMap::new();

        models.insert(
            "test-model".to_string(),
            json!({
                "id": "test-model",
                "urlIdx": 0,
                "pipeline": {
                    "type": "filter",
                    "priority": 15
                }
            }),
        );

        let filters = get_sorted_filters("test-model", &models);

        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].id, "test-model");
        assert_eq!(filters[0].priority, 15);
    }
}
