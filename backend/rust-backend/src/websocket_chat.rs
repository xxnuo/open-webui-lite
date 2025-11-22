use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_ws::Message as WsMessage;
use futures::stream::StreamExt;

use crate::AppState;

/// WebSocket handler for real-time chat streaming
pub async fn websocket_chat_handler(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    tracing::info!("WebSocket connection established");

    // Clone state for spawned task
    let state_clone = state.clone();

    // Spawn task to handle WebSocket messages
    actix_web::rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                WsMessage::Text(text) => {
                    tracing::debug!("Received WebSocket text: {}", text);

                    // Parse the chat request
                    match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(payload) => {
                            // Process chat completion in real-time
                            if let Err(e) =
                                process_chat_stream(&state_clone, payload, &mut session).await
                            {
                                tracing::error!("Error processing chat: {}", e);
                                let error_msg = serde_json::json!({
                                    "error": format!("Error: {}", e)
                                })
                                .to_string();
                                let _ = session.text(error_msg).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse WebSocket message: {}", e);
                            let error_msg = serde_json::json!({
                                "error": format!("Invalid JSON: {}", e)
                            })
                            .to_string();
                            let _ = session.text(error_msg).await;
                        }
                    }
                }
                WsMessage::Ping(bytes) => {
                    let _ = session.pong(&bytes).await;
                }
                WsMessage::Close(reason) => {
                    tracing::info!("WebSocket close: {:?}", reason);
                    let _ = session.close(reason).await;
                    break;
                }
                _ => {}
            }
        }

        tracing::info!("WebSocket connection closed");
    });

    Ok(response)
}

/// Process chat completion and stream results in real-time
async fn process_chat_stream(
    state: &web::Data<AppState>,
    payload: serde_json::Value,
    session: &mut actix_ws::Session,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::services::chat::ChatService;

    // Extract metadata
    let chat_id = payload
        .get("metadata")
        .and_then(|m| m.get("chat_id"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let message_id = payload
        .get("metadata")
        .and_then(|m| m.get("message_id"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract model
    let model_id = payload
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or("Model ID required")?
        .to_string();

    // Get OpenAI configuration
    let (url, key) = {
        let config = state.config.read().unwrap();
        if !config.enable_openai_api || config.openai_api_base_urls.is_empty() {
            return Err("OpenAI API not configured".into());
        }
        let url = config.openai_api_base_urls[0].clone();
        let key = config.openai_api_keys.get(0).cloned().unwrap_or_default();
        (url, key)
    };

    // Make request to OpenAI API with streaming - ZERO BUFFERING
    let client = reqwest::Client::builder()
        .tcp_nodelay(true) // Disable Nagle's algorithm for real-time streaming
        .timeout(std::time::Duration::from_secs(300)) // 5 min timeout
        .http2_keep_alive_interval(Some(std::time::Duration::from_secs(5)))
        .http2_keep_alive_while_idle(true)
        .build()?;

    let response = client
        .post(format!("{}/chat/completions", url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", key))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream") // Explicitly request SSE
        .header("Cache-Control", "no-cache")
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        session
            .text(
                serde_json::json!({
                    "error": error
                })
                .to_string(),
            )
            .await?;
        return Err(error.into());
    }

    // Check if streaming
    let is_stream = payload
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_stream {
        // REAL-TIME STREAMING - ZERO BUFFERING APPROACH
        // Use chunk_completion_stream to get smallest possible chunks
        let mut stream = response.bytes_stream();
        let mut accumulated_content = String::new();
        let mut buffer = String::new();

        tracing::info!("🔴 LIVE STREAMING STARTED - forwarding chunks in real-time");

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Convert bytes to text
                    if let Ok(text) = std::str::from_utf8(&chunk) {
                        tracing::debug!("⚡ Received chunk: {} bytes", text.len());

                        // CRITICAL: Send IMMEDIATELY to WebSocket without ANY delay
                        // This ensures real-time streaming like live chat
                        // actix-ws automatically flushes on each send, no manual flush needed
                        if let Err(e) = session.text(text.to_string()).await {
                            tracing::error!("❌ Failed to send WebSocket message: {}", e);
                            return Err(e.into());
                        }

                        // Accumulate for parsing (for DB save only - doesn't affect streaming)
                        buffer.push_str(text);

                        // Parse accumulated buffer line by line for content extraction
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer.drain(..=newline_pos);

                            if line.is_empty() {
                                continue;
                            }

                            // Parse SSE data for content accumulation (DB save)
                            if line.starts_with("data: ") {
                                let data = line.strip_prefix("data: ").unwrap_or("");

                                if data == "[DONE]" {
                                    tracing::info!("✅ Streaming completed - received [DONE]");
                                    break;
                                }

                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                    if let Some(choices) =
                                        json.get("choices").and_then(|v| v.as_array())
                                    {
                                        if let Some(choice) = choices.first() {
                                            if let Some(delta) = choice.get("delta") {
                                                if let Some(content) =
                                                    delta.get("content").and_then(|v| v.as_str())
                                                {
                                                    accumulated_content.push_str(content);
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
                    session
                        .text(
                            serde_json::json!({
                                "error": format!("Stream error: {}", e)
                            })
                            .to_string(),
                        )
                        .await?;
                    break;
                }
            }
        }

        tracing::info!(
            "🏁 Streaming finished. Total content length: {}",
            accumulated_content.len()
        );

        // Save to database after streaming completes - use Python's structure
        if let (Some(ch_id), Some(msg_id)) = (chat_id, message_id) {
            if !accumulated_content.is_empty() {
                let chat_service = ChatService::new(&state.db);

                // Get or create chat
                if let Ok(Some(chat)) = chat_service.get_chat_by_id(&ch_id).await {
                    let mut chat_json = chat.chat.clone();

                    // Use Python's structure: chat.chat.history.messages.{message_id}
                    if let Some(obj) = chat_json.as_object_mut() {
                        // Get or create history object
                        let history = obj
                            .entry("history")
                            .or_insert_with(|| serde_json::json!({}));

                        if let Some(history_obj) = history.as_object_mut() {
                            // Get or create messages object (not array, but object/map)
                            let messages = history_obj
                                .entry("messages")
                                .or_insert_with(|| serde_json::json!({}));

                            if let Some(messages_obj) = messages.as_object_mut() {
                                // Create message data matching Python format
                                let message_data = serde_json::json!({
                                    "id": msg_id,
                                    "role": "assistant",
                                    "content": accumulated_content,
                                    "model": model_id,
                                    "timestamp": chrono::Utc::now().timestamp(),
                                });

                                // Upsert message by message_id as key (Python uses this pattern)
                                if let Some(existing_msg) = messages_obj.get(&msg_id) {
                                    // Merge with existing message
                                    if let Some(existing_obj) = existing_msg.as_object() {
                                        let mut merged = existing_obj.clone();
                                        if let Some(new_obj) = message_data.as_object() {
                                            for (k, v) in new_obj {
                                                merged.insert(k.clone(), v.clone());
                                            }
                                        }
                                        messages_obj.insert(
                                            msg_id.clone(),
                                            serde_json::Value::Object(merged),
                                        );
                                    }
                                } else {
                                    messages_obj.insert(msg_id.clone(), message_data);
                                }

                                // Set currentId like Python does
                                history_obj.insert(
                                    "currentId".to_string(),
                                    serde_json::Value::String(msg_id.clone()),
                                );
                            }
                        }
                    }

                    // Update chat in database
                    use crate::models::chat::UpdateChatRequest;
                    let update_result = chat_service
                        .update_chat(
                            &ch_id,
                            &chat.user_id,
                            UpdateChatRequest {
                                title: None,
                                chat: Some(chat_json),
                                folder_id: None,
                                archived: None,
                                pinned: None,
                            },
                        )
                        .await;

                    match update_result {
                        Ok(_) => tracing::info!(
                            "✓ Saved chat message to DB: chat_id={}, msg_id={}, content_len={}",
                            ch_id,
                            msg_id,
                            accumulated_content.len()
                        ),
                        Err(e) => tracing::error!("✗ Failed to save chat message: {}", e),
                    }
                }
            }
        }
    } else {
        // Non-streaming response
        let json_response = response.json::<serde_json::Value>().await?;
        session.text(serde_json::to_string(&json_response)?).await?;
    }

    Ok(())
}
