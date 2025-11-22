//! Comprehensive cache system demonstration
//!
//! This example shows how to use the powerful caching system in Open WebUI Rust Backend.
//!
//! Run with: `cargo run --example cache_demo`

use open_webui_rust::utils::cache::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiResponse {
    data: String,
    timestamp: i64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("=== Open WebUI Rust Cache System Demo ===\n");

    // Example 1: Basic In-Memory Cache
    demo_memory_cache().await?;

    // Example 2: Multi-Tier Cache
    demo_multi_tier_cache().await?;

    // Example 3: Cached Function Wrapper
    demo_cached_function().await?;

    // Example 4: Cache Stampede Prevention
    demo_stampede_prevention().await?;

    // Example 5: Cache Warming
    demo_cache_warming().await?;

    // Example 6: Batch Operations
    demo_batch_operations().await?;

    // Example 7: Cache Statistics
    demo_cache_stats().await?;

    println!("\n=== Demo Complete ===");
    Ok(())
}

async fn demo_memory_cache() -> CacheResult<()> {
    println!("--- Example 1: Basic In-Memory Cache ---");

    let config = CacheConfig {
        max_size: 100,
        default_ttl: Some(Duration::from_secs(60)),
        compression_threshold: Some(1024),
        enable_stampede_prevention: true,
    };

    let cache: MemoryCache<String, Vec<u8>> = MemoryCache::new(config);

    // Store a user
    let user = User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };

    cache.set("user:1", &user, None).await?;
    println!("✓ Stored user in cache");

    // Retrieve the user
    let cached_user: Option<User> = cache.get(&"user:1").await?;
    println!("✓ Retrieved user: {:?}", cached_user);

    // Check existence
    let exists = cache.exists(&"user:1").await?;
    println!("✓ User exists in cache: {}", exists);

    // Delete the user
    cache.delete(&"user:1").await?;
    println!("✓ Deleted user from cache");

    let cached_user: Option<User> = cache.get(&"user:1").await?;
    println!("✓ User after deletion: {:?}\n", cached_user);

    Ok(())
}

async fn demo_multi_tier_cache() -> CacheResult<()> {
    println!("--- Example 2: Multi-Tier Cache ---");

    // Create a memory-only multi-tier cache (L1 only)
    let config = CacheConfig::default();
    let cache = Arc::new(MultiTierCache::memory_only(config));

    // Store data
    let data = ApiResponse {
        data: "Hello from multi-tier cache".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    cache
        .set("api:response:1", &data, Some(Duration::from_secs(300)))
        .await?;
    println!("✓ Stored data in multi-tier cache");

    // Retrieve data (will hit L1)
    let cached_data: Option<ApiResponse> = cache.get(&"api:response:1").await?;
    println!("✓ Retrieved data: {:?}", cached_data);

    // In production, you would use:
    // let redis_pool = create_redis_pool();
    // let cache = MultiTierCache::new(config, Some(redis_pool));
    // This would enable both L1 (memory) and L2 (Redis) caching

    println!();
    Ok(())
}

async fn demo_cached_function() -> CacheResult<()> {
    println!("--- Example 3: Cached Function Wrapper ---");

    let cache = Arc::new(MultiTierCache::memory_only(CacheConfig::default()));
    let cached_fn = CachedFn::new(cache, "expensive_operation");

    // Simulated expensive operation
    async fn fetch_data(id: u64) -> CacheResult<String> {
        println!("  [Simulating expensive database query...]");
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(format!("Data for ID: {}", id))
    }

    // First call - will execute the function
    println!("First call (cache miss):");
    let result = cached_fn
        .call("user_123", Some(Duration::from_secs(60)), || {
            fetch_data(123)
        })
        .await?;
    println!("✓ Result: {}", result);

    // Second call - will use cache
    println!("\nSecond call (cache hit):");
    let result = cached_fn
        .call("user_123", Some(Duration::from_secs(60)), || {
            fetch_data(123)
        })
        .await?;
    println!("✓ Result: {}", result);

    // Invalidate cache
    cached_fn.invalidate("user_123").await?;
    println!("✓ Cache invalidated\n");

    Ok(())
}

async fn demo_stampede_prevention() -> CacheResult<()> {
    println!("--- Example 4: Cache Stampede Prevention ---");

    let guard = StampedeGuard::new();

    // Simulate multiple concurrent requests for the same data
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let guard = guard.clone();
            tokio::spawn(async move {
                let result = guard
                    .execute("expensive_query", || async {
                        println!("  [Thread {} executing expensive query]", i);
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        Ok::<_, CacheError>("Computed result".to_string())
                    })
                    .await;
                println!("  Thread {} completed", i);
                result
            })
        })
        .collect();

    // Wait for all threads
    for handle in handles {
        handle.await.unwrap()?;
    }

    println!("✓ Only one thread executed the expensive query (stampede prevented)\n");
    Ok(())
}

async fn demo_cache_warming() -> CacheResult<()> {
    println!("--- Example 5: Cache Warming ---");

    let cache = Arc::new(MultiTierCache::memory_only(CacheConfig::default()));
    let warmer = CacheWarmer::new(cache.clone());

    // Prepare data to warm the cache
    let mut entries = HashMap::new();
    for i in 1..=10 {
        let user = User {
            id: i,
            name: format!("User{}", i),
            email: format!("user{}@example.com", i),
        };
        entries.insert(format!("user:{}", i), user);
    }

    // Warm the cache
    let count = warmer.warm(entries, Some(Duration::from_secs(300))).await?;
    println!("✓ Warmed cache with {} users", count);

    // Verify cache is warmed
    let user: Option<User> = cache.get(&"user:5").await?;
    println!("✓ Retrieved warmed user: {:?}\n", user);

    Ok(())
}

async fn demo_batch_operations() -> CacheResult<()> {
    println!("--- Example 6: Batch Operations ---");

    let cache: MemoryCache<String, Vec<u8>> = MemoryCache::new(CacheConfig::default());

    // Batch set
    let mut entries = HashMap::new();
    for i in 1..=5 {
        entries.insert(format!("key{}", i), format!("value{}", i));
    }
    cache.set_many(entries, None).await?;
    println!("✓ Batch set 5 entries");

    // Batch get
    let keys: Vec<String> = (1..=5).map(|i| format!("key{}", i)).collect();
    let results: HashMap<String, String> = cache.get_many(&keys).await?;
    println!("✓ Batch get {} entries", results.len());
    for (key, value) in &results {
        println!("  {}: {}", key, value);
    }

    // Batch delete
    let deleted = cache.delete_many(&keys).await?;
    println!("✓ Batch deleted {} entries\n", deleted);

    Ok(())
}

async fn demo_cache_stats() -> CacheResult<()> {
    println!("--- Example 7: Cache Statistics ---");

    let cache: MemoryCache<String, Vec<u8>> = MemoryCache::new(CacheConfig::default());

    // Perform some operations
    for i in 1..=10 {
        cache
            .set(format!("key{}", i), format!("value{}", i), None)
            .await?;
    }

    // Some hits
    for i in 1..=5 {
        let _: Option<String> = cache.get(&format!("key{}", i)).await?;
    }

    // Some misses
    for i in 11..=15 {
        let _: Option<String> = cache.get(&format!("key{}", i)).await?;
    }

    // Get statistics
    let stats = cache.stats().await;
    println!("Cache Statistics:");
    println!("  Hits: {}", stats.hits);
    println!("  Misses: {}", stats.misses);
    println!("  Sets: {}", stats.sets);
    println!("  Deletes: {}", stats.deletes);
    println!("  Evictions: {}", stats.evictions);
    println!("  Size: {}", stats.size);
    println!("  Hit Rate: {:.2}%", stats.hit_rate() * 100.0);

    Ok(())
}
