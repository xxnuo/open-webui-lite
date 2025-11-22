use crate::error::SandboxResult;
use crate::models::{ExecutionContext, ExecutionResult};
/// Container runtime abstraction for future extensibility
/// This allows us to potentially support other container runtimes besides Docker
use async_trait::async_trait;

#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    async fn create_container(&self, ctx: &ExecutionContext) -> SandboxResult<String>;
    async fn start_container(&self, container_id: &str) -> SandboxResult<()>;
    async fn execute(
        &self,
        container_id: &str,
        ctx: &ExecutionContext,
    ) -> SandboxResult<ExecutionResult>;
    async fn stop_container(&self, container_id: &str) -> SandboxResult<()>;
    async fn remove_container(&self, container_id: &str) -> SandboxResult<()>;
    async fn health_check(&self) -> SandboxResult<String>;
}

// Future implementations could include:
// - Podman runtime
// - Containerd runtime
// - Firecracker microVMs
// - gVisor runtime
