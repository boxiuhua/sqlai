//! SELECT-only 校验：基于 sqlparser-rs 的 AST，强制只接受只读语句。

use sqlparser::ast::Statement;
use sqlparser::dialect::ClickHouseDialect as ChDialect;
use sqlparser::parser::Parser;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("only SELECT/SHOW/EXPLAIN/DESCRIBE statements are allowed; got: {kind}")]
    NotReadOnly { kind: String },

    #[error("multiple statements are not allowed; got {count}")]
    MultiStatement { count: usize },
}

/// 已通过 SELECT-only 校验的 SQL。无公开构造函数 —— 只能由 `validate()` 产出。
#[derive(Debug, Clone)]
pub struct ValidatedSql(String);

impl ValidatedSql {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 把原始 SQL 校验为 ValidatedSql。
pub fn validate(sql: &str) -> Result<ValidatedSql, ValidationError> {
    let stmts =
        Parser::parse_sql(&ChDialect {}, sql).map_err(|e| ValidationError::Parse(e.to_string()))?;

    if stmts.len() != 1 {
        return Err(ValidationError::MultiStatement { count: stmts.len() });
    }

    let stmt = &stmts[0];
    let kind = stmt_kind(stmt);
    if !is_readonly(stmt) {
        return Err(ValidationError::NotReadOnly { kind });
    }
    Ok(ValidatedSql(sql.to_string()))
}

fn is_readonly(stmt: &Statement) -> bool {
    match stmt {
        Statement::Query(_)
        | Statement::ShowVariable { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowTables { .. }
        | Statement::ShowFunctions { .. }
        | Statement::ExplainTable { .. } => true,
        // EXPLAIN <inner> is only readonly if the inner statement is also readonly.
        // Otherwise `EXPLAIN DROP TABLE t` would slip through.
        Statement::Explain { statement, .. } => is_readonly(statement),
        _ => false,
    }
}

fn stmt_kind(stmt: &Statement) -> String {
    let s = format!("{:?}", stmt);
    s.split_whitespace().next().unwrap_or("Unknown").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_passes() {
        assert!(validate("SELECT 1").is_ok());
        assert!(validate("SELECT a, b FROM t WHERE c > 1 LIMIT 10").is_ok());
    }

    #[test]
    fn explain_passes() {
        assert!(validate("EXPLAIN SELECT 1").is_ok());
    }

    #[test]
    fn show_passes() {
        assert!(validate("SHOW TABLES").is_ok());
    }

    #[test]
    fn insert_rejected() {
        let err = validate("INSERT INTO t VALUES (1)").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn update_rejected() {
        let err = validate("UPDATE t SET a = 1 WHERE b = 2").unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::NotReadOnly { .. } | ValidationError::Parse(_)
            ),
            "got {:?}",
            err
        );
    }

    #[test]
    fn delete_rejected() {
        let err = validate("DELETE FROM t WHERE a = 1").unwrap_err();
        assert!(matches!(
            err,
            ValidationError::NotReadOnly { .. } | ValidationError::Parse(_)
        ));
    }

    #[test]
    fn drop_rejected() {
        let err = validate("DROP TABLE t").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn alter_rejected() {
        let err = validate("ALTER TABLE t ADD COLUMN x Int32").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn create_rejected() {
        let err = validate("CREATE TABLE t (a Int32) ENGINE = Memory").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn truncate_rejected() {
        let err = validate("TRUNCATE TABLE t").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn multi_statement_rejected() {
        let err = validate("SELECT 1; SELECT 2").unwrap_err();
        assert!(matches!(err, ValidationError::MultiStatement { count: 2 }));
    }

    #[test]
    fn invalid_sql_returns_parse_error() {
        let err = validate("NOT_A_SQL_STATEMENT %%").unwrap_err();
        assert!(matches!(err, ValidationError::Parse(_)));
    }

    #[test]
    fn explain_of_drop_is_rejected() {
        let err = validate("EXPLAIN DROP TABLE t").unwrap_err();
        assert!(
            matches!(err, ValidationError::NotReadOnly { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn explain_of_insert_is_rejected() {
        let err = validate("EXPLAIN INSERT INTO t VALUES (1)").unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::NotReadOnly { .. } | ValidationError::Parse(_)
            ),
            "got {:?}",
            err
        );
    }

    #[test]
    fn use_is_rejected() {
        let err = validate("USE db1").unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::NotReadOnly { .. } | ValidationError::Parse(_)
            ),
            "got {:?}",
            err
        );
    }

    #[test]
    fn set_variable_is_rejected() {
        let err = validate("SET max_threads = 4").unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::NotReadOnly { .. } | ValidationError::Parse(_)
            ),
            "got {:?}",
            err
        );
    }
}
