use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::model::{Model, ModelForm};
use crate::utils::time::current_timestamp_seconds;
use sqlx::Row;

#[allow(dead_code)]
pub struct ModelService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> ModelService<'a> {
    pub fn new(db: &'a Database) -> Self {
        ModelService { db }
    }

    pub async fn create_model(
        &self,
        id: &str,
        user_id: &str,
        base_model_id: Option<&str>,
        name: &str,
        params: serde_json::Value,
        meta: serde_json::Value,
    ) -> AppResult<Model> {
        let now = current_timestamp_seconds();
        let params_str = serde_json::to_string(&params).unwrap_or_else(|_| "{}".to_string());
        let meta_str = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            INSERT INTO model (id, user_id, base_model_id, name, params, meta, created_at, updated_at, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(base_model_id)
        .bind(name)
        .bind(&params_str)
        .bind(&meta_str)
        .bind(now)
        .bind(now)
        .bind(true)
        .execute(&self.db.pool)
        .await?;

        self.get_model_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create model".to_string()))
    }

    pub async fn get_model_by_id(&self, id: &str) -> AppResult<Option<Model>> {
        let result = sqlx::query_as::<_, Model>(
            r#"
            SELECT id, user_id, base_model_id, name, params, meta, access_control, created_at, updated_at, is_active
            FROM model
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_all_models(&self) -> AppResult<Vec<Model>> {
        let models = sqlx::query_as::<_, Model>(
            r#"
            SELECT id, user_id, base_model_id, name, params, meta, access_control, created_at, updated_at, is_active
            FROM model
            WHERE base_model_id IS NOT NULL
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(models)
    }

    pub async fn get_base_models(&self) -> AppResult<Vec<Model>> {
        let models = sqlx::query_as::<_, Model>(
            r#"
            SELECT id, user_id, base_model_id, name, params, meta, access_control, created_at, updated_at, is_active
            FROM model
            WHERE base_model_id IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(models)
    }

    pub async fn get_models_by_user_id(&self, user_id: &str) -> AppResult<Vec<Model>> {
        let models = sqlx::query_as::<_, Model>(
            r#"
            SELECT id, user_id, base_model_id, name, params, meta, access_control, created_at, updated_at, is_active
            FROM model
            WHERE user_id = $1 AND base_model_id IS NOT NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(models)
    }

    pub async fn update_model(
        &self,
        id: &str,
        name: Option<&str>,
        params: Option<serde_json::Value>,
        meta: Option<serde_json::Value>,
    ) -> AppResult<Model> {
        let now = current_timestamp_seconds();

        let mut query_parts = vec!["UPDATE model SET updated_at = $2"];
        let mut bind_values: Vec<String> = vec![now.to_string()];

        if let Some(name) = name {
            query_parts.push("name = $1");
            bind_values.push(name.to_string());
        }
        if let Some(params) = params {
            query_parts.push("params = ?");
            bind_values.push(serde_json::to_string(&params).unwrap());
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

        self.get_model_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Model not found".to_string()))
    }

    pub async fn toggle_model_active(&self, id: &str) -> AppResult<Model> {
        sqlx::query(
            r#"
            UPDATE model
            SET is_active = CASE WHEN is_active = 1 THEN 0 ELSE 1 END, updated_at = $2
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(current_timestamp_seconds())
        .execute(&self.db.pool)
        .await?;

        self.get_model_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Model not found".to_string()))
    }

    pub async fn delete_model(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM model WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_all_models(&self) -> AppResult<()> {
        sqlx::query("DELETE FROM model")
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn sync_models(&self, user_id: &str, models: Vec<Model>) -> AppResult<Vec<Model>> {
        // Get existing model IDs
        let existing: Vec<String> = sqlx::query("SELECT id FROM model")
            .fetch_all(&self.db.pool)
            .await?
            .into_iter()
            .map(|row| row.get("id"))
            .collect();

        let now = current_timestamp_seconds();

        // Upsert models
        for model in &models {
            // Ensure params is always a valid JSON object
            let params_json = model
                .meta
                .as_ref()
                .map(|m| m.clone())
                .unwrap_or_else(|| serde_json::json!({}));

            // Ensure meta is always a valid JSON object
            let meta_json = model
                .meta
                .as_ref()
                .map(|m| m.clone())
                .unwrap_or_else(|| serde_json::json!({}));

            if existing.contains(&model.id) {
                // Update existing
                sqlx::query(
                    r#"
                    UPDATE model
                    SET name = $1, params = $2, meta = $3, updated_at = $4, access_control = $5
                    WHERE id = $6
                    "#,
                )
                .bind(&model.name)
                .bind(&model.params)
                .bind(&meta_json)
                .bind(now)
                .bind(&model.access_control)
                .bind(&model.id)
                .execute(&self.db.pool)
                .await?;
            } else {
                // Insert new
                sqlx::query(
                    r#"
                    INSERT INTO model (id, user_id, base_model_id, name, params, meta, access_control, created_at, updated_at, is_active)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    "#,
                )
                .bind(&model.id)
                .bind(user_id)
                .bind(&model.base_model_id)
                .bind(&model.name)
                .bind(&model.params)
                .bind(&meta_json)
                .bind(&model.access_control)
                .bind(now)
                .bind(now)
                .bind(true)
                .execute(&self.db.pool)
                .await?;
            }
        }

        // Remove models not in the sync list
        let model_ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        if !model_ids.is_empty() {
            let placeholders = vec!["$1"; model_ids.len()].join(", ");
            let delete_query = format!("DELETE FROM model WHERE id NOT IN ({})", placeholders);

            let mut query = sqlx::query(&delete_query);
            for id in &model_ids {
                query = query.bind(id);
            }
            query.execute(&self.db.pool).await?;
        }

        self.get_all_models().await
    }

    // New helper methods for models router
    pub async fn get_models(&self) -> AppResult<Vec<Model>> {
        self.get_all_models().await
    }

    pub async fn insert_new_model(&self, form: ModelForm, user_id: &str) -> AppResult<Model> {
        let now = current_timestamp_seconds();

        // Ensure params is always a valid JSON object
        let params_json = if form.params.is_null() {
            serde_json::json!({})
        } else {
            form.params
        };

        // Ensure meta is always a valid JSON object
        let meta_json = if form.meta.is_null() {
            serde_json::json!({})
        } else {
            form.meta
        };

        sqlx::query(
            r#"
            INSERT INTO model (id, user_id, base_model_id, name, params, meta, access_control, created_at, updated_at, is_active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(&form.id)
        .bind(user_id)
        .bind(&form.base_model_id)
        .bind(&form.name)
        .bind(&params_json)
        .bind(&meta_json)
        .bind(&form.access_control)
        .bind(now)
        .bind(now)
        .bind(true)
        .execute(&self.db.pool)
        .await?;

        self.get_model_by_id(&form.id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create model".to_string()))
    }

    pub async fn update_model_by_id(&self, id: &str, form: ModelForm) -> AppResult<Model> {
        let now = current_timestamp_seconds();

        // Ensure params is always a valid JSON object
        let params_json = if form.params.is_null() {
            serde_json::json!({})
        } else {
            form.params
        };

        // Ensure meta is always a valid JSON object
        let meta_json = if form.meta.is_null() {
            serde_json::json!({})
        } else {
            form.meta
        };

        sqlx::query(
            r#"
            UPDATE model
            SET base_model_id = $1, name = $2, params = $3, meta = $4, access_control = $5, updated_at = $6
            WHERE id = $7
            "#,
        )
        .bind(&form.base_model_id)
        .bind(&form.name)
        .bind(&params_json)
        .bind(&meta_json)
        .bind(&form.access_control)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_model_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Model not found".to_string()))
    }

    pub async fn toggle_model_by_id(&self, id: &str) -> AppResult<Model> {
        self.toggle_model_active(id).await
    }

    pub async fn delete_model_by_id(&self, id: &str) -> AppResult<bool> {
        self.delete_model(id).await?;
        Ok(true)
    }
}
