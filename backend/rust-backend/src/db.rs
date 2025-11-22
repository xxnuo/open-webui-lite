use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::str::FromStr;
use std::time::Duration;

// Load SQLite schema from external file
const SQLITE_SCHEMA: &str = include_str!("schema.sql");

#[derive(Clone)]
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let connect_options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(3600))
            .connect_with(connect_options)
            .await?;

        Ok(Database { pool })
    }

    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        tracing::info!("Initializing database schema");

        // Parse and execute all SQL statements
        let statements = Self::parse_sql_statements(SQLITE_SCHEMA);
        for (idx, statement) in statements.iter().enumerate() {
            let trimmed = statement.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("--") {
                if let Err(e) = sqlx::query(trimmed).execute(&self.pool).await {
                    tracing::warn!(
                        "Error executing statement {}: {} - Error: {}",
                        idx + 1,
                        trimmed.chars().take(100).collect::<String>(),
                        e
                    );
                }
            }
        }

        tracing::info!("Database schema initialization completed");
        Ok(())
    }

    /// Parse SQL statements from schema
    fn parse_sql_statements(sql: &str) -> Vec<String> {
        let mut statements = Vec::new();
        let mut current_statement = String::new();

        for line in sql.lines() {
            let trimmed_line = line.trim();

            // Skip empty lines and comments at the start
            if current_statement.is_empty()
                && (trimmed_line.is_empty() || trimmed_line.starts_with("--"))
            {
                continue;
            }

            current_statement.push_str(line);
            current_statement.push('\n');

            if trimmed_line.ends_with(';') {
                statements.push(current_statement.clone());
                current_statement.clear();
            }
        }

        // Add any remaining statement
        if !current_statement.trim().is_empty() {
            statements.push(current_statement);
        }

        statements
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // User methods
    pub async fn get_user_by_id(
        &self,
        user_id: &str,
    ) -> Result<crate::models::user::User, sqlx::Error> {
        let user: crate::models::user::User = sqlx::query_as(
            r#"
            SELECT id, name, email, username, role, profile_image_url, bio, gender, 
                   date_of_birth, info, settings,
                   api_key, oauth_sub, last_active_at, updated_at, created_at
            FROM "user" 
            WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn get_all_users(&self) -> Result<Vec<crate::models::user::User>, sqlx::Error> {
        let users: Vec<crate::models::user::User> = sqlx::query_as(
            r#"
            SELECT id, name, email, username, role, profile_image_url, bio, gender, 
                   date_of_birth, info, settings,
                   api_key, oauth_sub, last_active_at, updated_at, created_at
            FROM "user"
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(users)
    }

    // Group methods
    pub async fn get_group_by_id(
        &self,
        group_id: &str,
    ) -> Result<crate::models::group::Group, sqlx::Error> {
        let group: crate::models::group::Group = sqlx::query_as(
            r#"
            SELECT id, user_id, name, description, 
                   permissions, user_ids, meta, created_at, updated_at
            FROM "group" 
            WHERE id = $1
            "#,
        )
        .bind(group_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(group)
    }

    pub async fn get_all_groups(&self) -> Result<Vec<crate::models::group::Group>, sqlx::Error> {
        let groups: Vec<crate::models::group::Group> = sqlx::query_as(
            r#"
            SELECT id, user_id, name, description, 
                   permissions, user_ids, meta, created_at, updated_at
            FROM "group"
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(groups)
    }

    // Model methods
    pub async fn get_model_by_id(
        &self,
        model_id: &str,
    ) -> Result<crate::models::model::Model, sqlx::Error> {
        let model: crate::models::model::Model = sqlx::query_as(
            r#"
            SELECT id, user_id, base_model_id, name, 
                   params, meta, access_control,
                   is_active, created_at, updated_at
            FROM model 
            WHERE id = $1
            "#,
        )
        .bind(model_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(model)
    }

    pub async fn get_all_models(&self) -> Result<Vec<crate::models::model::Model>, sqlx::Error> {
        let models: Vec<crate::models::model::Model> = sqlx::query_as(
            r#"
            SELECT id, user_id, base_model_id, name, 
                   params, meta, access_control,
                   is_active, created_at, updated_at
            FROM model
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(models)
    }
}
