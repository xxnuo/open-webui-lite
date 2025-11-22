use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::AppError;
use crate::models::user::User;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: i32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

/// Get system message from message list
#[allow(dead_code)]
pub fn get_system_message(messages: &[ChatMessage]) -> Option<&ChatMessage> {
    messages.iter().find(|m| m.role == "system")
}

/// Deep update a JSON value with another
#[allow(dead_code)]
pub fn deep_update(target: &mut Value, source: &Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source_obj {
            if let Some(target_value) = target_obj.get_mut(key) {
                if target_value.is_object() && value.is_object() {
                    deep_update(target_value, value);
                } else {
                    *target_value = value.clone();
                }
            } else {
                target_obj.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Get message list from messages map and message ID
#[allow(dead_code)]
pub fn get_message_list(
    messages_map: &serde_json::Map<String, Value>,
    message_id: &str,
) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut current_id = Some(message_id.to_string());

    while let Some(id) = current_id {
        if let Some(message) = messages_map.get(&id) {
            messages.push(message.clone());
            current_id = message
                .get("parentId")
                .and_then(|v| v.as_str())
                .map(String::from);
        } else {
            break;
        }
    }

    messages.reverse();
    messages
}

/// Add or update user message with additional content
#[allow(dead_code)]
pub fn add_or_update_user_message(content: &str, messages: &mut Vec<ChatMessage>) {
    if let Some(last_message) = messages.last_mut() {
        if last_message.role == "user" {
            last_message.content = format!("{}\n\n{}", last_message.content, content);
        } else {
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: content.to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
    } else {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: content.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }
}

/// Apply system prompt to form data
#[allow(dead_code)]
pub fn apply_system_prompt_to_body(
    system_prompt: &str,
    form_data: &mut Value,
    _metadata: &Value,
    user: &User,
) -> Result<(), AppError> {
    // Replace variables in system prompt
    let mut processed_prompt = system_prompt.to_string();

    // Replace {{USER_NAME}}
    processed_prompt = processed_prompt.replace("{{USER_NAME}}", &user.name);

    // Replace {{USER_EMAIL}}
    processed_prompt = processed_prompt.replace("{{USER_EMAIL}}", &user.email);

    // Replace {{USER_ROLE}}
    processed_prompt = processed_prompt.replace("{{USER_ROLE}}", &user.role);

    // Replace {{CURRENT_DATE}}
    let current_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    processed_prompt = processed_prompt.replace("{{CURRENT_DATE}}", &current_date);

    // Replace {{CURRENT_TIME}}
    let current_time = chrono::Utc::now().format("%H:%M:%S").to_string();
    processed_prompt = processed_prompt.replace("{{CURRENT_TIME}}", &current_time);

    // Update the system message in form_data
    if let Some(messages) = form_data.get_mut("messages").and_then(|v| v.as_array_mut()) {
        // Find and update system message, or add new one
        let mut found = false;
        for message in messages.iter_mut() {
            if message.get("role").and_then(|v| v.as_str()) == Some("system") {
                if let Some(content) = message.get_mut("content") {
                    *content = json!(processed_prompt);
                    found = true;
                    break;
                }
            }
        }

        if !found {
            messages.insert(
                0,
                json!({
                    "role": "system",
                    "content": processed_prompt
                }),
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deep_update() {
        let mut target = json!({
            "a": 1,
            "b": {
                "c": 2,
                "d": 3
            }
        });

        let source = json!({
            "b": {
                "c": 5,
                "e": 6
            },
            "f": 7
        });

        deep_update(&mut target, &source);

        assert_eq!(target["a"], 1);
        assert_eq!(target["b"]["c"], 5);
        assert_eq!(target["b"]["d"], 3);
        assert_eq!(target["b"]["e"], 6);
        assert_eq!(target["f"], 7);
    }

    #[test]
    fn test_get_system_message() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let system_msg = get_system_message(&messages);
        assert!(system_msg.is_some());
        assert_eq!(system_msg.unwrap().content, "You are a helpful assistant.");
    }
}
