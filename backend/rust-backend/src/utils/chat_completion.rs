// Chat completion streaming utilities for actix-web
// This module handles SSE (Server-Sent Events) streaming for chat completions
// and provides utilities for both Socket.IO and HTTP streaming

use actix_web::{web, HttpResponse};
use bytes::Bytes;
use futures::stream::StreamExt;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::{
    error::AppError,
    middleware::code_interpreter::{
        execute_code_block, format_execution_result, get_code_interpreter_timeout,
        get_sandbox_client, is_code_interpreter_enabled, CodeBlockDetector,
    },
    AppState,
};

/// Default delta chunk size for batching streamed responses
/// Matches Python's CHAT_RESPONSE_STREAM_DELTA_CHUNK_SIZE
pub const DEFAULT_DELTA_CHUNK_SIZE: usize = 1;

/// Default title generation prompt template
pub const DEFAULT_TITLE_GENERATION_PROMPT_TEMPLATE: &str = r#"### Task:
Generate a concise, 3-5 word title with an emoji summarizing the chat history.
### Guidelines:
- The title should clearly represent the main theme or subject of the conversation.
- Use emojis that enhance understanding of the topic, but avoid quotation marks or special formatting.
- Write the title in the chat's primary language; default to English if multilingual.
- Prioritize accuracy over excessive creativity; keep it clear and simple.
- Your entire response must consist solely of the JSON object, without any introductory or concluding text.
- The output must be a single, raw JSON object, without any markdown code fences or other encapsulating text.
- Ensure no conversational text, affirmations, or explanations precede or follow the raw JSON output, as this will cause direct parsing failure.
### Output:
JSON format: { "title": "your concise title here" }
### Examples:
- { "title": "📉 Stock Market Trends" },
- { "title": "🍪 Perfect Chocolate Chip Recipe" },
- { "title": "Evolution of Music Streaming" },
- { "title": "Remote Work Productivity Tips" },
- { "title": "Artificial Intelligence in Healthcare" },
- { "title": "🎮 Video Game Development Insights" }
### Chat History:
<chat_history>
{{MESSAGES:END:2}}
</chat_history>"#;

/// Context for streaming chat completions
pub struct StreamingContext {
    pub state: web::Data<AppState>,
    pub user_id: String,
    pub model_id: String,
    pub messages: Vec<Value>,
    pub chat_id: Option<String>,
    pub message_id: Option<String>,
    pub session_id: Option<String>,
    pub should_generate_title: bool,
    pub model_item: Value,
    pub endpoint_url: String,
    pub endpoint_key: String,
    pub tool_ids: Vec<String>,
    pub tool_specs: Vec<Value>,
    pub delta_chunk_size: Option<usize>,
}

/// Create an HTTP SSE streaming response
/// This is used when Socket.IO metadata is not present (API calls, integrations, etc.)
pub fn create_sse_stream(response: reqwest::Response) -> Result<HttpResponse, AppError> {
    tracing::debug!("Creating HTTP SSE streaming response");

    let stream = response.bytes_stream().map(move |result| match result {
        Ok(bytes) => {
            // Forward immediately without ANY processing
            Ok::<Bytes, actix_web::Error>(bytes)
        }
        Err(e) => {
            tracing::error!("SSE stream error: {}", e);
            Err(actix_web::error::ErrorInternalServerError(e))
        }
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream; charset=utf-8")
        .append_header(("Cache-Control", "no-cache, no-transform"))
        .append_header(("X-Accel-Buffering", "no"))
        .append_header(("Connection", "keep-alive"))
        .insert_header(("Transfer-Encoding", "chunked"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        // CRITICAL: Disable compression for streaming on Windows
        // Compress middleware buffers responses, causing delays in real-time streaming
        .insert_header(("Content-Encoding", "identity"))
        .streaming(stream))
}

/// Process streaming response and emit events via Socket.IO
/// This mimics Python's middleware.py process_chat_response streaming logic
pub async fn process_streaming_via_socketio(
    response: reqwest::Response,
    context: StreamingContext,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get socket state
    let socket_state = match &context.state.socket_state {
        Some(state) => state.clone(),
        _ => {
            tracing::warn!("Socket state not available, cannot emit streaming events");
            return Ok(());
        }
    };

    // Create event emitter
    let event_emitter = crate::socket::get_event_emitter(
        socket_state,
        context.user_id.clone(),
        context.chat_id.clone(),
        context.message_id.clone(),
        context.session_id.clone(),
    );

    // Stream the response with batching like Python backend
    let mut stream = response.bytes_stream();
    let mut content = String::new();

    // Delta batching to prevent flooding frontend
    // Use configurable chunk size or default to 1
    let delta_chunk_size = context.delta_chunk_size.unwrap_or(DEFAULT_DELTA_CHUNK_SIZE);
    let mut delta_count = 0;
    let mut last_delta_data: Option<Value> = None;

    tracing::debug!("💬 Delta chunk size: {}", delta_chunk_size);

    // Tool call tracking
    let mut collected_tool_calls: HashMap<usize, Value> = HashMap::new();
    let mut has_tool_calls = false;

    // Code interpreter tracking
    let code_interpreter_enabled = is_code_interpreter_enabled(&context.state);
    let sandbox_client = if code_interpreter_enabled {
        get_sandbox_client(&context.state)
    } else {
        None
    };
    let code_interpreter_timeout = get_code_interpreter_timeout(&context.state);
    let mut code_block_detector = if code_interpreter_enabled && sandbox_client.is_some() {
        Some(CodeBlockDetector::new())
    } else {
        None
    };

    tracing::info!(
        "🔴 Socket.IO STREAMING STARTED for user {} (code_interpreter: {})",
        context.user_id,
        code_interpreter_enabled
    );

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Convert bytes to text
                if let Ok(text) = std::str::from_utf8(&chunk) {
                    tracing::debug!("⚡ Received chunk: {} bytes", text.len());

                    // Parse SSE format lines
                    for line in text.lines() {
                        let line = line.trim();

                        // Skip empty lines
                        if line.is_empty() {
                            continue;
                        }

                        // Handle SSE data lines
                        if line.starts_with("data: ") {
                            let data_str = &line[6..]; // Remove "data: " prefix

                            // Skip [DONE] marker
                            if data_str == "[DONE]" {
                                tracing::info!("✅ Streaming completed");

                                // Flush any pending delta
                                if let Some(pending_data) = last_delta_data.take() {
                                    let completion_event = json!({
                                        "type": "chat:completion",
                                        "data": pending_data
                                    });
                                    event_emitter(completion_event).await;
                                }
                                break;
                            }

                            // Parse JSON data
                            if let Ok(mut data) = serde_json::from_str::<Value>(data_str) {
                                // Extract delta content
                                if let Some(choices) =
                                    data.get("choices").and_then(|c| c.as_array())
                                {
                                    if let Some(first_choice) = choices.first() {
                                        if let Some(delta) = first_choice.get("delta") {
                                            // Check for content delta
                                            if let Some(delta_content) =
                                                delta.get("content").and_then(|c| c.as_str())
                                            {
                                                content.push_str(delta_content);

                                                // Check for code blocks if code interpreter is enabled
                                                if let Some(ref mut detector) = code_block_detector
                                                {
                                                    if let Some(ref client) = sandbox_client {
                                                        let (code_blocks, _) =
                                                            detector.process_chunk(delta_content);

                                                        for code_block in code_blocks {
                                                            tracing::info!(
                                                                "🔍 Detected complete code block: {} ({} bytes)",
                                                                code_block.language,
                                                                code_block.code.len()
                                                            );

                                                            // Execute the code block
                                                            match execute_code_block(
                                                                &code_block,
                                                                client,
                                                                &context.user_id,
                                                                code_interpreter_timeout,
                                                            )
                                                            .await
                                                            {
                                                                Ok(result) => {
                                                                    tracing::info!(
                                                                        "✅ Code execution completed: {} ({}ms)",
                                                                        result.status,
                                                                        result.execution_time_ms
                                                                    );

                                                                    // Format and emit execution result
                                                                    let formatted_result =
                                                                        format_execution_result(
                                                                            &result,
                                                                        );

                                                                    // Add the execution result to content
                                                                    content.push_str(
                                                                        &formatted_result,
                                                                    );

                                                                    // Emit the execution result as a completion event
                                                                    let result_event = json!({
                                                                        "type": "chat:completion",
                                                                        "data": {
                                                                            "choices": [{
                                                                                "index": 0,
                                                                                "delta": {
                                                                                    "content": formatted_result
                                                                                }
                                                                            }]
                                                                        }
                                                                    });
                                                                    event_emitter(result_event)
                                                                        .await;
                                                                }
                                                                Err(e) => {
                                                                    tracing::error!(
                                                                        "❌ Code execution failed: {}",
                                                                        e
                                                                    );

                                                                    // Emit error as part of the stream
                                                                    let error_msg = format!(
                                                                        "\n**Code Execution Error:**\n```\n{}\n```\n\n",
                                                                        e
                                                                    );
                                                                    content.push_str(&error_msg);

                                                                    let error_event = json!({
                                                                        "type": "chat:completion",
                                                                        "data": {
                                                                            "choices": [{
                                                                                "index": 0,
                                                                                "delta": {
                                                                                    "content": error_msg
                                                                                }
                                                                            }]
                                                                        }
                                                                    });
                                                                    event_emitter(error_event)
                                                                        .await;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                // Batch deltas like Python backend
                                                delta_count += 1;
                                                last_delta_data = Some(data.clone());

                                                // Only emit when batch size reached
                                                if delta_count >= delta_chunk_size {
                                                    let completion_event = json!({
                                                        "type": "chat:completion",
                                                        "data": data
                                                    });
                                                    event_emitter(completion_event).await;
                                                    delta_count = 0;
                                                    last_delta_data = None;
                                                }
                                            }

                                            // Check for tool_calls delta
                                            if let Some(tool_calls) = delta.get("tool_calls") {
                                                tracing::info!(
                                                    "🔧 Tool calls detected in stream: {:?}",
                                                    tool_calls
                                                );
                                                has_tool_calls = true;

                                                // Accumulate tool_calls by index
                                                if let Some(tool_calls_array) =
                                                    tool_calls.as_array()
                                                {
                                                    accumulate_tool_calls(
                                                        tool_calls_array,
                                                        &mut collected_tool_calls,
                                                    );
                                                }

                                                // Emit tool_calls immediately (don't batch)
                                                let completion_event = json!({
                                                    "type": "chat:completion",
                                                    "data": data
                                                });
                                                event_emitter(completion_event).await;

                                                // Clear any pending deltas
                                                last_delta_data = None;
                                                delta_count = 0;
                                            }
                                        }

                                        // Check for finish_reason
                                        if let Some(finish_reason) =
                                            first_choice.get("finish_reason")
                                        {
                                            if !finish_reason.is_null() {
                                                tracing::info!(
                                                    "✅ Stream finished with reason: {:?}",
                                                    finish_reason
                                                );

                                                // Flush any pending delta first
                                                if let Some(pending_data) = last_delta_data.take() {
                                                    let completion_event = json!({
                                                        "type": "chat:completion",
                                                        "data": pending_data
                                                    });
                                                    event_emitter(completion_event).await;
                                                    delta_count = 0;
                                                }

                                                // Mark as done and send final data with finish_reason
                                                data["done"] = json!(true);
                                                let completion_event = json!({
                                                    "type": "chat:completion",
                                                    "data": data
                                                });
                                                event_emitter(completion_event).await;

                                                // Save to database
                                                if let (Some(cid), Some(mid)) = (
                                                    context.chat_id.as_ref(),
                                                    context.message_id.as_ref(),
                                                ) {
                                                    let _ = upsert_chat_message(
                                                        &context.state.db,
                                                        cid,
                                                        mid,
                                                        json!({
                                                            "role": "assistant",
                                                            "content": content.clone(),
                                                            "done": true,
                                                            "model": context.model_id.clone(),
                                                        }),
                                                    )
                                                    .await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("❌ Stream error: {}", e);

                // Emit error event
                let event_data = json!({
                    "type": "chat:completion",
                    "data": {
                        "error": {
                            "content": format!("Stream error: {}", e)
                        }
                    }
                });
                event_emitter(event_data).await;

                return Err(e.into());
            }
        }
    }

    // Execute tools if tool_calls were detected
    if has_tool_calls && !collected_tool_calls.is_empty() {
        execute_tools_and_continue(
            collected_tool_calls,
            content,
            context,
            event_emitter,
            delta_chunk_size,
        )
        .await?;
    } else {
        // No tool calls - generate title if requested (normal completion path)
        if context.should_generate_title && context.chat_id.is_some() {
            tracing::info!("🏷️  No tools used, triggering title generation");
            spawn_title_generation(context).await;
        }
    }

    Ok(())
}

/// Accumulate tool calls from streaming chunks
fn accumulate_tool_calls(
    tool_calls_array: &[Value],
    collected_tool_calls: &mut HashMap<usize, Value>,
) {
    for tool_call in tool_calls_array {
        if let Some(index) = tool_call.get("index").and_then(|i| i.as_u64()) {
            let idx = index as usize;
            let entry = collected_tool_calls.entry(idx).or_insert_with(|| {
                json!({
                    "id": "",
                    "type": "function",
                    "function": {
                        "name": "",
                        "arguments": ""
                    }
                })
            });

            // Merge fields
            if let Some(id) = tool_call.get("id") {
                entry["id"] = id.clone();
            }
            if let Some(tc_type) = tool_call.get("type") {
                entry["type"] = tc_type.clone();
            }
            if let Some(function) = tool_call.get("function") {
                if let Some(name) = function.get("name") {
                    entry["function"]["name"] = name.clone();
                }
                if let Some(args) = function.get("arguments").and_then(|a| a.as_str()) {
                    let current_args = entry["function"]["arguments"].as_str().unwrap_or("");
                    entry["function"]["arguments"] = json!(format!("{}{}", current_args, args));
                }
            }
        }
    }
}

/// Execute tools and continue with multi-turn conversation
async fn execute_tools_and_continue(
    collected_tool_calls: HashMap<usize, Value>,
    content: String,
    context: StreamingContext,
    event_emitter: impl Fn(Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Clone,
    delta_chunk_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(
        "🔧 Executing {} tool(s) after stream completion",
        collected_tool_calls.len()
    );

    // Convert collected_tool_calls HashMap to Vec, sorted by index
    let mut tool_calls_vec: Vec<_> = collected_tool_calls.into_iter().collect();
    tool_calls_vec.sort_by_key(|(index, _)| *index);
    let final_tool_calls: Vec<Value> = tool_calls_vec.into_iter().map(|(_, tc)| tc).collect();

    tracing::debug!("Final tool_calls to execute: {:?}", final_tool_calls);

    // Execute each tool and collect results
    let mut tool_results: Vec<Value> = Vec::new();

    for tool_call in &final_tool_calls {
        let result = execute_single_tool(
            tool_call,
            &context.state,
            &context.user_id,
            &context.tool_ids,
        )
        .await;
        tool_results.push(result);
    }

    // Emit tool results to frontend
    tracing::info!(
        "📤 Emitting {} tool result(s) to frontend",
        tool_results.len()
    );
    for result in &tool_results {
        let tool_result_event = json!({
            "type": "chat:completion",
            "data": {
                "content": format!("\n\n**Tool Result:**\n{}", result.get("content").and_then(|c| c.as_str()).unwrap_or(""))
            }
        });
        event_emitter(tool_result_event).await;
    }

    // Multi-turn: Make a new chat completion request with tool results
    tracing::info!(
        "🔄 Starting multi-turn: sending tool results back to LLM for natural language response"
    );

    // Build new messages array
    let mut new_messages = context.messages.clone();

    // Add assistant message with tool_calls
    new_messages.push(json!({
        "role": "assistant",
        "content": "",
        "tool_calls": final_tool_calls
    }));

    // Add tool result messages
    for result in &tool_results {
        new_messages.push(result.clone());
    }

    // Make second request to LLM
    let second_response = make_tool_response_request(
        &context.state.http_client,
        &context.endpoint_url,
        &context.endpoint_key,
        &context.model_id,
        &new_messages,
        &context.tool_specs,
    )
    .await?;

    // Stream the second response
    stream_second_response(
        second_response,
        event_emitter,
        delta_chunk_size,
        &context.state,
        &context.chat_id,
        &context.message_id,
        &context.model_id,
        content,
    )
    .await?;

    // Generate title if requested
    if context.should_generate_title && context.chat_id.is_some() {
        spawn_title_generation(context).await;
    }

    Ok(())
}

/// Execute a single tool
async fn execute_single_tool(
    tool_call: &Value,
    state: &web::Data<AppState>,
    user_id: &str,
    tool_ids: &[String],
) -> Value {
    let tool_call_id = tool_call.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let tool_name = tool_call
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");
    let tool_args_str = tool_call
        .get("function")
        .and_then(|f| f.get("arguments"))
        .and_then(|a| a.as_str())
        .unwrap_or("{}");

    tracing::info!(
        "🔧 Executing tool: {} with args: {}",
        tool_name,
        tool_args_str
    );

    // Parse arguments
    let tool_args: HashMap<String, Value> = match serde_json::from_str(tool_args_str) {
        Ok(args) => args,
        Err(e) => {
            tracing::error!("Failed to parse tool arguments: {}", e);
            return json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": format!("Error: Failed to parse arguments - {}", e)
            });
        }
    };

    // Find and execute the tool
    let mut tool_result_content = format!("Error: Tool '{}' not found", tool_name);

    for tool_id in tool_ids {
        let tool_service = crate::services::tool::ToolService::new(&state.db);
        if let Ok(Some(tool)) = tool_service.get_tool_by_id(tool_id).await {
            if let Ok(tool_def) =
                serde_json::from_str::<crate::models::tool_runtime::ToolDefinition>(&tool.content)
            {
                if tool_def.tools.iter().any(|t| t.name == tool_name) {
                    tracing::info!(
                        "✅ Found tool spec for: {} in tool_id: {}",
                        tool_name,
                        tool_id
                    );

                    // Execute the tool
                    let runtime_service = crate::services::tool_runtime::ToolRuntimeService::new();

                    // Build execution context
                    let mut environment = HashMap::new();
                    for key in &[
                        "OPENWEATHER_API_KEY",
                        "OPENAI_API_KEY",
                        "ANTHROPIC_API_KEY",
                        "GOOGLE_API_KEY",
                    ] {
                        if let Ok(val) = std::env::var(key) {
                            environment.insert(key.to_string(), val);
                        }
                    }

                    let execution_context = crate::models::tool_runtime::ExecutionContext {
                        user: Some(crate::models::tool_runtime::UserContext {
                            id: user_id.to_string(),
                            name: "User".to_string(),
                            email: "user@example.com".to_string(),
                            role: Some("user".to_string()),
                        }),
                        environment,
                        session: HashMap::new(),
                    };

                    let exec_request = crate::models::tool_runtime::ToolExecutionRequest {
                        tool_id: tool_id.clone(),
                        tool_name: tool_name.to_string(),
                        parameters: tool_args.clone(),
                        context: execution_context,
                    };

                    match runtime_service.execute_tool(&state.db, exec_request).await {
                        Ok(exec_response) => {
                            tool_result_content = serde_json::to_string(&exec_response.result)
                                .unwrap_or_else(|_| "Error serializing result".to_string());
                            tracing::info!(
                                "✅ Tool executed successfully: {}",
                                tool_result_content
                            );
                        }
                        Err(e) => {
                            tool_result_content = format!("Error executing tool: {}", e);
                            tracing::error!("❌ Tool execution error: {}", e);
                        }
                    }
                    break;
                }
            }
        }
    }

    json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": tool_result_content
    })
}

/// Make a second request to LLM with tool results
async fn make_tool_response_request(
    client: &reqwest::Client,
    endpoint_url: &str,
    endpoint_key: &str,
    model_id: &str,
    messages: &[Value],
    tool_specs: &[Value],
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
    let mut request_builder = client
        .post(format!("{}/chat/completions", endpoint_url))
        .header("Content-Type", "application/json");

    if !endpoint_key.is_empty() {
        request_builder =
            request_builder.header("Authorization", format!("Bearer {}", endpoint_key));
    }

    let payload = json!({
        "model": model_id,
        "messages": messages,
        "stream": true,
        "tools": tool_specs.iter().map(|spec| json!({
            "type": "function",
            "function": spec
        })).collect::<Vec<_>>(),
        "tool_choice": "auto"
    });

    tracing::info!("🔄 Sending second request to LLM with tool results");

    let response = request_builder.json(&payload).send().await?;

    if !response.status().is_success() {
        return Err(format!("Second request failed with status: {}", response.status()).into());
    }

    Ok(response)
}

/// Stream the second response from tool execution
async fn stream_second_response(
    response: reqwest::Response,
    event_emitter: impl Fn(Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send,
    delta_chunk_size: usize,
    state: &web::Data<AppState>,
    chat_id: &Option<String>,
    message_id: &Option<String>,
    model_id: &str,
    previous_content: String,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("✅ Second request successful, streaming response...");

    let mut second_stream = response.bytes_stream();
    let mut second_content = String::new();
    let mut second_delta_count = 0;
    let mut second_last_delta: Option<Value> = None;

    while let Some(chunk_result) = second_stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if let Ok(text) = std::str::from_utf8(&chunk) {
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if line.starts_with("data: ") {
                            let data_str = &line[6..];
                            if data_str == "[DONE]" {
                                // Flush pending delta
                                if let Some(pending) = second_last_delta.take() {
                                    let event = json!({
                                        "type": "chat:completion",
                                        "data": pending
                                    });
                                    event_emitter(event).await;
                                }
                                break;
                            }

                            if let Ok(mut data) = serde_json::from_str::<Value>(data_str) {
                                if let Some(choices) =
                                    data.get("choices").and_then(|c| c.as_array())
                                {
                                    if let Some(first_choice) = choices.first() {
                                        if let Some(delta) = first_choice.get("delta") {
                                            if let Some(delta_content) =
                                                delta.get("content").and_then(|c| c.as_str())
                                            {
                                                second_content.push_str(delta_content);
                                                second_delta_count += 1;
                                                second_last_delta = Some(data.clone());

                                                if second_delta_count >= delta_chunk_size {
                                                    let event = json!({
                                                        "type": "chat:completion",
                                                        "data": data
                                                    });
                                                    event_emitter(event).await;
                                                    second_delta_count = 0;
                                                    second_last_delta = None;
                                                }
                                            }
                                        }

                                        // Check for finish_reason
                                        if let Some(finish_reason) =
                                            first_choice.get("finish_reason")
                                        {
                                            if !finish_reason.is_null() {
                                                tracing::info!(
                                                    "✅ Second stream finished with reason: {:?}",
                                                    finish_reason
                                                );

                                                // Flush pending delta
                                                if let Some(pending) = second_last_delta.take() {
                                                    let event = json!({
                                                        "type": "chat:completion",
                                                        "data": pending
                                                    });
                                                    event_emitter(event).await;
                                                }

                                                // Send final message with done flag
                                                data["done"] = json!(true);
                                                let event = json!({
                                                    "type": "chat:completion",
                                                    "data": data
                                                });
                                                event_emitter(event).await;

                                                // Update database
                                                if let (Some(cid), Some(mid)) =
                                                    (chat_id.as_ref(), message_id.as_ref())
                                                {
                                                    let final_content = format!(
                                                        "{}\n\n{}",
                                                        previous_content, second_content
                                                    );
                                                    let _ = upsert_chat_message(
                                                        &state.db,
                                                        cid,
                                                        mid,
                                                        json!({
                                                            "role": "assistant",
                                                            "content": final_content,
                                                            "done": true,
                                                            "model": model_id,
                                                        }),
                                                    )
                                                    .await;
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("❌ Error in second stream: {}", e);
                break;
            }
        }
    }

    tracing::info!("✅ Multi-turn conversation completed successfully");
    Ok(())
}

/// Upsert a message to a chat
async fn upsert_chat_message(
    db: &crate::db::Database,
    chat_id: &str,
    message_id: &str,
    message_data: Value,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::services::chat::ChatService;

    let chat_service = ChatService::new(db);
    if let Some(chat) = chat_service.get_chat_by_id(chat_id).await? {
        let mut chat_json = chat.chat.clone();

        // Sanitize content for null characters
        let mut sanitized_message_data = message_data.clone();
        if let Some(content) = sanitized_message_data
            .get("content")
            .and_then(|v| v.as_str())
        {
            let sanitized_content = content.replace("\x00", "");
            if let Some(obj) = sanitized_message_data.as_object_mut() {
                obj.insert("content".to_string(), json!(sanitized_content));
            }
        }

        // Ensure chat.history.messages structure exists
        if let Some(obj) = chat_json.as_object_mut() {
            let history = obj.entry("history").or_insert_with(|| json!({}));

            if let Some(history_obj) = history.as_object_mut() {
                let messages = history_obj.entry("messages").or_insert_with(|| json!({}));

                if let Some(messages_obj) = messages.as_object_mut() {
                    // Get existing message or create new one
                    let existing_message = messages_obj
                        .get(message_id)
                        .and_then(|v| v.as_object())
                        .cloned();

                    if let Some(mut existing_msg) = existing_message {
                        // Merge with existing message
                        if let Some(new_data_obj) = sanitized_message_data.as_object() {
                            for (key, value) in new_data_obj.iter() {
                                existing_msg.insert(key.clone(), value.clone());
                            }
                        }
                        messages_obj.insert(message_id.to_string(), json!(existing_msg));
                    } else {
                        // Insert new message
                        messages_obj.insert(message_id.to_string(), sanitized_message_data);
                    }
                }

                // Update currentId
                history_obj.insert("currentId".to_string(), json!(message_id));
            }
        }

        // Update the chat in database
        use crate::models::chat::UpdateChatRequest;
        let update_req = UpdateChatRequest {
            title: None,
            chat: Some(chat_json),
            folder_id: None,
            archived: None,
            pinned: None,
        };

        chat_service
            .update_chat(chat_id, &chat.user_id, update_req)
            .await?;
    }

    Ok(())
}

/// Spawn title generation as background task
async fn spawn_title_generation(context: StreamingContext) {
    tracing::info!(
        "Triggering title generation for chat: {}",
        context.chat_id.as_ref().unwrap()
    );

    tokio::spawn(async move {
        if let Err(e) = generate_and_update_title(context).await {
            tracing::error!("Failed to generate title: {}", e);
        }
    });
}

/// Generate title and update chat
async fn generate_and_update_title(
    context: StreamingContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::services::chat::ChatService;

    let chat_id = context.chat_id.as_ref().unwrap();

    tracing::info!(
        "🏷️  Title generation starting - model: {}, user: {}, endpoint: {}, has_key: {}",
        context.model_id,
        context.user_id,
        context.endpoint_url,
        !context.endpoint_key.is_empty()
    );

    // Check if title generation is enabled
    let prompt = {
        let config = context.state.config.read().unwrap();

        if !config.enable_title_generation {
            tracing::info!("🏷️  Title generation is DISABLED in config");
            return Ok(());
        }

        tracing::info!("🏷️  Title generation is ENABLED in config");

        // Get the last 2 messages
        let messages_for_title: Vec<_> = context
            .messages
            .iter()
            .rev()
            .take(2)
            .rev()
            .cloned()
            .collect();

        tracing::info!(
            "🏷️  Using {} messages for title generation",
            messages_for_title.len()
        );

        // Build prompt
        let template = if config.title_generation_prompt_template.is_empty() {
            DEFAULT_TITLE_GENERATION_PROMPT_TEMPLATE.to_string()
        } else {
            config.title_generation_prompt_template.clone()
        };

        let messages_text = messages_for_title
            .iter()
            .map(|m| {
                let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = m.get("content").and_then(|v| v.as_str()).unwrap_or("");
                format!("{}: {}", role, content)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let final_prompt = template.replace("{{MESSAGES:END:2}}", &messages_text);
        tracing::debug!("🏷️  Title generation prompt: {}", final_prompt);

        final_prompt
    };

    // Build request payload
    let title_payload = json!({
        "model": context.model_id,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": 50,
        "temperature": 0.1,
        "stream": false
    });

    let url = format!(
        "{}/chat/completions",
        context.endpoint_url.trim_end_matches('/')
    );

    tracing::info!("🏷️  Sending title generation request to: {}", url);
    tracing::debug!("🏷️  Title payload: {:?}", title_payload);

    // Use shared HTTP client for title generation
    let mut request_builder = context
        .state
        .http_client
        .post(&url)
        .timeout(std::time::Duration::from_secs(30)) // 30 sec timeout for title gen
        .header("Content-Type", "application/json");

    if !context.endpoint_key.is_empty() {
        request_builder =
            request_builder.header("Authorization", format!("Bearer {}", context.endpoint_key));
        tracing::debug!("🏷️  API key is present");
    } else {
        tracing::warn!("🏷️  NO API KEY provided for title generation!");
    }

    match request_builder.json(&title_payload).send().await {
        Ok(response) if response.status().is_success() => {
            tracing::info!("🏷️  Title generation response received successfully");
            let json_response = response.json::<Value>().await?;
            tracing::debug!("🏷️  Response JSON: {:?}", json_response);

            // Extract title from response
            if let Some(title_string) = json_response
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                tracing::info!("🏷️  Extracted title string from LLM: '{}'", title_string);
                // Parse JSON from response
                let start = title_string.find('{');
                let end = title_string.rfind('}');

                tracing::debug!(
                    "🏷️  Looking for JSON in title_string - start: {:?}, end: {:?}",
                    start,
                    end
                );

                if let (Some(start), Some(end)) = (start, end) {
                    let json_str = &title_string[start..=end];
                    tracing::debug!("🏷️  Extracted JSON string: {}", json_str);

                    if let Ok(title_json) = serde_json::from_str::<Value>(json_str) {
                        tracing::debug!("🏷️  Parsed title JSON: {:?}", title_json);

                        if let Some(title) = title_json.get("title").and_then(|t| t.as_str()) {
                            tracing::info!("🏷️  ✅ Successfully extracted title: '{}'", title);

                            // Update chat title
                            let chat_service = ChatService::new(&context.state.db);

                            use crate::models::chat::UpdateChatRequest;
                            if let Some(chat) = chat_service.get_chat_by_id(chat_id).await? {
                                let update_req = UpdateChatRequest {
                                    title: Some(title.to_string()),
                                    chat: None,
                                    folder_id: None,
                                    archived: None,
                                    pinned: None,
                                };

                                chat_service
                                    .update_chat(chat_id, &chat.user_id, update_req)
                                    .await?;
                                tracing::info!(
                                    "🏷️  ✅ Updated chat {} in database with title: '{}'",
                                    chat_id,
                                    title
                                );

                                // Emit chat:title event
                                if let Some(socket_state) = &context.state.socket_state {
                                    let event_payload = json!({
                                        "chat_id": chat_id,
                                        "message_id": context.message_id.as_deref(),
                                        "data": {
                                            "type": "chat:title",
                                            "data": title,
                                        }
                                    });

                                    tracing::debug!(
                                        "🏷️  Emitting event payload: {:?}",
                                        event_payload
                                    );

                                    match socket_state
                                        .native_handler
                                        .emit_to_user(
                                            &context.user_id,
                                            "chat-events",
                                            event_payload,
                                        )
                                        .await
                                    {
                                        Ok(sent_count) => {
                                            tracing::info!(
                                                "🏷️  ✅ Emitted chat:title event to {} session(s) for user: {}",
                                                sent_count,
                                                context.user_id
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "🏷️  ❌ Failed to emit chat:title event: {}",
                                                e
                                            );
                                        }
                                    }
                                } else {
                                    tracing::warn!("🏷️  ⚠️  Socket state not available, cannot emit chat:title event");
                                }
                            } else {
                                tracing::warn!("🏷️  ⚠️  Chat {} not found in database", chat_id);
                            }
                        } else {
                            tracing::warn!("🏷️  ⚠️  No 'title' field found in JSON response");
                        }
                    } else {
                        tracing::warn!("🏷️  ⚠️  Failed to parse JSON from title string");
                    }
                } else {
                    tracing::warn!(
                        "🏷️  ⚠️  No JSON object found in title response string: '{}'",
                        title_string
                    );
                }
            } else {
                tracing::warn!("🏷️  ⚠️  No content found in LLM response");
                tracing::debug!("🏷️  Full response: {:?}", json_response);
            }
        }
        Ok(response) => {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            tracing::warn!(
                "🏷️  ❌ Title generation failed with status: {} - {}",
                status,
                error_text
            );
        }
        Err(e) => {
            tracing::error!("🏷️  ❌ Title generation request error: {}", e);
        }
    }

    Ok(())
}
