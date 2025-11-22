/// Structured Logging for Socket.IO
///
/// Provides correlation IDs and structured logging for better observability
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Correlation ID for tracing requests across services
#[derive(Debug, Clone)]
pub struct CorrelationId(String);

impl CorrelationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    #[allow(dead_code)]
    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Logging context for Socket.IO operations
#[derive(Debug, Clone)]
pub struct LogContext {
    pub correlation_id: CorrelationId,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub event: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl LogContext {
    pub fn new() -> Self {
        Self {
            correlation_id: CorrelationId::new(),
            session_id: None,
            user_id: None,
            event: None,
            metadata: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    #[allow(dead_code)]
    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    #[allow(dead_code)]
    pub fn with_event(mut self, event: String) -> Self {
        self.event = Some(event);
        self
    }

    #[allow(dead_code)]
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Format context for structured logging
    #[allow(dead_code)]
    pub fn format(&self) -> String {
        let mut parts = vec![format!("correlation_id={}", self.correlation_id.as_str())];

        if let Some(sid) = &self.session_id {
            parts.push(format!("session_id={}", sid));
        }

        if let Some(uid) = &self.user_id {
            parts.push(format!("user_id={}", uid));
        }

        if let Some(evt) = &self.event {
            parts.push(format!("event={}", evt));
        }

        for (key, value) in &self.metadata {
            parts.push(format!("{}={}", key, value));
        }

        parts.join(" ")
    }
}

impl Default for LogContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Structured logger for Socket.IO
pub struct StructuredLogger {
    contexts: Arc<RwLock<HashMap<String, LogContext>>>,
}

impl StructuredLogger {
    pub fn new() -> Self {
        Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a log context for a session
    #[allow(dead_code)]
    pub async fn create_context(&self, session_id: &str) -> LogContext {
        let context = LogContext::new().with_session(session_id.to_string());

        let mut contexts = self.contexts.write().await;
        contexts.insert(session_id.to_string(), context.clone());

        context
    }

    /// Get log context for a session
    #[allow(dead_code)]
    pub async fn get_context(&self, session_id: &str) -> Option<LogContext> {
        let contexts = self.contexts.read().await;
        contexts.get(session_id).cloned()
    }

    /// Update log context
    #[allow(dead_code)]
    pub async fn update_context<F>(&self, session_id: &str, updater: F)
    where
        F: FnOnce(LogContext) -> LogContext,
    {
        let mut contexts = self.contexts.write().await;

        if let Some(context) = contexts.get(session_id) {
            let updated = updater(context.clone());
            contexts.insert(session_id.to_string(), updated);
        }
    }

    /// Remove log context
    #[allow(dead_code)]
    pub async fn remove_context(&self, session_id: &str) {
        let mut contexts = self.contexts.write().await;
        contexts.remove(session_id);
    }

    /// Log with context
    #[allow(dead_code)]
    pub async fn log_info(&self, session_id: &str, message: &str) {
        if let Some(context) = self.get_context(session_id).await {
            tracing::info!("[{}] {}", context.format(), message);
        } else {
            tracing::info!("{}", message);
        }
    }

    #[allow(dead_code)]
    pub async fn log_warn(&self, session_id: &str, message: &str) {
        if let Some(context) = self.get_context(session_id).await {
            tracing::warn!("[{}] {}", context.format(), message);
        } else {
            tracing::warn!("{}", message);
        }
    }

    #[allow(dead_code)]
    pub async fn log_error(&self, session_id: &str, message: &str) {
        if let Some(context) = self.get_context(session_id).await {
            tracing::error!("[{}] {}", context.format(), message);
        } else {
            tracing::error!("{}", message);
        }
    }

    #[allow(dead_code)]
    pub async fn log_debug(&self, session_id: &str, message: &str) {
        if let Some(context) = self.get_context(session_id).await {
            tracing::debug!("[{}] {}", context.format(), message);
        } else {
            tracing::debug!("{}", message);
        }
    }
}

impl Default for StructuredLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro for structured logging with context
#[macro_export]
macro_rules! log_with_context {
    ($logger:expr, $session_id:expr, $level:ident, $($arg:tt)*) => {
        if let Some(context) = $logger.get_context($session_id).await {
            tracing::$level!("[{}] {}", context.format(), format!($($arg)*));
        } else {
            tracing::$level!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_id() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();

        assert_ne!(id1.as_str(), id2.as_str());
    }

    #[test]
    fn test_log_context() {
        let context = LogContext::new()
            .with_session("session-123".to_string())
            .with_user("user-456".to_string())
            .with_event("user-join".to_string());

        let formatted = context.format();

        assert!(formatted.contains("session_id=session-123"));
        assert!(formatted.contains("user_id=user-456"));
        assert!(formatted.contains("event=user-join"));
    }

    #[tokio::test]
    async fn test_structured_logger() {
        let logger = StructuredLogger::new();

        let context = logger.create_context("session-1").await;
        assert_eq!(context.session_id, Some("session-1".to_string()));

        let retrieved = logger.get_context("session-1").await;
        assert!(retrieved.is_some());

        logger.remove_context("session-1").await;
        let removed = logger.get_context("session-1").await;
        assert!(removed.is_none());
    }
}
