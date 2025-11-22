//! Practical cache integration examples
//!
//! Shows how to integrate caching into real-world services

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

// Mock types for demonstration
#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: String,
    name: String,
    email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelResponse {
    model: String,
    response: String,
    tokens: u32,
}

// ============================================================================
// Example 1: User Service with Caching
// ============================================================================

struct UserService {
    // In real implementation, this would be your database connection
}

impl UserService {
    async fn fetch_user_from_db(&self, user_id: &str) -> Result<User, Box<dyn std::error::Error>> {
        // Simulate database query
        println!("  [DB] Fetching user {} from database...", user_id);
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(User {
            id: user_id.to_string(),
            name: format!("User {}", user_id),
            email: format!("user{}@example.com", user_id),
        })
    }

    /// Get user with automatic caching
    async fn get_user(&self, user_id: &str) -> Result<User, Box<dyn std::error::Error>> {
        // In real implementation, import from cache_manager
        use open_webui_rust::utils::cache::*;

        let cache_key = format!("user:{}", user_id);
        let cache = MemoryCache::new(CacheConfig::default());

        // Try cache first
        if let Ok(Some(user)) = cache.get::<_, User>(&cache_key).await {
            println!("✓ Cache hit for user {}", user_id);
            return Ok(user);
        }

        println!("✗ Cache miss for user {}", user_id);

        // Fetch from database
        let user = self.fetch_user_from_db(user_id).await?;

        // Cache the result
        let _ = cache
            .set(&cache_key, &user, Some(Duration::from_secs(300)))
            .await;

        Ok(user)
    }

    /// Update user and invalidate cache
    async fn update_user(&self, user: &User) -> Result<(), Box<dyn std::error::Error>> {
        use open_webui_rust::utils::cache::*;

        // Update database (simulated)
        println!("  [DB] Updating user {} in database...", user.id);
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Invalidate cache
        let cache_key = format!("user:{}", user.id);
        let cache = MemoryCache::new(CacheConfig::default());
        cache.delete(&cache_key).await?;

        println!("✓ Invalidated cache for user {}", user.id);
        Ok(())
    }
}

// ============================================================================
// Example 2: AI Model Service with Response Caching
// ============================================================================

struct ModelService {
    // In real implementation, this would be your AI client
}

impl ModelService {
    async fn generate_response(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<ModelResponse, Box<dyn std::error::Error>> {
        // Simulate expensive AI model inference
        println!("  [AI] Generating response with model {}...", model);
        tokio::time::sleep(Duration::from_secs(2)).await;

        Ok(ModelResponse {
            model: model.to_string(),
            response: format!("Response to: {}", prompt),
            tokens: 150,
        })
    }

    /// Generate with automatic caching based on prompt hash
    async fn generate_cached(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<ModelResponse, Box<dyn std::error::Error>> {
        use open_webui_rust::utils::cache::*;
        use sha2::{Digest, Sha256};

        // Hash the prompt for cache key
        let prompt_hash = format!("{:x}", Sha256::digest(prompt.as_bytes()));
        let cache_key = make_cache_key(&["model", model, &prompt_hash]);

        let cache = MemoryCache::new(CacheConfig {
            max_size: 1000,
            default_ttl: Some(Duration::from_secs(7200)), // 2 hours
            ..Default::default()
        });

        // Try cache first
        if let Ok(Some(response)) = cache.get::<_, ModelResponse>(&cache_key).await {
            println!("✓ Using cached model response");
            return Ok(response);
        }

        println!("✗ Cache miss, generating new response");

        // Generate new response
        let response = self.generate_response(model, prompt).await?;

        // Cache it
        let _ = cache.set(&cache_key, &response, None).await;

        Ok(response)
    }
}

// ============================================================================
// Example 3: API Client with Rate-Limited Caching
// ============================================================================

struct ExternalApiClient {
    cache: Arc<open_webui_rust::utils::cache::MemoryCache<String, Vec<u8>>>,
}

impl ExternalApiClient {
    fn new() -> Self {
        use open_webui_rust::utils::cache::*;

        Self {
            cache: Arc::new(MemoryCache::new(CacheConfig {
                max_size: 5000,
                default_ttl: Some(Duration::from_secs(300)), // 5 minutes
                ..Default::default()
            })),
        }
    }

    async fn fetch_external_data(
        &self,
        endpoint: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        // Simulate external API call
        println!("  [API] Calling external API: {}", endpoint);
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(serde_json::json!({
            "endpoint": endpoint,
            "data": "External data",
            "timestamp": chrono::Utc::now().timestamp()
        }))
    }

    /// Fetch with caching and rate limiting
    async fn get(&self, endpoint: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        use open_webui_rust::utils::cache::*;

        let cache_key = format!("api:{}", endpoint);

        // Try cache first
        if let Ok(Some(data)) = self.cache.get::<_, serde_json::Value>(&cache_key).await {
            println!("✓ Using cached API response");
            return Ok(data);
        }

        println!("✗ Cache miss, calling external API");

        // Fetch from external API
        let data = self.fetch_external_data(endpoint).await?;

        // Cache the result
        let _ = self.cache.set(&cache_key, &data, None).await;

        Ok(data)
    }

    /// Clear cache for specific endpoint
    async fn invalidate(&self, endpoint: &str) -> Result<(), Box<dyn std::error::Error>> {
        use open_webui_rust::utils::cache::*;

        let cache_key = format!("api:{}", endpoint);
        self.cache.delete(&cache_key).await?;
        println!("✓ Invalidated cache for endpoint: {}", endpoint);
        Ok(())
    }
}

// ============================================================================
// Example 4: Multi-Request Caching with Stampede Prevention
// ============================================================================

async fn demo_stampede_prevention() -> Result<(), Box<dyn std::error::Error>> {
    use open_webui_rust::utils::cache::StampedeGuard;

    println!("\n--- Stampede Prevention Demo ---");

    let guard = Arc::new(StampedeGuard::new());
    let mut handles = vec![];

    // Simulate 10 concurrent requests for the same expensive data
    for i in 0..10 {
        let guard = guard.clone();
        let handle = tokio::spawn(async move {
            let result = guard
                .execute("expensive_report", || async {
                    println!("  [{}] Executing expensive report generation...", i);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>("Report data".to_string())
                })
                .await;

            println!("  [{}] Got result", i);
            result
        });
        handles.push(handle);
    }

    // Wait for all requests
    for handle in handles {
        handle.await??;
    }

    println!("✓ Only one expensive operation executed (others waited)");
    Ok(())
}

// ============================================================================
// Main Demo
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Cache Integration Examples ===\n");

    // Example 1: User Service
    println!("--- Example 1: User Service ---");
    let user_service = UserService {};

    // First call - cache miss
    let user = user_service.get_user("123").await?;
    println!("Retrieved: {:?}\n", user);

    // Second call - cache hit
    let user = user_service.get_user("123").await?;
    println!("Retrieved: {:?}\n", user);

    // Update and invalidate
    let mut updated_user = user;
    updated_user.name = "Updated Name".to_string();
    user_service.update_user(&updated_user).await?;

    // Third call - cache miss again (after invalidation)
    let user = user_service.get_user("123").await?;
    println!("Retrieved after update: {:?}\n", user);

    // Example 2: Model Service
    println!("\n--- Example 2: AI Model Service ---");
    let model_service = ModelService {};

    let prompt = "What is the capital of France?";

    // First call - generates response
    let response = model_service.generate_cached("gpt-4", prompt).await?;
    println!("Response: {:?}\n", response);

    // Second call with same prompt - uses cache
    let response = model_service.generate_cached("gpt-4", prompt).await?;
    println!("Cached response: {:?}\n", response);

    // Different prompt - generates new response
    let response = model_service
        .generate_cached("gpt-4", "What is 2+2?")
        .await?;
    println!("New response: {:?}\n", response);

    // Example 3: External API Client
    println!("\n--- Example 3: External API Client ---");
    let api_client = ExternalApiClient::new();

    // First call - fetches from API
    let data = api_client.get("/users/list").await?;
    println!("Data: {}\n", data);

    // Second call - uses cache
    let data = api_client.get("/users/list").await?;
    println!("Cached data: {}\n", data);

    // Invalidate and refetch
    api_client.invalidate("/users/list").await?;
    let data = api_client.get("/users/list").await?;
    println!("Fresh data after invalidation: {}\n", data);

    // Example 4: Stampede Prevention
    demo_stampede_prevention().await?;

    println!("\n=== All Examples Complete ===");
    Ok(())
}
