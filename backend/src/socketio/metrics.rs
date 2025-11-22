/// Socket.IO Metrics and Monitoring
///
/// Provides metrics collection for monitoring and observability
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Metrics collector for Socket.IO
#[derive(Clone)]
pub struct SocketIOMetrics {
    /// Connection count
    connections: Arc<RwLock<ConnectionMetrics>>,

    /// Event metrics
    events: Arc<RwLock<EventMetrics>>,

    /// Room metrics
    rooms: Arc<RwLock<RoomMetrics>>,

    /// Latency tracking
    latency: Arc<RwLock<LatencyMetrics>>,
}

#[derive(Default)]
struct ConnectionMetrics {
    total_connections: u64,
    active_connections: u64,
    failed_connections: u64,
    reconnections: u64,
}

#[derive(Default)]
struct EventMetrics {
    events_received: HashMap<String, u64>,
    events_sent: HashMap<String, u64>,
    events_failed: HashMap<String, u64>,
}

#[derive(Default)]
struct RoomMetrics {
    total_rooms: u64,
    room_sizes: HashMap<String, usize>,
}

#[derive(Default)]
struct LatencyMetrics {
    samples: Vec<u64>, // in microseconds
    max_samples: usize,
}

impl SocketIOMetrics {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(ConnectionMetrics::default())),
            events: Arc::new(RwLock::new(EventMetrics::default())),
            rooms: Arc::new(RwLock::new(RoomMetrics::default())),
            latency: Arc::new(RwLock::new(LatencyMetrics {
                samples: Vec::new(),
                max_samples: 1000,
            })),
        }
    }

    /// Record a new connection
    pub async fn record_connection(&self) {
        let mut conn = self.connections.write().await;
        conn.total_connections += 1;
        conn.active_connections += 1;
    }

    /// Record a disconnection
    pub async fn record_disconnection(&self) {
        let mut conn = self.connections.write().await;
        if conn.active_connections > 0 {
            conn.active_connections -= 1;
        }
    }

    /// Record a failed connection
    pub async fn record_failed_connection(&self) {
        let mut conn = self.connections.write().await;
        conn.failed_connections += 1;
    }

    /// Record a reconnection
    pub async fn record_reconnection(&self) {
        let mut conn = self.connections.write().await;
        conn.reconnections += 1;
    }

    /// Record an event received
    pub async fn record_event_received(&self, event: &str) {
        let mut events = self.events.write().await;
        *events.events_received.entry(event.to_string()).or_insert(0) += 1;
    }

    /// Record an event sent
    pub async fn record_event_sent(&self, event: &str) {
        let mut events = self.events.write().await;
        *events.events_sent.entry(event.to_string()).or_insert(0) += 1;
    }

    /// Record a failed event
    pub async fn record_event_failed(&self, event: &str) {
        let mut events = self.events.write().await;
        *events.events_failed.entry(event.to_string()).or_insert(0) += 1;
    }

    /// Record room size
    pub async fn record_room_size(&self, room: &str, size: usize) {
        let mut rooms = self.rooms.write().await;
        if size > 0 {
            rooms.room_sizes.insert(room.to_string(), size);
            rooms.total_rooms = rooms.room_sizes.len() as u64;
        } else {
            rooms.room_sizes.remove(room);
            rooms.total_rooms = rooms.room_sizes.len() as u64;
        }
    }

    /// Record latency sample
    pub async fn record_latency(&self, duration_micros: u64) {
        let mut latency = self.latency.write().await;
        latency.samples.push(duration_micros);

        // Keep only recent samples
        let max_samples = latency.max_samples;
        if latency.samples.len() > max_samples {
            let samples_len = latency.samples.len();
            latency.samples.drain(0..samples_len - max_samples);
        }
    }

    /// Get connection metrics
    pub async fn get_connection_metrics(&self) -> ConnectionStats {
        let conn = self.connections.read().await;
        ConnectionStats {
            total: conn.total_connections,
            active: conn.active_connections,
            failed: conn.failed_connections,
            reconnections: conn.reconnections,
        }
    }

    /// Get event metrics
    pub async fn get_event_metrics(&self) -> EventStats {
        let events = self.events.read().await;

        let total_received: u64 = events.events_received.values().sum();
        let total_sent: u64 = events.events_sent.values().sum();
        let total_failed: u64 = events.events_failed.values().sum();

        EventStats {
            total_received,
            total_sent,
            total_failed,
            by_type_received: events.events_received.clone(),
            by_type_sent: events.events_sent.clone(),
            by_type_failed: events.events_failed.clone(),
        }
    }

    /// Get room metrics
    pub async fn get_room_metrics(&self) -> RoomStats {
        let rooms = self.rooms.read().await;

        let mut sizes: Vec<usize> = rooms.room_sizes.values().copied().collect();
        sizes.sort_unstable();

        let avg_size = if sizes.is_empty() {
            0.0
        } else {
            sizes.iter().sum::<usize>() as f64 / sizes.len() as f64
        };

        let median_size = if sizes.is_empty() {
            0
        } else {
            sizes[sizes.len() / 2]
        };

        let max_size = sizes.last().copied().unwrap_or(0);

        RoomStats {
            total_rooms: rooms.total_rooms,
            avg_room_size: avg_size,
            median_room_size: median_size,
            max_room_size: max_size,
        }
    }

    /// Get latency metrics
    pub async fn get_latency_metrics(&self) -> LatencyStats {
        let latency = self.latency.read().await;

        if latency.samples.is_empty() {
            return LatencyStats {
                p50: 0,
                p95: 0,
                p99: 0,
                max: 0,
                samples: 0,
            };
        }

        let mut sorted = latency.samples.clone();
        sorted.sort_unstable();

        let p50_idx = (sorted.len() as f64 * 0.50) as usize;
        let p95_idx = (sorted.len() as f64 * 0.95) as usize;
        let p99_idx = (sorted.len() as f64 * 0.99) as usize;

        LatencyStats {
            p50: sorted.get(p50_idx).copied().unwrap_or(0),
            p95: sorted.get(p95_idx).copied().unwrap_or(0),
            p99: sorted.get(p99_idx).copied().unwrap_or(0),
            max: sorted.last().copied().unwrap_or(0),
            samples: sorted.len(),
        }
    }

    /// Get all metrics as JSON
    pub async fn get_all_metrics(&self) -> serde_json::Value {
        let conn = self.get_connection_metrics().await;
        let events = self.get_event_metrics().await;
        let rooms = self.get_room_metrics().await;
        let latency = self.get_latency_metrics().await;

        serde_json::json!({
            "connections": {
                "total": conn.total,
                "active": conn.active,
                "failed": conn.failed,
                "reconnections": conn.reconnections,
            },
            "events": {
                "total_received": events.total_received,
                "total_sent": events.total_sent,
                "total_failed": events.total_failed,
                "by_type_received": events.by_type_received,
                "by_type_sent": events.by_type_sent,
                "by_type_failed": events.by_type_failed,
            },
            "rooms": {
                "total": rooms.total_rooms,
                "avg_size": rooms.avg_room_size,
                "median_size": rooms.median_room_size,
                "max_size": rooms.max_room_size,
            },
            "latency": {
                "p50_micros": latency.p50,
                "p95_micros": latency.p95,
                "p99_micros": latency.p99,
                "max_micros": latency.max,
                "samples": latency.samples,
            }
        })
    }

    /// Reset all metrics
    pub async fn reset(&self) {
        let mut conn = self.connections.write().await;
        *conn = ConnectionMetrics::default();

        let mut events = self.events.write().await;
        *events = EventMetrics::default();

        let mut rooms = self.rooms.write().await;
        *rooms = RoomMetrics::default();

        let mut latency = self.latency.write().await;
        latency.samples.clear();
    }
}

impl Default for SocketIOMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub total: u64,
    pub active: u64,
    pub failed: u64,
    pub reconnections: u64,
}

/// Event statistics
#[derive(Debug, Clone)]
pub struct EventStats {
    pub total_received: u64,
    pub total_sent: u64,
    pub total_failed: u64,
    pub by_type_received: HashMap<String, u64>,
    pub by_type_sent: HashMap<String, u64>,
    pub by_type_failed: HashMap<String, u64>,
}

/// Room statistics
#[derive(Debug, Clone)]
pub struct RoomStats {
    pub total_rooms: u64,
    pub avg_room_size: f64,
    pub median_room_size: usize,
    pub max_room_size: usize,
}

/// Latency statistics (all values in microseconds)
#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub p50: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
    pub samples: usize,
}

/// Latency tracker helper
#[allow(dead_code)]
pub struct LatencyTracker {
    start: Instant,
    metrics: SocketIOMetrics,
}

#[allow(dead_code)]
impl LatencyTracker {
    pub fn new(metrics: SocketIOMetrics) -> Self {
        Self {
            start: Instant::now(),
            metrics,
        }
    }

    pub async fn finish(self) {
        let duration = self.start.elapsed();
        self.metrics
            .record_latency(duration.as_micros() as u64)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_metrics() {
        let metrics = SocketIOMetrics::new();

        metrics.record_connection().await;
        metrics.record_connection().await;

        let stats = metrics.get_connection_metrics().await;
        assert_eq!(stats.total, 2);
        assert_eq!(stats.active, 2);

        metrics.record_disconnection().await;
        let stats = metrics.get_connection_metrics().await;
        assert_eq!(stats.active, 1);
    }

    #[tokio::test]
    async fn test_event_metrics() {
        let metrics = SocketIOMetrics::new();

        metrics.record_event_received("user-join").await;
        metrics.record_event_received("user-join").await;
        metrics.record_event_sent("chat-events").await;

        let stats = metrics.get_event_metrics().await;
        assert_eq!(stats.total_received, 2);
        assert_eq!(stats.total_sent, 1);
        assert_eq!(stats.by_type_received.get("user-join"), Some(&2));
    }

    #[tokio::test]
    async fn test_latency_metrics() {
        let metrics = SocketIOMetrics::new();

        metrics.record_latency(100).await;
        metrics.record_latency(200).await;
        metrics.record_latency(300).await;

        let stats = metrics.get_latency_metrics().await;
        assert_eq!(stats.samples, 3);
        assert_eq!(stats.p50, 200);
        assert_eq!(stats.max, 300);
    }
}
