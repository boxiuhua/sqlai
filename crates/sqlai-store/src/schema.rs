use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TableMetaRecord {
    pub id: Uuid,
    pub datasource_id: Uuid,
    pub db: String,
    pub table_name: String,
    pub comment: Option<String>,
    pub row_count_est: Option<i64>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ColumnMetaRecord {
    pub id: Uuid,
    pub table_id: Uuid,
    pub name: String,
    pub data_type: String,
    pub comment: Option<String>,
    pub sample_values: serde_json::Value,
    pub distinct_count_est: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpsertTable<'a> {
    pub datasource_id: Uuid,
    pub db: &'a str,
    pub table_name: &'a str,
    pub comment: Option<&'a str>,
    pub row_count_est: Option<i64>,
    pub embedding: Vec<f32>, // 1024 dim
}

#[derive(Debug, Clone)]
pub struct UpsertColumn<'a> {
    pub table_id: Uuid,
    pub name: &'a str,
    pub data_type: &'a str,
    pub comment: Option<&'a str>,
    pub sample_values: serde_json::Value,
    pub distinct_count_est: Option<i64>,
    pub embedding: Vec<f32>,
}

pub async fn upsert_table(
    pool: &PgPool,
    t: UpsertTable<'_>,
) -> Result<TableMetaRecord, StoreError> {
    let v = Vector::from(t.embedding);
    sqlx::query_as::<_, TableMetaRecord>(
        r#"
        INSERT INTO table_meta (datasource_id, db, table_name, comment, row_count_est, embedding, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, now())
        ON CONFLICT (datasource_id, db, table_name)
        DO UPDATE SET comment = EXCLUDED.comment,
                      row_count_est = EXCLUDED.row_count_est,
                      embedding = EXCLUDED.embedding,
                      updated_at = now()
        RETURNING id, datasource_id, db, table_name, comment, row_count_est, updated_at
        "#,
    )
    .bind(t.datasource_id)
    .bind(t.db)
    .bind(t.table_name)
    .bind(t.comment)
    .bind(t.row_count_est)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn upsert_column(
    pool: &PgPool,
    c: UpsertColumn<'_>,
) -> Result<ColumnMetaRecord, StoreError> {
    let v = Vector::from(c.embedding);
    sqlx::query_as::<_, ColumnMetaRecord>(
        r#"
        INSERT INTO column_meta (table_id, name, data_type, comment, sample_values, distinct_count_est, embedding)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (table_id, name)
        DO UPDATE SET data_type = EXCLUDED.data_type,
                      comment = EXCLUDED.comment,
                      sample_values = EXCLUDED.sample_values,
                      distinct_count_est = EXCLUDED.distinct_count_est,
                      embedding = EXCLUDED.embedding
        RETURNING id, table_id, name, data_type, comment, sample_values, distinct_count_est
        "#,
    )
    .bind(c.table_id)
    .bind(c.name)
    .bind(c.data_type)
    .bind(c.comment)
    .bind(&c.sample_values)
    .bind(c.distinct_count_est)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

/// 用 cosine 距离（`<=>`）找前 K 个相似表。返回 (record, distance)，distance 越小越相似。
#[allow(clippy::type_complexity)]
pub async fn top_k_tables_by_embedding(
    pool: &PgPool,
    datasource_id: Uuid,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(TableMetaRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    let rows: Vec<(
        Uuid,
        Uuid,
        String,
        String,
        Option<String>,
        Option<i64>,
        DateTime<Utc>,
        f64,
    )> = sqlx::query_as(
        r#"
            SELECT id, datasource_id, db, table_name, comment, row_count_est, updated_at,
                   (embedding <=> $2) AS distance
            FROM table_meta
            WHERE datasource_id = $1 AND embedding IS NOT NULL
            ORDER BY embedding <=> $2
            LIMIT $3
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
            |(id, datasource_id, db, table_name, comment, row_count_est, updated_at, dist)| {
                (
                    TableMetaRecord {
                        id,
                        datasource_id,
                        db,
                        table_name,
                        comment,
                        row_count_est,
                        updated_at,
                    },
                    dist,
                )
            },
        )
        .collect())
}

#[allow(clippy::type_complexity)]
pub async fn top_k_columns_by_embedding(
    pool: &PgPool,
    table_ids: &[Uuid],
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(ColumnMetaRecord, f64)>, StoreError> {
    if table_ids.is_empty() {
        return Ok(vec![]);
    }
    let v = Vector::from(query);
    let rows: Vec<(
        Uuid,
        Uuid,
        String,
        String,
        Option<String>,
        serde_json::Value,
        Option<i64>,
        f64,
    )> = sqlx::query_as(
        r#"
            SELECT id, table_id, name, data_type, comment, sample_values, distinct_count_est,
                   (embedding <=> $2) AS distance
            FROM column_meta
            WHERE table_id = ANY($1) AND embedding IS NOT NULL
            ORDER BY embedding <=> $2
            LIMIT $3
            "#,
    )
    .bind(table_ids)
    .bind(&v)
    .bind(k)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(
            |(id, table_id, name, data_type, comment, sample_values, distinct_count_est, dist)| {
                (
                    ColumnMetaRecord {
                        id,
                        table_id,
                        name,
                        data_type,
                        comment,
                        sample_values,
                        distinct_count_est,
                    },
                    dist,
                )
            },
        )
        .collect())
}
