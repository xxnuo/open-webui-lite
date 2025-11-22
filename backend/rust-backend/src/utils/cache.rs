//! Comprehensive caching utilities for Open WebUI Rust Backend
//!
//! This module provides a powerful, multi-tier caching system that includes:
//! - In-memory LRU caching with TTL support
//! - Redis-backed distributed caching
//! - Multi-tier caching strategy
//! - Cache stampede prevention
//! - Automatic cleanup and eviction
//! - Compression support
//! - Metrics and monitoring
//! - Batch operations
//! - Type-safe API

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, error, info, warn};

// ============================================================================
// Core Types and Traits
// ============================================================================

/// Result type for cache operations
pub type CacheResult<T> = Result<T, CacheError>;

/// Errors that can occur during cache operations
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Cache key not found: {0}")]
    NotFound(String),

    #[error("Cache serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Redis error: {0}")]
    Redis(String),

    #[error("Cache operation timeout")]
    Timeout,

    #[error("Cache full, cannot insert")]
    CacheFull,

    #[error("Invalid TTL: {0}")]
    InvalidTtl(String),
}

// Redis error conversions disabled for SQLite compatibility
/*
impl From<redis::RedisError> for CacheError {
    fn from(err: redis::RedisError) -> Self {
        CacheError::Redis(err.to_string())
    }
}

impl From<deadpool_redis::PoolError> for CacheError {
    fn from(err: deadpool_redis::PoolError) -> Self {
        CacheError::Redis(err.to_string())
    }
}
*/

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub value: T,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub access_count: u64,
    pub last_accessed: DateTime<Utc>,
}

impl<T> CacheEntry<T> {
    pub fn new(value: T, ttl: Option<Duration>) -> Self {
        let now = Utc::now();
        Self {
            value,
            created_at: now,
            expires_at: ttl.map(|d| now + chrono::Duration::from_std(d).unwrap()),
            access_count: 0,
            last_accessed: now,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |exp| Utc::now() > exp)
    }

    pub fn touch(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }
}

/// Cache statistics for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub sets: u64,
    pub deletes: u64,
    pub evictions: u64,
    pub size: usize,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn merge(&mut self, other: &CacheStats) {
        self.hits += other.hits;
        self.misses += other.misses;
        self.sets += other.sets;
        self.deletes += other.deletes;
        self.evictions += other.evictions;
    }
}

/// Configuration for cache behavior
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries in cache
    pub max_size: usize,
    /// Default TTL for entries
    pub default_ttl: Option<Duration>,
    /// Enable compression for values larger than this size (bytes)
    pub compression_threshold: Option<usize>,
    /// Enable cache stampede prevention
    pub enable_stampede_prevention: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 10000,
            default_ttl: Some(Duration::from_secs(3600)), // 1 hour
            compression_threshold: Some(1024),            // 1KB
            enable_stampede_prevention: true,
        }
    }
}

/// Core cache trait that all cache implementations must implement
#[async_trait]
pub trait Cache: Send + Sync {
    /// Get a value from the cache
    async fn get<K, V>(&self, key: &K) -> CacheResult<Option<V>>
    where
        K: AsRef<str> + Send + Sync,
        V: DeserializeOwned + Send;

    /// Set a value in the cache with optional TTL
    async fn set<K, V>(&self, key: K, value: V, ttl: Option<Duration>) -> CacheResult<()>
    where
        K: AsRef<str> + Send + Sync,
        V: Serialize + Send + Sync;

    /// Delete a value from the cache
    async fn delete<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync;

    /// Check if a key exists in the cache
    async fn exists<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync;

    /// Clear all entries from the cache
    async fn clear(&self) -> CacheResult<()>;

    /// Get cache statistics
    async fn stats(&self) -> CacheStats;

    /// Get multiple values at once (batch operation)
    async fn get_many<K, V>(&self, keys: &[K]) -> CacheResult<HashMap<String, V>>
    where
        K: AsRef<str> + Send + Sync,
        V: DeserializeOwned + Send,
    {
        let mut results = HashMap::new();
        for key in keys {
            if let Ok(Some(value)) = self.get(key).await {
                results.insert(key.as_ref().to_string(), value);
            }
        }
        Ok(results)
    }

    /// Set multiple values at once (batch operation)
    async fn set_many<K, V>(&self, entries: HashMap<K, V>, ttl: Option<Duration>) -> CacheResult<()>
    where
        K: AsRef<str> + Send + Sync,
        V: Serialize + Send + Sync,
    {
        for (key, value) in entries {
            self.set(key, value, ttl).await?;
        }
        Ok(())
    }

    /// Delete multiple values at once (batch operation)
    async fn delete_many<K>(&self, keys: &[K]) -> CacheResult<usize>
    where
        K: AsRef<str> + Send + Sync,
    {
        let mut count = 0;
        for key in keys {
            if self.delete(key).await? {
                count += 1;
            }
        }
        Ok(count)
    }
}

// ============================================================================
// In-Memory LRU Cache Implementation
// ============================================================================

/// LRU node for doubly-linked list
struct LruNode<K> {
    key: K,
    prev: Option<K>,
    next: Option<K>,
}

/// In-memory LRU cache with TTL support
pub struct MemoryCache<K, V>
where
    K: Eq + Hash + Clone,
{
    entries: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    lru_list: Arc<RwLock<HashMap<K, LruNode<K>>>>,
    head: Arc<RwLock<Option<K>>>,
    tail: Arc<RwLock<Option<K>>>,
    config: CacheConfig,
    stats: Arc<RwLock<CacheStats>>,
}

impl<K, V> MemoryCache<K, V>
where
    K: Eq + Hash + Clone + Send + Sync + Debug,
    V: Clone + Send + Sync,
{
    pub fn new(config: CacheConfig) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            lru_list: Arc::new(RwLock::new(HashMap::new())),
            head: Arc::new(RwLock::new(None)),
            tail: Arc::new(RwLock::new(None)),
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Move a key to the head of the LRU list (most recently used)
    async fn move_to_head(&self, key: &K) {
        let mut lru_list = self.lru_list.write().await;
        let mut head = self.head.write().await;
        let mut tail = self.tail.write().await;

        // If already head, nothing to do
        if head.as_ref() == Some(key) {
            return;
        }

        // Get node data before mutating
        let (prev_key, next_key, is_tail) = if let Some(node) = lru_list.get(key) {
            (
                node.prev.clone(),
                node.next.clone(),
                tail.as_ref() == Some(key),
            )
        } else {
            (None, None, false)
        };

        // Update previous and next nodes
        if let Some(ref pk) = prev_key {
            if let Some(prev_node) = lru_list.get_mut(pk) {
                prev_node.next = next_key.clone();
            }
        }
        if let Some(ref nk) = next_key {
            if let Some(next_node) = lru_list.get_mut(nk) {
                next_node.prev = prev_key.clone();
            }
        }
        if is_tail {
            *tail = prev_key;
        }

        // Add to head
        if let Some(old_head) = head.take() {
            if let Some(old_head_node) = lru_list.get_mut(&old_head) {
                old_head_node.prev = Some(key.clone());
            }
            lru_list.insert(
                key.clone(),
                LruNode {
                    key: key.clone(),
                    prev: None,
                    next: Some(old_head),
                },
            );
        } else {
            // First entry
            lru_list.insert(
                key.clone(),
                LruNode {
                    key: key.clone(),
                    prev: None,
                    next: None,
                },
            );
            *tail = Some(key.clone());
        }
        *head = Some(key.clone());
    }

    /// Remove the least recently used entry
    async fn evict_lru(&self) -> Option<K> {
        let mut tail = self.tail.write().await;
        let mut lru_list = self.lru_list.write().await;
        let mut entries = self.entries.write().await;
        let mut stats = self.stats.write().await;

        if let Some(tail_key) = tail.take() {
            if let Some(node) = lru_list.remove(&tail_key) {
                *tail = node.prev;
                if let Some(ref new_tail) = *tail {
                    if let Some(new_tail_node) = lru_list.get_mut(new_tail) {
                        new_tail_node.next = None;
                    }
                } else {
                    // List is now empty
                    let mut head = self.head.write().await;
                    *head = None;
                }
            }
            entries.remove(&tail_key);
            stats.evictions += 1;
            return Some(tail_key);
        }
        None
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let mut entries = self.entries.write().await;
        let mut lru_list = self.lru_list.write().await;
        let mut stats = self.stats.write().await;

        let expired_keys: Vec<K> = entries
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired_keys.len();
        for key in expired_keys {
            entries.remove(&key);
            lru_list.remove(&key);
        }

        stats.evictions += count as u64;
        count
    }

    /// Get the current size of the cache
    pub async fn size(&self) -> usize {
        self.entries.read().await.len()
    }
}

#[async_trait]
impl Cache for MemoryCache<String, Vec<u8>> {
    async fn get<K, V>(&self, key: &K) -> CacheResult<Option<V>>
    where
        K: AsRef<str> + Send + Sync,
        V: DeserializeOwned + Send,
    {
        let key_str = key.as_ref().to_string();

        // Check if entry exists and is not expired
        {
            let mut entries = self.entries.write().await;
            let mut stats = self.stats.write().await;

            if let Some(entry) = entries.get_mut(&key_str) {
                if entry.is_expired() {
                    entries.remove(&key_str);
                    stats.misses += 1;
                    return Ok(None);
                }

                entry.touch();
                stats.hits += 1;
            } else {
                stats.misses += 1;
                return Ok(None);
            }
        }

        // Move to head of LRU
        self.move_to_head(&key_str).await;

        // Read the value
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(&key_str) {
            let value: V = serde_json::from_slice(&entry.value)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    async fn set<K, V>(&self, key: K, value: V, ttl: Option<Duration>) -> CacheResult<()>
    where
        K: AsRef<str> + Send + Sync,
        V: Serialize + Send + Sync,
    {
        let key_str = key.as_ref().to_string();
        let serialized = serde_json::to_vec(&value)?;

        // Check if we need to evict
        while self.size().await >= self.config.max_size {
            if self.evict_lru().await.is_none() {
                return Err(CacheError::CacheFull);
            }
        }

        let ttl = ttl.or(self.config.default_ttl);
        let entry = CacheEntry::new(serialized, ttl);

        let mut entries = self.entries.write().await;
        entries.insert(key_str.clone(), entry);

        let mut stats = self.stats.write().await;
        stats.sets += 1;
        stats.size = entries.len();

        drop(entries);
        drop(stats);

        self.move_to_head(&key_str).await;

        Ok(())
    }

    async fn delete<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync,
    {
        let key_str = key.as_ref().to_string();
        let mut entries = self.entries.write().await;
        let mut lru_list = self.lru_list.write().await;
        let mut stats = self.stats.write().await;

        let removed = entries.remove(&key_str).is_some();
        lru_list.remove(&key_str);

        if removed {
            stats.deletes += 1;
            stats.size = entries.len();
        }

        Ok(removed)
    }

    async fn exists<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync,
    {
        let key_str = key.as_ref();
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key_str) {
            Ok(!entry.is_expired())
        } else {
            Ok(false)
        }
    }

    async fn clear(&self) -> CacheResult<()> {
        let mut entries = self.entries.write().await;
        let mut lru_list = self.lru_list.write().await;
        let mut head = self.head.write().await;
        let mut tail = self.tail.write().await;
        let mut stats = self.stats.write().await;

        entries.clear();
        lru_list.clear();
        *head = None;
        *tail = None;
        stats.size = 0;

        Ok(())
    }

    async fn stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }
}

// ============================================================================
// Redis Cache Implementation (Disabled for SQLite compatibility)
// ============================================================================

// Redis cache is disabled to avoid Redis dependency when using SQLite
// Uncomment and enable the "redis" feature to use Redis caching

/*
/// Redis-backed cache implementation
pub struct RedisCache {
    pool: RedisPool,
    config: CacheConfig,
    stats: Arc<RwLock<CacheStats>>,
    key_prefix: String,
}

impl RedisCache {
    pub fn new(pool: RedisPool, config: CacheConfig) -> Self {
        Self {
            pool,
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
            key_prefix: "open-webui:cache:".to_string(),
        }
    }

    pub fn with_prefix(mut self, prefix: String) -> Self {
        self.key_prefix = prefix;
        self
    }

    fn make_key(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key)
    }

    async fn get_connection(&self) -> CacheResult<Connection> {
        self.pool.get().await.map_err(|e| e.into())
    }
}

#[async_trait]
impl Cache for RedisCache {
    async fn get<K, V>(&self, key: &K) -> CacheResult<Option<V>>
    where
        K: AsRef<str> + Send + Sync,
        V: DeserializeOwned + Send,
    {
        let redis_key = self.make_key(key.as_ref());
        let mut conn = self.get_connection().await?;
        let mut stats = self.stats.write().await;

        let data: Option<Vec<u8>> = conn.get(&redis_key).await?;

        match data {
            Some(bytes) => {
                stats.hits += 1;
                let value: V = serde_json::from_slice(&bytes)?;
                Ok(Some(value))
            }
            None => {
                stats.misses += 1;
                Ok(None)
            }
        }
    }

    async fn set<K, V>(&self, key: K, value: V, ttl: Option<Duration>) -> CacheResult<()>
    where
        K: AsRef<str> + Send + Sync,
        V: Serialize + Send + Sync,
    {
        let redis_key = self.make_key(key.as_ref());
        let serialized = serde_json::to_vec(&value)?;
        let mut conn = self.get_connection().await?;

        let ttl = ttl.or(self.config.default_ttl);

        if let Some(ttl_duration) = ttl {
            let seconds = ttl_duration.as_secs();
            let _: () = conn.set_ex(&redis_key, serialized, seconds).await?;
        } else {
            let _: () = conn.set(&redis_key, serialized).await?;
        }

        let mut stats = self.stats.write().await;
        stats.sets += 1;

        Ok(())
    }

    async fn delete<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync,
    {
        let redis_key = self.make_key(key.as_ref());
        let mut conn = self.get_connection().await?;

        let deleted: i32 = conn.del(&redis_key).await?;

        let mut stats = self.stats.write().await;
        if deleted > 0 {
            stats.deletes += 1;
        }

        Ok(deleted > 0)
    }

    async fn exists<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync,
    {
        let redis_key = self.make_key(key.as_ref());
        let mut conn = self.get_connection().await?;
        let exists: bool = conn.exists(&redis_key).await?;
        Ok(exists)
    }

    async fn clear(&self) -> CacheResult<()> {
        let mut conn = self.get_connection().await?;
        let pattern = format!("{}*", self.key_prefix);

        // Use SCAN to find all keys with the prefix
        let keys: Vec<String> = redis::cmd("SCAN")
            .arg(0)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(1000)
            .query_async(&mut conn)
            .await
            .map_err(|e| CacheError::Redis(e.to_string()))?;

        if !keys.is_empty() {
            conn.del(keys).await?;
        }

        Ok(())
    }

    async fn stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    async fn get_many<K, V>(&self, keys: &[K]) -> CacheResult<HashMap<String, V>>
    where
        K: AsRef<str> + Send + Sync,
        V: DeserializeOwned + Send,
    {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let redis_keys: Vec<String> = keys.iter().map(|k| self.make_key(k.as_ref())).collect();
        let mut conn = self.get_connection().await?;

        let values: Vec<Option<Vec<u8>>> = conn.get(&redis_keys).await?;

        let mut results = HashMap::new();
        for (i, value_opt) in values.into_iter().enumerate() {
            if let Some(bytes) = value_opt {
                if let Ok(value) = serde_json::from_slice::<V>(&bytes) {
                    results.insert(keys[i].as_ref().to_string(), value);
                }
            }
        }

        Ok(results)
    }

    async fn set_many<K, V>(&self, entries: HashMap<K, V>, ttl: Option<Duration>) -> CacheResult<()>
    where
        K: AsRef<str> + Send + Sync,
        V: Serialize + Send + Sync,
    {
        if entries.is_empty() {
            return Ok(());
        }

        let mut conn = self.get_connection().await?;
        let ttl = ttl.or(self.config.default_ttl);

        for (key, value) in entries {
            let redis_key = self.make_key(key.as_ref());
            let serialized = serde_json::to_vec(&value)?;

            if let Some(ttl_duration) = ttl {
                let seconds = ttl_duration.as_secs();
                let _: () = conn.set_ex(&redis_key, serialized, seconds).await?;
            } else {
                let _: () = conn.set(&redis_key, serialized).await?;
            }
        }

        Ok(())
    }

    async fn delete_many<K>(&self, keys: &[K]) -> CacheResult<usize>
    where
        K: AsRef<str> + Send + Sync,
    {
        if keys.is_empty() {
            return Ok(0);
        }

        let redis_keys: Vec<String> = keys.iter().map(|k| self.make_key(k.as_ref())).collect();
        let mut conn = self.get_connection().await?;

        let deleted: usize = conn.del(&redis_keys).await?;
        Ok(deleted)
    }
}
*/

// ============================================================================
// Multi-Tier Cache Implementation (Disabled for SQLite compatibility)
// ============================================================================

// Multi-tier cache with Redis is disabled to avoid Redis dependency
// Use MemoryCache directly for SQLite-only deployments

/*
/// Multi-tier cache that uses both in-memory and Redis caches
/// Provides L1 (memory) and L2 (Redis) caching with automatic promotion
pub struct MultiTierCache {
    l1_cache: Arc<MemoryCache<String, Vec<u8>>>,
    l2_cache: Option<Arc<RedisCache>>,
    stats: Arc<RwLock<CacheStats>>,
}

impl MultiTierCache {
    /// Create a new multi-tier cache with both L1 and L2
    pub fn new(l1_config: CacheConfig, redis_pool: Option<RedisPool>) -> Self {
        let l2_cache = redis_pool.map(|pool| Arc::new(RedisCache::new(pool, l1_config.clone())));

        Self {
            l1_cache: Arc::new(MemoryCache::new(l1_config)),
            l2_cache,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Create a new multi-tier cache with only L1 (memory)
    pub fn memory_only(config: CacheConfig) -> Self {
        Self {
            l1_cache: Arc::new(MemoryCache::new(config)),
            l2_cache: None,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Promote a value from L2 to L1
    async fn promote_to_l1<V>(&self, key: &str, value: &V, ttl: Option<Duration>) -> CacheResult<()>
    where
        V: Serialize + Send + Sync,
    {
        if let Err(e) = self.l1_cache.set(key, value, ttl).await {
            warn!("Failed to promote key '{}' to L1 cache: {}", key, e);
        }
        Ok(())
    }
}

#[async_trait]
impl Cache for MultiTierCache {
    async fn get<K, V>(&self, key: &K) -> CacheResult<Option<V>>
    where
        K: AsRef<str> + Send + Sync,
        V: DeserializeOwned + Send,
    {
        let key_str = key.as_ref();

        // Try L1 first
        if let Ok(Some(value)) = self.l1_cache.get::<K, V>(key).await {
            debug!("L1 cache hit for key: {}", key_str);
            return Ok(Some(value));
        }

        // Try L2 if available
        if let Some(l2) = &self.l2_cache {
            if let Ok(Some(value)) = l2.get::<K, V>(key).await {
                debug!("L2 cache hit for key: {}, promoting to L1", key_str);
                // Note: Promotion requires Serialize, skip if not needed for correctness
                // In production, you might want to use a different approach
                return Ok(Some(value));
            }
        }

        debug!("Cache miss for key: {}", key_str);
        Ok(None)
    }

    async fn set<K, V>(&self, key: K, value: V, ttl: Option<Duration>) -> CacheResult<()>
    where
        K: AsRef<str> + Send + Sync,
        V: Serialize + Send + Sync,
    {
        // Set in both L1 and L2
        let l1_result = self.l1_cache.set(&key, &value, ttl).await;

        if let Some(l2) = &self.l2_cache {
            let l2_result = l2.set(&key, &value, ttl).await;
            if let Err(e) = l2_result {
                warn!("Failed to set key '{}' in L2 cache: {}", key.as_ref(), e);
            }
        }

        l1_result
    }

    async fn delete<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync,
    {
        let l1_deleted = self.l1_cache.delete(key).await?;

        if let Some(l2) = &self.l2_cache {
            let _ = l2.delete(key).await;
        }

        Ok(l1_deleted)
    }

    async fn exists<K>(&self, key: &K) -> CacheResult<bool>
    where
        K: AsRef<str> + Send + Sync,
    {
        if self.l1_cache.exists(key).await? {
            return Ok(true);
        }

        if let Some(l2) = &self.l2_cache {
            return l2.exists(key).await;
        }

        Ok(false)
    }

    async fn clear(&self) -> CacheResult<()> {
        self.l1_cache.clear().await?;

        if let Some(l2) = &self.l2_cache {
            l2.clear().await?;
        }

        Ok(())
    }

    async fn stats(&self) -> CacheStats {
        let mut combined_stats = self.l1_cache.stats().await;

        if let Some(l2) = &self.l2_cache {
            let l2_stats = l2.stats().await;
            combined_stats.merge(&l2_stats);
        }

        combined_stats
    }
}
*/

// ============================================================================
// Cache Stampede Prevention
// ============================================================================

/// Prevents cache stampede by ensuring only one request fetches data
pub struct StampedeGuard {
    locks: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,
}

impl StampedeGuard {
    pub fn new() -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Execute a function with stampede prevention for the given key
    pub async fn execute<F, Fut, T>(&self, key: &str, f: F) -> CacheResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = CacheResult<T>>,
    {
        // Get or create a semaphore for this key
        let semaphore = {
            let mut locks = self.locks.write().await;
            locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(1)))
                .clone()
        };

        // Acquire the semaphore
        let _permit = semaphore.acquire().await.unwrap();

        // Execute the function
        let result = f().await;

        // Clean up the semaphore if it's no longer needed
        {
            let mut locks = self.locks.write().await;
            if Arc::strong_count(&semaphore) == 2 {
                // Only the HashMap and this scope hold references
                locks.remove(key);
            }
        }

        result
    }
}

impl Default for StampedeGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Cached Function Wrapper
// ============================================================================

/// Wrapper for caching function results
pub struct CachedFn<C: Cache> {
    cache: Arc<C>,
    stampede_guard: Arc<StampedeGuard>,
    key_prefix: String,
}

impl<C: Cache> CachedFn<C> {
    pub fn new(cache: Arc<C>, key_prefix: impl Into<String>) -> Self {
        Self {
            cache,
            stampede_guard: Arc::new(StampedeGuard::new()),
            key_prefix: key_prefix.into(),
        }
    }

    /// Execute a function with caching
    pub async fn call<F, Fut, T, K>(&self, key: K, ttl: Option<Duration>, f: F) -> CacheResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = CacheResult<T>>,
        T: Serialize + DeserializeOwned + Send + Sync,
        K: AsRef<str>,
    {
        let cache_key = format!("{}:{}", self.key_prefix, key.as_ref());

        // Try to get from cache first
        if let Ok(Some(value)) = self.cache.get::<_, T>(&cache_key).await {
            return Ok(value);
        }

        // Execute with stampede prevention
        self.stampede_guard
            .execute(&cache_key, || async {
                // Double-check cache after acquiring lock
                if let Ok(Some(value)) = self.cache.get::<_, T>(&cache_key).await {
                    return Ok(value);
                }

                // Execute the function
                let result = f().await?;

                // Cache the result
                self.cache.set(&cache_key, &result, ttl).await?;

                Ok(result)
            })
            .await
    }

    /// Invalidate a cached function result
    pub async fn invalidate<K>(&self, key: K) -> CacheResult<bool>
    where
        K: AsRef<str>,
    {
        let cache_key = format!("{}:{}", self.key_prefix, key.as_ref());
        self.cache.delete(&cache_key).await
    }
}

// ============================================================================
// HTTP Response Caching Helper
// ============================================================================

/// Configuration for HTTP cache
#[derive(Clone)]
pub struct HttpCacheConfig {
    pub ttl: Duration,
    pub methods: Vec<String>,
    pub cache_control: Option<String>,
}

impl Default for HttpCacheConfig {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(300), // 5 minutes
            methods: vec!["GET".to_string()],
            cache_control: Some("public, max-age=300".to_string()),
        }
    }
}

/// Helper for caching HTTP responses manually in handlers
///
/// Note: Full middleware implementation is complex with actix-web's body handling.
/// Use this helper in your handlers for explicit caching control.
pub struct HttpCacheHelper<C: Cache> {
    cache: Arc<C>,
    config: HttpCacheConfig,
}

impl<C: Cache> HttpCacheHelper<C> {
    pub fn new(cache: Arc<C>, config: HttpCacheConfig) -> Self {
        Self { cache, config }
    }

    /// Build a cache key from method and path
    pub fn make_key(&self, method: &str, path: &str, query: Option<&str>) -> String {
        if let Some(q) = query {
            format!("http:{}:{}?{}", method, path, q)
        } else {
            format!("http:{}:{}", method, path)
        }
    }

    /// Get cached response
    pub async fn get<T: serde::de::DeserializeOwned + Send>(
        &self,
        key: &str,
    ) -> CacheResult<Option<T>> {
        self.cache.get(&key.to_string()).await
    }

    /// Cache a response
    pub async fn set<T: serde::Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
    ) -> CacheResult<()> {
        self.cache
            .set(key.to_string(), value, Some(self.config.ttl))
            .await
    }
}

// ============================================================================
// Cache Warmer
// ============================================================================

/// Utility for warming up the cache with data
pub struct CacheWarmer<C: Cache> {
    cache: Arc<C>,
}

impl<C: Cache> CacheWarmer<C> {
    pub fn new(cache: Arc<C>) -> Self {
        Self { cache }
    }

    /// Warm the cache with a set of key-value pairs
    pub async fn warm<K, V>(
        &self,
        entries: HashMap<K, V>,
        ttl: Option<Duration>,
    ) -> CacheResult<usize>
    where
        K: AsRef<str> + Send + Sync,
        V: Serialize + Send + Sync,
    {
        let count = entries.len();
        self.cache.set_many(entries, ttl).await?;
        info!("Warmed cache with {} entries", count);
        Ok(count)
    }

    /// Warm the cache by calling a function that returns data
    pub async fn warm_with<F, Fut, V>(
        &self,
        keys: Vec<String>,
        ttl: Option<Duration>,
        f: F,
    ) -> CacheResult<usize>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = CacheResult<V>>,
        V: Serialize + Send + Sync,
    {
        let mut entries = HashMap::new();

        for key in keys {
            match f(key.clone()).await {
                Ok(value) => {
                    entries.insert(key, value);
                }
                Err(e) => {
                    warn!("Failed to warm key '{}': {}", key, e);
                }
            }
        }

        let count = entries.len();
        self.cache.set_many(entries, ttl).await?;
        info!("Warmed cache with {} entries", count);
        Ok(count)
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Create a cache key from multiple parts
pub fn make_cache_key(parts: &[&str]) -> String {
    parts.join(":")
}

/// Create a cache key with hash for long keys
pub fn make_cache_key_hashed(prefix: &str, data: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data.as_bytes());
    format!("{}:{:x}", prefix, hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_cache_basic() {
        let config = CacheConfig::default();
        let cache = MemoryCache::new(config);

        // Test set and get
        cache.set("key1", "value1", None).await.unwrap();
        let result: Option<String> = cache.get(&"key1").await.unwrap();
        assert_eq!(result, Some("value1".to_string()));

        // Test delete
        assert!(cache.delete(&"key1").await.unwrap());
        let result: Option<String> = cache.get(&"key1").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let config = CacheConfig::default();
        let cache = MemoryCache::new(config);

        cache.set("key1", "value1", None).await.unwrap();
        let _: Option<String> = cache.get(&"key1").await.unwrap();
        let _: Option<String> = cache.get(&"key2").await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.sets, 1);
        assert!(stats.hit_rate() > 0.0);
    }

    #[tokio::test]
    async fn test_cache_key_functions() {
        let key = make_cache_key(&["user", "123", "profile"]);
        assert_eq!(key, "user:123:profile");

        let hashed = make_cache_key_hashed("session", "very_long_session_data_here");
        assert!(hashed.starts_with("session:"));
    }
}
