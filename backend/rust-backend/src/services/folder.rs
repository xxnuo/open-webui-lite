use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::folder::{Folder, FolderForm, FolderUpdateForm};
use crate::utils::time::current_timestamp_seconds;

pub struct FolderService<'a> {
    db: &'a Database,
}

impl<'a> FolderService<'a> {
    pub fn new(db: &'a Database) -> Self {
        FolderService { db }
    }

    pub async fn insert_new_folder(
        &self,
        user_id: &str,
        form_data: &FolderForm,
    ) -> AppResult<Folder> {
        let id = uuid::Uuid::new_v4().to_string();
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

        sqlx::query(
            r#"
            INSERT INTO folder (id, user_id, name, parent_id, data, meta, is_expanded, created_at, updated_at)
            VALUES ($1, $2, $3, NULL, $4, $5, false, $6, $7)
            "#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(&form_data.name)
        .bind(&data_json)
        .bind(&meta_json)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_folder_by_id_and_user_id(&id, user_id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create folder".to_string()))
    }

    pub async fn get_folder_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
    ) -> AppResult<Option<Folder>> {
        let result = sqlx::query_as::<_, Folder>(
            r#"
            SELECT id, user_id, name, 
                   NULLIF(parent_id, '') as parent_id, 
                   is_expanded, 
                   NULL as items_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(data AS TEXT) as data_str,
                   created_at, updated_at
            FROM folder
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }
    pub async fn get_folders_by_user_id(&self, user_id: &str) -> AppResult<Vec<Folder>> {
        let folders = sqlx::query_as::<_, Folder>(
            r#"
            SELECT id, user_id, name, 
                   NULLIF(parent_id, '') as parent_id, 
                   is_expanded,
                   NULL as items_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(data AS TEXT) as data_str,
                   created_at, updated_at
            FROM folder
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(folders)
    }
    pub async fn get_folder_by_parent_id_and_user_id_and_name(
        &self,
        parent_id: Option<&str>,
        user_id: &str,
        name: &str,
    ) -> AppResult<Option<Folder>> {
        let result = if let Some(parent_id) = parent_id {
            sqlx::query_as::<_, Folder>(
                r#"
                SELECT id, user_id, name, 
                       NULLIF(parent_id, '') as parent_id, 
                       is_expanded,
                       NULL as items_str,
                       CAST(meta AS TEXT) as meta_str,
                       CAST(data AS TEXT) as data_str,
                       created_at, updated_at
                FROM folder
                WHERE parent_id = $1 AND user_id = $2 AND LOWER(name) = LOWER($3)
                "#,
            )
            .bind(parent_id)
            .bind(user_id)
            .bind(name)
            .fetch_optional(&self.db.pool)
            .await?
        } else {
            sqlx::query_as::<_, Folder>(
                r#"
                SELECT id, user_id, name, 
                       NULLIF(parent_id, '') as parent_id, 
                       is_expanded,
                       NULL as items_str,
                       CAST(meta AS TEXT) as meta_str,
                       CAST(data AS TEXT) as data_str,
                       created_at, updated_at
                FROM folder
                WHERE parent_id IS NULL AND user_id = $1 AND LOWER(name) = LOWER($2)
                "#,
            )
            .bind(user_id)
            .bind(name)
            .fetch_optional(&self.db.pool)
            .await?
        };

        Ok(result)
    }

    pub async fn update_folder_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
        form_data: &FolderUpdateForm,
    ) -> AppResult<Folder> {
        let now = current_timestamp_seconds();
        let folder = self
            .get_folder_by_id_and_user_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))?;

        let name = form_data.name.as_ref().unwrap_or(&folder.name);

        // Merge data and meta fields
        let mut data_value = folder.data.clone().unwrap_or(serde_json::json!({}));
        if let Some(new_data) = &form_data.data {
            if let Some(data_obj) = data_value.as_object_mut() {
                if let Some(new_data_obj) = new_data.as_object() {
                    for (k, v) in new_data_obj {
                        data_obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        let mut meta_value = folder.meta.clone().unwrap_or(serde_json::json!({}));
        if let Some(new_meta) = &form_data.meta {
            if let Some(meta_obj) = meta_value.as_object_mut() {
                if let Some(new_meta_obj) = new_meta.as_object() {
                    for (k, v) in new_meta_obj {
                        meta_obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        let data_json = serde_json::to_string(&data_value).ok();
        let meta_json = serde_json::to_string(&meta_value).ok();

        sqlx::query(
            r#"
            UPDATE folder
            SET name = $1, data = $2, meta = $3, updated_at = $4
            WHERE id = $5 AND user_id = $6
            "#,
        )
        .bind(name)
        .bind(&data_json)
        .bind(&meta_json)
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_folder_by_id_and_user_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))
    }

    pub async fn update_folder_parent_id_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
        parent_id: Option<&str>,
    ) -> AppResult<Folder> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            UPDATE folder
            SET parent_id = $1, updated_at = $2
            WHERE id = $3 AND user_id = $4
            "#,
        )
        .bind(parent_id)
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_folder_by_id_and_user_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))
    }

    pub async fn update_folder_is_expanded_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
        is_expanded: bool,
    ) -> AppResult<Folder> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            UPDATE folder
            SET is_expanded = $1, updated_at = $2
            WHERE id = $3 AND user_id = $4
            "#,
        )
        .bind(is_expanded)
        .bind(now)
        .bind(id)
        .bind(user_id)
        .execute(&self.db.pool)
        .await?;

        self.get_folder_by_id_and_user_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))
    }

    pub async fn delete_folder_by_id_and_user_id(
        &self,
        id: &str,
        user_id: &str,
    ) -> AppResult<Vec<String>> {
        let mut folder_ids = Vec::new();

        // Get folder to verify ownership
        let folder = self
            .get_folder_by_id_and_user_id(id, user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Folder not found".to_string()))?;

        folder_ids.push(folder.id.clone());

        // Recursively delete child folders
        self.delete_children_folders(&folder.id, user_id, &mut folder_ids)
            .await?;

        // Delete the folder and all its children
        // Build IN clause for SQLite
        let placeholders = folder_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!("DELETE FROM folder WHERE id IN ({})", placeholders);
        let mut q = sqlx::query(&query);
        for id in &folder_ids {
            q = q.bind(id);
        }
        q.execute(&self.db.pool).await?;

        Ok(folder_ids)
    }

    fn delete_children_folders<'b>(
        &'b self,
        parent_id: &'b str,
        user_id: &'b str,
        folder_ids: &'b mut Vec<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + 'b>> {
        Box::pin(async move {
            let children = sqlx::query_as::<_, Folder>(
                r#"
                SELECT id, user_id, name, 
                       NULLIF(parent_id, '') as parent_id, 
                       is_expanded,
                       NULL as items_str,
                       CAST(meta AS TEXT) as meta_str,
                       CAST(data AS TEXT) as data_str,
                       created_at, updated_at
                FROM folder
                WHERE parent_id = $1 AND user_id = $2
                "#,
            )
            .bind(parent_id)
            .bind(user_id)
            .fetch_all(&self.db.pool)
            .await?;

            for child in children {
                folder_ids.push(child.id.clone());
                self.delete_children_folders(&child.id, user_id, folder_ids)
                    .await?;
            }

            Ok(())
        })
    }

    pub async fn count_chats_by_folder_id_and_user_id(
        &self,
        folder_id: &str,
        user_id: &str,
    ) -> AppResult<i64> {
        let result: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM chat
            WHERE folder_id = $1 AND user_id = $2
            "#,
        )
        .bind(folder_id)
        .bind(user_id)
        .fetch_one(&self.db.pool)
        .await?;

        Ok(result.0)
    }
}
