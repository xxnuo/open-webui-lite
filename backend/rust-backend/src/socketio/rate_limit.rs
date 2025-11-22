/// Rate Limiting and Backpressure for Socket.IO
///
/// Provides per-user rate limiting to prevent abuse and ensure fair resource allocation
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Configuration for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum events per window
    pub max_events: usize,
    /// Time window duration
    pub window_duration: Duration,
    /// Maximum queue size per connection
    pub max_queue_size: usize,
    /// Burst allowance (extra events allowed in short bursts)
    pub burst_allowance: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_events: 100,                          // 100 events
            window_duration: Duration::from_secs(60), // per minute
            max_queue_size: 1000,                     // 1000 queued messages max
            burst_allowance: 20,                      // Allow 20 extra events in bursts
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    capacity: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(capacity: usize, window_duration: Duration) -> Self {
        let capacity_f64 = capacity as f64;
        let refill_rate = capacity_f64 / window_duration.as_secs_f64();

        Self {
            tokens: capacity_f64,
            last_refill: Instant::now(),
            capacity: capacity_f64,
            refill_rate,
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        // Add tokens based on elapsed time
        let new_tokens = elapsed * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    fn try_consume(&mut self, count: usize) -> bool {
        self.refill();

        let count_f64 = count as f64;
        if self.tokens >= count_f64 {
            self.tokens -= count_f64;
            true
        } else {
            false
        }
    }

    fn available_tokens(&mut self) -> usize {
        self.refill();
        self.tokens.floor() as usize
    }
}

/// Rate limiter for Socket.IO events
pub struct RateLimiter {
    config: RateLimitConfig,
    /// Per-user token buckets
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    /// Per-session queue sizes
    queue_sizes: Arc<RwLock<HashMap<String, usize>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
            queue_sizes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if an event is allowed for a user
    pub async fn check_rate_limit(
        &self,
        user_id: &str,
        event_count: usize,
    ) -> Result<(), RateLimitError> {
        let mut buckets = self.buckets.write().await;

        let bucket = buckets.entry(user_id.to_string()).or_insert_with(|| {
            TokenBucket::new(
                self.config.max_events + self.config.burst_allowance,
                self.config.window_duration,
            )
        });

        if bucket.try_consume(event_count) {
            Ok(())
        } else {
            Err(RateLimitError::ExceededLimit {
                user_id: user_id.to_string(),
                available: bucket.available_tokens(),
                requested: event_count,
            })
        }
    }

    /// Check queue backpressure for a session
    pub async fn check_queue_size(&self, sid: &str) -> Result<(), RateLimitError> {
        let queue_sizes = self.queue_sizes.read().await;

        let current_size = queue_sizes.get(sid).copied().unwrap_or(0);

        if current_size >= self.config.max_queue_size {
            Err(RateLimitError::QueueFull {
                sid: sid.to_string(),
                current_size,
                max_size: self.config.max_queue_size,
            })
        } else {
            Ok(())
        }
    }

    /// Increment queue size for a session
    pub async fn increment_queue(&self, sid: &str, amount: usize) {
        let mut queue_sizes = self.queue_sizes.write().await;
        *queue_sizes.entry(sid.to_string()).or_insert(0) += amount;
    }

    /// Decrement queue size for a session
    pub async fn decrement_queue(&self, sid: &str, amount: usize) {
        let mut queue_sizes = self.queue_sizes.write().await;

        if let Some(size) = queue_sizes.get_mut(sid) {
            *size = size.saturating_sub(amount);
            if *size == 0 {
                queue_sizes.remove(sid);
            }
        }
    }

    /// Remove session from tracking
    pub async fn remove_session(&self, sid: &str) {
        let mut queue_sizes = self.queue_sizes.write().await;
        queue_sizes.remove(sid);
    }

    /// Remove user from tracking
    pub async fn remove_user(&self, user_id: &str) {
        let mut buckets = self.buckets.write().await;
        buckets.remove(user_id);
    }

    /// Get statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        let buckets = self.buckets.read().await;
        let queue_sizes = self.queue_sizes.read().await;

        RateLimitStats {
            active_users: buckets.len(),
            active_sessions: queue_sizes.len(),
            total_queue_size: queue_sizes.values().sum(),
        }
    }

    /// Clean up old buckets (call periodically)
    pub async fn cleanup_old_buckets(&self) {
        let mut buckets = self.buckets.write().await;

        // Remove buckets that have been idle for a long time
        let idle_threshold = Duration::from_secs(600); // 10 minutes
        let now = Instant::now();

        buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < idle_threshold);
    }
}

/// Rate limiting statistics
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub active_users: usize,
    pub active_sessions: usize,
    pub total_queue_size: usize,
}

/// Rate limiting errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum RateLimitError {
    #[error(
        "Rate limit exceeded for user {user_id}: requested {requested}, available {available}"
    )]
    ExceededLimit {
        user_id: String,
        available: usize,
        requested: usize,
    },

    #[error("Queue full for session {sid}: {current_size}/{max_size}")]
    QueueFull {
        sid: String,
        current_size: usize,
        max_size: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter() {
        let config = RateLimitConfig {
            max_events: 10,
            window_duration: Duration::from_secs(1),
            max_queue_size: 100,
            burst_allowance: 5,
        };

        let limiter = RateLimiter::new(config);

        // Should allow initial events (max_events + burst_allowance = 10 + 5 = 15)
        for _ in 0..15 {
            assert!(limiter.check_rate_limit("user-1", 1).await.is_ok());
        }

        // Should block after limit (15 tokens consumed)
        assert!(limiter.check_rate_limit("user-1", 1).await.is_err());

        // Wait for refill
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Should allow again
        assert!(limiter.check_rate_limit("user-1", 1).await.is_ok());
    }

    #[tokio::test]
    async fn test_queue_backpressure() {
        let config = RateLimitConfig {
            max_events: 100,
            window_duration: Duration::from_secs(60),
            max_queue_size: 10,
            burst_allowance: 0,
        };

        let limiter = RateLimiter::new(config);
        let sid = "session-1";

        // Increment queue
        limiter.increment_queue(sid, 5).await;
        assert!(limiter.check_queue_size(sid).await.is_ok());

        // Increment to max
        limiter.increment_queue(sid, 5).await;
        assert!(limiter.check_queue_size(sid).await.is_err());

        // Decrement
        limiter.decrement_queue(sid, 5).await;
        assert!(limiter.check_queue_size(sid).await.is_ok());
    }

    #[test]
    fn test_token_bucket() {
        let mut bucket = TokenBucket::new(10, Duration::from_secs(1));

        // Should allow consuming tokens
        assert!(bucket.try_consume(5));
        assert_eq!(bucket.available_tokens(), 5);

        // Should block when out of tokens
        assert!(bucket.try_consume(5));
        assert!(!bucket.try_consume(1));
    }
}
