use crate::middleware::auth::AuthUser;
use crate::services::sandbox_executor::SandboxExecutorClient;
/// Code execution routes using secure Sandbox Executor
/// This replaces Jupyter-based code execution with a secure, isolated sandbox
use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecutionRequest {
    pub code: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecutionResponse {
    pub execution_id: String,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<String>,
    pub execution_time_ms: u64,
    pub memory_used_mb: Option<f64>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

pub struct CodeExecutionService {
    client: Arc<SandboxExecutorClient>,
}

impl CodeExecutionService {
    pub fn new(sandbox_url: String) -> Self {
        Self {
            client: Arc::new(SandboxExecutorClient::new(sandbox_url)),
        }
    }
}

/// Execute code in secure sandbox
pub async fn execute_code(
    user: AuthUser,
    request: web::Json<CodeExecutionRequest>,
    service: web::Data<CodeExecutionService>,
) -> Result<HttpResponse> {
    info!(
        "Code execution request from user {} for language: {}",
        user.id, request.language
    );

    let result = service
        .client
        .execute_code(
            request.code.clone(),
            request.language.clone(),
            request.timeout,
            Some(user.id.clone()),
            None,
        )
        .await;

    match result {
        Ok(response) => {
            info!(
                "Code execution {} completed with status: {}",
                response.execution_id, response.status
            );
            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            error!("Code execution failed: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "ExecutionFailed",
                "message": e,
            })))
        }
    }
}

/// Get sandbox executor health status
pub async fn health_check(service: web::Data<CodeExecutionService>) -> Result<HttpResponse> {
    match service.client.health_check().await {
        Ok(health) => Ok(HttpResponse::Ok().json(health)),
        Err(e) => Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": "SandboxUnavailable",
            "message": e,
        }))),
    }
}

/// Get sandbox executor configuration
pub async fn get_config(
    _user: AuthUser,
    service: web::Data<CodeExecutionService>,
) -> Result<HttpResponse> {
    match service.client.get_config().await {
        Ok(config) => Ok(HttpResponse::Ok().json(config)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "ConfigFetchFailed",
            "message": e,
        }))),
    }
}

/// Get sandbox executor statistics (admin only)
pub async fn get_stats(
    user: AuthUser,
    service: web::Data<CodeExecutionService>,
) -> Result<HttpResponse> {
    // TODO: Add admin role check
    // For now, any authenticated user can see stats

    match service.client.get_stats().await {
        Ok(stats) => Ok(HttpResponse::Ok().json(stats)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "StatsFetchFailed",
            "message": e,
        }))),
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/code")
            .route("/execute", web::post().to(execute_code))
            .route("/sandbox/health", web::get().to(health_check))
            .route("/sandbox/config", web::get().to(get_config))
            .route("/sandbox/stats", web::get().to(get_stats)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_execution_request() {
        let req = CodeExecutionRequest {
            code: "print('hello')".to_string(),
            language: "python".to_string(),
            timeout: Some(30),
        };
        assert_eq!(req.language, "python");
    }
}
