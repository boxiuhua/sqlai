use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::{quote_ident, quote_lit, time_bucket_clickhouse};
use crate::{AnalysisSkill, SkillSchema};

pub struct TrendSegment;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    dimension: String,
    date_column: String,
    measure_sql: String,
    granularity: String,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
}

impl AnalysisSkill for TrendSegment {
    fn name(&self) -> &'static str {
        "trend_segment"
    }
    fn description(&self) -> &'static str {
        "把单一指标按维度分组、随时间分桶展示，支持多折线图。"
    }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db":           {"type":"string"},
                    "table":        {"type":"string"},
                    "dimension":    {"type":"string"},
                    "date_column":  {"type":"string"},
                    "measure_sql":  {"type":"string"},
                    "granularity":  {"type":"string", "enum":["day","week","month"]},
                    "start_date":   {"type":"string"},
                    "end_date":     {"type":"string"}
                },
                "required": ["db","table","dimension","date_column","measure_sql","granularity"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("trend_segment", e.to_string()))?;
        let bucket = time_bucket_clickhouse(&a.date_column, &a.granularity)
            .map_err(|e| SkillError::InvalidArg("granularity", e))?;
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let dim = quote_ident(&a.dimension);
        let mut wh: Vec<String> = vec![];
        if let Some(s) = &a.start_date {
            wh.push(format!(
                "{} >= {}",
                quote_ident(&a.date_column),
                quote_lit(s)
            ));
        }
        if let Some(s) = &a.end_date {
            wh.push(format!(
                "{} <= {}",
                quote_ident(&a.date_column),
                quote_lit(s)
            ));
        }
        let where_sql = if wh.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", wh.join(" AND "))
        };
        let sql = format!(
            "SELECT {bucket} AS bucket, {dim} AS segment, {m} AS value FROM {t}{w} \
             GROUP BY bucket, {dim} ORDER BY bucket, value DESC",
            bucket = bucket,
            dim = dim,
            m = a.measure_sql,
            t = table,
            w = where_sql,
        );
        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!(
                    "{}.{} 分{}+按{}分组",
                    a.db, a.table, a.granularity, a.dimension
                ),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Line,
                x: Some("bucket".into()),
                y: Some("value".into()),
            }),
            explanation: format!("按{}分桶并按 {} 分组的趋势", a.granularity, a.dimension),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    #[test]
    fn renders_trend_segment_sql() {
        let p = TrendSegment
            .plan(
                &serde_json::json!({
                    "db": "default", "table": "orders", "dimension": "channel",
                    "date_column": "d", "measure_sql": "sum(amount)", "granularity": "week"
                }),
                &RetrievalContext {
                    tables: vec![],
                    columns: vec![],
                    business_terms: vec![],
                    few_shots: vec![],
                },
            )
            .unwrap();
        let AnalysisStep::Sql(s) = &p.steps[0] else {
            panic!("expected Sql step")
        };
        assert!(s.sql.contains("toStartOfWeek(`d`)"));
        assert!(s.sql.contains("`channel` AS segment"));
    }
}
