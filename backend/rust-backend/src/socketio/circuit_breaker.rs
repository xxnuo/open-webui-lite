/// Circuit Breaker Pattern for Redis and External Services
///
/// Implements the circuit breaker pattern to prevent cascading failures
/// and provide graceful degradation when external services fail
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CircuitState {
    /// Circuit is closed, requests flow through normally
    Closed,
    /// Circuit is open, requests are blocked
    Open,
    /// Circuit is half-open, allowing a limited number of test requests
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open the circuit
    pub failure_threshold: usize,
    /// Success threshold to close the circuit from half-open
    pub success_threshold: usize,
    /// Time window for counting failures
    pub failure_window: Duration,
    /// Timeout before attempting recovery (Open -> HalfOpen)
    pub timeout: Duration,
    /// Maximum number of half-open requests
    pub half_open_max_requests: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,                    // 5 failures
            success_threshold: 2,                    // 2 successes to recover
            failure_window: Duration::from_secs(60), // in 60 seconds
            timeout: Duration::from_secs(30),        // 30 seconds before retry
            half_open_max_requests: 3,               // max 3 test requests
        }
    }
}

/// Circuit breaker implementation
pub struct CircuitBreaker {
    /// Current circuit state
    state: Arc<RwLock<CircuitState>>,

    /// Configuration
    config: CircuitBreakerConfig,

    /// Failure count in current window
    failure_count: AtomicUsize,

    /// Success count in half-open state
    success_count: AtomicUsize,

    /// Last failure timestamp
    last_failure_time: Arc<RwLock<Option<Instant>>>,

    /// When circuit was opened
    opened_at: Arc<RwLock<Option<Instant>>>,

    /// Half-open request count
    half_open_requests: AtomicUsize,

    /// Total operations
    total_operations: AtomicU64,

    /// Total failures
    total_failures: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            config,
            failure_count: AtomicUsize::new(0),
            success_count: AtomicUsize::new(0),
            last_failure_time: Arc::new(RwLock::new(None)),
            opened_at: Arc::new(RwLock::new(None)),
            half_open_requests: AtomicUsize::new(0),
            total_operations: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
        }
    }

    /// Check if request is allowed through the circuit breaker
    pub async fn allow_request(&self) -> bool {
        self.total_operations.fetch_add(1, Ordering::Relaxed);

        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                let opened_at = self.opened_at.read().await;
                if let Some(opened_time) = *opened_at {
                    if opened_time.elapsed() >= self.config.timeout {
                        drop(opened_at);
                        // Transition to half-open
                        let mut state_write = self.state.write().await;
                        *state_write = CircuitState::HalfOpen;
                        self.success_count.store(0, Ordering::Relaxed);
                        self.half_open_requests.store(0, Ordering::Relaxed);
                        tracing::info!("Circuit breaker transitioning to half-open");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited number of requests
                let current = self.half_open_requests.fetch_add(1, Ordering::SeqCst);
                current < self.config.half_open_max_requests
            }
        }
    }

    /// Record a successful operation
    pub async fn record_success(&self) {
        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;

                if successes >= self.config.success_threshold {
                    // Transition to closed
                    let mut state_write = self.state.write().await;
                    *state_write = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                    self.half_open_requests.store(0, Ordering::Relaxed);

                    let mut opened_at = self.opened_at.write().await;
                    *opened_at = None;

                    tracing::info!("Circuit breaker closed after successful recovery");
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
            }
        }
    }

    /// Record a failed operation
    pub async fn record_failure(&self) {
        self.total_failures.fetch_add(1, Ordering::Relaxed);

        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => {
                // Check if failure window has expired
                let mut last_failure = self.last_failure_time.write().await;
                let now = Instant::now();

                if let Some(last_time) = *last_failure {
                    if now.duration_since(last_time) > self.config.failure_window {
                        // Reset window
                        self.failure_count.store(1, Ordering::Relaxed);
                        *last_failure = Some(now);
                        return;
                    }
                }

                *last_failure = Some(now);
                drop(last_failure);

                let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;

                if failures >= self.config.failure_threshold {
                    // Open the circuit
                    let mut state_write = self.state.write().await;
                    *state_write = CircuitState::Open;

                    let mut opened_at = self.opened_at.write().await;
                    *opened_at = Some(Instant::now());

                    tracing::warn!("Circuit breaker opened after {} failures", failures);
                }
            }
            CircuitState::HalfOpen => {
                // Failure in half-open state reopens the circuit
                let mut state_write = self.state.write().await;
                *state_write = CircuitState::Open;

                let mut opened_at = self.opened_at.write().await;
                *opened_at = Some(Instant::now());

                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
                self.half_open_requests.store(0, Ordering::Relaxed);

                tracing::warn!("Circuit breaker reopened after failure in half-open state");
            }
            CircuitState::Open => {
                // Already open, nothing to do
            }
        }
    }

    /// Get current circuit state
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// Get statistics
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        let state = *self.state.read().await;
        let opened_at = *self.opened_at.read().await;

        CircuitBreakerStats {
            state,
            failure_count: self.failure_count.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
            total_operations: self.total_operations.load(Ordering::Relaxed),
            total_failures: self.total_failures.load(Ordering::Relaxed),
            opened_at_ms: opened_at.map(|t| t.elapsed().as_millis() as u64),
        }
    }

    /// Reset the circuit breaker (for testing/admin purposes)
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;

        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.half_open_requests.store(0, Ordering::Relaxed);

        let mut last_failure = self.last_failure_time.write().await;
        *last_failure = None;

        let mut opened_at = self.opened_at.write().await;
        *opened_at = None;

        tracing::info!("Circuit breaker manually reset");
    }

    /// Execute a function with circuit breaker protection
    pub async fn execute<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if !self.allow_request().await {
            return Err(CircuitBreakerError::CircuitOpen);
        }

        match f() {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(err) => {
                self.record_failure().await;
                Err(CircuitBreakerError::OperationFailed(err))
            }
        }
    }

    /// Execute an async function with circuit breaker protection
    pub async fn execute_async<F, Fut, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if !self.allow_request().await {
            return Err(CircuitBreakerError::CircuitOpen);
        }

        match f().await {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(err) => {
                self.record_failure().await;
                Err(CircuitBreakerError::OperationFailed(err))
            }
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }
}

/// Circuit breaker error
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker is open")]
    CircuitOpen,

    #[error("Operation failed: {0}")]
    OperationFailed(E),
}

/// Circuit breaker statistics
#[derive(Debug, Clone, Serialize)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: usize,
    pub success_count: usize,
    pub total_operations: u64,
    pub total_failures: u64,
    pub opened_at_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_opens() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_millis(100),
            ..Default::default()
        };

        let cb = CircuitBreaker::new(config);

        // Record failures
        for _ in 0..3 {
            cb.record_failure().await;
        }

        assert_eq!(cb.get_state().await, CircuitState::Open);
        assert!(!cb.allow_request().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            timeout: Duration::from_millis(50),
            ..Default::default()
        };

        let cb = CircuitBreaker::new(config);

        // Open circuit
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.get_state().await, CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should transition to half-open
        assert!(cb.allow_request().await);
        assert_eq!(cb.get_state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(50),
            ..Default::default()
        };

        let cb = CircuitBreaker::new(config);

        // Open circuit
        cb.record_failure().await;
        cb.record_failure().await;

        // Wait and transition to half-open
        tokio::time::sleep(Duration::from_millis(60)).await;
        cb.allow_request().await;

        // Record successes
        cb.record_success().await;
        cb.record_success().await;

        assert_eq!(cb.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_execute_with_circuit_breaker() {
        let cb = CircuitBreaker::default();

        // Successful execution
        let result = cb.execute(|| Ok::<i32, String>(42)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        // Failed execution
        let result = cb.execute(|| Err::<i32, String>("error".to_string())).await;
        assert!(matches!(
            result,
            Err(CircuitBreakerError::OperationFailed(_))
        ));
    }
}
