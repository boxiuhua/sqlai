use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

use crate::error::StoreError;

#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub url: String, // postgres://user:pass@host:5432/db
    pub max_connections: u32,
}

impl StoreConfig {
    pub fn from_env() -> Result<Self, StoreError> {
        let url = std::env::var("SQLAI_PG_URL")
            .map_err(|_| StoreError::Migrate("SQLAI_PG_URL not set".into()))?;
        let max_connections = std::env::var("SQLAI_PG_MAX_CONN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        Ok(Self {
            url,
            max_connections,
        })
    }
}

pub async fn connect(cfg: &StoreConfig) -> Result<PgPool, StoreError> {
    PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&cfg.url)
        .await
        .map_err(StoreError::Sql)
}

/// 跑同目录下 `migrations/` 中的 .sql 文件（按文件名升序，幂等）。
pub async fn run_migrations(pool: &PgPool, dir: &std::path::Path) -> Result<(), StoreError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| StoreError::Migrate(format!("read_dir {dir:?}: {e}")))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let sql = std::fs::read_to_string(entry.path())
            .map_err(|e| StoreError::Migrate(format!("read {entry:?}: {e}")))?;
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .map_err(|e| StoreError::Migrate(format!("apply {:?}: {e}", entry.file_name())))?;
    }
    Ok(())
}
