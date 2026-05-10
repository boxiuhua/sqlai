use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BusinessTermRecord {
    pub id: Uuid,
    pub term: String,
    pub aliases: Vec<String>,
    pub definition: String,
    pub formula: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpsertTerm<'a> {
    pub term: &'a str,
    pub aliases: &'a [String],
    pub definition: &'a str,
    pub formula: Option<&'a str>,
    pub embedding: Vec<f32>,
}

pub async fn upsert_term(
    pool: &PgPool,
    t: UpsertTerm<'_>,
) -> Result<BusinessTermRecord, StoreError> {
    let v = Vector::from(t.embedding);
    sqlx::query_as::<_, BusinessTermRecord>(
        r#"
        INSERT INTO business_term (term, aliases, definition, formula, embedding)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (term) DO UPDATE SET
            aliases = EXCLUDED.aliases,
            definition = EXCLUDED.definition,
            formula = EXCLUDED.formula,
            embedding = EXCLUDED.embedding
        RETURNING id, term, aliases, definition, formula
        "#,
    )
    .bind(t.term)
    .bind(t.aliases)
    .bind(t.definition)
    .bind(t.formula)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

#[allow(clippy::type_complexity)]
pub async fn top_k_terms(
    pool: &PgPool,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(BusinessTermRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    let rows: Vec<(Uuid, String, Vec<String>, String, Option<String>, f64)> = sqlx::query_as(
        r#"
            SELECT id, term, aliases, definition, formula, (embedding <=> $1) AS distance
            FROM business_term WHERE embedding IS NOT NULL
            ORDER BY embedding <=> $1 LIMIT $2
            "#,
    )
    .bind(&v)
    .bind(k)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(|(id, term, aliases, definition, formula, dist)| {
            (
                BusinessTermRecord {
                    id,
                    term,
                    aliases,
                    definition,
                    formula,
                },
                dist,
            )
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MetricDefRecord {
    pub id: Uuid,
    pub name: String,
    pub dimension_keys: Vec<String>,
    pub measure_sql: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpsertMetric<'a> {
    pub name: &'a str,
    pub dimension_keys: &'a [String],
    pub measure_sql: &'a str,
    pub owner: Option<&'a str>,
    pub embedding: Vec<f32>,
}

pub async fn upsert_metric(
    pool: &PgPool,
    m: UpsertMetric<'_>,
) -> Result<MetricDefRecord, StoreError> {
    let v = Vector::from(m.embedding);
    sqlx::query_as::<_, MetricDefRecord>(
        r#"
        INSERT INTO metric_def (name, dimension_keys, measure_sql, owner, embedding)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (name) DO UPDATE SET
            dimension_keys = EXCLUDED.dimension_keys,
            measure_sql = EXCLUDED.measure_sql,
            owner = EXCLUDED.owner,
            embedding = EXCLUDED.embedding
        RETURNING id, name, dimension_keys, measure_sql, owner
        "#,
    )
    .bind(m.name)
    .bind(m.dimension_keys)
    .bind(m.measure_sql)
    .bind(m.owner)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

#[allow(clippy::type_complexity)]
pub async fn top_k_metrics(
    pool: &PgPool,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(MetricDefRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    let rows: Vec<(Uuid, String, Vec<String>, String, Option<String>, f64)> = sqlx::query_as(
        r#"
            SELECT id, name, dimension_keys, measure_sql, owner, (embedding <=> $1) AS distance
            FROM metric_def WHERE embedding IS NOT NULL
            ORDER BY embedding <=> $1 LIMIT $2
            "#,
    )
    .bind(&v)
    .bind(k)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(|(id, name, dimension_keys, measure_sql, owner, dist)| {
            (
                MetricDefRecord {
                    id,
                    name,
                    dimension_keys,
                    measure_sql,
                    owner,
                },
                dist,
            )
        })
        .collect())
}
