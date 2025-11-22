use actix_web::{web, HttpResponse, ResponseError, Result};
use tracing::{error, info};
use validator::Validate;

use crate::executor::ExecutionEngine;
use crate::models::{ConfigResponse, ExecuteRequest, ExecuteResponse, HealthResponse};
use crate::state::AppState;

pub async fn health_check(state: web::Data<AppState>) -> Result<HttpResponse> {
    let stats = state.get_stats().await;

    let docker_status = match state.container_manager.check_docker_health().await {
        Ok(version) => version,
        Err(e) => format!("Error: {}", e),
    };

    let response = HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: state.uptime_seconds(),
        active_executions: stats.active_executions,
        total_executions: stats.total_executions,
        docker_status,
    };

    Ok(HttpResponse::Ok().json(response))
}

pub async fn get_config(state: web::Data<AppState>) -> Result<HttpResponse> {
    let mut supported_languages = Vec::new();

    if state.config.enable_python {
        supported_languages.push("python".to_string());
    }
    if state.config.enable_javascript {
        supported_languages.push("javascript".to_string());
    }
    if state.config.enable_shell {
        supported_languages.push("shell".to_string());
    }
    if state.config.enable_rust {
        supported_languages.push("rust".to_string());
    }

    let response = ConfigResponse {
        max_execution_time: state.config.max_execution_time,
        max_memory_mb: state.config.max_memory_mb,
        max_cpu_quota: state.config.max_cpu_quota,
        supported_languages,
        rate_limit_per_minute: state.config.rate_limit_per_minute,
    };

    Ok(HttpResponse::Ok().json(response))
}

pub async fn execute_code(
    state: web::Data<AppState>,
    request: web::Json<ExecuteRequest>,
) -> Result<HttpResponse> {
    // Validate request
    if let Err(e) = request.validate() {
        error!("Invalid request: {}", e);
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "ValidationError",
            "message": format!("Invalid request: {}", e),
        })));
    }

    info!(
        "Received execution request for language: {}",
        request.language
    );

    // Check concurrent execution limit
    let stats = state.get_stats().await;
    if stats.active_executions >= state.config.max_concurrent_executions {
        return Ok(HttpResponse::TooManyRequests().json(serde_json::json!({
            "error": "TooManyRequests",
            "message": format!(
                "Maximum concurrent executions ({}) reached",
                state.config.max_concurrent_executions
            ),
        })));
    }

    // Increment active executions
    state.increment_executions().await;

    // Create execution engine with container pool for fast execution
    let engine = ExecutionEngine::new(
        state.container_manager.clone(),
        Some(state.container_pool.clone()),
        state.config.clone(),
    );

    // Execute code
    let result = engine.execute(request.into_inner()).await;

    // Decrement active executions
    state.decrement_active_executions().await;

    match result {
        Ok(response) => {
            info!("Execution completed: {}", response.execution_id);

            // Update stats
            let mut stats = state.stats.write().await;
            match response.status {
                crate::models::ExecutionStatus::Success => {
                    stats.successful_executions += 1;
                }
                crate::models::ExecutionStatus::Timeout => {
                    stats.timeout_executions += 1;
                    stats.failed_executions += 1;
                }
                crate::models::ExecutionStatus::Failed => {
                    stats.failed_executions += 1;
                }
                _ => {}
            }

            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            error!("Execution error: {}", e);

            // Update failed stats
            let mut stats = state.stats.write().await;
            stats.failed_executions += 1;

            Ok(e.error_response())
        }
    }
}

pub async fn get_stats(state: web::Data<AppState>) -> Result<HttpResponse> {
    let stats = state.get_stats().await;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "total_executions": stats.total_executions,
        "active_executions": stats.active_executions,
        "successful_executions": stats.successful_executions,
        "failed_executions": stats.failed_executions,
        "timeout_executions": stats.timeout_executions,
        "uptime_seconds": state.uptime_seconds(),
        "success_rate": if stats.total_executions > 0 {
            (stats.successful_executions as f64 / stats.total_executions as f64) * 100.0
        } else {
            0.0
        },
    })))
}
