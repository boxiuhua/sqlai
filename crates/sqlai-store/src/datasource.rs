use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DatasourceRecord {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub host: String,
    pub port: i32,
    pub db: String,
    pub user_name: String,
    pub secret_ref: String,
    pub readonly: bool,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewDatasource<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub host: &'a str,
    pub port: i32,
    pub db: &'a str,
    pub user_name: &'a str,
    pub secret_ref: &'a str,
    pub readonly: bool,
    pub settings: serde_json::Value,
}

pub async fn insert(pool: &PgPool, ds: NewDatasource<'_>) -> Result<DatasourceRecord, StoreError> {
    sqlx::query_as::<_, DatasourceRecord>(
        r#"
        INSERT INTO datasource (name, kind, host, port, db, user_name, secret_ref, readonly, settings)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, name, kind, host, port, db, user_name, secret_ref, readonly,
                  settings, created_at, updated_at
        "#,
    )
    .bind(ds.name)
    .bind(ds.kind)
    .bind(ds.host)
    .bind(ds.port)
    .bind(ds.db)
    .bind(ds.user_name)
    .bind(ds.secret_ref)
    .bind(ds.readonly)
    .bind(&ds.settings)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<DatasourceRecord, StoreError> {
    sqlx::query_as::<_, DatasourceRecord>(
        "SELECT id, name, kind, host, port, db, user_name, secret_ref, readonly, settings, created_at, updated_at FROM datasource WHERE name = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)
}

pub async fn list(pool: &PgPool) -> Result<Vec<DatasourceRecord>, StoreError> {
    sqlx::query_as::<_, DatasourceRecord>(
        "SELECT id, name, kind, host, port, db, user_name, secret_ref, readonly, settings, created_at, updated_at FROM datasource ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)
}
