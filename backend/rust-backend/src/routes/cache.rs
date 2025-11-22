//! Cache management API routes
//!
//! Provides endpoints for monitoring and managing the cache system

use actix_web::{delete, get, post, web, HttpResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cache_manager::CacheManager;
use crate::error::AppError;
use crate::middleware::auth::AuthUser;
use crate::utils::cache::Cache;

/// Helper to check if user is admin
fn check_admin(user: &AuthUser) -> Result<(), AppError> {
    if user.role != "admin" {
        return Err(AppError::Forbidden("Admin access required".to_string()));
    }
    Ok(())
}

/// Get cache statistics
#[get("/cache/stats")]
pub async fn get_cache_stats(user: AuthUser) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    let manager = CacheManager::get_or_init();
    let stats = manager.get_stats().await;

    Ok(HttpResponse::Ok().json(stats))
}

/// Clear all caches
#[delete("/cache")]
pub async fn clear_all_caches(user: AuthUser) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    let manager = CacheManager::get_or_init();
    manager
        .clear_all()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to clear caches: {}", e)))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "All caches cleared successfully"
    })))
}

/// Clear a specific cache by name
#[delete("/cache/{cache_name}")]
pub async fn clear_cache(
    user: AuthUser,
    cache_name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    let manager = CacheManager::get_or_init();
    let cache_name = cache_name.into_inner();

    let result = match cache_name.as_str() {
        "app" => manager.app_cache.clear().await,
        "session" => manager.session_cache.clear().await,
        "model" => manager.model_cache.clear().await,
        "api" => manager.api_cache.clear().await,
        _ => {
            return Err(AppError::BadRequest(format!(
                "Unknown cache name: {}. Valid options: app, session, model, api",
                cache_name
            )))
        }
    };

    result.map_err(|e| {
        AppError::InternalServerError(format!("Failed to clear {} cache: {}", cache_name, e))
    })?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": format!("{} cache cleared successfully", cache_name)
    })))
}

#[derive(Deserialize)]
pub struct DeleteKeyRequest {
    pub key: String,
}

/// Delete a specific cache key
#[delete("/cache/key")]
pub async fn delete_cache_key(
    user: AuthUser,
    req: web::Json<DeleteKeyRequest>,
) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    let manager = CacheManager::get_or_init();

    let deleted =
        manager.app_cache.delete(&req.key).await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to delete cache key: {}", e))
        })?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "deleted": deleted,
        "key": req.key
    })))
}

#[derive(Deserialize)]
pub struct BatchDeleteRequest {
    pub keys: Vec<String>,
}

/// Delete multiple cache keys at once
#[delete("/cache/keys")]
pub async fn delete_cache_keys(
    user: AuthUser,
    req: web::Json<BatchDeleteRequest>,
) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    let manager = CacheManager::get_or_init();

    let count = manager
        .app_cache
        .delete_many(&req.keys)
        .await
        .map_err(|e| {
            AppError::InternalServerError(format!("Failed to delete cache keys: {}", e))
        })?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "deleted_count": count,
        "requested_count": req.keys.len()
    })))
}

#[derive(Deserialize)]
pub struct CheckKeyRequest {
    pub key: String,
}

/// Check if a key exists in the cache
#[post("/cache/exists")]
pub async fn check_cache_key(
    user: AuthUser,
    req: web::Json<CheckKeyRequest>,
) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    let manager = CacheManager::get_or_init();

    let exists =
        manager.app_cache.exists(&req.key).await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to check cache key: {}", e))
        })?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "exists": exists,
        "key": req.key
    })))
}

/// Get cache health information
#[get("/cache/health")]
pub async fn get_cache_health(user: AuthUser) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    let manager = CacheManager::get_or_init();
    let stats = manager.get_stats().await;

    let total = stats.total();
    let health_status = if total.hit_rate() > 0.8 {
        "excellent"
    } else if total.hit_rate() > 0.5 {
        "good"
    } else if total.hit_rate() > 0.3 {
        "fair"
    } else {
        "poor"
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": health_status,
        "hit_rate": total.hit_rate(),
        "total_hits": total.hits,
        "total_misses": total.misses,
        "total_sets": total.sets,
        "total_evictions": total.evictions,
        "caches": {
            "app": {
                "size": stats.app_cache.size,
                "hit_rate": stats.app_cache.hit_rate(),
            },
            "session": {
                "size": stats.session_cache.size,
                "hit_rate": stats.session_cache.hit_rate(),
            },
            "model": {
                "size": stats.model_cache.size,
                "hit_rate": stats.model_cache.hit_rate(),
            },
            "api": {
                "size": stats.api_cache.size,
                "hit_rate": stats.api_cache.hit_rate(),
            },
        }
    })))
}

#[derive(Deserialize)]
pub struct WarmCacheRequest {
    pub entries: HashMap<String, serde_json::Value>,
    pub ttl_seconds: Option<u64>,
}

/// Warm the cache with provided entries
#[post("/cache/warm")]
pub async fn warm_cache(
    user: AuthUser,
    req: web::Json<WarmCacheRequest>,
) -> Result<HttpResponse, AppError> {
    check_admin(&user)?;
    use crate::utils::cache::CacheWarmer;

    let manager = CacheManager::get_or_init();
    let warmer = CacheWarmer::new(manager.app_cache.clone());

    let ttl = req.ttl_seconds.map(std::time::Duration::from_secs);
    let count = warmer
        .warm(req.entries.clone(), ttl)
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to warm cache: {}", e)))?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "warmed_count": count,
        "message": format!("Successfully warmed cache with {} entries", count)
    })))
}

/// Configure cache routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_cache_stats)
        .service(clear_all_caches)
        .service(clear_cache)
        .service(delete_cache_key)
        .service(delete_cache_keys)
        .service(check_cache_key)
        .service(get_cache_health)
        .service(warm_cache);
}
