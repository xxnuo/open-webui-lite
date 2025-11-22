use crate::config::Config;
use crate::container::{ContainerManager, ContainerPool};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub container_manager: Arc<ContainerManager>,
    pub container_pool: Arc<ContainerPool>,
    pub stats: Arc<RwLock<ServiceStats>>,
    pub start_time: Instant,
}

impl AppState {
    pub fn new(
        config: Config,
        container_manager: ContainerManager,
        container_pool: ContainerPool,
    ) -> Self {
        Self {
            config,
            container_manager: Arc::new(container_manager),
            container_pool: Arc::new(container_pool),
            stats: Arc::new(RwLock::new(ServiceStats::default())),
            start_time: Instant::now(),
        }
    }

    pub async fn increment_executions(&self) {
        let mut stats = self.stats.write().await;
        stats.total_executions += 1;
        stats.active_executions += 1;
    }

    pub async fn decrement_active_executions(&self) {
        let mut stats = self.stats.write().await;
        if stats.active_executions > 0 {
            stats.active_executions -= 1;
        }
    }

    pub async fn get_stats(&self) -> ServiceStats {
        self.stats.read().await.clone()
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServiceStats {
    pub total_executions: u64,
    pub active_executions: usize,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub timeout_executions: u64,
}
