use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FewShotRecord {
    pub id: Uuid,
    pub question: String,
    pub skill_call: serde_json::Value,
    pub sql_text: String,
    pub datasource_id: Option<Uuid>,
    pub vote: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewFewShot<'a> {
    pub question: &'a str,
    pub skill_call: serde_json::Value,
    pub sql_text: &'a str,
    pub datasource_id: Option<Uuid>,
    pub embedding: Vec<f32>,
}

pub async fn insert(pool: &PgPool, fs: NewFewShot<'_>) -> Result<FewShotRecord, StoreError> {
    let v = Vector::from(fs.embedding);
    sqlx::query_as::<_, FewShotRecord>(
        r#"
        INSERT INTO few_shot (question, skill_call, sql_text, datasource_id, embedding)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, question, skill_call, sql_text, datasource_id, vote, created_at
        "#,
    )
    .bind(fs.question)
    .bind(fs.skill_call)
    .bind(fs.sql_text)
    .bind(fs.datasource_id)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn vote(pool: &PgPool, id: Uuid, delta: i32) -> Result<FewShotRecord, StoreError> {
    sqlx::query_as::<_, FewShotRecord>(
        r#"
        UPDATE few_shot SET vote = vote + $2 WHERE id = $1
        RETURNING id, question, skill_call, sql_text, datasource_id, vote, created_at
        "#,
    )
    .bind(id)
    .bind(delta)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), StoreError> {
    let n = sqlx::query("DELETE FROM few_shot WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(StoreError::Sql)?
        .rows_affected();
    if n == 0 {
        Err(StoreError::NotFound)
    } else {
        Ok(())
    }
}

pub async fn list(pool: &PgPool, limit: i64) -> Result<Vec<FewShotRecord>, StoreError> {
    sqlx::query_as::<_, FewShotRecord>(
        r#"
        SELECT id, question, skill_call, sql_text, datasource_id, vote, created_at
        FROM few_shot ORDER BY vote DESC, created_at DESC LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)
}

#[allow(clippy::type_complexity)]
pub async fn top_k(
    pool: &PgPool,
    datasource_id: Option<Uuid>,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(FewShotRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    let rows: Vec<(
        Uuid,
        String,
        serde_json::Value,
        String,
        Option<Uuid>,
        i32,
        DateTime<Utc>,
        f64,
    )> = sqlx::query_as(
        r#"
            SELECT id, question, skill_call, sql_text, datasource_id, vote, created_at,
                   (embedding <=> $2) AS distance
            FROM few_shot
            WHERE embedding IS NOT NULL
              AND ($1::uuid IS NULL OR datasource_id IS NULL OR datasource_id = $1)
              AND vote >= 0
            ORDER BY embedding <=> $2 LIMIT $3
            "#,
    )
    .bind(datasource_id)
    .bind(&v)
    .bind(k)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(
            |(id, question, skill_call, sql_text, datasource_id, vote, created_at, dist)| {
                (
                    FewShotRecord {
                        id,
                        question,
                        skill_call,
                        sql_text,
                        datasource_id,
                        vote,
                        created_at,
                    },
                    dist,
                )
            },
        )
        .collect())
}
