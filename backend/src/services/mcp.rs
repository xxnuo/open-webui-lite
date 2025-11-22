use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info};

use crate::error::{AppError, AppResult};

/// Model Context Protocol (MCP) client for tool server connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub url: String,
    pub auth_token: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolCall {
    pub tool: String,
    pub arguments: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolResponse {
    pub result: Value,
    pub error: Option<String>,
}

#[allow(dead_code)]
pub struct McpClient {
    servers: Vec<McpServerConfig>,
    client: Client,
}

#[allow(dead_code)]
impl McpClient {
    pub fn new(servers: Vec<McpServerConfig>) -> Self {
        Self {
            servers,
            client: Client::new(),
        }
    }

    /// Discover available tools from all connected servers
    pub async fn discover_tools(&self) -> AppResult<Vec<McpTool>> {
        let mut all_tools = Vec::new();

        for server in &self.servers {
            if !server.enabled {
                continue;
            }

            match self.get_server_tools(server).await {
                Ok(tools) => {
                    info!(
                        "Discovered {} tools from server: {}",
                        tools.len(),
                        server.name
                    );
                    all_tools.extend(tools);
                }
                Err(e) => {
                    error!("Failed to discover tools from {}: {}", server.name, e);
                }
            }
        }

        Ok(all_tools)
    }

    /// Get tools from a specific server
    async fn get_server_tools(&self, server: &McpServerConfig) -> AppResult<Vec<McpTool>> {
        let url = format!("{}/tools", server.url);

        let mut request = self.client.get(&url);

        if let Some(token) = &server.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(|e| {
            error!("Failed to fetch tools from {}: {}", server.name, e);
            AppError::InternalServerError(format!("Failed to connect to tool server: {}", e))
        })?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::InternalServerError(format!(
                "Tool server returned error: {}",
                error_text
            )));
        }

        let tools_data: Value = response.json().await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to parse tools response: {}", e))
        })?;

        // Parse tools and add server name
        let tools: Vec<McpTool> =
            if let Some(tools_array) = tools_data.get("tools").and_then(|t| t.as_array()) {
                tools_array
                    .iter()
                    .filter_map(|tool| {
                        Some(McpTool {
                            name: tool.get("name")?.as_str()?.to_string(),
                            description: tool.get("description")?.as_str()?.to_string(),
                            input_schema: tool.get("inputSchema")?.clone(),
                            server: server.name.clone(),
                        })
                    })
                    .collect()
            } else {
                vec![]
            };

        Ok(tools)
    }

    /// Execute a tool call on the appropriate server
    pub async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> AppResult<McpToolResponse> {
        // Find which server provides this tool
        let tools = self.discover_tools().await?;
        let tool = tools
            .iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| AppError::NotFound(format!("Tool not found: {}", tool_name)))?;

        // Find the server
        let server = self
            .servers
            .iter()
            .find(|s| s.name == tool.server)
            .ok_or_else(|| {
                AppError::InternalServerError("Server configuration not found".to_string())
            })?;

        // Execute tool call
        let url = format!("{}/tools/{}/execute", server.url, tool_name);

        let mut request = self.client.post(&url).json(&serde_json::json!({
            "arguments": arguments
        }));

        if let Some(token) = &server.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(|e| {
            error!("Failed to execute tool {}: {}", tool_name, e);
            AppError::InternalServerError(format!("Failed to execute tool: {}", e))
        })?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Ok(McpToolResponse {
                result: Value::Null,
                error: Some(format!("Tool execution failed: {}", error_text)),
            });
        }

        let result: Value = response.json().await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to parse tool response: {}", e))
        })?;

        info!("Successfully executed tool: {}", tool_name);

        Ok(McpToolResponse {
            result,
            error: None,
        })
    }

    /// Health check for all servers
    pub async fn health_check(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();

        for server in &self.servers {
            let url = format!("{}/health", server.url);

            let mut request = self.client.get(&url);

            if let Some(token) = &server.auth_token {
                request = request.bearer_auth(token);
            }

            let is_healthy = match request.send().await {
                Ok(response) => response.status().is_success(),
                Err(_) => false,
            };

            results.push((server.name.clone(), is_healthy));
        }

        results
    }

    /// Add a new server at runtime
    pub fn add_server(&mut self, server: McpServerConfig) {
        self.servers.push(server);
    }

    /// Remove a server
    pub fn remove_server(&mut self, name: &str) -> bool {
        let initial_len = self.servers.len();
        self.servers.retain(|s| s.name != name);
        self.servers.len() < initial_len
    }

    /// Enable/disable a server
    pub fn set_server_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(server) = self.servers.iter_mut().find(|s| s.name == name) {
            server.enabled = enabled;
            true
        } else {
            false
        }
    }
}

/// Tool server manager with connection pooling
#[allow(dead_code)]
pub struct ToolServerManager {
    mcp_client: McpClient,
}

#[allow(dead_code)]
impl ToolServerManager {
    pub fn new(servers: Vec<McpServerConfig>) -> Self {
        Self {
            mcp_client: McpClient::new(servers),
        }
    }

    pub async fn get_available_tools(&self) -> AppResult<Vec<McpTool>> {
        self.mcp_client.discover_tools().await
    }

    pub async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> AppResult<McpToolResponse> {
        self.mcp_client.execute_tool(tool_name, arguments).await
    }

    pub async fn health_check(&self) -> Vec<(String, bool)> {
        self.mcp_client.health_check().await
    }

    pub fn add_server(&mut self, server: McpServerConfig) {
        self.mcp_client.add_server(server);
    }

    pub fn remove_server(&mut self, name: &str) -> bool {
        self.mcp_client.remove_server(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_client_creation() {
        let servers = vec![McpServerConfig {
            name: "test-server".to_string(),
            url: "http://localhost:8000".to_string(),
            auth_token: None,
            enabled: true,
        }];

        let client = McpClient::new(servers);
        assert_eq!(client.servers.len(), 1);
    }

    #[test]
    fn test_add_remove_server() {
        let mut client = McpClient::new(vec![]);

        let server = McpServerConfig {
            name: "test-server".to_string(),
            url: "http://localhost:8000".to_string(),
            auth_token: None,
            enabled: true,
        };

        client.add_server(server);
        assert_eq!(client.servers.len(), 1);

        assert!(client.remove_server("test-server"));
        assert_eq!(client.servers.len(), 0);
    }
}
