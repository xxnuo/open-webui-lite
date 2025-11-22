use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::tool::Tool;
use crate::utils::time::current_timestamp_seconds;

#[allow(dead_code)]
pub struct ToolService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> ToolService<'a> {
    pub fn new(db: &'a Database) -> Self {
        ToolService { db }
    }

    pub async fn create_tool(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        content: &str,
        specs: serde_json::Value,
        meta: serde_json::Value,
        access_control: Option<serde_json::Value>,
    ) -> AppResult<Tool> {
        let now = current_timestamp_seconds();

        let specs_str = serde_json::to_string(&specs).unwrap();
        let meta_str = serde_json::to_string(&meta).unwrap();
        let access_control_str = access_control
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap());

        sqlx::query(
            r#"
            INSERT INTO tool (id, user_id, name, content, specs, meta, access_control, is_active, valves, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(content)
        .bind(&specs_str)
        .bind(&meta_str)
        .bind(&access_control_str)
        .bind(true)
        .bind("{}")
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_tool_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create tool".to_string()))
    }

    pub async fn get_tool_by_id(&self, id: &str) -> AppResult<Option<Tool>> {
        let mut result = sqlx::query_as::<_, Tool>(
            r#"
            SELECT id, user_id, name, content, is_active,
                   CAST(specs AS TEXT) as specs_str, 
                   CAST(meta AS TEXT) as meta_str, 
                   CAST(access_control AS TEXT) as access_control_str,
                   CAST(valves AS TEXT) as valves_str,
                   created_at, updated_at
            FROM tool
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(ref mut tool) = result {
            tool.parse_specs();
            tool.parse_meta();
            tool.parse_access_control();
            tool.parse_valves();
        }

        Ok(result)
    }

    pub async fn get_tools_by_user_id(&self, user_id: &str) -> AppResult<Vec<Tool>> {
        let mut tools = sqlx::query_as::<_, Tool>(
            r#"
            SELECT id, user_id, name, content, is_active,
                   CAST(specs AS TEXT) as specs_str, 
                   CAST(meta AS TEXT) as meta_str, 
                   CAST(access_control AS TEXT) as access_control_str,
                   CAST(valves AS TEXT) as valves_str,
                   created_at, updated_at
            FROM tool
            WHERE user_id = $1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        for tool in &mut tools {
            tool.parse_specs();
            tool.parse_meta();
            tool.parse_access_control();
            tool.parse_valves();
        }

        Ok(tools)
    }

    pub async fn get_all_tools(&self) -> AppResult<Vec<Tool>> {
        let mut tools = sqlx::query_as::<_, Tool>(
            r#"
            SELECT id, user_id, name, content, is_active,
                   CAST(specs AS TEXT) as specs_str, 
                   CAST(meta AS TEXT) as meta_str, 
                   CAST(access_control AS TEXT) as access_control_str,
                   CAST(valves AS TEXT) as valves_str,
                   created_at, updated_at
            FROM tool
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        for tool in &mut tools {
            tool.parse_specs();
            tool.parse_meta();
            tool.parse_access_control();
            tool.parse_valves();
        }

        Ok(tools)
    }

    pub async fn update_tool(
        &self,
        id: &str,
        name: Option<&str>,
        content: Option<&str>,
        specs: Option<serde_json::Value>,
        meta: Option<serde_json::Value>,
        access_control: Option<serde_json::Value>,
    ) -> AppResult<Tool> {
        let now = current_timestamp_seconds();

        // Build dynamic query parts
        let mut updates = vec!["updated_at = $1".to_string()];
        let mut bind_count = 2;

        if name.is_some() {
            updates.push(format!("name = ${}", bind_count));
            bind_count += 1;
        }
        if content.is_some() {
            updates.push(format!("content = ${}", bind_count));
            bind_count += 1;
        }
        if specs.is_some() {
            updates.push(format!("specs = ${}", bind_count));
            bind_count += 1;
        }
        if meta.is_some() {
            updates.push(format!("meta = ${}", bind_count));
            bind_count += 1;
        }
        if access_control.is_some() {
            updates.push(format!("access_control = ${}", bind_count));
            bind_count += 1;
        }

        let query_str = format!(
            "UPDATE tool SET {} WHERE id = ${}",
            updates.join(", "),
            bind_count
        );

        let mut query = sqlx::query(&query_str);
        query = query.bind(now);

        if let Some(n) = name {
            query = query.bind(n);
        }
        if let Some(c) = content {
            query = query.bind(c);
        }
        if let Some(s) = specs {
            query = query.bind(serde_json::to_string(&s).unwrap());
        }
        if let Some(m) = meta {
            query = query.bind(serde_json::to_string(&m).unwrap());
        }
        if let Some(ac) = access_control {
            query = query.bind(serde_json::to_string(&ac).unwrap());
        }

        query = query.bind(id);

        query.execute(&self.db.pool).await?;

        self.get_tool_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Tool not found".to_string()))
    }

    pub async fn update_tool_valves(&self, id: &str, valves: serde_json::Value) -> AppResult<()> {
        let now = current_timestamp_seconds();
        let valves_str = serde_json::to_string(&valves).unwrap();

        sqlx::query("UPDATE tool SET valves = $1, updated_at = $2 WHERE id = $3")
            .bind(&valves_str)
            .bind(now)
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_tool(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM tool WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_tools_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM tool WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }
}
