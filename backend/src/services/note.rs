use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::models::note::{Note, NoteForm, NoteUpdateForm};
use crate::utils::time::current_timestamp_nanos;
use uuid::Uuid;

pub struct NoteService<'a> {
    db: &'a Database,
}

impl<'a> NoteService<'a> {
    pub fn new(db: &'a Database) -> Self {
        NoteService { db }
    }

    pub async fn insert_new_note(&self, user_id: &str, form_data: &NoteForm) -> AppResult<Note> {
        let now = current_timestamp_nanos();
        let id = Uuid::new_v4().to_string();

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

        let access_control_json = form_data
            .access_control
            .as_ref()
            .map(|ac| serde_json::to_string(ac).ok())
            .flatten();

        sqlx::query(
            r#"
            INSERT INTO note (id, user_id, title, data, meta, access_control, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(&form_data.title)
        .bind(&data_json)
        .bind(&meta_json)
        .bind(&access_control_json)
        .bind(now)
        .bind(now)
        .execute(&self.db.pool)
        .await?;

        self.get_note_by_id(&id)
            .await?
            .ok_or_else(|| AppError::InternalServerError("Failed to create note".to_string()))
    }

    pub async fn get_note_by_id(&self, id: &str) -> AppResult<Option<Note>> {
        let result = sqlx::query_as::<_, Note>(
            r#"
            SELECT id, user_id, title, created_at, updated_at,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(access_control AS TEXT) as access_control_str
            FROM note
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.db.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_notes_by_permission(
        &self,
        user_id: &str,
        user_group_ids: &std::collections::HashSet<String>,
        permission: &str,
        skip: Option<i64>,
        limit: Option<i64>,
    ) -> AppResult<Vec<Note>> {
        // Get all notes ordered by updated_at DESC
        let all_notes = sqlx::query_as::<_, Note>(
            r#"
            SELECT id, user_id, title, created_at, updated_at,
                   CAST(data AS TEXT) as data_str,
                   CAST(meta AS TEXT) as meta_str,
                   CAST(access_control AS TEXT) as access_control_str
            FROM note
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.db.pool)
        .await?;

        // Filter by permission
        let mut filtered_notes = Vec::new();
        let mut skipped = 0i64;

        for mut note in all_notes {
            note.parse_json_fields();

            // Check permission
            let has_permission = if note.user_id == user_id {
                true
            } else if note.access_control.is_none() {
                // Public access only for read
                permission == "read"
            } else {
                crate::utils::misc::has_access(
                    user_id,
                    permission,
                    &note.access_control,
                    user_group_ids,
                )
            };

            if !has_permission {
                continue;
            }

            // Apply skip
            if let Some(skip_count) = skip {
                if skipped < skip_count {
                    skipped += 1;
                    continue;
                }
            }

            filtered_notes.push(note);

            // Apply limit
            if let Some(limit_count) = limit {
                if filtered_notes.len() >= limit_count as usize {
                    break;
                }
            }
        }

        Ok(filtered_notes)
    }

    pub async fn update_note_by_id(&self, id: &str, form_data: &NoteUpdateForm) -> AppResult<Note> {
        let now = current_timestamp_nanos();

        // Get the existing note to merge data fields
        let mut existing_note = self
            .get_note_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Note not found".to_string()))?;
        existing_note.parse_json_fields();

        // Update title if provided
        let title = form_data.title.as_ref().unwrap_or(&existing_note.title);

        // Merge the data field: preserve existing fields and merge incoming data
        let merged_data = if let Some(incoming_data) = &form_data.data {
            if let Some(mut existing_data) = existing_note.data.clone() {
                // Merge incoming data into existing data
                if let (Some(existing_obj), Some(incoming_obj)) =
                    (existing_data.as_object_mut(), incoming_data.as_object())
                {
                    for (key, value) in incoming_obj {
                        existing_obj.insert(key.clone(), value.clone());
                    }
                }
                Some(existing_data)
            } else {
                Some(incoming_data.clone())
            }
        } else {
            existing_note.data.clone()
        };

        // Merge the meta field: preserve existing fields and merge incoming meta
        let merged_meta = if let Some(incoming_meta) = &form_data.meta {
            if let Some(mut existing_meta) = existing_note.meta.clone() {
                // Merge incoming meta into existing meta
                if let (Some(existing_obj), Some(incoming_obj)) =
                    (existing_meta.as_object_mut(), incoming_meta.as_object())
                {
                    for (key, value) in incoming_obj {
                        existing_obj.insert(key.clone(), value.clone());
                    }
                }
                Some(existing_meta)
            } else {
                Some(incoming_meta.clone())
            }
        } else {
            existing_note.meta.clone()
        };

        // access_control is replaced entirely if provided (not merged)
        let access_control = form_data
            .access_control
            .as_ref()
            .or(existing_note.access_control.as_ref());

        let data_json = merged_data
            .as_ref()
            .map(|d| serde_json::to_string(d).ok())
            .flatten();

        let meta_json = merged_meta
            .as_ref()
            .map(|m| serde_json::to_string(m).ok())
            .flatten();

        let access_control_json = access_control
            .map(|ac| serde_json::to_string(ac).ok())
            .flatten();

        sqlx::query(
            r#"
            UPDATE note
            SET title = $1, data = $2, meta = $3, 
                access_control = $4, updated_at = $5
            WHERE id = $6
            "#,
        )
        .bind(title)
        .bind(&data_json)
        .bind(&meta_json)
        .bind(&access_control_json)
        .bind(now)
        .bind(id)
        .execute(&self.db.pool)
        .await?;

        self.get_note_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Note not found".to_string()))
    }

    pub async fn delete_note_by_id(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM note WHERE id = $1")
            .bind(id)
            .execute(&self.db.pool)
            .await?;

        Ok(())
    }
}
