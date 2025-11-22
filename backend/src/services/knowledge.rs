use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::knowledge::Knowledge;
use crate::utils::time::current_timestamp_seconds;

#[allow(dead_code)]
pub struct KnowledgeService<'a> {
    db: &'a Database,
}

#[allow(dead_code)]
impl<'a> KnowledgeService<'a> {
    pub fn new(db: &'a Database) -> Self {
        KnowledgeService { db }
    }

    pub async fn create_knowledge(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        description: Option<&str>,
        data: Option<serde_json::Value>,
    ) -> AppResult<Knowledge> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            INSERT INTO knowledge (id, user_id, name, description, data, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(description)
        .bind(&data)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_knowledge_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create knowledge".to_string()))
    }
    pub async fn get_knowledge_by_id(&self, id: &str) -> AppResult<Option<Knowledge>> {
        let mut result = sqlx::query_as::<_, Knowledge>(
            r#"
            SELECT 
                id, 
                user_id, 
                name, 
                description, 
                CAST(data AS TEXT) as data_str,
                CAST(meta AS TEXT) as meta_str,
                CAST(access_control AS TEXT) as access_control_str,
                created_at, 
                updated_at
            FROM knowledge
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        if let Some(ref mut knowledge) = result {
            knowledge.parse_json_fields();
        }

        Ok(result)
    }

    pub async fn get_knowledge_by_user_id(&self, user_id: &str) -> AppResult<Vec<Knowledge>> {
        let mut knowledge = sqlx::query_as::<_, Knowledge>(
            r#"
            SELECT 
                id, 
                user_id, 
                name, 
                description, 
                CAST(data AS TEXT) as data_str,
                CAST(meta AS TEXT) as meta_str,
                CAST(access_control AS TEXT) as access_control_str,
                created_at, 
                updated_at
            FROM knowledge
            WHERE user_id = $1
            ORDER BY updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db.pool)
        .await?;

        for k in &mut knowledge {
            k.parse_json_fields();
        }

        Ok(knowledge)
    }

    pub async fn get_all_knowledge(&self) -> AppResult<Vec<Knowledge>> {
        let mut knowledge = sqlx::query_as::<_, Knowledge>(
            r#"
            SELECT 
                id, 
                user_id, 
                name, 
                description, 
                CAST(data AS TEXT) as data_str,
                CAST(meta AS TEXT) as meta_str,
                CAST(access_control AS TEXT) as access_control_str,
                created_at, 
                updated_at
            FROM knowledge
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        for k in &mut knowledge {
            k.parse_json_fields();
        }

        Ok(knowledge)
    }

    pub async fn create_knowledge_with_access_control(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        description: Option<&str>,
        data: Option<serde_json::Value>,
        access_control: Option<serde_json::Value>,
    ) -> AppResult<Knowledge> {
        let now = current_timestamp_seconds();

        sqlx::query(
            r#"
            INSERT INTO knowledge (id, user_id, name, description, data, access_control, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(user_id)
        .bind(name)
        .bind(description)
        .bind(&data)
        .bind(&access_control)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_knowledge_by_id(id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create knowledge".to_string()))
    }

    pub async fn update_knowledge(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        data: Option<serde_json::Value>,
    ) -> AppResult<Knowledge> {
        let now = current_timestamp_seconds();

        let mut updates = vec!["updated_at = $1".to_string()];
        let mut bind_count = 2;

        if name.is_some() {
            updates.push(format!("name = ${}", bind_count));
            bind_count += 1;
        }
        if description.is_some() {
            updates.push(format!("description = ${}", bind_count));
            bind_count += 1;
        }
        if data.is_some() {
            updates.push(format!("data = ${}", bind_count));
            bind_count += 1;
        }

        let query_str = format!(
            "UPDATE knowledge SET {} WHERE id = ${}",
            updates.join(", "),
            bind_count
        );

        let mut query = sqlx::query(&query_str);
        query = query.bind(now);

        if let Some(n) = name {
            query = query.bind(n);
        }
        if let Some(d) = description {
            query = query.bind(d);
        }
        if let Some(ref data_val) = data {
            query = query.bind(data_val);
        }
        query = query.bind(id);

        query.execute(&self.db.pool).await?;

        self.get_knowledge_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Knowledge not found".to_string()))
    }

    pub async fn update_knowledge_full(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        data: Option<serde_json::Value>,
        access_control: Option<serde_json::Value>,
    ) -> AppResult<Knowledge> {
        let now = current_timestamp_seconds();

        let mut updates = vec!["updated_at = $1".to_string()];
        let mut bind_count = 2;

        if name.is_some() {
            updates.push(format!("name = ${}", bind_count));
            bind_count += 1;
        }
        if description.is_some() {
            updates.push(format!("description = ${}", bind_count));
            bind_count += 1;
        }
        if data.is_some() {
            updates.push(format!("data = ${}", bind_count));
            bind_count += 1;
        }
        if access_control.is_some() {
            updates.push(format!("access_control = ${}", bind_count));
            bind_count += 1;
        }

        let query_str = format!(
            "UPDATE knowledge SET {} WHERE id = ${}",
            updates.join(", "),
            bind_count
        );

        let mut query = sqlx::query(&query_str);
        query = query.bind(now);

        if let Some(n) = name {
            query = query.bind(n);
        }
        if let Some(d) = description {
            query = query.bind(d);
        }
        if let Some(ref data_val) = data {
            query = query.bind(data_val);
        }
        if let Some(ref ac) = access_control {
            query = query.bind(ac);
        }
        query = query.bind(id);

        query.execute(&self.db.pool).await?;

        self.get_knowledge_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Knowledge not found".to_string()))
    }

    pub async fn update_knowledge_data(
        &self,
        id: &str,
        data: serde_json::Value,
    ) -> AppResult<Knowledge> {
        let now = current_timestamp_seconds();

        sqlx::query("UPDATE knowledge SET data = $1, updated_at = $2 WHERE id = $3")
            .bind(&data)
            .bind(now)
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        self.get_knowledge_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Knowledge not found".to_string()))
    }

    pub async fn delete_knowledge(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM knowledge WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn delete_knowledge_by_user_id(&self, user_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM knowledge WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }

    pub async fn check_access_by_user_id(
        &self,
        id: &str,
        user_id: &str,
        permission: &str,
        user_group_ids: &std::collections::HashSet<String>,
    ) -> AppResult<bool> {
        let knowledge = self.get_knowledge_by_id(id).await?;

        if let Some(knowledge) = knowledge {
            // Owner always has access
            if knowledge.user_id == user_id {
                return Ok(true);
            }

            // Check access control
            use crate::utils::misc::has_access;
            Ok(has_access(
                user_id,
                permission,
                &knowledge.access_control,
                user_group_ids,
            ))
        } else {
            Ok(false)
        }
    }
}
