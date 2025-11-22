use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::channel::Channel;
use crate::utils::time::current_timestamp;

#[allow(dead_code)]
pub struct ChannelService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> ChannelService<'a> {
    pub fn new(db: &'a Database) -> Self {
        ChannelService { db }
    }

    pub async fn create_channel(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        description: Option<&str>,
        channel_type: Option<&str>,
        data: Option<serde_json::Value>,
        meta: Option<serde_json::Value>,
        access_control: Option<serde_json::Value>,
    ) -> AppResult<Channel> {
        let now = current_timestamp();

        let data_str = data
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));
        let meta_str = meta
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));
        let access_control_str = access_control
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));

        sqlx::query(
            r#"
            INSERT INTO channel (id, name, description, user_id, type, data, meta, access_control, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(user_id)
        .bind(channel_type)
        .bind(&data_str)
        .bind(&meta_str)
        .bind(&access_control_str)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_channel_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create channel".to_string()))
    }

    pub async fn get_channel_by_id(&self, id: &str) -> AppResult<Option<Channel>> {
        let mut result = sqlx::query_as::<_, Channel>(
            r#"
            SELECT id, name, description, user_id, type as channel_type,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(access_control AS TEXT) as access_control_str,
                   created_at, updated_at
            FROM channel
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(ref mut channel) = result {
            channel.parse_data();
            channel.parse_meta();
            channel.parse_access_control();
        }

        Ok(result)
    }

    pub async fn get_channels_by_user_id(&self, user_id: &str) -> AppResult<Vec<Channel>> {
        // Get all channels first
        let mut all_channels = sqlx::query_as::<_, Channel>(
            r#"
            SELECT id, name, description, user_id, type as channel_type,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(access_control AS TEXT) as access_control_str,
                   created_at, updated_at
            FROM channel
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        tracing::info!(
            "get_channels_by_user_id: Found {} total channels for user_id={}",
            all_channels.len(),
            user_id
        );

        for channel in &mut all_channels {
            channel.parse_data();
            channel.parse_meta();
            channel.parse_access_control();
        }

        // Filter channels based on ownership or access control
        let mut accessible_channels = Vec::new();
        for channel in all_channels {
            tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            tracing::info!(
                "Checking access for channel '{}' (id={})",
                channel.name,
                channel.id
            );
            tracing::info!("  Channel owner: {}", channel.user_id);
            tracing::info!("  Current user: {}", user_id);
            tracing::info!("  Access control: {:?}", channel.access_control);

            // Owner always has access
            if channel.user_id == user_id {
                tracing::info!(
                    "  ✓ User is owner - granting access to channel '{}'",
                    channel.name
                );
                accessible_channels.push(channel);
                continue;
            }

            // Check access control permissions for "read" access
            // If access_control is None, the channel is public (readable by all)
            let has_read_access = crate::utils::access_control::has_access(
                self.db,
                user_id,
                "read",
                channel.access_control.as_ref(),
                true, // strict mode - if no access_control, read is allowed per Python logic
            )
            .await
            .unwrap_or(false);

            tracing::debug!("  has_read_access result: {}", has_read_access);

            if has_read_access {
                tracing::info!(
                    "  ✓ User has read access - granting access to channel '{}'",
                    channel.name
                );
                accessible_channels.push(channel);
            } else {
                tracing::warn!(
                    "  ✗ User does NOT have access to channel '{}'",
                    channel.name
                );
            }
        }

        tracing::info!(
            "get_channels_by_user_id: Returning {} accessible channels for user {}",
            accessible_channels.len(),
            user_id
        );
        Ok(accessible_channels)
    }

    pub async fn get_all_channels(&self) -> AppResult<Vec<Channel>> {
        let mut channels = sqlx::query_as::<_, Channel>(
            r#"
            SELECT id, name, description, user_id, type as channel_type,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(access_control AS TEXT) as access_control_str,
                   created_at, updated_at
            FROM channel
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        for channel in &mut channels {
            channel.parse_data();
            channel.parse_meta();
            channel.parse_access_control();
        }

        Ok(channels)
    }

    pub async fn update_channel(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        channel_type: Option<&str>,
        data: Option<serde_json::Value>,
        meta: Option<serde_json::Value>,
        access_control: Option<serde_json::Value>,
    ) -> AppResult<Channel> {
        let now = current_timestamp();

        let mut updates = vec!["updated_at = $1".to_string()];
        let mut bind_count = 2;

        if name.is_some() {
            updates.push(format!("name = ${}", bind_count));
            bind_count += 1;
        }
        if description.is_some() {
            updates.push(format!("description = ${}", bind_count));
            bind_count += 1;
        }
        if channel_type.is_some() {
            updates.push(format!("type = ${}", bind_count));
            bind_count += 1;
        }
        if data.is_some() {
            updates.push(format!("data = ${}", bind_count));
            bind_count += 1;
        }
        if meta.is_some() {
            updates.push(format!("meta = ${}", bind_count));
            bind_count += 1;
        }
        if access_control.is_some() {
            updates.push(format!("access_control = ${}", bind_count));
            bind_count += 1;
        }

        let query_str = format!(
            "UPDATE channel SET {} WHERE id = ${}",
            updates.join(", "),
            bind_count
        );

        let mut query = sqlx::query(&query_str);
        query = query.bind(now);

        if let Some(n) = name {
            query = query.bind(n);
        }
        if let Some(d) = description {
            query = query.bind(d);
        }
        if let Some(ct) = channel_type {
            query = query.bind(ct);
        }
        if let Some(data_val) = data {
            query = query.bind(serde_json::to_string(&data_val).unwrap());
        }
        if let Some(meta_val) = meta {
            query = query.bind(serde_json::to_string(&meta_val).unwrap());
        }
        if let Some(ac_val) = access_control {
            query = query.bind(serde_json::to_string(&ac_val).unwrap());
        }

        query = query.bind(id);

        query.execute(&self.db.pool).await?;

        self.get_channel_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))
    }

    pub async fn delete_channel(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM channel WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_channels_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM channel WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }
}
