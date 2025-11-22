use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::User;
use crate::utils::time::current_timestamp_seconds;
use chrono::NaiveDate;
use sqlx::Row;

pub struct UserService<'a> {
    db: &'a Database,
}

impl<'a> UserService<'a> {
    pub fn new(db: &'a Database) -> Self {
        UserService { db }
    }

    pub async fn get_user_by_id(&self, id: &str) -> AppResult<Option<User>> {
        let result = sqlx::query_as::<_, User>(
            r#"
            SELECT id, name, email, username, role, profile_image_url, bio, gender, 
                   date_of_birth, 
                   COALESCE(info, '{}') as info, 
                   COALESCE(settings, '{}') as settings, 
                   api_key, oauth_sub, 
                   last_active_at, updated_at, created_at
            FROM "user"
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_user_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let result = sqlx::query_as::<_, User>(
            r#"
            SELECT id, name, email, username, role, profile_image_url, bio, gender, 
                   date_of_birth, 
                   COALESCE(info, '{}') as info, 
                   COALESCE(settings, '{}') as settings, 
                   api_key, oauth_sub, 
                   last_active_at, updated_at, created_at
            FROM "user"
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn get_user_by_api_key(&self, api_key: &str) -> AppResult<Option<User>> {
        let result = sqlx::query_as::<_, User>(
            r#"
            SELECT id, name, email, username, role, profile_image_url, bio, gender, 
                   date_of_birth, 
                   COALESCE(info, '{}') as info, 
                   COALESCE(settings, '{}') as settings, 
                   api_key, oauth_sub, 
                   last_active_at, updated_at, created_at
            FROM "user"
            WHERE api_key = $1
            "#,
        )
        .bind(api_key)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_first_user(&self) -> AppResult<Option<User>> {
        let result = sqlx::query_as::<_, User>(
            r#"
            SELECT id, name, email, username, role, profile_image_url, bio, gender, 
                   date_of_birth, 
                   COALESCE(info, '{}') as info, 
                   COALESCE(settings, '{}') as settings, 
                   api_key, oauth_sub, 
                   last_active_at, updated_at, created_at
            FROM "user"
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn create_user(
        &self,
        id: &str,
        name: &str,
        email: &str,
        role: &str,
        profile_image_url: &str,
    ) -> AppResult<User> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            INSERT INTO "user" (id, name, email, role, profile_image_url, last_active_at, updated_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(email)
        .bind(role)
        .bind(profile_image_url)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_user_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create user".to_string()))
    }

    #[allow(dead_code)]
    pub async fn update_user_last_active(&self, id: &str) -> AppResult<()> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            UPDATE "user"
            SET last_active_at = $1
            WHERE id = $2
            "#,
        )
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn list_users(&self, skip: i64, limit: i64) -> AppResult<Vec<User>> {
        let users = sqlx::query_as::<_, User>(
            r#"
            SELECT id, name, email, username, role, profile_image_url, bio, gender, 
                   date_of_birth, 
                   COALESCE(info, '{}') as info, 
                   COALESCE(settings, '{}') as settings, 
                   api_key, oauth_sub, 
                   last_active_at, updated_at, created_at
            FROM "user"
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(skip)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(users)
    }

    pub async fn count_users(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query("SELECT COUNT(*) as count FROM \"user\"")
            .fetch_one(&self.db.pool)
            .await?
            .try_get("count")?;

        Ok(count)
    }

    pub async fn update_user_role(&self, id: &str, role: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE "user"
            SET role = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(role)
        .bind(current_timestamp_seconds())
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_user(&self, id: &str) -> AppResult<()> {
        sqlx::query(r#"DELETE FROM "user" WHERE id = $1"#)
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn update_user_settings(
        &self,
        id: &str,
        settings: &serde_json::Value,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE "user"
            SET settings = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(settings)
        .bind(current_timestamp_seconds())
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    /// Update user profile information (name, profile_image_url, bio, gender, date_of_birth)
    pub async fn update_user_profile(
        &self,
        id: &str,
        name: Option<&str>,
        profile_image_url: Option<&str>,
        bio: Option<&str>,
        gender: Option<&str>,
        date_of_birth: Option<NaiveDate>,
    ) -> AppResult<()> {
        let now = current_timestamp_seconds();

        // Build dynamic SQL query based on which fields are provided
        let mut query_parts = Vec::new();
        let mut bind_index = 1;

        if name.is_some() {
            query_parts.push(format!("name = ${}", bind_index));
            bind_index += 1;
        }
        if profile_image_url.is_some() {
            query_parts.push(format!("profile_image_url = ${}", bind_index));
            bind_index += 1;
        }
        if bio.is_some() {
            query_parts.push(format!("bio = ${}", bind_index));
            bind_index += 1;
        }
        if gender.is_some() {
            query_parts.push(format!("gender = ${}", bind_index));
            bind_index += 1;
        }
        if date_of_birth.is_some() {
            query_parts.push(format!("date_of_birth = ${}", bind_index));
            bind_index += 1;
        }

        // Always update updated_at
        query_parts.push(format!("updated_at = ${}", bind_index));
        bind_index += 1;

        if query_parts.len() == 1 {
            // Only updated_at would be updated, nothing to do
            return Ok(());
        }

        let query_str = format!(
            r#"UPDATE "user" SET {} WHERE id = ${}"#,
            query_parts.join(", "),
            bind_index
        );

        let mut query = sqlx::query(&query_str);

        if let Some(n) = name {
            query = query.bind(n);
        }
        if let Some(p) = profile_image_url {
            query = query.bind(p);
        }
        if let Some(b) = bio {
            query = query.bind(b);
        }
        if let Some(g) = gender {
            query = query.bind(g);
        }
        if let Some(d) = date_of_birth {
            query = query.bind(d);
        }

        query = query.bind(now);
        query = query.bind(id);

        query.execute(&self.db.pool).await?;

        Ok(())
    }

    pub async fn get_user_count(&self) -> AppResult<i64> {
        let result = sqlx::query("SELECT COUNT(*) as count FROM \"user\"")
            .fetch_one(&self.db.pool)
            .await?;

        let count: i64 = result.try_get("count")?;
        Ok(count)
    }

    pub async fn get_valid_user_ids(&self, user_ids: &[String]) -> AppResult<Vec<String>> {
        if user_ids.is_empty() {
            return Ok(vec![]);
        }

        // Build IN clause for SQLite
        let placeholders = user_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(r#"SELECT id FROM "user" WHERE id IN ({})"#, placeholders);

        let mut q = sqlx::query_as(&query);
        for id in user_ids {
            q = q.bind(id);
        }

        let result: Vec<(String,)> = q.fetch_all(&self.db.pool).await?;

        Ok(result.into_iter().map(|(id,)| id).collect())
    }
}
