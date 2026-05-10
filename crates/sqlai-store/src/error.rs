use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sql error: {0}")]
    Sql(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migrate(String),

    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),
}
