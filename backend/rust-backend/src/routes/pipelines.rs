use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::error::{AppError, AppResult};
use crate::middleware::auth::{AdminMiddleware, AuthUser};
use crate::AppState;

#[derive(Debug, Serialize)]
struct PipelineInfo {
    url: String,
    idx: usize,
}

#[derive(Debug, Serialize)]
struct PipelineListResponse {
    data: Vec<PipelineInfo>,
}

#[derive(Debug, Deserialize)]
struct AddPipelineForm {
    url: String,
    #[serde(rename = "urlIdx")]
    url_idx: usize,
}

#[derive(Debug, Deserialize)]
struct DeletePipelineForm {
    id: String,
    #[serde(rename = "urlIdx")]
    url_idx: usize,
}

#[derive(Debug, Deserialize)]
struct PipelineQuery {
    #[serde(rename = "urlIdx")]
    url_idx: Option<usize>,
}

/// Helper function to get all models responses from OpenAI endpoints
async fn get_all_models_responses(state: &web::Data<AppState>) -> Vec<Option<serde_json::Value>> {
    let config = state.config.read().unwrap();
    let mut responses = Vec::new();

    let client = reqwest::Client::new();

    for (idx, url) in config.openai_api_base_urls.iter().enumerate() {
        let key = config
            .openai_api_keys
            .get(idx)
            .map(|s| s.as_str())
            .unwrap_or("");

        // Try to fetch models from this endpoint
        match client
            .get(format!("{}/models", url))
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => responses.push(Some(data)),
                    Err(_) => responses.push(None),
                }
            }
            _ => responses.push(None),
        }
    }

    responses
}

// GET /list - Get pipelines list (admin only)
async fn get_pipelines_list(
    state: web::Data<AppState>,
    _user: AuthUser,
) -> AppResult<HttpResponse> {
    // Get all models responses to check which endpoints have pipelines
    let responses = get_all_models_responses(&state).await;

    let config = state.config.read().unwrap();

    // Find URL indices that have "pipelines" field in their response
    let pipeline_urls: Vec<PipelineInfo> = responses
        .iter()
        .enumerate()
        .filter_map(|(idx, response)| {
            if let Some(data) = response {
                if data.get("pipelines").is_some() {
                    if idx < config.openai_api_base_urls.len() {
                        return Some(PipelineInfo {
                            url: config.openai_api_base_urls[idx].clone(),
                            idx,
                        });
                    }
                }
            }
            None
        })
        .collect();

    Ok(HttpResponse::Ok().json(PipelineListResponse {
        data: pipeline_urls,
    }))
}

// POST /upload - Upload pipeline (admin only)
async fn upload_pipeline(
    state: web::Data<AppState>,
    _user: AuthUser,
    mut payload: Multipart,
) -> AppResult<HttpResponse> {
    let mut url_idx: Option<usize> = None;
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    // Parse multipart form
    while let Some(item) = payload.next().await {
        let mut field =
            item.map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?;
        let content_disposition = field.content_disposition();
        let field_name = content_disposition
            .as_ref()
            .and_then(|cd| cd.get_name())
            .unwrap_or("");

        match field_name {
            "urlIdx" => {
                let mut data = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk =
                        chunk.map_err(|e| AppError::BadRequest(format!("Chunk error: {}", e)))?;
                    data.extend_from_slice(&chunk);
                }
                let idx_str = String::from_utf8(data)
                    .map_err(|e| AppError::BadRequest(format!("Invalid urlIdx: {}", e)))?;
                url_idx = idx_str.trim().parse().ok();
            }
            "file" => {
                filename = content_disposition
                    .as_ref()
                    .and_then(|cd| cd.get_filename())
                    .map(|s| s.to_string());

                let mut data = Vec::new();
                while let Some(chunk) = field.next().await {
                    let chunk =
                        chunk.map_err(|e| AppError::BadRequest(format!("Chunk error: {}", e)))?;
                    data.extend_from_slice(&chunk);
                }
                file_data = Some(data);
            }
            _ => {}
        }
    }

    let url_idx = url_idx.ok_or_else(|| AppError::BadRequest("urlIdx is required".to_string()))?;
    let file_data =
        file_data.ok_or_else(|| AppError::BadRequest("file is required".to_string()))?;
    let filename =
        filename.ok_or_else(|| AppError::BadRequest("filename is required".to_string()))?;

    // Check if file is a Python file
    if !filename.ends_with(".py") {
        return Err(AppError::BadRequest(
            "Only Python (.py) files are allowed".to_string(),
        ));
    }

    let (url, key) = {
        let config = state.config.read().unwrap();

        if url_idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(
                "Pipeline endpoint not found".to_string(),
            ));
        }

        let url = config.openai_api_base_urls[url_idx].clone();
        let key = config
            .openai_api_keys
            .get(url_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();

        (url, key)
    };

    // Forward the file upload to the pipeline endpoint
    let client = reqwest::Client::new();

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(file_data)
            .file_name(filename)
            .mime_str("text/x-python")
            .unwrap(),
    );

    match client
        .post(format!("{}/pipelines/upload", url))
        .header("Authorization", format!("Bearer {}", key))
        .multipart(form)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let data = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::InternalServerError(format!("Parse error: {}", e)))?;
            Ok(HttpResponse::Ok().json(data))
        }
        Ok(response) => {
            let status = response.status();
            let error_data = response.json::<serde_json::Value>().await.ok();

            let detail = error_data
                .as_ref()
                .and_then(|d| d.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("Pipeline not found");

            Err(AppError::ExternalServiceError(format!(
                "Pipeline endpoint error ({}): {}",
                status, detail
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Connection error: {}",
            e
        ))),
    }
}

// POST /add - Add pipeline from URL (admin only)
async fn add_pipeline(
    state: web::Data<AppState>,
    _user: AuthUser,
    form: web::Json<AddPipelineForm>,
) -> AppResult<HttpResponse> {
    let (endpoint_url, key) = {
        let config = state.config.read().unwrap();

        if form.url_idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(
                "Pipeline endpoint not found".to_string(),
            ));
        }

        let endpoint_url = config.openai_api_base_urls[form.url_idx].clone();
        let key = config
            .openai_api_keys
            .get(form.url_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();

        (endpoint_url, key)
    };

    // Forward the request to the pipeline endpoint
    let client = reqwest::Client::new();

    match client
        .post(format!("{}/pipelines/add", endpoint_url))
        .header("Authorization", format!("Bearer {}", key))
        .json(&serde_json::json!({
            "url": form.url
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let data = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::InternalServerError(format!("Parse error: {}", e)))?;
            Ok(HttpResponse::Ok().json(data))
        }
        Ok(response) => {
            let status = response.status();
            let error_data = response.json::<serde_json::Value>().await.ok();

            let detail = error_data
                .as_ref()
                .and_then(|d| d.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("Pipeline not found");

            Err(AppError::ExternalServiceError(format!(
                "Pipeline endpoint error ({}): {}",
                status, detail
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Connection error: {}",
            e
        ))),
    }
}

// DELETE /delete - Delete pipeline (admin only)
async fn delete_pipeline(
    state: web::Data<AppState>,
    _user: AuthUser,
    form: web::Json<DeletePipelineForm>,
) -> AppResult<HttpResponse> {
    let (endpoint_url, key) = {
        let config = state.config.read().unwrap();

        if form.url_idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(
                "Pipeline endpoint not found".to_string(),
            ));
        }

        let endpoint_url = config.openai_api_base_urls[form.url_idx].clone();
        let key = config
            .openai_api_keys
            .get(form.url_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();

        (endpoint_url, key)
    };

    // Forward the request to the pipeline endpoint
    let client = reqwest::Client::new();

    match client
        .delete(format!("{}/pipelines/delete", endpoint_url))
        .header("Authorization", format!("Bearer {}", key))
        .json(&serde_json::json!({
            "id": form.id
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let data = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::InternalServerError(format!("Parse error: {}", e)))?;
            Ok(HttpResponse::Ok().json(data))
        }
        Ok(response) => {
            let status = response.status();
            let error_data = response.json::<serde_json::Value>().await.ok();

            let detail = error_data
                .as_ref()
                .and_then(|d| d.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("Pipeline not found");

            Err(AppError::ExternalServiceError(format!(
                "Pipeline endpoint error ({}): {}",
                status, detail
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Connection error: {}",
            e
        ))),
    }
}

// GET / - Get all pipelines (admin only)
async fn get_all_pipelines(
    state: web::Data<AppState>,
    _user: AuthUser,
    query: web::Query<PipelineQuery>,
) -> AppResult<HttpResponse> {
    let url_idx = query
        .url_idx
        .ok_or_else(|| AppError::BadRequest("urlIdx parameter is required".to_string()))?;

    let (endpoint_url, key) = {
        let config = state.config.read().unwrap();

        if url_idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(
                "Pipeline endpoint not found".to_string(),
            ));
        }

        let endpoint_url = config.openai_api_base_urls[url_idx].clone();
        let key = config
            .openai_api_keys
            .get(url_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();

        (endpoint_url, key)
    };

    // Fetch pipelines from the endpoint
    let client = reqwest::Client::new();

    match client
        .get(format!("{}/pipelines", endpoint_url))
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let data = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::InternalServerError(format!("Parse error: {}", e)))?;
            Ok(HttpResponse::Ok().json(data))
        }
        Ok(response) => {
            let status = response.status();
            let error_data = response.json::<serde_json::Value>().await.ok();

            let detail = error_data
                .as_ref()
                .and_then(|d| d.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("Pipeline not found");

            Err(AppError::ExternalServiceError(format!(
                "Pipeline endpoint error ({}): {}",
                status, detail
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Connection error: {}",
            e
        ))),
    }
}

// GET /{pipeline_id}/valves - Get pipeline valves (admin only)
async fn get_pipeline_valves(
    state: web::Data<AppState>,
    _user: AuthUser,
    pipeline_id: web::Path<String>,
    query: web::Query<PipelineQuery>,
) -> AppResult<HttpResponse> {
    let url_idx = query
        .url_idx
        .ok_or_else(|| AppError::BadRequest("urlIdx parameter is required".to_string()))?;

    let (endpoint_url, key) = {
        let config = state.config.read().unwrap();

        if url_idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(
                "Pipeline endpoint not found".to_string(),
            ));
        }

        let endpoint_url = config.openai_api_base_urls[url_idx].clone();
        let key = config
            .openai_api_keys
            .get(url_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();

        (endpoint_url, key)
    };

    let client = reqwest::Client::new();

    match client
        .get(format!("{}/{}/valves", endpoint_url, pipeline_id))
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let data = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::InternalServerError(format!("Parse error: {}", e)))?;
            Ok(HttpResponse::Ok().json(data))
        }
        Ok(response) => {
            let status = response.status();
            let error_data = response.json::<serde_json::Value>().await.ok();

            let detail = error_data
                .as_ref()
                .and_then(|d| d.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("Pipeline not found");

            Err(AppError::ExternalServiceError(format!(
                "Pipeline endpoint error ({}): {}",
                status, detail
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Connection error: {}",
            e
        ))),
    }
}

// GET /{pipeline_id}/valves/spec - Get pipeline valves spec (admin only)
async fn get_pipeline_valves_spec(
    state: web::Data<AppState>,
    _user: AuthUser,
    pipeline_id: web::Path<String>,
    query: web::Query<PipelineQuery>,
) -> AppResult<HttpResponse> {
    let url_idx = query
        .url_idx
        .ok_or_else(|| AppError::BadRequest("urlIdx parameter is required".to_string()))?;

    let (endpoint_url, key) = {
        let config = state.config.read().unwrap();

        if url_idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(
                "Pipeline endpoint not found".to_string(),
            ));
        }

        let endpoint_url = config.openai_api_base_urls[url_idx].clone();
        let key = config
            .openai_api_keys
            .get(url_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();

        (endpoint_url, key)
    };

    let client = reqwest::Client::new();

    match client
        .get(format!("{}/{}/valves/spec", endpoint_url, pipeline_id))
        .header("Authorization", format!("Bearer {}", key))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let data = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::InternalServerError(format!("Parse error: {}", e)))?;
            Ok(HttpResponse::Ok().json(data))
        }
        Ok(response) => {
            let status = response.status();
            let error_data = response.json::<serde_json::Value>().await.ok();

            let detail = error_data
                .as_ref()
                .and_then(|d| d.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("Pipeline not found");

            Err(AppError::ExternalServiceError(format!(
                "Pipeline endpoint error ({}): {}",
                status, detail
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Connection error: {}",
            e
        ))),
    }
}

// POST /{pipeline_id}/valves/update - Update pipeline valves (admin only)
async fn update_pipeline_valves(
    state: web::Data<AppState>,
    _user: AuthUser,
    pipeline_id: web::Path<String>,
    query: web::Query<PipelineQuery>,
    form: web::Json<serde_json::Value>,
) -> AppResult<HttpResponse> {
    let url_idx = query
        .url_idx
        .ok_or_else(|| AppError::BadRequest("urlIdx parameter is required".to_string()))?;

    let (endpoint_url, key) = {
        let config = state.config.read().unwrap();

        if url_idx >= config.openai_api_base_urls.len() {
            return Err(AppError::NotFound(
                "Pipeline endpoint not found".to_string(),
            ));
        }

        let endpoint_url = config.openai_api_base_urls[url_idx].clone();
        let key = config
            .openai_api_keys
            .get(url_idx)
            .map(|s| s.to_string())
            .unwrap_or_default();

        (endpoint_url, key)
    };

    let client = reqwest::Client::new();

    match client
        .post(format!("{}/{}/valves/update", endpoint_url, pipeline_id))
        .header("Authorization", format!("Bearer {}", key))
        .json(&form.into_inner())
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let data = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::InternalServerError(format!("Parse error: {}", e)))?;
            Ok(HttpResponse::Ok().json(data))
        }
        Ok(response) => {
            let status = response.status();
            let error_data = response.json::<serde_json::Value>().await.ok();

            let detail = error_data
                .as_ref()
                .and_then(|d| d.get("detail"))
                .and_then(|v| v.as_str())
                .unwrap_or("Pipeline not found");

            Err(AppError::ExternalServiceError(format!(
                "Pipeline endpoint error ({}): {}",
                status, detail
            )))
        }
        Err(e) => Err(AppError::InternalServerError(format!(
            "Connection error: {}",
            e
        ))),
    }
}

pub fn create_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .wrap(AdminMiddleware)
            .route("/list", web::get().to(get_pipelines_list))
            .route("/upload", web::post().to(upload_pipeline))
            .route("/add", web::post().to(add_pipeline))
            .route("/delete", web::delete().to(delete_pipeline))
            .route("/", web::get().to(get_all_pipelines))
            .route("/{pipeline_id}/valves", web::get().to(get_pipeline_valves))
            .route(
                "/{pipeline_id}/valves/spec",
                web::get().to(get_pipeline_valves_spec),
            )
            .route(
                "/{pipeline_id}/valves/update",
                web::post().to(update_pipeline_valves),
            ),
    );
}
