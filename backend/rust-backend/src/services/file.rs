use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::file::File;
use crate::utils::time::current_timestamp_seconds;

#[allow(dead_code)]
pub struct FileService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> FileService<'a> {
    pub fn new(db: &'a Database) -> Self {
        FileService { db }
    }

    pub async fn create_file(
        &self,
        id: &str,
        user_id: &str,
        filename: &str,
        path: &str,
        meta: Option<serde_json::Value>,
    ) -> AppResult<File> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            INSERT INTO file (id, user_id, filename, path, meta, hash, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(filename)
        .bind(path)
        .bind(&meta)
        .bind(None::<String>)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_file_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create file".to_string()))
    }

    pub async fn get_file_by_id(&self, id: &str) -> AppResult<Option<File>> {
        let result = sqlx::query_as::<_, File>(
            r#"
            SELECT id, user_id, filename, path,
                   data as data_str, meta as meta_str, access_control as access_control_str,
                   hash, created_at, updated_at
            FROM file
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_file_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
    ) -> AppResult<Option<File>> {
        let result = sqlx::query_as::<_, File>(
            r#"
            SELECT id, user_id, filename, path,
                   data as data_str, meta as meta_str, access_control as access_control_str,
                   hash, created_at, updated_at
            FROM file
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_files_by_user_id(&self, user_id: &str) -> AppResult<Vec<File>> {
        let files = sqlx::query_as::<_, File>(
            r#"
            SELECT id, user_id, filename, path, 
                   data as data_str, meta as meta_str, access_control as access_control_str,
                   hash, created_at, updated_at
            FROM file
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(files)
    }

    pub async fn get_all_files(&self) -> AppResult<Vec<File>> {
        let files = sqlx::query_as::<_, File>(
            r#"
            SELECT id, user_id, filename, path,
                   data as data_str, meta as meta_str, access_control as access_control_str,
                   hash, created_at, updated_at
            FROM file
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(files)
    }

    pub async fn update_file_metadata(&self, id: &str, meta: serde_json::Value) -> AppResult<File> {
        let now = current_timestamp_seconds();
        let meta_str = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            UPDATE file
            SET meta = $6, updated_at = $5
            WHERE id = $4
            "#,
        )
        .bind(&meta_str)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_file_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("File not found".to_string()))
    }

    pub async fn delete_file(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM file WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_file_by_id_and_user_id(&self, id: &str, user_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM file WHERE id = $2 AND user_id = $1")
            .bind(id)
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_all_files_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM file WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_all_files(&self) -> AppResult<()> {
        sqlx::query("DELETE FROM file")
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn update_file_data(&self, id: &str, data: serde_json::Value) -> AppResult<File> {
        let now = current_timestamp_seconds();
        let data_str = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            UPDATE file
            SET data = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(&data_str)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_file_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("File not found".to_string()))
    }

    pub async fn search_files_by_pattern(
        &self,
        user_id: Option<&str>,
        pattern: &str,
    ) -> AppResult<Vec<File>> {
        let search_pattern = format!("%{}%", pattern);

        let files = if let Some(uid) = user_id {
            sqlx::query_as::<_, File>(
                r#"
                SELECT id, user_id, filename, path,
                       data as data_str, meta as meta_str, access_control as access_control_str,
                       hash, created_at, updated_at
                FROM file
                WHERE user_id = $1 AND filename LIKE $2
                ORDER BY created_at DESC
                "#,
            )
            .bind(uid)
            .bind(&search_pattern)
            .fetch_all(&self.db.pool)
            .await?
        } else {
            sqlx::query_as::<_, File>(
                r#"
                SELECT id, user_id, filename, path,
                       data as data_str, meta as meta_str, access_control as access_control_str,
                       hash, created_at, updated_at
                FROM file
                WHERE filename LIKE $1
                ORDER BY created_at DESC
                "#,
            )
            .bind(&search_pattern)
            .fetch_all(&self.db.pool)
            .await?
        };

        Ok(files)
    }

    pub async fn get_files_by_ids(&self, ids: &[String]) -> AppResult<Vec<File>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build placeholders for IN clause
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
        let query_str = format!(
            r#"
            SELECT id, user_id, filename, path,
                   data as data_str, meta as meta_str, access_control as access_control_str,
                   hash, created_at, updated_at
            FROM file
            WHERE id IN ({})
            ORDER BY updated_at DESC
            "#,
            placeholders.join(", ")
        );

        let mut query = sqlx::query_as::<_, File>(&query_str);
        for id in ids {
            query = query.bind(id);
        }

        let files = query.fetch_all(&self.db.pool).await?;
        Ok(files)
    }

    pub async fn get_file_metadatas_by_ids(
        &self,
        ids: &[String],
    ) -> AppResult<Vec<serde_json::Value>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build placeholders for IN clause
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
        let query_str = format!(
            r#"
            SELECT id, meta as meta_str, created_at, updated_at
            FROM file
            WHERE id IN ({})
            ORDER BY updated_at DESC
            "#,
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&query_str);
        for id in ids {
            query = query.bind(id);
        }

        let rows = query.fetch_all(&self.db.pool).await?;

        let mut metadatas = Vec::new();
        for row in rows {
            use sqlx::Row;
            let id: String = row.get("id");
            let meta_str: Option<String> = row.get("meta_str");
            let created_at: i64 = row.get("created_at");
            let updated_at: i64 = row.get("updated_at");

            let meta: serde_json::Value = meta_str
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_else(|| serde_json::json!({}));

            metadatas.push(serde_json::json!({
                "id": id,
                "meta": meta,
                "created_at": created_at,
                "updated_at": updated_at,
            }));
        }

        Ok(metadatas)
    }
}
