use crate::Dialect;
use sqlai_core::DialectKind;

pub struct ClickHouseDialect;

impl Dialect for ClickHouseDialect {
    fn kind(&self) -> DialectKind {
        DialectKind::ClickHouse
    }

    fn limit_clause(&self, n: u64) -> String {
        format!(" LIMIT {}", n)
    }

    fn explain_prefix(&self) -> &'static str {
        "EXPLAIN SYNTAX "
    }

    fn prompt_hints(&self) -> &'static str {
        "ClickHouse 方言要点：使用 toDate/toDateTime 处理时间；聚合用 sum/avg/uniq/quantile；\
         窗口函数支持 over()；避免 SELECT *；大表查询应当带 PREWHERE/WHERE 与 LIMIT。"
    }
}
