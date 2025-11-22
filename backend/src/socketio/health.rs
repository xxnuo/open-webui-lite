/// Health Monitoring and Heartbeat System
///
/// Provides comprehensive health checks, heartbeat monitoring,
/// and connection quality metrics for Socket.IO connections
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Default instant for deserialization
fn default_instant() -> Instant {
    Instant::now()
}

/// Health status of a component
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Connection health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionHealth {
    pub session_id: String,
    pub status: HealthStatus,
    pub last_heartbeat: u64,
    pub missed_heartbeats: u32,
    pub latency_ms: Option<f64>,
    pub packet_loss: f64,
    pub connection_quality: ConnectionQuality,
    #[serde(skip, default = "default_instant")]
    pub last_heartbeat_instant: Instant,
}

/// Connection quality rating
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionQuality {
    Excellent, // < 50ms latency, < 1% loss
    Good,      // < 150ms latency, < 3% loss
    Fair,      // < 300ms latency, < 10% loss
    Poor,      // > 300ms latency or > 10% loss
}

impl ConnectionHealth {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            status: HealthStatus::Healthy,
            last_heartbeat: now_timestamp(),
            missed_heartbeats: 0,
            latency_ms: None,
            packet_loss: 0.0,
            connection_quality: ConnectionQuality::Excellent,
            last_heartbeat_instant: Instant::now(),
        }
    }

    /// Update connection quality based on metrics
    pub fn update_quality(&mut self) {
        let latency = self.latency_ms.unwrap_or(0.0);
        let loss = self.packet_loss;

        self.connection_quality = if latency < 50.0 && loss < 0.01 {
            ConnectionQuality::Excellent
        } else if latency < 150.0 && loss < 0.03 {
            ConnectionQuality::Good
        } else if latency < 300.0 && loss < 0.10 {
            ConnectionQuality::Fair
        } else {
            ConnectionQuality::Poor
        };

        // Update status based on quality and missed heartbeats
        self.status = if self.missed_heartbeats == 0 {
            match self.connection_quality {
                ConnectionQuality::Excellent | ConnectionQuality::Good => HealthStatus::Healthy,
                ConnectionQuality::Fair => HealthStatus::Degraded,
                ConnectionQuality::Poor => HealthStatus::Degraded,
            }
        } else if self.missed_heartbeats < 3 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Unhealthy
        };
    }
}

/// System-wide health check
#[derive(Debug, Clone, Serialize)]
pub struct SystemHealth {
    pub overall_status: HealthStatus,
    pub components: HashMap<String, ComponentHealth>,
    pub timestamp: u64,
}

/// Component health information
#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub last_check: u64,
    pub metrics: HashMap<String, serde_json::Value>,
}

impl ComponentHealth {
    pub fn healthy(message: Option<String>) -> Self {
        Self {
            status: HealthStatus::Healthy,
            message,
            last_check: now_timestamp(),
            metrics: HashMap::new(),
        }
    }

    pub fn degraded(message: String) -> Self {
        Self {
            status: HealthStatus::Degraded,
            message: Some(message),
            last_check: now_timestamp(),
            metrics: HashMap::new(),
        }
    }

    pub fn unhealthy(message: String) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: Some(message),
            last_check: now_timestamp(),
            metrics: HashMap::new(),
        }
    }

    pub fn with_metric(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metrics.insert(key.to_string(), value);
        self
    }
}

/// Health monitor configuration
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Heartbeat interval (expected time between heartbeats)
    pub heartbeat_interval: Duration,
    /// Maximum missed heartbeats before marking unhealthy
    pub max_missed_heartbeats: u32,
    /// Cleanup interval for stale connections
    pub cleanup_interval: Duration,
    /// Connection timeout (after which connection is considered dead)
    pub connection_timeout: Duration,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(25),
            max_missed_heartbeats: 3,
            cleanup_interval: Duration::from_secs(60),
            connection_timeout: Duration::from_secs(90),
        }
    }
}

/// Health monitor for Socket.IO connections
pub struct HealthMonitor {
    /// Per-connection health tracking
    connections: Arc<RwLock<HashMap<String, ConnectionHealth>>>,

    /// System component health
    components: Arc<RwLock<HashMap<String, ComponentHealth>>>,

    /// Configuration
    config: HealthConfig,

    /// Latency samples for rolling average
    latency_samples: Arc<RwLock<HashMap<String, Vec<f64>>>>,
}

impl HealthMonitor {
    pub fn new(config: HealthConfig) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            components: Arc::new(RwLock::new(HashMap::new())),
            config,
            latency_samples: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new connection
    pub async fn register_connection(&self, session_id: &str) {
        let mut connections = self.connections.write().await;
        connections.insert(
            session_id.to_string(),
            ConnectionHealth::new(session_id.to_string()),
        );
        tracing::debug!("Registered connection health monitoring for {}", session_id);
    }

    /// Remove a connection
    pub async fn remove_connection(&self, session_id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(session_id);

        let mut samples = self.latency_samples.write().await;
        samples.remove(session_id);

        tracing::debug!("Removed connection health monitoring for {}", session_id);
    }

    /// Record a heartbeat
    pub async fn record_heartbeat(&self, session_id: &str, latency_ms: Option<f64>) {
        let mut connections = self.connections.write().await;

        if let Some(health) = connections.get_mut(session_id) {
            health.last_heartbeat = now_timestamp();
            health.last_heartbeat_instant = Instant::now();
            health.missed_heartbeats = 0;

            if let Some(latency) = latency_ms {
                health.latency_ms = Some(latency);

                // Update rolling average
                drop(connections); // Release write lock
                let mut samples = self.latency_samples.write().await;
                let session_samples = samples
                    .entry(session_id.to_string())
                    .or_insert_with(Vec::new);
                session_samples.push(latency);

                // Keep only last 100 samples
                if session_samples.len() > 100 {
                    session_samples.remove(0);
                }

                let avg_latency: f64 =
                    session_samples.iter().sum::<f64>() / session_samples.len() as f64;

                // Reacquire write lock
                drop(samples);
                let mut connections = self.connections.write().await;
                if let Some(health) = connections.get_mut(session_id) {
                    health.latency_ms = Some(avg_latency);
                    health.update_quality();
                }
            }
        }
    }

    /// Check for missed heartbeats
    pub async fn check_heartbeats(&self) {
        let mut connections = self.connections.write().await;
        let now = Instant::now();

        for (sid, health) in connections.iter_mut() {
            let elapsed = now.duration_since(health.last_heartbeat_instant);

            if elapsed > self.config.heartbeat_interval {
                health.missed_heartbeats += 1;
                health.update_quality();

                tracing::warn!(
                    "Missed heartbeat for session {} (total missed: {})",
                    sid,
                    health.missed_heartbeats
                );

                if health.missed_heartbeats >= self.config.max_missed_heartbeats {
                    health.status = HealthStatus::Unhealthy;
                    tracing::error!(
                        "Session {} marked as unhealthy (missed {} heartbeats)",
                        sid,
                        health.missed_heartbeats
                    );
                }
            }
        }
    }

    /// Clean up dead connections
    pub async fn cleanup_dead_connections(&self) -> Vec<String> {
        let mut connections = self.connections.write().await;
        let now = Instant::now();

        let mut dead_sids = Vec::new();

        connections.retain(|sid, health| {
            let elapsed = now.duration_since(health.last_heartbeat_instant);

            if elapsed > self.config.connection_timeout {
                tracing::info!("Cleaning up dead connection: {}", sid);
                dead_sids.push(sid.clone());
                false
            } else {
                true
            }
        });

        dead_sids
    }

    /// Get connection health
    pub async fn get_connection_health(&self, session_id: &str) -> Option<ConnectionHealth> {
        let connections = self.connections.read().await;
        connections.get(session_id).cloned()
    }

    /// Get all connection health statuses
    pub async fn get_all_connection_health(&self) -> Vec<ConnectionHealth> {
        let connections = self.connections.read().await;
        connections.values().cloned().collect()
    }

    /// Update component health
    pub async fn update_component(&self, component: &str, health: ComponentHealth) {
        let mut components = self.components.write().await;
        components.insert(component.to_string(), health);
    }

    /// Get system health
    pub async fn get_system_health(&self) -> SystemHealth {
        let components = self.components.read().await;

        // Determine overall status
        let overall_status = if components.is_empty() {
            HealthStatus::Unknown
        } else {
            let unhealthy_count = components
                .values()
                .filter(|c| c.status == HealthStatus::Unhealthy)
                .count();

            let degraded_count = components
                .values()
                .filter(|c| c.status == HealthStatus::Degraded)
                .count();

            if unhealthy_count > 0 {
                HealthStatus::Unhealthy
            } else if degraded_count > 0 {
                HealthStatus::Degraded
            } else {
                HealthStatus::Healthy
            }
        };

        SystemHealth {
            overall_status,
            components: components.clone(),
            timestamp: now_timestamp(),
        }
    }

    /// Get health statistics
    pub async fn get_stats(&self) -> HealthStats {
        let connections = self.connections.read().await;

        let total = connections.len();
        let healthy = connections
            .values()
            .filter(|c| c.status == HealthStatus::Healthy)
            .count();
        let degraded = connections
            .values()
            .filter(|c| c.status == HealthStatus::Degraded)
            .count();
        let unhealthy = connections
            .values()
            .filter(|c| c.status == HealthStatus::Unhealthy)
            .count();

        let latencies: Vec<f64> = connections.values().filter_map(|c| c.latency_ms).collect();

        let avg_latency = if latencies.is_empty() {
            0.0
        } else {
            latencies.iter().sum::<f64>() / latencies.len() as f64
        };

        HealthStats {
            total_connections: total,
            healthy_connections: healthy,
            degraded_connections: degraded,
            unhealthy_connections: unhealthy,
            avg_latency_ms: avg_latency,
        }
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new(HealthConfig::default())
    }
}

/// Health statistics
#[derive(Debug, Clone, Serialize)]
pub struct HealthStats {
    pub total_connections: usize,
    pub healthy_connections: usize,
    pub degraded_connections: usize,
    pub unhealthy_connections: usize,
    pub avg_latency_ms: f64,
}

/// Get current Unix timestamp
fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_health() {
        let monitor = HealthMonitor::default();

        monitor.register_connection("session-1").await;

        let health = monitor.get_connection_health("session-1").await.unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);

        // Test with 40ms latency (< 50ms = Excellent)
        monitor.record_heartbeat("session-1", Some(40.0)).await;

        let health = monitor.get_connection_health("session-1").await.unwrap();
        assert_eq!(health.connection_quality, ConnectionQuality::Excellent);
    }

    #[tokio::test]
    async fn test_missed_heartbeats() {
        let config = HealthConfig {
            heartbeat_interval: Duration::from_millis(100),
            max_missed_heartbeats: 2,
            ..Default::default()
        };

        let monitor = HealthMonitor::new(config);
        monitor.register_connection("session-1").await;

        // Wait for heartbeat to be missed
        tokio::time::sleep(Duration::from_millis(150)).await;
        monitor.check_heartbeats().await;

        let health = monitor.get_connection_health("session-1").await.unwrap();
        assert!(health.missed_heartbeats > 0);
    }

    #[tokio::test]
    async fn test_connection_quality() {
        let mut health = ConnectionHealth::new("test".to_string());

        health.latency_ms = Some(40.0);
        health.packet_loss = 0.005;
        health.update_quality();
        assert_eq!(health.connection_quality, ConnectionQuality::Excellent);

        health.latency_ms = Some(200.0);
        health.packet_loss = 0.05;
        health.update_quality();
        assert_eq!(health.connection_quality, ConnectionQuality::Fair);
    }
}
