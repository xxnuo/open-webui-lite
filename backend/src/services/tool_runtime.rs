use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::tool_runtime::*;
use crate::services::tool::ToolService;
use crate::utils::template::TemplateEngine;
use evalexpr::{
    eval_with_context, ContextWithMutableVariables, DefaultNumericTypes, HashMapContext,
    Value as EvalValue,
};
use governor::{
    clock::DefaultClock, state::direct::NotKeyed, state::InMemoryState, Quota, RateLimiter,
};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Cache entry with expiration
#[derive(Debug, Clone)]
struct CacheEntry {
    value: Value,
    expires_at: Instant,
}

/// Tool runtime service for executing JSON-defined tools
pub struct ToolRuntimeService {
    http_client: Client,
    template_engine: TemplateEngine,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    rate_limiters:
        Arc<RwLock<HashMap<String, Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>>,
}

impl ToolRuntimeService {
    pub fn new() -> Self {
        ToolRuntimeService {
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            template_engine: TemplateEngine::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check and enforce rate limit for a tool
    async fn check_rate_limit(
        &self,
        tool_id: &str,
        rate_config: &RateLimitConfig,
    ) -> AppResult<()> {
        let mut limiters = self.rate_limiters.write().await;

        let limiter = limiters.entry(tool_id.to_string()).or_insert_with(|| {
            let quota = Quota::per_second(NonZeroU32::new(rate_config.requests).unwrap())
                .allow_burst(NonZeroU32::new(rate_config.requests).unwrap());
            Arc::new(RateLimiter::direct(quota))
        });

        // Check rate limit
        if limiter.check().is_err() {
            return Err(AppError::TooManyRequests(format!(
                "Rate limit exceeded for tool: {}. Maximum {} requests per {} seconds",
                tool_id, rate_config.requests, rate_config.window_seconds
            )));
        }

        Ok(())
    }

    /// Get cached result if available and not expired
    async fn get_cached(&self, cache_key: &str) -> Option<Value> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(cache_key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.value.clone());
            }
        }
        None
    }

    /// Store result in cache
    async fn set_cache(&self, cache_key: String, value: Value, ttl_seconds: u64) {
        let mut cache = self.cache.write().await;
        cache.insert(
            cache_key,
            CacheEntry {
                value,
                expires_at: Instant::now() + Duration::from_secs(ttl_seconds),
            },
        );
    }

    /// Clean expired cache entries
    async fn clean_cache(&self) {
        let mut cache = self.cache.write().await;
        let now = Instant::now();
        cache.retain(|_, entry| entry.expires_at > now);
    }

    /// Execute a tool by ID and name
    pub async fn execute_tool(
        &self,
        db: &Database,
        request: ToolExecutionRequest,
    ) -> AppResult<ToolExecutionResponse> {
        let start_time = Instant::now();

        // Get tool from database
        let tool_service = ToolService::new(db);
        let tool = tool_service
            .get_tool_by_id(&request.tool_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Tool not found: {}", request.tool_id)))?;

        // Parse tool definition from content
        let tool_def = ToolDefinition::from_json(&tool.content)
            .map_err(|e| AppError::BadRequest(format!("Invalid tool definition: {}", e)))?;

        // Find the specific tool
        let tool_spec = tool_def
            .find_tool(&request.tool_name)
            .ok_or_else(|| AppError::NotFound(format!("Tool not found: {}", request.tool_name)))?;

        // Check rate limit if configured
        if let Some(rate_config) = tool_def.get_rate_limit(&request.tool_name) {
            self.check_rate_limit(&request.tool_id, rate_config).await?;
        }

        // Check cache if enabled
        let cache_key = format!(
            "{}:{}:{:?}",
            request.tool_id, request.tool_name, request.parameters
        );
        if tool_spec.cache_enabled {
            if let Some(cached_result) = self.get_cached(&cache_key).await {
                return Ok(ToolExecutionResponse {
                    success: true,
                    result: Some(cached_result),
                    error: None,
                    metadata: Some(ExecutionMetadata {
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                        tool_type: format!("{:?}", tool_spec.tool_type),
                        http_status: None,
                    }),
                });
            }
        }

        // Validate required parameters
        self.validate_parameters(tool_spec, &request.parameters)?;

        // Validate required environment variables
        self.validate_environment(&tool_def.environment, &request.context.environment)?;

        // Execute with error handling strategy
        let result = self
            .execute_with_error_handling(tool_spec, &tool_def, &request)
            .await;

        let execution_time = start_time.elapsed().as_millis() as u64;

        match result {
            Ok((value, metadata)) => {
                // Cache result if enabled
                if tool_spec.cache_enabled {
                    if let Some(cache_config) = &tool_def.cache_config {
                        self.set_cache(cache_key, value.clone(), cache_config.ttl_seconds)
                            .await;
                    }
                }

                Ok(ToolExecutionResponse {
                    success: true,
                    result: Some(value),
                    error: None,
                    metadata: Some(ExecutionMetadata {
                        execution_time_ms: execution_time,
                        tool_type: format!("{:?}", tool_spec.tool_type),
                        http_status: metadata.and_then(|m| {
                            m.get("http_status")
                                .and_then(|v| v.as_u64().map(|n| n as u16))
                        }),
                    }),
                })
            }
            Err(e) => Ok(ToolExecutionResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                metadata: Some(ExecutionMetadata {
                    execution_time_ms: execution_time,
                    tool_type: format!("{:?}", tool_spec.tool_type),
                    http_status: None,
                }),
            }),
        }
    }

    /// Execute tool with error handling strategy
    async fn execute_with_error_handling(
        &self,
        tool_spec: &ToolSpec,
        tool_def: &ToolDefinition,
        request: &ToolExecutionRequest,
    ) -> Result<(Value, Option<HashMap<String, Value>>), AppError> {
        let error_handling = tool_spec
            .error_handling
            .clone()
            .unwrap_or(ErrorHandlingStrategy::Fail);

        match error_handling {
            ErrorHandlingStrategy::Retry {
                max_attempts,
                initial_delay_ms,
                max_delay_ms,
            } => {
                let mut attempt = 0;
                let mut delay = initial_delay_ms;

                loop {
                    attempt += 1;
                    let result = self
                        .execute_tool_handler(&tool_spec.handler, tool_def, request)
                        .await;

                    match result {
                        Ok(value) => return Ok(value),
                        Err(e) if attempt >= max_attempts => return Err(e),
                        Err(_) => {
                            sleep(Duration::from_millis(delay)).await;
                            delay = (delay * 2).min(max_delay_ms);
                        }
                    }
                }
            }
            ErrorHandlingStrategy::Fallback { fallback_tool } => {
                let result = self
                    .execute_tool_handler(&tool_spec.handler, tool_def, request)
                    .await;

                match result {
                    Ok(value) => Ok(value),
                    Err(_) => {
                        // Try fallback tool
                        if let Some(fallback_spec) = tool_def.find_tool(&fallback_tool) {
                            self.execute_tool_handler(&fallback_spec.handler, tool_def, request)
                                .await
                        } else {
                            Err(AppError::NotFound(format!(
                                "Fallback tool not found: {}",
                                fallback_tool
                            )))
                        }
                    }
                }
            }
            ErrorHandlingStrategy::Default { value } => {
                let result = self
                    .execute_tool_handler(&tool_spec.handler, tool_def, request)
                    .await;

                match result {
                    Ok(v) => Ok(v),
                    Err(_) => Ok((value, None)),
                }
            }
            ErrorHandlingStrategy::Fail => {
                self.execute_tool_handler(&tool_spec.handler, tool_def, request)
                    .await
            }
        }
    }

    /// Execute the actual tool handler
    async fn execute_tool_handler(
        &self,
        handler: &ToolHandler,
        tool_def: &ToolDefinition,
        request: &ToolExecutionRequest,
    ) -> Result<(Value, Option<HashMap<String, Value>>), AppError> {
        match handler {
            ToolHandler::Http {
                method,
                url,
                params,
                headers,
                body,
                response,
            } => {
                self.execute_http_tool(
                    method,
                    url,
                    params,
                    headers,
                    body,
                    response,
                    &request.parameters,
                    &request.context,
                )
                .await
            }
            ToolHandler::Expression { engine, expression } => {
                self.execute_expression_tool(engine, expression, &request.parameters)
                    .await
            }
            ToolHandler::Context { template } => {
                self.execute_context_tool(template, &request.parameters, &request.context)
                    .await
            }
            ToolHandler::Mcp { server, tool } => {
                self.execute_mcp_tool(
                    &tool_def.mcp_servers,
                    server,
                    tool,
                    &request.parameters,
                    &request.context,
                )
                .await
            }
            ToolHandler::BuiltIn { function } => {
                self.execute_builtin_tool(function, &request.parameters)
                    .await
            }
        }
    }

    /// Validate tool parameters
    fn validate_parameters(
        &self,
        tool_spec: &ToolSpec,
        parameters: &HashMap<String, Value>,
    ) -> AppResult<()> {
        for (param_name, param_spec) in &tool_spec.parameters {
            if param_spec.required && !parameters.contains_key(param_name) {
                return Err(AppError::BadRequest(format!(
                    "Missing required parameter: {}",
                    param_name
                )));
            }
        }
        Ok(())
    }

    /// Validate environment variables
    fn validate_environment(
        &self,
        env_config: &EnvironmentConfig,
        environment: &HashMap<String, String>,
    ) -> AppResult<()> {
        for env_var in &env_config.required {
            if !environment.contains_key(env_var) {
                return Err(AppError::BadRequest(format!(
                    "Missing required environment variable: {}",
                    env_var
                )));
            }
        }
        Ok(())
    }

    /// Execute HTTP API tool
    async fn execute_http_tool(
        &self,
        method: &HttpMethod,
        url: &str,
        params: &HashMap<String, String>,
        headers: &HashMap<String, String>,
        body: &Option<Value>,
        response: &ResponseTransform,
        parameters: &HashMap<String, Value>,
        context: &ExecutionContext,
    ) -> Result<(Value, Option<HashMap<String, Value>>), AppError> {
        // Convert user context to JSON once
        let user_json = context
            .user
            .as_ref()
            .map(|u| serde_json::to_value(u).unwrap());

        // Render URL with template variables
        let rendered_url = self.template_engine.render(
            url,
            parameters,
            &context.environment,
            user_json.as_ref(),
            None,
            None,
        );

        // Render query parameters
        let mut rendered_params = HashMap::new();
        for (key, value) in params {
            let rendered_value = self.template_engine.render(
                value,
                parameters,
                &context.environment,
                user_json.as_ref(),
                None,
                None,
            );
            rendered_params.insert(key.clone(), rendered_value);
        }

        // Render headers
        let mut rendered_headers = HashMap::new();
        for (key, value) in headers {
            let rendered_value = self.template_engine.render(
                value,
                parameters,
                &context.environment,
                user_json.as_ref(),
                None,
                None,
            );
            rendered_headers.insert(key.clone(), rendered_value);
        }

        // Build HTTP request
        let mut request_builder = match method {
            HttpMethod::Get => self.http_client.get(&rendered_url),
            HttpMethod::Post => self.http_client.post(&rendered_url),
            HttpMethod::Put => self.http_client.put(&rendered_url),
            HttpMethod::Patch => self.http_client.patch(&rendered_url),
            HttpMethod::Delete => self.http_client.delete(&rendered_url),
        };

        // Add query parameters
        if !rendered_params.is_empty() {
            request_builder = request_builder.query(&rendered_params);
        }

        // Add headers
        for (key, value) in rendered_headers {
            request_builder = request_builder.header(&key, &value);
        }

        // Add body for POST/PUT/PATCH
        if let Some(body_template) = body {
            if matches!(
                method,
                HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
            ) {
                request_builder = request_builder.json(body_template);
            }
        }

        // Execute request
        let response_result = request_builder
            .send()
            .await
            .map_err(|e| AppError::InternalServerError(format!("HTTP request failed: {}", e)))?;

        let status_code = response_result.status().as_u16();
        let response_headers_map: HashMap<String, String> = response_result
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Parse response body
        let response_body: Value = response_result.json().await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to parse response: {}", e))
        })?;

        // Transform response if specified
        let final_result = if let Some(transform_template) = &response.transform {
            let transformed = self.template_engine.render(
                transform_template,
                parameters,
                &context.environment,
                user_json.as_ref(),
                Some(&response_body),
                Some(&response_headers_map),
            );
            Value::String(transformed)
        } else if let Some(extract_path) = &response.extract {
            // Extract specific field from response
            self.extract_json_path(&response_body, extract_path)
                .unwrap_or(response_body)
        } else {
            response_body
        };

        let mut metadata = HashMap::new();
        metadata.insert("http_status".to_string(), Value::Number(status_code.into()));

        Ok((final_result, Some(metadata)))
    }

    /// Execute expression/calculator tool
    async fn execute_expression_tool(
        &self,
        _engine: &str,
        expression: &str,
        parameters: &HashMap<String, Value>,
    ) -> Result<(Value, Option<HashMap<String, Value>>), AppError> {
        // Create evaluation context
        let mut context: HashMapContext<DefaultNumericTypes> = HashMapContext::new();

        // Add parameters to context
        for (key, value) in parameters {
            let eval_value = match value {
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        EvalValue::Int(i)
                    } else if let Some(f) = n.as_f64() {
                        EvalValue::Float(f)
                    } else {
                        return Err(AppError::BadRequest(format!("Invalid number: {}", n)));
                    }
                }
                Value::String(s) => EvalValue::String(s.clone()),
                Value::Bool(b) => EvalValue::Boolean(*b),
                _ => {
                    return Err(AppError::BadRequest(format!(
                        "Unsupported parameter type for expression evaluation: {:?}",
                        value
                    )));
                }
            };

            ContextWithMutableVariables::set_value(&mut context, key.clone(), eval_value).map_err(
                |e| AppError::BadRequest(format!("Failed to set context variable {}: {}", key, e)),
            )?;
        }

        // Replace template placeholders in expression
        let mut expr = expression.to_string();
        for (key, _) in parameters {
            let placeholder = format!("{{{{{}}}}}", key);
            expr = expr.replace(&placeholder, key);
        }

        // Evaluate expression
        let result = eval_with_context(&expr, &context)
            .map_err(|e| AppError::BadRequest(format!("Expression evaluation failed: {}", e)))?;

        // Convert result back to JSON Value
        let json_result = match result {
            EvalValue::String(s) => Value::String(s),
            EvalValue::Float(f) => Value::Number(serde_json::Number::from_f64(f).unwrap()),
            EvalValue::Int(i) => Value::Number(i.into()),
            EvalValue::Boolean(b) => Value::Bool(b),
            EvalValue::Tuple(t) => {
                let values: Vec<Value> = t
                    .into_iter()
                    .map(|v| match v {
                        EvalValue::String(s) => Value::String(s),
                        EvalValue::Float(f) => {
                            Value::Number(serde_json::Number::from_f64(f).unwrap())
                        }
                        EvalValue::Int(i) => Value::Number(i.into()),
                        EvalValue::Boolean(b) => Value::Bool(b),
                        _ => Value::Null,
                    })
                    .collect();
                Value::Array(values)
            }
            EvalValue::Empty => Value::Null,
        };

        Ok((json_result, None))
    }

    /// Execute context template tool
    async fn execute_context_tool(
        &self,
        template: &str,
        parameters: &HashMap<String, Value>,
        context: &ExecutionContext,
    ) -> Result<(Value, Option<HashMap<String, Value>>), AppError> {
        // Convert user context to JSON
        let user_json = context
            .user
            .as_ref()
            .map(|u| serde_json::to_value(u).unwrap());

        let rendered = self.template_engine.render(
            template,
            parameters,
            &context.environment,
            user_json.as_ref(),
            None,
            None,
        );

        Ok((Value::String(rendered), None))
    }

    /// Execute MCP tool
    async fn execute_mcp_tool(
        &self,
        mcp_servers: &HashMap<String, McpServerConfig>,
        server_name: &str,
        tool_name: &str,
        parameters: &HashMap<String, Value>,
        _context: &ExecutionContext,
    ) -> Result<(Value, Option<HashMap<String, Value>>), AppError> {
        let server_config = mcp_servers
            .get(server_name)
            .ok_or_else(|| AppError::NotFound(format!("MCP server not found: {}", server_name)))?;

        // Build MCP request
        let url = format!("{}/tools/{}/execute", server_config.url, tool_name);
        let mut request_builder = self.http_client.post(&url).json(&serde_json::json!({
            "arguments": parameters
        }));

        // Add authentication
        if let Some(token) = &server_config.auth_token {
            if server_config.auth_type.as_deref() == Some("bearer") {
                request_builder = request_builder.bearer_auth(token);
            }
        }

        // Execute MCP request
        let response = request_builder
            .send()
            .await
            .map_err(|e| AppError::InternalServerError(format!("MCP request failed: {}", e)))?;

        let result: Value = response.json().await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to parse MCP response: {}", e))
        })?;

        Ok((result, None))
    }

    /// Execute built-in function tool
    async fn execute_builtin_tool(
        &self,
        function: &str,
        _parameters: &HashMap<String, Value>,
    ) -> Result<(Value, Option<HashMap<String, Value>>), AppError> {
        match function {
            "datetime.now" => {
                let now = chrono::Utc::now();
                Ok((Value::String(now.to_rfc3339()), None))
            }
            "datetime.timestamp" => {
                let now = chrono::Utc::now();
                Ok((Value::Number(now.timestamp().into()), None))
            }
            _ => Err(AppError::BadRequest(format!(
                "Unknown built-in function: {}",
                function
            ))),
        }
    }

    /// Extract value from JSON using a path
    fn extract_json_path(&self, value: &Value, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current.clone())
    }

    /// Execute a tool chain
    pub async fn execute_tool_chain(
        &self,
        db: &Database,
        tool_id: &str,
        chain_name: &str,
        initial_parameters: HashMap<String, Value>,
        context: ExecutionContext,
    ) -> AppResult<ToolExecutionResponse> {
        let start_time = Instant::now();

        // Get tool from database
        let tool_service = ToolService::new(db);
        let tool = tool_service
            .get_tool_by_id(tool_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Tool not found: {}", tool_id)))?;

        // Parse tool definition from content
        let tool_def = ToolDefinition::from_json(&tool.content)
            .map_err(|e| AppError::BadRequest(format!("Invalid tool definition: {}", e)))?;

        // Find the tool chain
        let chain = tool_def
            .find_chain(chain_name)
            .ok_or_else(|| AppError::NotFound(format!("Tool chain not found: {}", chain_name)))?;

        // Execute chain steps
        let mut current_parameters = initial_parameters;
        let mut chain_results = Vec::new();

        for (step_idx, step) in chain.steps.iter().enumerate() {
            // Check condition if specified
            if let Some(condition_expr) = &step.condition {
                let should_execute =
                    self.evaluate_condition(condition_expr, &current_parameters)?;
                if !should_execute {
                    continue;
                }
            }

            // Map parameters from previous results
            let mut step_parameters = current_parameters.clone();
            for (param_name, mapping_expr) in &step.parameter_mapping {
                if let Some(last_result) = chain_results.last() {
                    if let Some(mapped_value) = self.extract_json_path(last_result, mapping_expr) {
                        step_parameters.insert(param_name.clone(), mapped_value);
                    }
                }
            }

            // Execute the step with error handling
            let request = ToolExecutionRequest {
                tool_id: tool_id.to_string(),
                tool_name: step.tool_name.clone(),
                parameters: step_parameters.clone(),
                context: context.clone(),
            };

            let result = if let Some(error_strategy) = &step.error_handling {
                // Execute with custom error handling for this step
                let tool_spec = tool_def.find_tool(&step.tool_name).ok_or_else(|| {
                    AppError::NotFound(format!("Tool not found: {}", step.tool_name))
                })?;

                // Temporarily override error handling
                let mut modified_spec = tool_spec.clone();
                modified_spec.error_handling = Some(error_strategy.clone());

                self.execute_with_error_handling(&modified_spec, &tool_def, &request)
                    .await
            } else {
                // Use default error handling
                let tool_spec = tool_def.find_tool(&step.tool_name).ok_or_else(|| {
                    AppError::NotFound(format!("Tool not found: {}", step.tool_name))
                })?;

                self.execute_with_error_handling(tool_spec, &tool_def, &request)
                    .await
            };

            match result {
                Ok((value, _)) => {
                    chain_results.push(value.clone());
                    // Update parameters for next step
                    if let Value::Object(map) = value {
                        for (key, val) in map {
                            current_parameters.insert(key, val);
                        }
                    }
                }
                Err(e) => {
                    return Ok(ToolExecutionResponse {
                        success: false,
                        result: None,
                        error: Some(format!(
                            "Chain step {} ({}) failed: {}",
                            step_idx, step.tool_name, e
                        )),
                        metadata: Some(ExecutionMetadata {
                            execution_time_ms: start_time.elapsed().as_millis() as u64,
                            tool_type: "chain".to_string(),
                            http_status: None,
                        }),
                    });
                }
            }
        }

        // Return the result of the last step
        let final_result = chain_results.last().cloned().unwrap_or(Value::Null);

        Ok(ToolExecutionResponse {
            success: true,
            result: Some(final_result),
            error: None,
            metadata: Some(ExecutionMetadata {
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                tool_type: "chain".to_string(),
                http_status: None,
            }),
        })
    }

    /// Evaluate a condition expression
    fn evaluate_condition(
        &self,
        condition: &str,
        parameters: &HashMap<String, Value>,
    ) -> AppResult<bool> {
        // Create evaluation context
        let mut context: HashMapContext<DefaultNumericTypes> = HashMapContext::new();

        // Add parameters to context
        for (key, value) in parameters {
            let eval_value = match value {
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        EvalValue::Int(i)
                    } else if let Some(f) = n.as_f64() {
                        EvalValue::Float(f)
                    } else {
                        return Err(AppError::BadRequest(format!("Invalid number: {}", n)));
                    }
                }
                Value::String(s) => EvalValue::String(s.clone()),
                Value::Bool(b) => EvalValue::Boolean(*b),
                _ => continue,
            };

            ContextWithMutableVariables::set_value(&mut context, key.clone(), eval_value).map_err(
                |e| AppError::BadRequest(format!("Failed to set context variable {}: {}", key, e)),
            )?;
        }

        // Replace template placeholders in condition
        let mut expr = condition.to_string();
        for (key, _) in parameters {
            let placeholder = format!("{{{{{}}}}}", key);
            expr = expr.replace(&placeholder, key);
        }

        // Evaluate condition
        let result = eval_with_context(&expr, &context)
            .map_err(|e| AppError::BadRequest(format!("Condition evaluation failed: {}", e)))?;

        match result {
            EvalValue::Boolean(b) => Ok(b),
            _ => Err(AppError::BadRequest(
                "Condition must evaluate to a boolean".to_string(),
            )),
        }
    }
}

impl Default for ToolRuntimeService {
    fn default() -> Self {
        Self::new()
    }
}
