use crate::socketio::circuit_breaker::CircuitBreaker;
use crate::socketio::health::HealthMonitor;
/// Prometheus Metrics Exporter for Socket.IO
///
/// Exports Socket.IO metrics in Prometheus format for monitoring and alerting
use crate::socketio::metrics::SocketIOMetrics;
use crate::socketio::presence::PresenceManager;
use crate::socketio::rate_limit::RateLimiter;
use crate::socketio::recovery::RecoveryManager;
use std::fmt::Write;

/// Prometheus metrics exporter
pub struct PrometheusExporter {
    metrics: SocketIOMetrics,
    health_monitor: Option<HealthMonitor>,
    presence_manager: Option<PresenceManager>,
    rate_limiter: Option<RateLimiter>,
    recovery_manager: Option<RecoveryManager>,
    circuit_breakers: Vec<(String, CircuitBreaker)>,
}

impl PrometheusExporter {
    pub fn new(metrics: SocketIOMetrics) -> Self {
        Self {
            metrics,
            health_monitor: None,
            presence_manager: None,
            rate_limiter: None,
            recovery_manager: None,
            circuit_breakers: Vec::new(),
        }
    }

    pub fn with_health_monitor(mut self, monitor: HealthMonitor) -> Self {
        self.health_monitor = Some(monitor);
        self
    }

    pub fn with_presence_manager(mut self, manager: PresenceManager) -> Self {
        self.presence_manager = Some(manager);
        self
    }

    pub fn with_rate_limiter(mut self, limiter: RateLimiter) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    pub fn with_recovery_manager(mut self, manager: RecoveryManager) -> Self {
        self.recovery_manager = Some(manager);
        self
    }

    pub fn add_circuit_breaker(mut self, name: String, breaker: CircuitBreaker) -> Self {
        self.circuit_breakers.push((name, breaker));
        self
    }

    /// Generate Prometheus metrics in text format
    pub async fn export(&self) -> Result<String, std::fmt::Error> {
        let mut output = String::new();

        // Header
        writeln!(&mut output, "# Socket.IO Metrics")?;
        writeln!(&mut output)?;

        // Connection metrics
        let conn_metrics = self.metrics.get_connection_metrics().await;

        writeln!(
            &mut output,
            "# HELP socketio_connections_total Total number of connections since startup"
        )?;
        writeln!(&mut output, "# TYPE socketio_connections_total counter")?;
        writeln!(
            &mut output,
            "socketio_connections_total {}",
            conn_metrics.total
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_connections_active Number of active connections"
        )?;
        writeln!(&mut output, "# TYPE socketio_connections_active gauge")?;
        writeln!(
            &mut output,
            "socketio_connections_active {}",
            conn_metrics.active
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_connections_failed Total number of failed connections"
        )?;
        writeln!(&mut output, "# TYPE socketio_connections_failed counter")?;
        writeln!(
            &mut output,
            "socketio_connections_failed {}",
            conn_metrics.failed
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_connections_reconnections Total number of reconnections"
        )?;
        writeln!(
            &mut output,
            "# TYPE socketio_connections_reconnections counter"
        )?;
        writeln!(
            &mut output,
            "socketio_connections_reconnections {}",
            conn_metrics.reconnections
        )?;
        writeln!(&mut output)?;

        // Event metrics
        let event_metrics = self.metrics.get_event_metrics().await;

        writeln!(
            &mut output,
            "# HELP socketio_events_received_total Total number of events received"
        )?;
        writeln!(&mut output, "# TYPE socketio_events_received_total counter")?;
        writeln!(
            &mut output,
            "socketio_events_received_total {}",
            event_metrics.total_received
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_events_sent_total Total number of events sent"
        )?;
        writeln!(&mut output, "# TYPE socketio_events_sent_total counter")?;
        writeln!(
            &mut output,
            "socketio_events_sent_total {}",
            event_metrics.total_sent
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_events_failed_total Total number of failed events"
        )?;
        writeln!(&mut output, "# TYPE socketio_events_failed_total counter")?;
        writeln!(
            &mut output,
            "socketio_events_failed_total {}",
            event_metrics.total_failed
        )?;
        writeln!(&mut output)?;

        // Events by type
        writeln!(
            &mut output,
            "# HELP socketio_events_by_type_received Events received by type"
        )?;
        writeln!(
            &mut output,
            "# TYPE socketio_events_by_type_received counter"
        )?;
        for (event_type, count) in &event_metrics.by_type_received {
            writeln!(
                &mut output,
                "socketio_events_by_type_received{{type=\"{}\"}} {}",
                event_type, count
            )?;
        }
        writeln!(&mut output)?;

        // Room metrics
        let room_metrics = self.metrics.get_room_metrics().await;

        writeln!(
            &mut output,
            "# HELP socketio_rooms_total Total number of active rooms"
        )?;
        writeln!(&mut output, "# TYPE socketio_rooms_total gauge")?;
        writeln!(
            &mut output,
            "socketio_rooms_total {}",
            room_metrics.total_rooms
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_room_size_avg Average room size"
        )?;
        writeln!(&mut output, "# TYPE socketio_room_size_avg gauge")?;
        writeln!(
            &mut output,
            "socketio_room_size_avg {}",
            room_metrics.avg_room_size
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_room_size_max Maximum room size"
        )?;
        writeln!(&mut output, "# TYPE socketio_room_size_max gauge")?;
        writeln!(
            &mut output,
            "socketio_room_size_max {}",
            room_metrics.max_room_size
        )?;
        writeln!(&mut output)?;

        // Latency metrics
        let latency_metrics = self.metrics.get_latency_metrics().await;

        writeln!(
            &mut output,
            "# HELP socketio_latency_p50_microseconds 50th percentile latency in microseconds"
        )?;
        writeln!(
            &mut output,
            "# TYPE socketio_latency_p50_microseconds gauge"
        )?;
        writeln!(
            &mut output,
            "socketio_latency_p50_microseconds {}",
            latency_metrics.p50
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_latency_p95_microseconds 95th percentile latency in microseconds"
        )?;
        writeln!(
            &mut output,
            "# TYPE socketio_latency_p95_microseconds gauge"
        )?;
        writeln!(
            &mut output,
            "socketio_latency_p95_microseconds {}",
            latency_metrics.p95
        )?;
        writeln!(&mut output)?;

        writeln!(
            &mut output,
            "# HELP socketio_latency_p99_microseconds 99th percentile latency in microseconds"
        )?;
        writeln!(
            &mut output,
            "# TYPE socketio_latency_p99_microseconds gauge"
        )?;
        writeln!(
            &mut output,
            "socketio_latency_p99_microseconds {}",
            latency_metrics.p99
        )?;
        writeln!(&mut output)?;

        // Health metrics
        if let Some(health) = &self.health_monitor {
            let health_stats = health.get_stats().await;

            writeln!(
                &mut output,
                "# HELP socketio_health_total Total connections being monitored"
            )?;
            writeln!(&mut output, "# TYPE socketio_health_total gauge")?;
            writeln!(
                &mut output,
                "socketio_health_total {}",
                health_stats.total_connections
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_health_healthy Number of healthy connections"
            )?;
            writeln!(&mut output, "# TYPE socketio_health_healthy gauge")?;
            writeln!(
                &mut output,
                "socketio_health_healthy {}",
                health_stats.healthy_connections
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_health_degraded Number of degraded connections"
            )?;
            writeln!(&mut output, "# TYPE socketio_health_degraded gauge")?;
            writeln!(
                &mut output,
                "socketio_health_degraded {}",
                health_stats.degraded_connections
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_health_unhealthy Number of unhealthy connections"
            )?;
            writeln!(&mut output, "# TYPE socketio_health_unhealthy gauge")?;
            writeln!(
                &mut output,
                "socketio_health_unhealthy {}",
                health_stats.unhealthy_connections
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_health_avg_latency_ms Average connection latency in milliseconds"
            )?;
            writeln!(&mut output, "# TYPE socketio_health_avg_latency_ms gauge")?;
            writeln!(
                &mut output,
                "socketio_health_avg_latency_ms {}",
                health_stats.avg_latency_ms
            )?;
            writeln!(&mut output)?;
        }

        // Presence metrics
        if let Some(presence) = &self.presence_manager {
            let presence_stats = presence.get_stats().await;

            writeln!(
                &mut output,
                "# HELP socketio_presence_users_total Total users tracked"
            )?;
            writeln!(&mut output, "# TYPE socketio_presence_users_total gauge")?;
            writeln!(
                &mut output,
                "socketio_presence_users_total {}",
                presence_stats.total_users
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_presence_users_online Online users"
            )?;
            writeln!(&mut output, "# TYPE socketio_presence_users_online gauge")?;
            writeln!(
                &mut output,
                "socketio_presence_users_online {}",
                presence_stats.online_users
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_presence_users_away Away users"
            )?;
            writeln!(&mut output, "# TYPE socketio_presence_users_away gauge")?;
            writeln!(
                &mut output,
                "socketio_presence_users_away {}",
                presence_stats.away_users
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_presence_typing Number of users currently typing"
            )?;
            writeln!(&mut output, "# TYPE socketio_presence_typing gauge")?;
            writeln!(
                &mut output,
                "socketio_presence_typing {}",
                presence_stats.typing_users
            )?;
            writeln!(&mut output)?;
        }

        // Rate limiter metrics
        if let Some(rate_limiter) = &self.rate_limiter {
            let rl_stats = rate_limiter.get_stats().await;

            writeln!(
                &mut output,
                "# HELP socketio_ratelimit_active_users Users with active rate limit tracking"
            )?;
            writeln!(&mut output, "# TYPE socketio_ratelimit_active_users gauge")?;
            writeln!(
                &mut output,
                "socketio_ratelimit_active_users {}",
                rl_stats.active_users
            )?;
            writeln!(&mut output)?;

            writeln!(&mut output, "# HELP socketio_ratelimit_active_sessions Sessions with active rate limit tracking")?;
            writeln!(
                &mut output,
                "# TYPE socketio_ratelimit_active_sessions gauge"
            )?;
            writeln!(
                &mut output,
                "socketio_ratelimit_active_sessions {}",
                rl_stats.active_sessions
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_ratelimit_queue_size Total queue size across all sessions"
            )?;
            writeln!(&mut output, "# TYPE socketio_ratelimit_queue_size gauge")?;
            writeln!(
                &mut output,
                "socketio_ratelimit_queue_size {}",
                rl_stats.total_queue_size
            )?;
            writeln!(&mut output)?;
        }

        // Recovery metrics
        if let Some(recovery) = &self.recovery_manager {
            let recovery_stats = recovery.get_stats().await;

            writeln!(
                &mut output,
                "# HELP socketio_recovery_active_states Active recovery states"
            )?;
            writeln!(&mut output, "# TYPE socketio_recovery_active_states gauge")?;
            writeln!(
                &mut output,
                "socketio_recovery_active_states {}",
                recovery_stats.active_states
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_recovery_buffered_messages Total buffered messages"
            )?;
            writeln!(
                &mut output,
                "# TYPE socketio_recovery_buffered_messages gauge"
            )?;
            writeln!(
                &mut output,
                "socketio_recovery_buffered_messages {}",
                recovery_stats.total_buffered_messages
            )?;
            writeln!(&mut output)?;
        }

        // Circuit breaker metrics
        for (name, breaker) in &self.circuit_breakers {
            let stats = breaker.get_stats().await;

            writeln!(&mut output, "# HELP socketio_circuit_breaker_state Circuit breaker state (0=closed, 1=open, 2=half_open)")?;
            writeln!(&mut output, "# TYPE socketio_circuit_breaker_state gauge")?;
            let state_value = match stats.state {
                crate::socketio::circuit_breaker::CircuitState::Closed => 0,
                crate::socketio::circuit_breaker::CircuitState::Open => 1,
                crate::socketio::circuit_breaker::CircuitState::HalfOpen => 2,
            };
            writeln!(
                &mut output,
                "socketio_circuit_breaker_state{{name=\"{}\"}} {}",
                name, state_value
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_circuit_breaker_failures Current failure count"
            )?;
            writeln!(
                &mut output,
                "# TYPE socketio_circuit_breaker_failures gauge"
            )?;
            writeln!(
                &mut output,
                "socketio_circuit_breaker_failures{{name=\"{}\"}} {}",
                name, stats.failure_count
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_circuit_breaker_operations_total Total operations"
            )?;
            writeln!(
                &mut output,
                "# TYPE socketio_circuit_breaker_operations_total counter"
            )?;
            writeln!(
                &mut output,
                "socketio_circuit_breaker_operations_total{{name=\"{}\"}} {}",
                name, stats.total_operations
            )?;
            writeln!(&mut output)?;

            writeln!(
                &mut output,
                "# HELP socketio_circuit_breaker_failures_total Total failures"
            )?;
            writeln!(
                &mut output,
                "# TYPE socketio_circuit_breaker_failures_total counter"
            )?;
            writeln!(
                &mut output,
                "socketio_circuit_breaker_failures_total{{name=\"{}\"}} {}",
                name, stats.total_failures
            )?;
            writeln!(&mut output)?;
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prometheus_export() {
        let metrics = SocketIOMetrics::new();

        // Record some metrics
        metrics.record_connection().await;
        metrics.record_event_received("test-event").await;
        metrics.record_latency(1000).await;

        let exporter = PrometheusExporter::new(metrics);
        let output = exporter.export().await.unwrap();

        assert!(output.contains("socketio_connections_total"));
        assert!(output.contains("socketio_events_received_total"));
        assert!(output.contains("socketio_latency_p50_microseconds"));
    }
}
