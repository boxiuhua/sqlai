use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionRecord {
    pub id: Uuid,
    pub user_id: String,
    pub datasource_id: Option<Uuid>,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create_session(
    pool: &PgPool,
    user_id: &str,
    datasource_id: Option<Uuid>,
    title: Option<&str>,
) -> Result<SessionRecord, StoreError> {
    sqlx::query_as::<_, SessionRecord>(
        r#"
        INSERT INTO session (user_id, datasource_id, title)
        VALUES ($1, $2, $3)
        RETURNING id, user_id, datasource_id, title, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(datasource_id)
    .bind(title)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn touch_session(pool: &PgPool, id: Uuid) -> Result<(), StoreError> {
    sqlx::query("UPDATE session SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(StoreError::Sql)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,
    pub content: serde_json::Value,
    pub plan: Option<serde_json::Value>,
    pub chart_spec: Option<serde_json::Value>,
    pub rows_returned: Option<i32>,
    pub latency_ms: Option<i32>,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub session_id: Uuid,
    pub role: String,
    pub content: serde_json::Value,
    pub plan: Option<serde_json::Value>,
    pub chart_spec: Option<serde_json::Value>,
    pub rows_returned: Option<i32>,
    pub latency_ms: Option<i32>,
    pub parent_id: Option<Uuid>,
}

pub async fn append_message(pool: &PgPool, m: NewMessage) -> Result<MessageRecord, StoreError> {
    sqlx::query_as::<_, MessageRecord>(
        r#"
        INSERT INTO message (session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id, created_at
        "#,
    )
    .bind(m.session_id)
    .bind(m.role)
    .bind(m.content)
    .bind(m.plan)
    .bind(m.chart_spec)
    .bind(m.rows_returned)
    .bind(m.latency_ms)
    .bind(m.parent_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn list_messages(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<MessageRecord>, StoreError> {
    sqlx::query_as::<_, MessageRecord>(
        r#"
        SELECT id, session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id, created_at
        FROM message WHERE session_id = $1 ORDER BY created_at ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn get_message(pool: &PgPool, id: Uuid) -> Result<MessageRecord, StoreError> {
    sqlx::query_as::<_, MessageRecord>(
        r#"
        SELECT id, session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id, created_at
        FROM message WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)
}
