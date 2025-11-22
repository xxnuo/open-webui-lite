use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::prompt::{Prompt, PromptForm};
use crate::utils::time::current_timestamp_seconds;

pub struct PromptService<'a> {
    db: &'a Database,
}

impl<'a> PromptService<'a> {
    pub fn new(db: &'a Database) -> Self {
        PromptService { db }
    }

    pub async fn insert_new_prompt(
        &self,
        user_id: &str,
        form_data: &PromptForm,
    ) -> AppResult<Prompt> {
        let now = current_timestamp_seconds();

        let access_control_json = form_data
            .access_control
            .as_ref()
            .map(|ac| serde_json::to_string(ac).ok())
            .flatten();

        sqlx::query(
            r#"
            INSERT INTO prompt (command, user_id, title, content, access_control, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $6)
            "#,
        )
        .bind(&form_data.command)
        .bind(user_id)
        .bind(&form_data.title)
        .bind(&form_data.content)
        .bind(&access_control_json)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_prompt_by_command(&form_data.command)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create prompt".to_string()))
    }

    pub async fn get_prompt_by_command(&self, command: &str) -> AppResult<Option<Prompt>> {
        let result = sqlx::query_as::<_, Prompt>(
            r#"
            SELECT command, user_id, title, content, updated_at as timestamp,
                   CAST(access_control AS TEXT) as access_control_str
            FROM prompt
            WHERE command = $1
            "#,
        )
        .bind(command)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_all_prompts(&self) -> AppResult<Vec<Prompt>> {
        let prompts = sqlx::query_as::<_, Prompt>(
            r#"
            SELECT command, user_id, title, content, updated_at as timestamp,
                   CAST(access_control AS TEXT) as access_control_str
            FROM prompt
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(prompts)
    }

    pub async fn update_prompt_by_command(
        &self,
        command: &str,
        form_data: &PromptForm,
    ) -> AppResult<Prompt> {
        let now = current_timestamp_seconds();

        let access_control_json = form_data
            .access_control
            .as_ref()
            .map(|ac| serde_json::to_string(ac).ok())
            .flatten();

        sqlx::query(
            r#"
            UPDATE prompt
            SET title = $1, content = $2, access_control = $3, updated_at = $4
            WHERE command = $5
            "#,
        )
        .bind(&form_data.title)
        .bind(&form_data.content)
        .bind(&access_control_json)
        .bind(now)
        .bind(command)
        .execute(&self.db.pool)
        .await?;

        self.get_prompt_by_command(command)
            .await?
            .ok_or_else(|| AppError::NotFound("Prompt not found".to_string()))
    }

    pub async fn delete_prompt_by_command(&self, command: &str) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM prompt WHERE command = $1")
            .bind(command)
            .execute(&self.db.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
