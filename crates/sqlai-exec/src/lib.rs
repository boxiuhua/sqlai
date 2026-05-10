//! sqlai-exec：Executor trait + ClickHouse 只读执行实现。

pub mod clickhouse;

pub use crate::clickhouse::{ClickHouseExecutor, ReadonlyClickHouse, ReadonlyConfig};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use sqlai_dialect::ValidatedSql;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("engine error: {0}")]
    Engine(String),

    #[error("timeout")]
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Value>,
    pub truncated: bool,
}

#[async_trait]
pub trait Executor: Send + Sync {
    async fn explain(&self, sql: &ValidatedSql) -> Result<(), ExecError>;
    async fn run(&self, sql: &ValidatedSql) -> Result<ExecutionResult, ExecError>;
}
