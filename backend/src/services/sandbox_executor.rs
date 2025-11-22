/// Integration client for Sandbox Executor service
/// This replaces the Jupyter code execution with secure sandbox execution
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxExecuteRequest {
    pub code: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxExecuteResponse {
    pub execution_id: String,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub result: Option<String>,
    pub execution_time_ms: u64,
    pub memory_used_mb: Option<f64>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SandboxExecutorClient {
    client: Client,
    base_url: String,
}

impl SandboxExecutorClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300)) // 5 minutes max
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url }
    }

    /// Get the current base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn execute_code(
        &self,
        code: String,
        language: String,
        timeout: Option<u64>,
        user_id: Option<String>,
        request_id: Option<String>,
    ) -> Result<SandboxExecuteResponse, String> {
        let url = format!("{}/api/v1/execute", self.base_url);

        let request = SandboxExecuteRequest {
            code,
            language,
            timeout,
            user_id,
            request_id,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to send request to sandbox executor: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!(
                "Sandbox executor returned error {}: {}",
                status, error_text
            ));
        }

        let result = response
            .json::<SandboxExecuteResponse>()
            .await
            .map_err(|e| format!("Failed to parse sandbox executor response: {}", e))?;

        Ok(result)
    }

    pub async fn health_check(&self) -> Result<serde_json::Value, String> {
        let url = format!("{}/api/v1/health", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to sandbox executor: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Sandbox executor health check failed: {}",
                response.status()
            ));
        }

        let health = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse health response: {}", e))?;

        Ok(health)
    }

    pub async fn get_config(&self) -> Result<serde_json::Value, String> {
        let url = format!("{}/api/v1/config", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get sandbox executor config: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Failed to get config: {}", response.status()));
        }

        let config = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse config response: {}", e))?;

        Ok(config)
    }

    pub async fn get_stats(&self) -> Result<serde_json::Value, String> {
        let url = format!("{}/api/v1/stats", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get sandbox executor stats: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Failed to get stats: {}", response.status()));
        }

        let stats = response
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Failed to parse stats response: {}", e))?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = SandboxExecutorClient::new("http://localhost:8090".to_string());
        assert_eq!(client.base_url, "http://localhost:8090");
    }
}
