use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::function::Function;
use crate::utils::time::current_timestamp_seconds;

#[allow(dead_code)]
pub struct FunctionService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> FunctionService<'a> {
    pub fn new(db: &'a Database) -> Self {
        FunctionService { db }
    }

    pub async fn create_function(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        type_: &str,
        content: &str,
        meta: serde_json::Value,
        is_active: bool,
        is_global: bool,
    ) -> AppResult<Function> {
        let now = current_timestamp_seconds();
        let meta_str = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            INSERT INTO function (id, user_id, name, type, content, meta, is_active, is_global, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(type_)
        .bind(content)
        .bind(&meta_str)
        .bind(is_active)
        .bind(is_global)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_function_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create function".to_string()))
    }

    pub async fn get_function_by_id(&self, id: &str) -> AppResult<Option<Function>> {
        let result = sqlx::query_as::<_, Function>(
            r#"
            SELECT id, user_id, name, type, content, meta, is_active, is_global, created_at, updated_at
            FROM function
            WHERE id = $8
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_functions_by_user_id(&self, user_id: &str) -> AppResult<Vec<Function>> {
        let functions = sqlx::query_as::<_, Function>(
            r#"
            SELECT id, user_id, name, type, content, meta, is_active, is_global, created_at, updated_at
            FROM function
            WHERE user_id = $6
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        Ok(functions)
    }

    pub async fn get_all_functions(&self) -> AppResult<Vec<Function>> {
        let functions = sqlx::query_as::<_, Function>(
            r#"
            SELECT id, user_id, name, type, content, meta, is_active, is_global, created_at, updated_at
            FROM function
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(functions)
    }

    pub async fn get_global_functions(&self) -> AppResult<Vec<Function>> {
        let functions = sqlx::query_as::<_, Function>(
            r#"
            SELECT id, user_id, name, type, content, meta, is_active, is_global, created_at, updated_at
            FROM function
            WHERE is_global = 1
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        Ok(functions)
    }

    pub async fn update_function(
        &self,
        id: &str,
        name: Option<&str>,
        type_: Option<&str>,
        content: Option<&str>,
        meta: Option<serde_json::Value>,
        is_active: bool,
        is_global: bool,
    ) -> AppResult<Function> {
        let now = current_timestamp_seconds();

        // Build SET clause dynamically
        let mut set_clauses = vec!["updated_at = $1"];
        let mut param_count = 2;

        let name_param = name.map(|_| {
            let p = param_count;
            param_count += 1;
            format!("name = ${}", p)
        });
        let type_param = type_.map(|_| {
            let p = param_count;
            param_count += 1;
            format!("type = ${}", p)
        });
        let content_param = content.map(|_| {
            let p = param_count;
            param_count += 1;
            format!("content = ${}", p)
        });
        let meta_param = meta.as_ref().map(|_| {
            let p = param_count;
            param_count += 1;
            format!("meta = ${}", p)
        });
        let is_active_param = {
            let p = param_count;
            param_count += 1;
            format!("is_active = ${}", p)
        };
        let is_global_param = {
            let p = param_count;
            param_count += 1;
            format!("is_global = ${}", p)
        };

        if let Some(ref clause) = name_param {
            set_clauses.push(clause);
        }
        if let Some(ref clause) = type_param {
            set_clauses.push(clause);
        }
        if let Some(ref clause) = content_param {
            set_clauses.push(clause);
        }
        if let Some(ref clause) = meta_param {
            set_clauses.push(clause);
        }
        set_clauses.push(&is_active_param);
        set_clauses.push(&is_global_param);

        let query_str = format!(
            "UPDATE function SET {} WHERE id = ${}",
            set_clauses.join(", "),
            param_count
        );

        let mut query = sqlx::query(&query_str);
        query = query.bind(now);
        if let Some(n) = name {
            query = query.bind(n);
        }
        if let Some(t) = type_ {
            query = query.bind(t);
        }
        if let Some(c) = content {
            query = query.bind(c);
        }
        if let Some(m) = meta {
            query = query.bind(serde_json::to_string(&m).unwrap());
        }
        query = query.bind(is_active);
        query = query.bind(is_global);
        query = query.bind(id);

        query.execute(&self.db.pool).await?;

        self.get_function_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Function not found".to_string()))
    }

    pub async fn update_function_valves(
        &self,
        id: &str,
        valves: serde_json::Value,
    ) -> AppResult<()> {
        let valves_str = serde_json::to_string(&valves).unwrap_or_else(|_| "{}".to_string());

        sqlx::query(
            r#"
            UPDATE function
            SET valves = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(&valves_str)
        .bind(current_timestamp_seconds())
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        Ok(())
    }

    pub async fn toggle_function_active(&self, id: &str) -> AppResult<Function> {
        sqlx::query(
            r#"
            UPDATE function
            SET is_active = CASE WHEN is_active = 1 THEN 0 ELSE 1 END
            WHERE id = $4
            "#,
        )
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_function_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Function not found".to_string()))
    }

    pub async fn delete_function(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM function WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_functions_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM function WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }
}
