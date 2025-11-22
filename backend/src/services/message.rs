use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::message::{Message, MessageForm, MessageReaction, MessageResponse, Reaction};
use crate::models::user::UserNameResponse;
use crate::services::user::UserService;
use crate::utils::time::current_timestamp;
use std::collections::HashMap;

#[allow(dead_code)]
pub struct MessageService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> MessageService<'a> {
    pub fn new(db: &'a Database) -> Self {
        MessageService { db }
    }

    pub async fn create_message(
        &self,
        channel_id: &str,
        user_id: &str,
        form_data: &MessageForm,
    ) -> AppResult<Message> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = current_timestamp();

        let data_str = form_data
            .data
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));
        let meta_str = form_data
            .meta
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));

        sqlx::query(
            r#"
            INSERT INTO message (id, channel_id, user_id, content, reply_to_id, parent_id, data, meta, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(&id)
        .bind(channel_id)
        .bind(user_id)
        .bind(&form_data.content)
        .bind(&form_data.reply_to_id)
        .bind(&form_data.parent_id)
        .bind(&data_str)
        .bind(&meta_str)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_message_by_id(&id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create message".to_string()))
    }

    pub async fn get_message_by_id(&self, id: &str) -> AppResult<Option<Message>> {
        let mut result = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, chat_id, channel_id, user_id, content, role, model,
                   reply_to_id, parent_id,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   created_at, updated_at
            FROM message
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(ref mut message) = result {
            message.parse_data();
            message.parse_meta();
        }

        Ok(result)
    }

    pub async fn get_messages_by_channel_id(
        &self,
        channel_id: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<Message>> {
        let mut messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, chat_id, channel_id, user_id, content, role, model,
                   reply_to_id, parent_id,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   created_at, updated_at
            FROM message
            WHERE channel_id = $1 AND parent_id IS NULL
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(channel_id)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        for message in &mut messages {
            message.parse_data();
            message.parse_meta();
        }

        Ok(messages)
    }

    pub async fn get_thread_messages(
        &self,
        channel_id: &str,
        parent_id: &str,
        skip: i64,
        limit: i64,
    ) -> AppResult<Vec<Message>> {
        // First verify parent message exists
        let parent = self.get_message_by_id(parent_id).await?;
        if parent.is_none() {
            return Ok(vec![]);
        }

        let mut messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, chat_id, channel_id, user_id, content, role, model,
                   reply_to_id, parent_id,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   created_at, updated_at
            FROM message
            WHERE channel_id = $1 AND parent_id = $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(channel_id)
        .bind(parent_id)
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        // Add parent message if we have room
        if messages.len() < limit as usize {
            if let Some(parent_msg) = parent {
                messages.push(parent_msg);
            }
        }

        for message in &mut messages {
            message.parse_data();
            message.parse_meta();
        }

        Ok(messages)
    }

    pub async fn get_thread_replies_count(&self, message_id: &str) -> AppResult<i64> {
        let result =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM message WHERE parent_id = $1")
                .bind(message_id)
                .fetch_one(&self.db.pool)
                .await?;

        Ok(result)
    }

    pub async fn get_latest_thread_reply_at(&self, message_id: &str) -> AppResult<Option<i64>> {
        let result = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT created_at FROM message WHERE parent_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(message_id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result.flatten())
    }

    pub async fn update_message(
        &self,
        message_id: &str,
        form_data: &MessageForm,
    ) -> AppResult<Message> {
        // Get existing message to merge data and meta
        let existing = self
            .get_message_by_id(message_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

        // Merge data
        let mut merged_data = existing.get_data().unwrap_or_else(|| serde_json::json!({}));
        if let Some(new_data) = &form_data.data {
            if let (Some(merged_obj), Some(new_obj)) =
                (merged_data.as_object_mut(), new_data.as_object())
            {
                for (k, v) in new_obj {
                    merged_obj.insert(k.clone(), v.clone());
                }
            }
        }

        // Merge meta
        let mut merged_meta = existing.get_meta().unwrap_or_else(|| serde_json::json!({}));
        if let Some(new_meta) = &form_data.meta {
            if let (Some(merged_obj), Some(new_obj)) =
                (merged_meta.as_object_mut(), new_meta.as_object())
            {
                for (k, v) in new_obj {
                    merged_obj.insert(k.clone(), v.clone());
                }
            }
        }

        let now = current_timestamp();
        let data_str = serde_json::to_string(&merged_data).unwrap_or_else(|_| "{}".to_string());
        let meta_str = serde_json::to_string(&merged_meta).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            UPDATE message 
            SET content = $1, data = $2, meta = $3, updated_at = $4
            WHERE id = $5
            "#,
        )
        .bind(&form_data.content)
        .bind(&data_str)
        .bind(&meta_str)
        .bind(now)
        .bind(message_id)
        .execute(&self.db.pool)
        .await?;

        self.get_message_by_id(message_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Message not found".to_string()))
    }

    pub async fn delete_message(&self, message_id: &str) -> AppResult<()> {
        // Delete reactions first
        sqlx::query("DELETE FROM message_reaction WHERE message_id = $1")
            .bind(message_id)
            .execute(&self.db.pool)
            .await?;

        // Delete message
        sqlx::query("DELETE FROM message WHERE id = $1")
            .bind(message_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn add_reaction(
        &self,
        message_id: &str,
        user_id: &str,
        name: &str,
    ) -> AppResult<MessageReaction> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = current_timestamp();

        sqlx::query(
            r#"
            INSERT INTO message_reaction (id, message_id, user_id, name, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&id)
        .bind(message_id)
        .bind(user_id)
        .bind(name)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        Ok(MessageReaction {
            id,
            message_id: message_id.to_string(),
            user_id: user_id.to_string(),
            name: name.to_string(),
            created_at: now,
        })
    }

    pub async fn remove_reaction(
        &self,
        message_id: &str,
        user_id: &str,
        name: &str,
    ) -> AppResult<()> {
        sqlx::query(
            "DELETE FROM message_reaction WHERE message_id = $1 AND user_id = $2 AND name = $3",
        )
        .bind(message_id)
        .bind(user_id)
        .bind(name)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn get_reactions(&self, message_id: &str) -> AppResult<Vec<Reaction>> {
        let reactions = sqlx::query_as::<_, MessageReaction>(
            "SELECT id, message_id, user_id, name, created_at FROM message_reaction WHERE message_id = $1"
        )
        .bind(message_id)
        .fetch_all(&self.db.pool)
        .await?;

        // Group by name
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for reaction in reactions {
            grouped
                .entry(reaction.name.clone())
                .or_insert_with(Vec::new)
                .push(reaction.user_id);
        }

        let result = grouped
            .into_iter()
            .map(|(name, user_ids)| Reaction {
                count: user_ids.len(),
                name,
                user_ids,
            })
            .collect();

        Ok(result)
    }

    pub async fn delete_replies(&self, parent_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM message WHERE parent_id = $1")
            .bind(parent_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    /// Convert Message to MessageResponse with user information populated
    pub async fn to_message_response(&self, message: Message) -> AppResult<MessageResponse> {
        let user_service = UserService::new(self.db);
        let user = user_service
            .get_user_by_id(&message.user_id)
            .await
            .ok()
            .flatten();

        let mut response = MessageResponse::from(message);
        response.user = user.map(UserNameResponse::from);

        Ok(response)
    }
}
