//! sqlai-dialect：方言抽象 + ClickHouse 实现 + SELECT-only 校验。

pub mod clickhouse;
pub mod validator;

pub use validator::{validate, ValidatedSql, ValidationError};

use sqlai_core::DialectKind;

pub trait Dialect: Send + Sync {
    fn kind(&self) -> DialectKind;
    fn limit_clause(&self, n: u64) -> String;
    fn explain_prefix(&self) -> &'static str;
    fn prompt_hints(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clickhouse::ClickHouseDialect;

    #[test]
    fn clickhouse_dialect_basics() {
        let d = ClickHouseDialect;
        assert_eq!(d.kind(), DialectKind::ClickHouse);
        assert_eq!(d.limit_clause(100), " LIMIT 100");
        assert_eq!(d.explain_prefix(), "EXPLAIN SYNTAX ");
        assert!(d.prompt_hints().contains("ClickHouse"));
    }
}
