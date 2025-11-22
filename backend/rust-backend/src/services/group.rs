use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::group::{Group, GroupForm, GroupUpdateForm};
use crate::utils::time::current_timestamp_seconds;

pub struct GroupService<'a> {
    db: &'a Database,
}

impl<'a> GroupService<'a> {
    pub fn new(db: &'a Database) -> Self {
        GroupService { db }
    }

    pub async fn insert_new_group(&self, user_id: &str, form_data: &GroupForm) -> AppResult<Group> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = current_timestamp_seconds();

        let permissions_json = form_data
            .permissions
            .as_ref()
            .map(|p| serde_json::to_string(p).ok())
            .flatten();

        // Initialize all JSONB fields with proper defaults to match Python backend behavior
        // Python GroupModel defaults: user_ids=[], meta=None
        sqlx::query(
            r#"
            INSERT INTO "group" (id, user_id, name, description, meta, permissions, user_ids, created_at, updated_at)
            VALUES ($1, $2, $3, $4, NULL, $5, '[]', $6, $7)
            "#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(&form_data.name)
        .bind(&form_data.description)
        .bind(&permissions_json)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_group_by_id(&id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create group".to_string()))
    }

    pub async fn get_group_by_id(&self, id: &str) -> AppResult<Option<Group>> {
        let mut result = sqlx::query_as::<_, Group>(
            r#"
            SELECT id, user_id, name, description,
                   NULL as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(permissions AS TEXT) as permissions_str,
                   COALESCE(CAST(user_ids AS TEXT), '[]') as user_ids_str,
                   created_at, updated_at
            FROM "group"
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        // Parse JSON fields if group exists
        if let Some(ref mut group) = result {
            group.parse_json_fields();
        }

        Ok(result)
    }

    pub async fn get_all_groups(&self) -> AppResult<Vec<Group>> {
        let mut groups = sqlx::query_as::<_, Group>(
            r#"
            SELECT id, user_id, name, description,
                   NULL as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(permissions AS TEXT) as permissions_str,
                   COALESCE(CAST(user_ids AS TEXT), '[]') as user_ids_str,
                   created_at, updated_at
            FROM "group"
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        // Parse JSON fields for each group
        for group in &mut groups {
            group.parse_json_fields();
        }

        Ok(groups)
    }

    pub async fn get_groups_by_member_id(&self, user_id: &str) -> AppResult<Vec<Group>> {
        let search_pattern = format!("%\"{}\"%", user_id);
        tracing::info!(
            "get_groups_by_member_id: user_id={}, search_pattern={}",
            user_id,
            search_pattern
        );

        // First, let's see ALL groups in the database
        let all_groups = sqlx::query_as::<_, Group>(
            r#"
            SELECT id, user_id, name, description,
                   NULL as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(permissions AS TEXT) as permissions_str,
                   COALESCE(CAST(user_ids AS TEXT), '[]') as user_ids_str,
                   created_at, updated_at
            FROM "group"
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        tracing::info!("  Total groups in database: {}", all_groups.len());
        for group in &all_groups {
            tracing::debug!(
                "    Group '{}' (id={}): user_ids_str={}",
                group.name,
                group.id,
                group.user_ids_str.as_ref().unwrap_or(&"NULL".to_string())
            );
        }

        // Now execute the filtered query
        let mut groups = sqlx::query_as::<_, Group>(
            r#"
            SELECT id, user_id, name, description,
                   NULL as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(permissions AS TEXT) as permissions_str,
                   COALESCE(CAST(user_ids AS TEXT), '[]') as user_ids_str,
                   created_at, updated_at
            FROM "group"
            WHERE user_ids IS NOT NULL
              AND user_ids != '[]'
              AND CAST(user_ids AS TEXT) LIKE $1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(&search_pattern)
        .fetch_all(&self.db.pool)
        .await?;

        tracing::info!(
            "  Groups matching pattern '{}': {}",
            search_pattern,
            groups.len()
        );

        // Parse JSON fields for each group
        for group in &mut groups {
            tracing::debug!(
                "  Before parse - Group '{}': user_ids_str={:?}",
                group.name,
                group.user_ids_str
            );
            group.parse_json_fields();
            tracing::debug!(
                "  After parse - Group '{}': user_ids={:?}",
                group.name,
                group.user_ids
            );
        }

        Ok(groups)
    }

    pub async fn update_group_by_id(
        &self,
        id: &str,
        form_data: &GroupUpdateForm,
    ) -> AppResult<Group> {
        let now = current_timestamp_seconds();
        let group = self
            .get_group_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Group not found".to_string()))?;

        let name = form_data.name.as_ref().unwrap_or(&group.name);
        let description = form_data.description.as_ref().unwrap_or(&group.description);

        let permissions_json = form_data
            .permissions
            .as_ref()
            .or(group.permissions.as_ref())
            .map(|p| serde_json::to_string(p).ok())
            .flatten();

        let user_ids_json = form_data
            .user_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).ok())
            .flatten()
            .or_else(|| serde_json::to_string(&group.user_ids).ok());

        sqlx::query(
            r#"
            UPDATE "group"
            SET name = $1, description = $2, permissions = $3, user_ids = $4, updated_at = $5
            WHERE id = $6
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(&permissions_json)
        .bind(&user_ids_json)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_group_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Group not found".to_string()))
    }

    pub async fn add_users_to_group(&self, id: &str, user_ids: &[String]) -> AppResult<Group> {
        let mut group = self
            .get_group_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Group not found".to_string()))?;

        group.parse_json_fields();
        let mut group_user_ids = group.user_ids.clone();

        // Add new users (avoiding duplicates)
        for user_id in user_ids {
            if !group_user_ids.contains(user_id) {
                group_user_ids.push(user_id.clone());
            }
        }

        let now = current_timestamp_seconds();
        let user_ids_json = serde_json::to_string(&group_user_ids).ok();

        sqlx::query(
            r#"
            UPDATE "group"
            SET user_ids = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(&user_ids_json)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_group_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Group not found".to_string()))
    }

    pub async fn remove_users_from_group(&self, id: &str, user_ids: &[String]) -> AppResult<Group> {
        let mut group = self
            .get_group_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Group not found".to_string()))?;

        group.parse_json_fields();
        let mut group_user_ids = group.user_ids.clone();

        // Remove users
        for user_id in user_ids {
            group_user_ids.retain(|uid| uid != user_id);
        }

        let now = current_timestamp_seconds();
        let user_ids_json = serde_json::to_string(&group_user_ids).ok();

        sqlx::query(
            r#"
            UPDATE "group"
            SET user_ids = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(&user_ids_json)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_group_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Group not found".to_string()))
    }

    pub async fn delete_group_by_id(&self, id: &str) -> AppResult<bool> {
        let result = sqlx::query(r#"DELETE FROM "group" WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
