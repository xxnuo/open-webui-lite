use serde_json::json;
use std::sync::Arc;

/// Socket.IO state for managing connections
#[derive(Clone)]
pub struct SocketState {
    // Reference to native Socket.IO handler
    pub native_handler: Arc<crate::socketio::EventHandler>,
}

impl SocketState {
    pub fn new(handler: Arc<crate::socketio::EventHandler>) -> Self {
        Self {
            native_handler: handler,
        }
    }
}

/// Create event emitter function for streaming chat completions
pub fn get_event_emitter(
    socket_state: SocketState,
    user_id: String,
    chat_id: Option<String>,
    message_id: Option<String>,
    _session_id: Option<String>,
) -> impl Fn(serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
       + Send
       + Clone {
    move |event_data: serde_json::Value| {
        let socket_state = socket_state.clone();
        let user_id = user_id.clone();
        let chat_id = chat_id.clone();
        let message_id = message_id.clone();

        Box::pin(async move {
            // Prepare event payload
            let payload = json!({
                "chat_id": chat_id,
                "message_id": message_id,
                "data": event_data,
            });

            // Emit via native Socket.IO handler
            if let Err(e) = socket_state
                .native_handler
                .emit_to_user(&user_id, "chat-events", payload)
                .await
            {
                tracing::warn!("Failed to emit via native Socket.IO: {}", e);
            }
        })
    }
}
