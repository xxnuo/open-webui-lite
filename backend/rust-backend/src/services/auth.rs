use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::Auth;
use crate::utils::password::{hash_password, verify_password};
use crate::utils::time::current_timestamp_seconds;

pub struct AuthService<'a> {
    db: &'a Database,
}

impl<'a> AuthService<'a> {
    pub fn new(db: &'a Database) -> Self {
        AuthService { db }
    }

    pub async fn create_auth(&self, id: &str, email: &str, password: &str) -> AppResult<()> {
        let password_hash = hash_password(password)?;
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            INSERT INTO auth (id, email, password, active, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(email)
        .bind(password_hash)
        .bind(true)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn get_auth_by_email(&self, email: &str) -> AppResult<Option<Auth>> {
        let result = sqlx::query_as::<_, Auth>(
            r#"
            SELECT id, email, password, active, created_at, updated_at
            FROM auth
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn authenticate(&self, email: &str, password: &str) -> AppResult<Option<String>> {
        let auth = self.get_auth_by_email(email).await?;

        if let Some(auth) = auth {
            if !auth.active {
                return Err(AppError::Unauthorized("Account is not active".to_string()));
            }

            if verify_password(password, &auth.password)? {
                Ok(Some(auth.id))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    #[allow(dead_code)]
    pub async fn update_password(&self, id: &str, new_password: &str) -> AppResult<()> {
        let password_hash = hash_password(new_password)?;

        sqlx::query(
            r#"
            UPDATE auth
            SET password = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(password_hash)
        .bind(current_timestamp_seconds())
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn delete_auth(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM auth WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }
}
