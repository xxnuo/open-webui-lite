use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool definition structure from JSON content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub tools: Vec<ToolSpec>,
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub rate_limits: HashMap<String, RateLimitConfig>,
    #[serde(default)]
    pub cache_config: Option<CacheConfig>,
    #[serde(default)]
    pub tool_chains: Vec<ToolChain>,
}

/// Individual tool specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub tool_type: ToolType,
    #[serde(default)]
    pub parameters: HashMap<String, ParameterSpec>,
    pub handler: ToolHandler,
    #[serde(default)]
    pub error_handling: Option<ErrorHandlingStrategy>,
    #[serde(default)]
    pub cache_enabled: bool,
}

/// Tool types supported
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToolType {
    HttpApi,
    Expression,
    Context,
    Mcp,
    #[serde(rename = "function")]
    BuiltIn,
}

/// Parameter specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSpec {
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// Tool handler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolHandler {
    Http {
        method: HttpMethod,
        url: String,
        #[serde(default)]
        params: HashMap<String, String>,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        body: Option<serde_json::Value>,
        #[serde(default)]
        response: ResponseTransform,
    },
    Expression {
        engine: String,
        expression: String,
    },
    Context {
        template: String,
    },
    Mcp {
        server: String,
        tool: String,
    },
    #[serde(rename = "built_in")]
    BuiltIn {
        function: String,
    },
}

/// HTTP methods
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Response transformation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResponseTransform {
    #[serde(default)]
    pub transform: Option<String>,
    #[serde(default)]
    pub extract: Option<String>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub url: String,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub auth_token: Option<String>,
}

/// Environment configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentConfig {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

/// Tool execution request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    pub tool_id: String,
    pub tool_name: String,
    pub parameters: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub context: ExecutionContext,
}

/// Execution context with user and environment data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionContext {
    #[serde(default)]
    pub user: Option<UserContext>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
    #[serde(default)]
    pub session: HashMap<String, serde_json::Value>,
}

/// User context for tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub id: String,
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub role: Option<String>,
}

/// Tool execution response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ExecutionMetadata>,
}

/// Execution metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    pub execution_time_ms: u64,
    pub tool_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
}

/// Rate limit configuration per tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Number of requests allowed
    pub requests: u32,
    /// Time window in seconds
    pub window_seconds: u64,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Cache TTL in seconds
    pub ttl_seconds: u64,
    /// Maximum cache size in MB
    #[serde(default)]
    pub max_size_mb: Option<u64>,
}

/// Error handling strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorHandlingStrategy {
    /// Retry with exponential backoff
    Retry {
        max_attempts: u32,
        initial_delay_ms: u64,
        max_delay_ms: u64,
    },
    /// Fallback to another tool
    Fallback { fallback_tool: String },
    /// Return default value on error
    Default { value: serde_json::Value },
    /// Fail immediately
    Fail,
}

/// Tool chaining configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChain {
    pub name: String,
    pub description: String,
    pub steps: Vec<ToolChainStep>,
}

/// Individual step in a tool chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChainStep {
    pub tool_name: String,
    /// Map output from previous step to this step's parameters
    #[serde(default)]
    pub parameter_mapping: HashMap<String, String>,
    /// Condition to execute this step (expression)
    #[serde(default)]
    pub condition: Option<String>,
    /// Error handling for this step
    #[serde(default)]
    pub error_handling: Option<ErrorHandlingStrategy>,
}

/// Conditional execution rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalExecution {
    /// Condition expression (e.g., "{{result.status}} == 'success'")
    pub condition: String,
    /// Tool to execute if condition is true
    pub then_tool: String,
    /// Tool to execute if condition is false (optional)
    #[serde(default)]
    pub else_tool: Option<String>,
}

impl ToolDefinition {
    /// Parse tool definition from JSON content
    pub fn from_json(content: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(content)
    }

    /// Find a tool by name
    pub fn find_tool(&self, name: &str) -> Option<&ToolSpec> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Get rate limit for a specific tool
    pub fn get_rate_limit(&self, tool_name: &str) -> Option<&RateLimitConfig> {
        self.rate_limits.get(tool_name)
    }

    /// Find a tool chain by name
    pub fn find_chain(&self, name: &str) -> Option<&ToolChain> {
        self.tool_chains.iter().find(|c| c.name == name)
    }

    /// Convert to OpenAI function calling specs
    pub fn to_openai_specs(&self) -> Vec<serde_json::Value> {
        self.tools
            .iter()
            .map(|tool| {
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for (param_name, param_spec) in &tool.parameters {
                    let mut prop = serde_json::Map::new();
                    prop.insert("type".to_string(), serde_json::json!(param_spec.param_type));

                    if let Some(desc) = &param_spec.description {
                        prop.insert("description".to_string(), serde_json::json!(desc));
                    }

                    if let Some(enum_vals) = &param_spec.enum_values {
                        prop.insert("enum".to_string(), serde_json::json!(enum_vals));
                    }

                    properties.insert(param_name.clone(), serde_json::Value::Object(prop));

                    if param_spec.required {
                        required.push(param_name.clone());
                    }
                }

                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required
                    }
                })
            })
            .collect()
    }
}
