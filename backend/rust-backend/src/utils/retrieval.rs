// Retrieval utilities for processing notes, files, and chats as context sources
// This module provides functionality to extract content from various sources (notes, files, chats)
// and inject them as context into chat messages (RAG - Retrieval Augmented Generation)

use crate::{
    error::{AppError, AppResult},
    models::{chat::Chat, file::File, note::Note, user::User},
    services::{chat::ChatService, file::FileService, note::NoteService},
    utils::misc::{get_message_list, has_access},
    AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Default RAG template for injecting context into user messages
pub const DEFAULT_RAG_TEMPLATE: &str = r#"### Task:
Respond to the user query using the provided context, incorporating inline citations in the format [id] **only when the <source> tag includes an explicit id attribute** (e.g., <source id="1">).

### Guidelines:
- If you don't know the answer, clearly state that.
- If uncertain, ask the user for clarification.
- Respond in the same language as the user's query.
- If the context is unreadable or of poor quality, inform the user and provide the best possible answer.
- If the answer isn't present in the context but you possess the knowledge, explain this to the user and provide the answer using your own understanding.
- **Only include inline citations using [id] (e.g., [1], [2]) when the <source> tag includes an id attribute.**
- Do not cite if the <source> tag does not contain an id attribute.
- Do not use XML tags in your response.
- Ensure citations are concise and directly related to the information provided.

### Example of Citation:
If the user asks about a specific topic and the information is found in a source with a provided id attribute, the response should include the citation like in the following example:
* "According to the study, the proposed method increases efficiency by 20% [1]."

### Output:
Provide a clear and direct response to the user's query, including inline citations in the format [id] only when the <source> tag with id attribute is present in the context.

<context>
{{CONTEXT}}
</context>

<user_query>
{{QUERY}}
</user_query>
"#;

/// Item representing a file, note, or chat attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub context: Option<String>, // "full" or null
    pub collection_name: Option<String>,
    pub file: Option<Value>, // Contains data.content if available
    pub content: Option<String>,
}

/// Source result after processing a file item
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub source: Value,
    pub document: Vec<String>,
    pub metadata: Vec<Value>,
}

/// Get last user message content from messages array
pub fn get_last_user_message(messages: &[Value]) -> Option<String> {
    for message in messages.iter().rev() {
        if message.get("role").and_then(|r| r.as_str()) == Some("user") {
            if let Some(content) = message.get("content") {
                // Handle string content
                if let Some(text) = content.as_str() {
                    return Some(text.to_string());
                }
                // Handle array content (multi-modal messages)
                if let Some(arr) = content.as_array() {
                    for item in arr {
                        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                return Some(text.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Add or update user message content
pub fn add_or_update_user_message(content: &str, messages: &mut Vec<Value>, append: bool) {
    if let Some(last_msg) = messages.last_mut() {
        if last_msg.get("role").and_then(|r| r.as_str()) == Some("user") {
            // Update existing user message
            if let Some(existing_content) = last_msg.get("content").and_then(|c| c.as_str()) {
                let new_content = if append {
                    format!("{}\n{}", existing_content, content)
                } else {
                    format!("{}\n{}", content, existing_content)
                };
                last_msg["content"] = json!(new_content);
            } else {
                last_msg["content"] = json!(content);
            }
            return;
        }
    }

    // Add new user message
    messages.push(json!({
        "role": "user",
        "content": content
    }));
}

/// Apply RAG template to context and query
pub fn rag_template(template: &str, context: &str, query: &str) -> String {
    let template = if template.trim().is_empty() {
        DEFAULT_RAG_TEMPLATE
    } else {
        template
    };

    template
        .replace("{{CONTEXT}}", context)
        .replace("[context]", context)
        .replace("{{QUERY}}", query)
        .replace("[query]", query)
}

/// Process file items and extract sources for RAG
pub async fn get_sources_from_items(
    state: &AppState,
    items: Vec<FileItem>,
    user: &User,
    user_group_ids: &HashSet<String>,
) -> AppResult<Vec<Source>> {
    let mut sources = Vec::new();

    let note_service = NoteService::new(&state.db);
    let file_service = FileService::new(&state.db);
    let chat_service = ChatService::new(&state.db);

    for item in items {
        let mut query_result: Option<(Vec<String>, Vec<Value>)> = None;

        match item.item_type.as_str() {
            "text" => {
                // Raw text attachment (temporary uploads, web pages, etc.)
                if item.context.as_deref() == Some("full") {
                    if let Some(file_data) = &item.file {
                        if let Some(content) = file_data
                            .get("data")
                            .and_then(|d| d.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            let metadata = file_data.get("meta").cloned().unwrap_or(json!({}));
                            query_result = Some((vec![content.to_string()], vec![metadata]));
                        }
                    }
                }

                // Fallback to content field
                if query_result.is_none() {
                    if let Some(content) = &item.content {
                        query_result = Some((
                            vec![content.clone()],
                            vec![json!({
                                "file_id": item.id.as_deref().unwrap_or("N/A"),
                                "name": item.name.as_deref().unwrap_or("Unknown")
                            })],
                        ));
                    }
                }
            }

            "note" => {
                // Note attachment - extract note content
                if let Some(note_id) = &item.id {
                    match note_service.get_note_by_id(note_id).await? {
                        Some(mut note) => {
                            note.parse_json_fields();

                            // Check access
                            if user.role == "admin"
                                || note.user_id == user.id
                                || has_access(
                                    &user.id,
                                    "read",
                                    &note.access_control,
                                    user_group_ids,
                                )
                            {
                                // Extract markdown content from note.data.content.md
                                let content = note
                                    .data
                                    .as_ref()
                                    .and_then(|d| d.get("content"))
                                    .and_then(|c| c.get("md"))
                                    .and_then(|md| md.as_str())
                                    .unwrap_or("");

                                query_result = Some((
                                    vec![content.to_string()],
                                    vec![json!({
                                        "file_id": note.id,
                                        "name": note.title
                                    })],
                                ));

                                tracing::info!(
                                    "✅ Note '{}' (ID: {}) retrieved for chat context (length: {} chars)",
                                    note.title,
                                    note.id,
                                    content.len()
                                );
                            } else {
                                tracing::warn!(
                                    "❌ User {} does not have access to note {}",
                                    user.id,
                                    note_id
                                );
                            }
                        }
                        None => {
                            tracing::warn!("⚠️ Note {} not found", note_id);
                        }
                    }
                }
            }

            "chat" => {
                // Chat attachment - extract chat history
                if let Some(chat_id) = &item.id {
                    match chat_service
                        .get_chat_by_id_and_user_id(chat_id, &user.id)
                        .await?
                    {
                        Some(chat) => {
                            // Check access
                            if user.role == "admin" || chat.user_id == user.id {
                                // Extract messages from chat history
                                // chat.chat is a JsonValue (serde_json::Value), not Option
                                if let Some(history) = chat.chat.get("history") {
                                    let messages_map =
                                        history.get("messages").and_then(|m| m.as_object());
                                    let message_id =
                                        history.get("currentId").and_then(|id| id.as_str());

                                    if let (Some(messages_map_obj), Some(message_id)) =
                                        (messages_map, message_id)
                                    {
                                        // Convert serde_json::Map to HashMap<String, Value>
                                        let messages_hashmap: std::collections::HashMap<
                                            String,
                                            Value,
                                        > = messages_map_obj
                                            .iter()
                                            .map(|(k, v)| (k.clone(), v.clone()))
                                            .collect();

                                        // Reconstruct message list
                                        let message_list =
                                            get_message_list(&messages_hashmap, message_id);

                                        // Format messages as markdown
                                        let message_history: Vec<String> = message_list
                                            .iter()
                                            .filter_map(|m| {
                                                let role = m.get("role")?.as_str()?.to_string();
                                                let content =
                                                    m.get("content")?.as_str()?.to_string();
                                                Some(format!(
                                                    "#### {}\n{}\n",
                                                    role.chars()
                                                        .next()?
                                                        .to_uppercase()
                                                        .collect::<String>()
                                                        + &role[1..],
                                                    content
                                                ))
                                            })
                                            .collect();

                                        let content = message_history.join("\n");

                                        query_result = Some((
                                            vec![content],
                                            vec![json!({
                                                "file_id": chat.id,
                                                "name": chat.title
                                            })],
                                        ));

                                        tracing::info!(
                                            "✅ Chat '{}' retrieved for context ({} messages)",
                                            chat.title,
                                            message_list.len()
                                        );
                                    }
                                }
                            }
                        }
                        None => {
                            tracing::warn!("⚠️ Chat {} not found or no access", chat_id);
                        }
                    }
                }
            }

            "file" => {
                // File attachment
                // Check if full context mode or if file data is embedded
                if item.context.as_deref() == Some("full") {
                    if let Some(file_data) = &item.file {
                        if let Some(content) = file_data
                            .get("data")
                            .and_then(|d| d.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            let mut metadata = json!({
                                "file_id": item.id.as_deref().unwrap_or("N/A"),
                                "name": item.name.as_deref().unwrap_or("Unknown")
                            });

                            // Merge additional metadata if present
                            if let Some(extra_meta) =
                                file_data.get("data").and_then(|d| d.get("metadata"))
                            {
                                if let Some(obj) = metadata.as_object_mut() {
                                    if let Some(extra_obj) = extra_meta.as_object() {
                                        for (k, v) in extra_obj {
                                            obj.insert(k.clone(), v.clone());
                                        }
                                    }
                                }
                            }

                            query_result = Some((vec![content.to_string()], vec![metadata]));

                            tracing::info!(
                                "✅ File '{}' retrieved from embedded data (length: {} chars)",
                                item.name.as_deref().unwrap_or("Unknown"),
                                content.len()
                            );
                        }
                    } else if let Some(file_id) = &item.id {
                        // Fallback: fetch from database
                        match file_service.get_file_by_id(file_id).await? {
                            Some(mut file) => {
                                file.parse_json_fields();

                                let content = file
                                    .data
                                    .as_ref()
                                    .and_then(|d| d.get("content"))
                                    .and_then(|c| c.as_str())
                                    .unwrap_or("");

                                query_result = Some((
                                    vec![content.to_string()],
                                    vec![json!({
                                        "file_id": file_id,
                                        "name": file.filename,
                                        "source": file.filename
                                    })],
                                ));

                                tracing::info!(
                                    "✅ File '{}' retrieved from database (length: {} chars)",
                                    file.filename,
                                    content.len()
                                );
                            }
                            None => {
                                tracing::warn!("⚠️ File {} not found", file_id);
                            }
                        }
                    }
                }
                // Note: Vector search/embedding retrieval is not yet implemented
                // TODO: Implement collection-based retrieval with embeddings
            }

            _ => {
                tracing::warn!("Unknown item type: {}", item.item_type);
            }
        }

        // Add to sources if we got a result
        if let Some((documents, metadatas)) = query_result {
            let source_info = json!({
                "type": item.item_type,
                "id": item.id.unwrap_or_default(),
                "name": item.name.unwrap_or_default()
            });

            sources.push(Source {
                source: source_info,
                document: documents,
                metadata: metadatas,
            });
        }
    }

    tracing::info!("📦 Retrieved {} source(s) for RAG context", sources.len());

    Ok(sources)
}

/// Process sources and inject them into messages as RAG context
pub fn inject_sources_into_messages(
    sources: Vec<Source>,
    messages: &mut Vec<Value>,
    rag_template_str: &str,
) -> AppResult<Vec<Source>> {
    if sources.is_empty() {
        return Ok(sources);
    }

    // Build context string with citations
    let mut context_string = String::new();
    let mut citation_idx_map: HashMap<String, usize> = HashMap::new();

    for source in &sources {
        if !source.document.is_empty() {
            for (document_text, document_metadata) in
                source.document.iter().zip(source.metadata.iter())
            {
                let source_name = source.source.get("name").and_then(|n| n.as_str());
                let source_id = document_metadata
                    .get("source")
                    .and_then(|s| s.as_str())
                    .or_else(|| source.source.get("id").and_then(|i| i.as_str()))
                    .unwrap_or("N/A");

                // Assign citation index
                if !citation_idx_map.contains_key(source_id) {
                    let idx = citation_idx_map.len() + 1;
                    citation_idx_map.insert(source_id.to_string(), idx);
                }

                let citation_idx = citation_idx_map.get(source_id).unwrap();

                // Format: <source id="1" name="...">content</source>
                context_string.push_str(&format!(
                    "<source id=\"{}\"{}>{}</source>\n",
                    citation_idx,
                    source_name
                        .map(|n| format!(" name=\"{}\"", n))
                        .unwrap_or_default(),
                    document_text
                ));
            }
        }
    }

    context_string = context_string.trim().to_string();

    if context_string.is_empty() {
        return Ok(sources);
    }

    // Get last user message
    let query = get_last_user_message(messages).ok_or_else(|| {
        AppError::BadRequest("No user message found to inject context".to_string())
    })?;

    // Apply RAG template (call the function, not the parameter)
    let augmented_message = rag_template(rag_template_str, &context_string, &query);

    // Update last user message
    add_or_update_user_message(&augmented_message, messages, false);

    tracing::info!(
        "✅ Injected {} source(s) into user message ({} chars of context)",
        sources.len(),
        context_string.len()
    );

    Ok(sources)
}
