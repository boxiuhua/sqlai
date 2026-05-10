//! metric_overview：单一指标随时间分桶的趋势查询。

use serde::Deserialize;

use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::{quote_ident, quote_lit, time_bucket_clickhouse};
use crate::{AnalysisSkill, SkillSchema};

pub struct MetricOverview;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    date_column: String,
    measure_sql: String,
    granularity: String,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
    #[serde(default)]
    where_clause: Option<String>,
}

impl AnalysisSkill for MetricOverview {
    fn name(&self) -> &'static str {
        "metric_overview"
    }

    fn description(&self) -> &'static str {
        "查看一个度量随时间分桶的总体走势。适合\"近 30 天 GMV 趋势\" \"按周看 DAU\"。"
    }

    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db":            { "type": "string", "description": "ClickHouse 数据库名" },
                    "table":         { "type": "string", "description": "表名" },
                    "date_column":   { "type": "string", "description": "用于分桶的时间列" },
                    "measure_sql":   { "type": "string", "description": "聚合表达式，例如 'sum(amount)'" },
                    "granularity":   { "type": "string", "enum": ["day", "week", "month"] },
                    "start_date":    { "type": "string", "description": "YYYY-MM-DD（可选）" },
                    "end_date":      { "type": "string", "description": "YYYY-MM-DD（可选）" },
                    "where_clause":  { "type": "string", "description": "附加 WHERE 过滤（可选，不要含 'WHERE' 关键字）" }
                },
                "required": ["db", "table", "date_column", "measure_sql", "granularity"]
            }),
        }
    }

    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("metric_overview", e.to_string()))?;

        let bucket = time_bucket_clickhouse(&a.date_column, &a.granularity)
            .map_err(|e| SkillError::InvalidArg("granularity", e))?;
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));

        let mut where_parts: Vec<String> = vec![];
        if let Some(s) = &a.start_date {
            where_parts.push(format!(
                "{} >= {}",
                quote_ident(&a.date_column),
                quote_lit(s)
            ));
        }
        if let Some(s) = &a.end_date {
            where_parts.push(format!(
                "{} <= {}",
                quote_ident(&a.date_column),
                quote_lit(s)
            ));
        }
        if let Some(extra) = &a.where_clause {
            where_parts.push(format!("({})", extra));
        }
        let where_sql = if where_parts.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_parts.join(" AND "))
        };

        let sql = format!(
            "SELECT {bucket} AS bucket, {measure} AS value FROM {table}{where_sql} GROUP BY bucket ORDER BY bucket",
            bucket = bucket,
            measure = a.measure_sql,
            table = table,
            where_sql = where_sql,
        );

        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!("{}.{} 分{}聚合", a.db, a.table, a.granularity),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Line,
                x: Some("bucket".into()),
                y: Some("value".into()),
            }),
            explanation: format!(
                "按{}对 {}.{} 做 {} 的趋势聚合",
                a.granularity, a.db, a.table, a.measure_sql
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    fn empty_ctx() -> RetrievalContext {
        RetrievalContext {
            tables: vec![],
            columns: vec![],
            business_terms: vec![],
            few_shots: vec![],
        }
    }

    #[test]
    fn plan_renders_basic_sql() {
        let plan = MetricOverview
            .plan(
                &serde_json::json!({
                    "db": "default",
                    "table": "orders",
                    "date_column": "created_at",
                    "measure_sql": "sum(amount)",
                    "granularity": "day",
                    "start_date": "2025-01-01",
                    "end_date": "2025-01-31"
                }),
                &empty_ctx(),
            )
            .unwrap();
        assert_eq!(plan.steps.len(), 1);
        let AnalysisStep::Sql(s) = &plan.steps[0] else {
            panic!("expected Sql step")
        };
        assert!(
            s.sql.contains("toStartOfDay(`created_at`)"),
            "sql: {}",
            s.sql
        );
        assert!(s.sql.contains("`default`.`orders`"));
        assert!(s.sql.contains("sum(amount) AS value"));
        assert!(s.sql.contains("`created_at` >= '2025-01-01'"));
        assert!(s.sql.contains("`created_at` <= '2025-01-31'"));
        assert!(s.sql.contains("GROUP BY bucket"));
        assert_eq!(plan.chart_hint.as_ref().unwrap().kind, ChartKind::Line);
    }

    #[test]
    fn plan_without_dates_omits_where() {
        let plan = MetricOverview
            .plan(
                &serde_json::json!({
                    "db": "default",
                    "table": "orders",
                    "date_column": "d",
                    "measure_sql": "count()",
                    "granularity": "month"
                }),
                &empty_ctx(),
            )
            .unwrap();
        let AnalysisStep::Sql(s) = &plan.steps[0] else {
            panic!("expected Sql step")
        };
        assert!(!s.sql.contains("WHERE"), "sql: {}", s.sql);
    }

    #[test]
    fn invalid_granularity_rejected() {
        let err = MetricOverview
            .plan(
                &serde_json::json!({
                    "db": "default", "table": "x", "date_column": "d",
                    "measure_sql": "count()", "granularity": "year"
                }),
                &empty_ctx(),
            )
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArg("granularity", _)));
    }

    #[test]
    fn missing_required_field_rejected() {
        let err = MetricOverview
            .plan(
                &serde_json::json!({"db": "default", "table": "x"}),
                &empty_ctx(),
            )
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArg("metric_overview", _)));
    }

    #[test]
    fn schema_required_fields_listed() {
        let s = MetricOverview.schema();
        let req = s.parameters["required"].as_array().unwrap();
        let req: Vec<&str> = req.iter().filter_map(|v| v.as_str()).collect();
        for r in ["db", "table", "date_column", "measure_sql", "granularity"] {
            assert!(req.contains(&r), "{r} not in {req:?}");
        }
    }
}
