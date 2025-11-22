use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::chat::{Chat, CreateChatRequest, UpdateChatRequest};
use crate::utils::time::current_timestamp_seconds;
use sqlx::types::JsonValue;
use sqlx::Row;
use uuid::Uuid;

pub struct ChatService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> ChatService<'a> {
    pub fn new(db: &'a Database) -> Self {
        ChatService { db }
    }

    pub async fn create_chat(&self, user_id: &str, req: CreateChatRequest) -> AppResult<Chat> {
        let now = current_timestamp_seconds();
        let title = req.title.unwrap_or_else(|| "New Chat".to_string());
        let id = req.id;

        let meta_value: JsonValue = req.meta.unwrap_or_else(|| serde_json::json!({}));

        sqlx::query(
            r#"
            INSERT INTO chat (id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(&title)
        .bind(&req.chat)
        .bind(&req.folder_id)
        .bind(req.archived.unwrap_or(false))
        .bind(req.pinned.unwrap_or(false))
        .bind(&req.share_id)
        .bind(&meta_value)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id(&id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create chat".to_string()))
    }

    pub async fn get_chat_by_id(&self, id: &str) -> AppResult<Option<Chat>> {
        let result = sqlx::query_as::<_, Chat>(
            r#"
            SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
            FROM chat
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_chat_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
    ) -> AppResult<Option<Chat>> {
        let result = sqlx::query_as::<_, Chat>(
            r#"
            SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
            FROM chat
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_chats_by_user_id(
        &self,
        user_id: &str,
        include_archived: bool,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<Chat>> {
        let query = if include_archived {
            sqlx::query_as::<_, Chat>(
                r#"
                SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
                FROM chat
                WHERE user_id = $1
                ORDER BY updated_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
        } else {
            sqlx::query_as::<_, Chat>(
                r#"
                SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
                FROM chat
                WHERE user_id = $1 AND archived = 0
                ORDER BY updated_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
        };

        let chats = query
            .bind(user_id)
            .bind(limit)
            .bind(skip)
            .fetch_all(&self.db.pool)
            .await?;

        Ok(chats)
    }

    pub async fn get_pinned_chats_by_user_id(&self, user_id: &str) -> AppResult<Vec<Chat>> {
        let chats = sqlx::query_as::<_, Chat>(
            r#"
            SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
            FROM chat
            WHERE user_id = $1 AND pinned = 1 AND archived = 0
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(chats)
    }

    pub async fn get_chats_by_folder_id(
        &self,
        folder_id: &str,
        user_id: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<Chat>> {
        let chats = sqlx::query_as::<_, Chat>(
            r#"
            SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
            FROM chat
            WHERE folder_id = $1 AND user_id = $2 AND archived = 0 AND (pinned = 0 OR pinned IS NULL)
            ORDER BY updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(folder_id)
        .bind(user_id)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(chats)
    }

    pub async fn search_chats(
        &self,
        user_id: &str,
        search_text: &str,
        include_archived: bool,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<Chat>> {
        let search_pattern = format!("%{}%", search_text);

        let query = if include_archived {
            sqlx::query_as::<_, Chat>(
                r#"
                SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
                FROM chat
                WHERE user_id = $1 AND title LIKE $2
                ORDER BY updated_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
        } else {
            sqlx::query_as::<_, Chat>(
                r#"
                SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
                FROM chat
                WHERE user_id = $1 AND archived = 0 AND title LIKE $2
                ORDER BY updated_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
        };

        let chats = query
            .bind(user_id)
            .bind(&search_pattern)
            .bind(limit)
            .bind(skip)
            .fetch_all(&self.db.pool)
            .await?;

        Ok(chats)
    }

    pub async fn update_chat(
        &self,
        id: &str,
        user_id: &str,
        req: UpdateChatRequest,
    ) -> AppResult<Chat> {
        let now = current_timestamp_seconds();

        let mut query_builder = sqlx::QueryBuilder::new("UPDATE chat SET updated_at = ");
        query_builder.push_bind(now);

        if let Some(title) = req.title {
            query_builder.push(", title = ");
            query_builder.push_bind(title);
        }
        if let Some(chat) = req.chat {
            query_builder.push(", chat = ");
            query_builder.push_bind(chat);
        }
        if let Some(folder_id) = req.folder_id {
            query_builder.push(", folder_id = ");
            query_builder.push_bind(folder_id);
        }
        if let Some(archived) = req.archived {
            query_builder.push(", archived = ");
            query_builder.push_bind(archived);
        }
        if let Some(pinned) = req.pinned {
            query_builder.push(", pinned = ");
            query_builder.push_bind(pinned);
        }

        query_builder.push(" WHERE id = ");
        query_builder.push_bind(id);
        query_builder.push(" AND user_id = ");
        query_builder.push_bind(user_id);

        query_builder.build().execute(&self.db.pool).await?;

        self.get_chat_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found after update".to_string()))
    }

    pub async fn toggle_chat_pinned(&self, id: &str, user_id: &str) -> AppResult<Chat> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            UPDATE chat
            SET pinned = CASE WHEN pinned = 1 THEN 0 ELSE 1 END,
                updated_at = $1
            WHERE id = $2 AND user_id = $3
            "#,
        )
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))
    }

    pub async fn toggle_chat_archived(&self, id: &str, user_id: &str) -> AppResult<Chat> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            UPDATE chat
            SET archived = CASE WHEN archived = 1 THEN 0 ELSE 1 END,
                updated_at = $1
            WHERE id = $2 AND user_id = $3
            "#,
        )
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))
    }

    pub async fn archive_all_chats(&self, user_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE chat
            SET archived = 1
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn unarchive_all_chats(&self, user_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE chat
            SET archived = 0
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn create_shared_chat(&self, chat_id: &str) -> AppResult<String> {
        // Get the original chat
        let chat = self
            .get_chat_by_id(chat_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;

        // Check if already shared
        if let Some(share_id) = &chat.share_id {
            return Ok(share_id.clone());
        }

        // Create shared chat
        let share_id = Uuid::new_v4().to_string();
        let now = current_timestamp_seconds();

        let meta_value = chat.meta.unwrap_or_else(|| serde_json::json!({}));

        sqlx::query(
            r#"
            INSERT INTO chat (id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(&share_id)
        .bind(format!("shared-{}", chat_id))
        .bind(&chat.title)
        .bind(&chat.chat)
        .bind(&chat.folder_id)
        .bind(false)
        .bind(false)
        .bind(None::<String>)
        .bind(&meta_value)
        .bind(chat.created_at)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        // Update original chat with share_id
        sqlx::query(
            r#"
            UPDATE chat
            SET share_id = $1
            WHERE id = $2
            "#,
        )
        .bind(&share_id)
        .bind(chat_id)
        .execute(&self.db.pool)
        .await?;

        Ok(share_id)
    }

    pub async fn delete_shared_chat(&self, chat_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            DELETE FROM chat
            WHERE user_id = $1
            "#,
        )
        .bind(format!("shared-{}", chat_id))
        .execute(&self.db.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE chat
            SET share_id = NULL
            WHERE id = $1
            "#,
        )
        .bind(chat_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_chat(&self, id: &str, user_id: &str) -> AppResult<()> {
        // Delete shared chat if exists
        let _ = self.delete_shared_chat(id).await;

        sqlx::query(
            r#"
            DELETE FROM chat
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_chats_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            DELETE FROM chat
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_chats_by_folder_id(&self, folder_id: &str, user_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            DELETE FROM chat
            WHERE folder_id = $1 AND user_id = $2
            "#,
        )
        .bind(folder_id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn count_chats_by_user_id(&self, user_id: &str) -> AppResult<i64> {
        let count: i64 = sqlx::query("SELECT COUNT(*) as count FROM chat WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.db.pool)
            .await?
            .try_get("count")?;

        Ok(count)
    }

    // Helper methods for simpler route usage
    pub async fn get_chats_by_user(&self, user_id: &str) -> AppResult<Vec<Chat>> {
        self.get_chats_by_user_id(user_id, false, 0, 100).await
    }

    /// Upsert a message to chat's history (Python-compatible structure)
    /// Python structure: chat.chat.history.messages.{message_id} = message_data
    pub async fn upsert_message_to_chat(
        &self,
        chat_id: &str,
        message_id: &str,
        message: serde_json::Value,
    ) -> AppResult<()> {
        // Get existing chat
        let chat = self
            .get_chat_by_id(chat_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;

        let mut chat_json = chat.chat.clone();

        // Ensure structure: chat.history.messages.{message_id}
        if let Some(obj) = chat_json.as_object_mut() {
            // Get or create history
            let history = obj
                .entry("history")
                .or_insert_with(|| serde_json::json!({}));

            if let Some(history_obj) = history.as_object_mut() {
                // Get or create messages object
                let messages = history_obj
                    .entry("messages")
                    .or_insert_with(|| serde_json::json!({}));

                if let Some(messages_obj) = messages.as_object_mut() {
                    // Upsert message
                    if let Some(existing_msg) = messages_obj.get(message_id) {
                        // Merge with existing
                        if let Some(existing_obj) = existing_msg.as_object() {
                            let mut merged = existing_obj.clone();
                            if let Some(new_obj) = message.as_object() {
                                for (k, v) in new_obj {
                                    merged.insert(k.clone(), v.clone());
                                }
                            }
                            messages_obj
                                .insert(message_id.to_string(), serde_json::Value::Object(merged));
                        }
                    } else {
                        messages_obj.insert(message_id.to_string(), message);
                    }

                    // Update currentId
                    history_obj.insert(
                        "currentId".to_string(),
                        serde_json::Value::String(message_id.to_string()),
                    );
                }
            }
        }

        // Update in database
        let now = current_timestamp_seconds();
        sqlx::query(
            r#"
            UPDATE chat
            SET chat = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(&chat_json)
        .bind(now)
        .bind(chat_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_all_chats_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            DELETE FROM chat
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn get_chat_title_id_list_by_user_id(
        &self,
        user_id: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, updated_at, created_at, folder_id
            FROM chat
            WHERE user_id = $1 AND archived = 0
            ORDER BY updated_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        let result: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "title": row.get::<String, _>("title"),
                    "updated_at": row.get::<i64, _>("updated_at"),
                    "created_at": row.get::<i64, _>("created_at"),
                    "folder_id": row.get::<Option<String>, _>("folder_id"),
                })
            })
            .collect();

        Ok(result)
    }

    pub async fn get_chat_list_by_user_id(
        &self,
        user_id: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<serde_json::Value>> {
        self.get_chat_title_id_list_by_user_id(user_id, skip, limit)
            .await
    }

    pub async fn search_chats_by_user_id(
        &self,
        user_id: &str,
        text: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<serde_json::Value>> {
        let search_pattern = format!("%{}%", text);

        let rows = sqlx::query(
            r#"
            SELECT id, title, updated_at, created_at, folder_id
            FROM chat
            WHERE user_id = $1 AND archived = 0 
                AND (title LIKE $2 OR chat LIKE $2)
            ORDER BY updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(user_id)
        .bind(&search_pattern)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        let result: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "title": row.get::<String, _>("title"),
                    "updated_at": row.get::<i64, _>("updated_at"),
                    "created_at": row.get::<i64, _>("created_at"),
                    "folder_id": row.get::<Option<String>, _>("folder_id"),
                })
            })
            .collect();

        Ok(result)
    }

    pub async fn get_archived_chats_by_user_id(&self, user_id: &str) -> AppResult<Vec<Chat>> {
        let chats = sqlx::query_as::<_, Chat>(
            r#"
            SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
            FROM chat
            WHERE user_id = $1 AND archived = 1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(chats)
    }

    pub async fn get_archived_chat_list_by_user_id(
        &self,
        user_id: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, updated_at, created_at, folder_id
            FROM chat
            WHERE user_id = $1 AND archived = 1
            ORDER BY updated_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        let result: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "title": row.get::<String, _>("title"),
                    "updated_at": row.get::<i64, _>("updated_at"),
                    "created_at": row.get::<i64, _>("created_at"),
                    "folder_id": row.get::<Option<String>, _>("folder_id"),
                })
            })
            .collect();

        Ok(result)
    }

    pub async fn get_chats_by_folder_id_full(
        &self,
        folder_id: &str,
        user_id: &str,
    ) -> AppResult<Vec<Chat>> {
        let chats = sqlx::query_as::<_, Chat>(
            r#"
            SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
            FROM chat
            WHERE user_id = $1 AND folder_id = $2 AND archived = 0
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .bind(folder_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(chats)
    }

    pub async fn get_chat_list_by_folder_id(
        &self,
        folder_id: &str,
        user_id: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, updated_at
            FROM chat
            WHERE user_id = $1 AND folder_id = $2 AND archived = 0
            ORDER BY updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(user_id)
        .bind(folder_id)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        let result: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "title": row.get::<String, _>("title"),
                    "updated_at": row.get::<i64, _>("updated_at"),
                })
            })
            .collect();

        Ok(result)
    }

    pub async fn get_chat_list_by_user_id_and_tag_name(
        &self,
        user_id: &str,
        tag_name: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<serde_json::Value>> {
        // SQLite doesn't support PostgreSQL's JSONB operators
        // Use json_extract to search for tag in meta.tags array
        let tag_search = format!("%\"{}%", tag_name);
        let rows = sqlx::query(
            r#"
            SELECT id, title, updated_at, created_at, folder_id
            FROM chat
            WHERE user_id = $1 AND archived = 0
                AND json_extract(meta, '$.tags') LIKE $2
            ORDER BY updated_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(user_id)
        .bind(&tag_search)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        let result: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<String, _>("id"),
                    "title": row.get::<String, _>("title"),
                    "updated_at": row.get::<i64, _>("updated_at"),
                    "created_at": row.get::<i64, _>("created_at"),
                    "folder_id": row.get::<Option<String>, _>("folder_id"),
                })
            })
            .collect();

        Ok(result)
    }

    pub async fn get_chat_by_share_id(&self, share_id: &str) -> AppResult<Option<Chat>> {
        let result = sqlx::query_as::<_, Chat>(
            r#"
            SELECT id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at
            FROM chat
            WHERE share_id = $1
            "#,
        )
        .bind(share_id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn clone_chat(
        &self,
        user_id: &str,
        source_chat: Chat,
        title: Option<String>,
    ) -> AppResult<Chat> {
        let new_id = Uuid::new_v4().to_string();
        let now = current_timestamp_seconds();

        // Prepare cloned chat data
        let mut chat_data = source_chat.chat.clone();
        if let Some(obj) = chat_data.as_object_mut() {
            obj.insert(
                "originalChatId".to_string(),
                serde_json::Value::String(source_chat.id.clone()),
            );

            // Get branch point message ID
            if let Some(history) = obj.get("history").and_then(|h| h.as_object()) {
                if let Some(current_id) = history.get("currentId") {
                    obj.insert("branchPointMessageId".to_string(), current_id.clone());
                }
            }
        }

        let new_title = title.unwrap_or_else(|| format!("Clone of {}", &source_chat.title));

        sqlx::query(
            r#"
            INSERT INTO chat (id, user_id, title, chat, folder_id, archived, pinned, share_id, meta, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(&new_id)
        .bind(user_id)
        .bind(&new_title)
        .bind(&chat_data)
        .bind(&source_chat.folder_id)
        .bind(source_chat.archived)
        .bind(source_chat.pinned)
        .bind::<Option<String>>(None)
        .bind(&source_chat.meta)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id(&new_id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to clone chat".to_string()))
    }

    pub async fn update_chat_folder(
        &self,
        id: &str,
        user_id: &str,
        folder_id: Option<String>,
    ) -> AppResult<Chat> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            UPDATE chat
            SET folder_id = $1, updated_at = $2
            WHERE id = $3 AND user_id = $4
            "#,
        )
        .bind(&folder_id)
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id_and_user_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))
    }

    pub async fn update_chat_message(
        &self,
        chat_id: &str,
        message_id: &str,
        user_id: &str,
        content: &str,
    ) -> AppResult<Chat> {
        // Get existing chat
        let chat = self
            .get_chat_by_id_and_user_id(chat_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;

        let mut chat_data = chat.chat.clone();

        // Update message content in chat.history.messages.{message_id}.content
        if let Some(obj) = chat_data.as_object_mut() {
            if let Some(history) = obj.get_mut("history").and_then(|h| h.as_object_mut()) {
                if let Some(messages) = history.get_mut("messages").and_then(|m| m.as_object_mut())
                {
                    if let Some(message) =
                        messages.get_mut(message_id).and_then(|m| m.as_object_mut())
                    {
                        message.insert(
                            "content".to_string(),
                            serde_json::Value::String(content.to_string()),
                        );
                    }
                }
            }
        }

        let now = current_timestamp_seconds();
        sqlx::query(
            r#"
            UPDATE chat
            SET chat = $1, updated_at = $2
            WHERE id = $3 AND user_id = $4
            "#,
        )
        .bind(&chat_data)
        .bind(now)
        .bind(chat_id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id_and_user_id(chat_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))
    }

    pub async fn add_chat_tag(
        &self,
        chat_id: &str,
        user_id: &str,
        tag_name: &str,
    ) -> AppResult<Chat> {
        let chat = self
            .get_chat_by_id_and_user_id(chat_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;

        let mut meta = chat.meta.unwrap_or_else(|| serde_json::json!({}));

        // Get or create tags array
        let tags_array = meta
            .get_mut("tags")
            .and_then(|t| t.as_array_mut())
            .map(|arr| arr.clone())
            .unwrap_or_else(Vec::new);

        let tag_id = tag_name.replace(" ", "_").to_lowercase();

        // Check if tag already exists
        let tag_exists = tags_array
            .iter()
            .any(|t| t.as_str().map(|s| s == tag_id).unwrap_or(false));

        if !tag_exists {
            let mut new_tags = tags_array;
            new_tags.push(serde_json::Value::String(tag_id));
            meta["tags"] = serde_json::Value::Array(new_tags);
        }

        let now = current_timestamp_seconds();
        sqlx::query(
            r#"
            UPDATE chat
            SET meta = $1, updated_at = $2
            WHERE id = $3 AND user_id = $4
            "#,
        )
        .bind(&meta)
        .bind(now)
        .bind(chat_id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id_and_user_id(chat_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))
    }

    pub async fn delete_chat_tag(
        &self,
        chat_id: &str,
        user_id: &str,
        tag_name: &str,
    ) -> AppResult<Chat> {
        let chat = self
            .get_chat_by_id_and_user_id(chat_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;

        let mut meta = chat.meta.unwrap_or_else(|| serde_json::json!({}));

        if let Some(tags) = meta.get_mut("tags").and_then(|t| t.as_array_mut()) {
            let tag_id = tag_name.replace(" ", "_").to_lowercase();
            tags.retain(|t| t.as_str().map(|s| s != tag_id).unwrap_or(true));
        }

        let now = current_timestamp_seconds();
        sqlx::query(
            r#"
            UPDATE chat
            SET meta = $1, updated_at = $2
            WHERE id = $3 AND user_id = $4
            "#,
        )
        .bind(&meta)
        .bind(now)
        .bind(chat_id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_chat_by_id_and_user_id(chat_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))
    }

    pub async fn delete_all_chat_tags(&self, chat_id: &str, user_id: &str) -> AppResult<()> {
        let chat = self
            .get_chat_by_id_and_user_id(chat_id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Chat not found".to_string()))?;

        let mut meta = chat.meta.unwrap_or_else(|| serde_json::json!({}));
        meta["tags"] = serde_json::Value::Array(vec![]);

        let now = current_timestamp_seconds();
        sqlx::query(
            r#"
            UPDATE chat
            SET meta = $1, updated_at = $2
            WHERE id = $3 AND user_id = $4
            "#,
        )
        .bind(&meta)
        .bind(now)
        .bind(chat_id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }
}
