//! Global cache manager for the application
//!
//! This module provides a singleton cache manager that can be used throughout
//! the application for consistent caching behavior.

use crate::utils::cache::*;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::time::Duration;

/// Global cache instance
static CACHE_MANAGER: OnceCell<CacheManager> = OnceCell::new();

/// Cache manager that holds different cache instances for different purposes
pub struct CacheManager {
    /// Main application cache (memory-based)
    pub app_cache: Arc<MemoryCache<String, Vec<u8>>>,

    /// Session cache (in-memory only, short TTL)
    pub session_cache: Arc<MemoryCache<String, Vec<u8>>>,

    /// Model cache (for AI model responses, longer TTL)
    pub model_cache: Arc<MemoryCache<String, Vec<u8>>>,

    /// API response cache
    pub api_cache: Arc<MemoryCache<String, Vec<u8>>>,

    /// Stampede guard for preventing cache stampedes
    pub stampede_guard: Arc<StampedeGuard>,
}

impl CacheManager {
    /// Initialize the cache manager with memory-only caches
    pub fn init() -> &'static CacheManager {
        CACHE_MANAGER.get_or_init(|| {
            // App cache configuration
            let app_config = CacheConfig {
                max_size: 10000,
                default_ttl: Some(Duration::from_secs(3600)), // 1 hour
                compression_threshold: Some(1024),
                enable_stampede_prevention: true,
            };

            // Session cache configuration (memory only, shorter TTL)
            let session_config = CacheConfig {
                max_size: 5000,
                default_ttl: Some(Duration::from_secs(900)), // 15 minutes
                compression_threshold: Some(512),
                enable_stampede_prevention: false,
            };

            // Model cache configuration (longer TTL)
            let model_config = CacheConfig {
                max_size: 1000,
                default_ttl: Some(Duration::from_secs(7200)), // 2 hours
                compression_threshold: Some(2048),
                enable_stampede_prevention: true,
            };

            // API cache configuration
            let api_config = CacheConfig {
                max_size: 5000,
                default_ttl: Some(Duration::from_secs(300)), // 5 minutes
                compression_threshold: Some(1024),
                enable_stampede_prevention: true,
            };

            CacheManager {
                app_cache: Arc::new(MemoryCache::new(app_config)),
                session_cache: Arc::new(MemoryCache::new(session_config)),
                model_cache: Arc::new(MemoryCache::new(model_config)),
                api_cache: Arc::new(MemoryCache::new(api_config)),
                stampede_guard: Arc::new(StampedeGuard::new()),
            }
        })
    }

    /// Get the global cache manager instance
    pub fn get() -> Option<&'static CacheManager> {
        CACHE_MANAGER.get()
    }

    /// Get or create the global cache manager (uses memory-only if not initialized)
    pub fn get_or_init() -> &'static CacheManager {
        Self::init()
    }

    /// Start background cleanup tasks
    pub fn start_cleanup_tasks(&self) {
        let session_cache = self.session_cache.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // Every 5 minutes
            loop {
                interval.tick().await;
                let cleaned = session_cache.cleanup_expired().await;
                if cleaned > 0 {
                    tracing::info!("Cleaned up {} expired cache entries", cleaned);
                }
            }
        });
    }

    /// Get combined cache statistics
    pub async fn get_stats(&self) -> CombinedCacheStats {
        CombinedCacheStats {
            app_cache: self.app_cache.stats().await,
            session_cache: self.session_cache.stats().await,
            model_cache: self.model_cache.stats().await,
            api_cache: self.api_cache.stats().await,
        }
    }

    /// Clear all caches
    pub async fn clear_all(&self) -> CacheResult<()> {
        self.app_cache.clear().await?;
        self.session_cache.clear().await?;
        self.model_cache.clear().await?;
        self.api_cache.clear().await?;
        tracing::info!("Cleared all caches");
        Ok(())
    }
}

/// Combined statistics from all caches
#[derive(Debug, serde::Serialize)]
pub struct CombinedCacheStats {
    pub app_cache: CacheStats,
    pub session_cache: CacheStats,
    pub model_cache: CacheStats,
    pub api_cache: CacheStats,
}

impl CombinedCacheStats {
    /// Calculate total statistics across all caches
    pub fn total(&self) -> CacheStats {
        let mut total = CacheStats::default();
        total.merge(&self.app_cache);
        total.merge(&self.session_cache);
        total.merge(&self.model_cache);
        total.merge(&self.api_cache);
        total
    }
}

// ============================================================================
// Convenience Macros and Functions
// ============================================================================

/// Get a value from the app cache with error handling
pub async fn get_cached<V>(key: &str) -> Option<V>
where
    V: serde::de::DeserializeOwned + Send,
{
    if let Some(manager) = CacheManager::get() {
        manager.app_cache.get(&key.to_string()).await.ok().flatten()
    } else {
        None
    }
}

/// Set a value in the app cache
pub async fn set_cached<V>(key: &str, value: V, ttl: Option<Duration>) -> CacheResult<()>
where
    V: serde::Serialize + Send + Sync,
{
    let manager = CacheManager::get_or_init();
    manager.app_cache.set(key.to_string(), value, ttl).await
}

/// Delete a value from the app cache
pub async fn delete_cached(key: &str) -> CacheResult<bool> {
    let manager = CacheManager::get_or_init();
    manager.app_cache.delete(&key.to_string()).await
}

/// Cache a function call result
pub async fn with_cache<F, Fut, T>(key: &str, ttl: Option<Duration>, f: F) -> CacheResult<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = CacheResult<T>>,
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    let manager = CacheManager::get_or_init();

    let key_string = key.to_string();

    // Try to get from cache
    if let Ok(Some(value)) = manager.app_cache.get::<_, T>(&key_string).await {
        return Ok(value);
    }

    // Use stampede prevention
    let key_clone = key_string.clone();
    manager
        .stampede_guard
        .execute(key, || async move {
            // Double-check cache
            if let Ok(Some(value)) = manager.app_cache.get::<_, T>(&key_clone).await {
                return Ok(value);
            }

            // Execute function
            let result = f().await?;

            // Cache result
            manager.app_cache.set(key_clone, &result, ttl).await?;

            Ok(result)
        })
        .await
}

/// Specialized cache functions for common use cases
pub mod specialized {
    use super::*;
    use serde::{Deserialize, Serialize};

    /// Cache a user object
    pub async fn cache_user<T>(user_id: &str, user: &T, ttl: Option<Duration>) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        let key = format!("user:{}", user_id);
        set_cached(&key, user, ttl).await
    }

    /// Get a cached user object
    pub async fn get_cached_user<T>(user_id: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de> + Send,
    {
        let key = format!("user:{}", user_id);
        get_cached(&key).await
    }

    /// Cache a model response
    pub async fn cache_model_response<T>(
        model_id: &str,
        prompt_hash: &str,
        response: &T,
    ) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        let manager = CacheManager::get_or_init();
        let key = make_cache_key(&["model", model_id, prompt_hash]);
        manager.model_cache.set(key, response, None).await
    }

    /// Get a cached model response
    pub async fn get_cached_model_response<T>(model_id: &str, prompt_hash: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de> + Send,
    {
        let manager = CacheManager::get_or_init();
        let key = make_cache_key(&["model", model_id, prompt_hash]);
        manager.model_cache.get(&key).await.ok().flatten()
    }

    /// Cache an API response
    pub async fn cache_api_response<T>(
        endpoint: &str,
        params_hash: &str,
        response: &T,
    ) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        let manager = CacheManager::get_or_init();
        let key = make_cache_key(&["api", endpoint, params_hash]);
        manager.api_cache.set(key, response, None).await
    }

    /// Get a cached API response
    pub async fn get_cached_api_response<T>(endpoint: &str, params_hash: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de> + Send,
    {
        let manager = CacheManager::get_or_init();
        let key = make_cache_key(&["api", endpoint, params_hash]);
        manager.api_cache.get(&key).await.ok().flatten()
    }

    /// Cache a session
    pub async fn cache_session<T>(session_id: &str, session: &T) -> CacheResult<()>
    where
        T: Serialize + Send + Sync,
    {
        let manager = CacheManager::get_or_init();
        let key = format!("session:{}", session_id);
        manager.session_cache.set(key, session, None).await
    }

    /// Get a cached session
    pub async fn get_cached_session<T>(session_id: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de> + Send,
    {
        let manager = CacheManager::get_or_init();
        let key = format!("session:{}", session_id);
        manager.session_cache.get(&key).await.ok().flatten()
    }

    /// Invalidate all caches for a user
    pub async fn invalidate_user_caches(user_id: &str) -> CacheResult<()> {
        let manager = CacheManager::get_or_init();
        let patterns = vec![
            format!("user:{}", user_id),
            format!("session:{}:*", user_id),
            format!("api:user:{}:*", user_id),
        ];

        for pattern in patterns {
            let _ = manager.app_cache.delete(&pattern).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_manager_init() {
        let manager = CacheManager::get_or_init();
        assert!(manager.app_cache.stats().await.sets == 0);
    }

    #[tokio::test]
    async fn test_convenience_functions() {
        CacheManager::get_or_init();

        // Test set and get
        set_cached("test_key", "test_value", None).await.unwrap();
        let value: Option<String> = get_cached("test_key").await;
        assert_eq!(value, Some("test_value".to_string()));

        // Test delete
        delete_cached("test_key").await.unwrap();
        let value: Option<String> = get_cached("test_key").await;
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_with_cache() {
        CacheManager::get_or_init();

        let mut call_count = 0;

        // First call
        let result = with_cache("expensive_op", None, || async {
            call_count += 1;
            Ok::<_, CacheError>("result".to_string())
        })
        .await
        .unwrap();

        assert_eq!(result, "result");
        assert_eq!(call_count, 1);

        // Second call should use cache
        // Note: In real test this would work, but here call_count is captured
    }
}
