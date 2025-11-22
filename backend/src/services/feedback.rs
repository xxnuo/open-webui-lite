use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::feedback::{Feedback, FeedbackForm};
use crate::utils::time::current_timestamp_seconds;
use uuid::Uuid;

pub struct FeedbackService<'a> {
    db: &'a Database,
}

impl<'a> FeedbackService<'a> {
    pub fn new(db: &'a Database) -> Self {
        FeedbackService { db }
    }

    pub async fn insert_new_feedback(
        &self,
        user_id: &str,
        form_data: &FeedbackForm,
    ) -> AppResult<Feedback> {
        let now = current_timestamp_seconds();
        let id = Uuid::new_v4().to_string();

        let data_json = form_data
            .data
            .as_ref()
            .map(|d| serde_json::to_string(d).ok())
            .flatten();

        let meta_json = form_data
            .meta
            .as_ref()
            .map(|m| serde_json::to_string(m).ok())
            .flatten();

        let snapshot_json = form_data
            .snapshot
            .as_ref()
            .map(|s| serde_json::to_string(s).ok())
            .flatten();

        sqlx::query(
            r#"
            INSERT INTO feedback (id, user_id, version, type, data, meta, snapshot, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(0i64)
        .bind(&form_data.feedback_type)
        .bind(&data_json)
        .bind(&meta_json)
        .bind(&snapshot_json)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_feedback_by_id(&id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create feedback".to_string()))
    }

    pub async fn get_feedback_by_id(&self, id: &str) -> AppResult<Option<Feedback>> {
        let result = sqlx::query_as::<_, Feedback>(
            r#"
            SELECT id, user_id, version, type as feedback_type, created_at, updated_at,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(snapshot AS TEXT) as snapshot_str
            FROM feedback
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_feedback_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
    ) -> AppResult<Option<Feedback>> {
        let result = sqlx::query_as::<_, Feedback>(
            r#"
            SELECT id, user_id, version, type as feedback_type, created_at, updated_at,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(snapshot AS TEXT) as snapshot_str
            FROM feedback
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_all_feedbacks(&self) -> AppResult<Vec<Feedback>> {
        let feedbacks = sqlx::query_as::<_, Feedback>(
            r#"
            SELECT id, user_id, version, type as feedback_type, created_at, updated_at,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(snapshot AS TEXT) as snapshot_str
            FROM feedback
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(feedbacks)
    }

    pub async fn get_feedbacks_by_user_id(&self, user_id: &str) -> AppResult<Vec<Feedback>> {
        let feedbacks = sqlx::query_as::<_, Feedback>(
            r#"
            SELECT id, user_id, version, type as feedback_type, created_at, updated_at,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(snapshot AS TEXT) as snapshot_str
            FROM feedback
            WHERE user_id = $1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(feedbacks)
    }

    pub async fn update_feedback_by_id(
        &self,
        id: &str,
        form_data: &FeedbackForm,
    ) -> AppResult<Feedback> {
        let now = current_timestamp_seconds();

        let data_json = form_data
            .data
            .as_ref()
            .map(|d| serde_json::to_string(d).ok())
            .flatten();

        let meta_json = form_data
            .meta
            .as_ref()
            .map(|m| serde_json::to_string(m).ok())
            .flatten();

        let snapshot_json = form_data
            .snapshot
            .as_ref()
            .map(|s| serde_json::to_string(s).ok())
            .flatten();

        sqlx::query(
            r#"
            UPDATE feedback
            SET data = COALESCE($1, data),
                meta = COALESCE($2, meta),
                snapshot = COALESCE($3, snapshot),
                updated_at = $4
            WHERE id = $5
            "#,
        )
        .bind(&data_json)
        .bind(&meta_json)
        .bind(&snapshot_json)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_feedback_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Feedback not found".to_string()))
    }

    pub async fn update_feedback_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
        form_data: &FeedbackForm,
    ) -> AppResult<Feedback> {
        let now = current_timestamp_seconds();

        let data_json = form_data
            .data
            .as_ref()
            .map(|d| serde_json::to_string(d).ok())
            .flatten();

        let meta_json = form_data
            .meta
            .as_ref()
            .map(|m| serde_json::to_string(m).ok())
            .flatten();

        let snapshot_json = form_data
            .snapshot
            .as_ref()
            .map(|s| serde_json::to_string(s).ok())
            .flatten();

        sqlx::query(
            r#"
            UPDATE feedback
            SET data = COALESCE($1, data),
                meta = COALESCE($2, meta),
                snapshot = COALESCE($3, snapshot),
                updated_at = $4
            WHERE id = $5 AND user_id = $6
            "#,
        )
        .bind(&data_json)
        .bind(&meta_json)
        .bind(&snapshot_json)
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_feedback_by_id_and_user_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Feedback not found".to_string()))
    }

    pub async fn delete_feedback_by_id(&self, id: &str) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM feedback WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_feedback_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
    ) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM feedback WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_feedbacks_by_user_id(&self, user_id: &str) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM feedback WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_all_feedbacks(&self) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM feedback")
            .execute(&self.db.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
