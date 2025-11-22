use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::memory::Memory;
use crate::utils::time::current_timestamp_seconds;

#[allow(dead_code)]
pub struct MemoryService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> MemoryService<'a> {
    pub fn new(db: &'a Database) -> Self {
        MemoryService { db }
    }

    pub async fn create_memory(
        &self,
        id: &str,
        user_id: &str,
        content: &str,
        meta: Option<serde_json::Value>,
    ) -> AppResult<Memory> {
        let now = current_timestamp_seconds();
        let meta_str = meta
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()));

        sqlx::query(
            r#"
            INSERT INTO memory (id, user_id, content, meta, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(content)
        .bind(&meta_str)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_memory_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create memory".to_string()))
    }

    pub async fn get_memory_by_id(&self, id: &str) -> AppResult<Option<Memory>> {
        let result = sqlx::query_as::<_, Memory>(
            r#"
            SELECT id, user_id, content, meta, created_at, updated_at
            FROM memory
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_memories_by_user_id(&self, user_id: &str) -> AppResult<Vec<Memory>> {
        let memories = sqlx::query_as::<_, Memory>(
            r#"
            SELECT id, user_id, content, meta, created_at, updated_at
            FROM memory
            WHERE user_id = $1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(memories)
    }

    pub async fn get_all_memories(&self) -> AppResult<Vec<Memory>> {
        let memories = sqlx::query_as::<_, Memory>(
            r#"
            SELECT id, user_id, content, meta, created_at, updated_at
            FROM memory
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(memories)
    }

    pub async fn update_memory(
        &self,
        id: &str,
        content: Option<&str>,
        meta: Option<serde_json::Value>,
    ) -> AppResult<Memory> {
        let now = current_timestamp_seconds();

        let mut query_parts = vec!["UPDATE memory SET updated_at = $2"];
        let mut bind_values: Vec<String> = vec![now.to_string()];

        if let Some(content) = content {
            query_parts.push("content = $1");
            bind_values.push(content.to_string());
        }
        if let Some(meta) = meta {
            query_parts.push("meta = ?");
            bind_values.push(serde_json::to_string(&meta).unwrap());
        }

        let query_str = format!("{} WHERE id = $1", query_parts.join(", "));

        let mut query = sqlx::query(&query_str);
        for value in bind_values {
            query = query.bind(value);
        }
        query = query.bind(id);

        query.execute(&self.db.pool).await?;

        self.get_memory_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Memory not found".to_string()))
    }

    pub async fn delete_memory(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM memory WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_memories_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM memory WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn query_memories(
        &self,
        user_id: &str,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<Memory>> {
        let search_pattern = format!("%{}%", query);

        let memories = sqlx::query_as::<_, Memory>(
            r#"
            SELECT id, user_id, content, meta, created_at, updated_at
            FROM memory
            WHERE user_id = $1 AND content LIKE $2
            ORDER BY updated_at DESC
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(&search_pattern)
        .bind(limit)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(memories)
    }
}
