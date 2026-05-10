//! SQL 片段拼装的安全工具。
//!
//! 这些工具只针对"已被 LLM/skill 信任的标识符 + 字面量"做转义，
//! 真正的护栏在下游 sqlai-dialect::validate 与 ClickHouse EXPLAIN。

/// 用反引号包住标识符，把内含的反引号 double 掉。
pub fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// 用单引号包住字符串字面量，把内含的单引号 double 掉。
pub fn quote_lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 拼装 ClickHouse 时间分桶表达式。
/// granularity ∈ {"day", "week", "month"}；其它返回错误。
pub fn time_bucket_clickhouse(date_col: &str, granularity: &str) -> Result<String, String> {
    let f = match granularity {
        "day" => "toStartOfDay",
        "week" => "toStartOfWeek",
        "month" => "toStartOfMonth",
        other => return Err(format!("unsupported granularity: {other}")),
    };
    Ok(format!("{}({})", f, quote_ident(date_col)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_ident_escapes_backticks() {
        assert_eq!(quote_ident("orders"), "`orders`");
        assert_eq!(quote_ident("a`b"), "`a``b`");
    }

    #[test]
    fn quote_lit_escapes_single_quotes() {
        assert_eq!(quote_lit("alice"), "'alice'");
        assert_eq!(quote_lit("a'b"), "'a''b'");
    }

    #[test]
    fn time_bucket_known_granularities() {
        assert_eq!(
            time_bucket_clickhouse("created_at", "day").unwrap(),
            "toStartOfDay(`created_at`)"
        );
        assert_eq!(
            time_bucket_clickhouse("d", "week").unwrap(),
            "toStartOfWeek(`d`)"
        );
        assert_eq!(
            time_bucket_clickhouse("d", "month").unwrap(),
            "toStartOfMonth(`d`)"
        );
    }

    #[test]
    fn time_bucket_unknown_returns_error() {
        assert!(time_bucket_clickhouse("d", "year").is_err());
    }
}
