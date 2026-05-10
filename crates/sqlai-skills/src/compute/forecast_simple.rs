//! forecast_simple：移动均值 + 线性外推。

use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{
    AnalysisPlan, AnalysisStep, ChartHint, ChartKind, ComputeFn, ComputeStep, SqlStep,
};
use crate::render::{quote_ident, quote_lit, time_bucket_clickhouse};
use crate::{AnalysisSkill, SkillSchema};

pub struct ForecastSimple;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    date_column: String,
    measure_sql: String,
    granularity: String,
    #[serde(default = "default_window")]
    window: u32,
    #[serde(default = "default_horizon")]
    horizon: u32,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
}
fn default_window() -> u32 {
    7
}
fn default_horizon() -> u32 {
    7
}

impl AnalysisSkill for ForecastSimple {
    fn name(&self) -> &'static str {
        "forecast_simple"
    }
    fn description(&self) -> &'static str {
        "对 (date, measure) 时间序列做移动均值平滑 + 线性外推 N 期。"
    }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db":          { "type":"string" },
                    "table":       { "type":"string" },
                    "date_column": { "type":"string" },
                    "measure_sql": { "type":"string" },
                    "granularity": { "type":"string", "enum":["day","week","month"] },
                    "window":      { "type":"integer", "default":7, "minimum":2, "maximum":60 },
                    "horizon":     { "type":"integer", "default":7, "minimum":1, "maximum":30 },
                    "start_date":  { "type":"string" },
                    "end_date":    { "type":"string" }
                },
                "required": ["db","table","date_column","measure_sql","granularity"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("forecast_simple", e.to_string()))?;
        let bucket = time_bucket_clickhouse(&a.date_column, &a.granularity)
            .map_err(|e| SkillError::InvalidArg("granularity", e))?;
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
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
        let sql =
            format!(
            "SELECT {bucket} AS bucket, {m} AS value FROM {t}{w} GROUP BY bucket ORDER BY bucket",
            bucket = bucket, m = a.measure_sql, t = table, w = where_sql,
        );
        Ok(AnalysisPlan {
            steps: vec![
                AnalysisStep::Sql(SqlStep {
                    label: format!("{}.{} 历史聚合", a.db, a.table),
                    sql,
                }),
                AnalysisStep::Compute(ComputeStep {
                    label: format!("移动均值 (window={})", a.window),
                    function: ComputeFn::MovingAverage,
                    source_step: 0,
                    params: serde_json::json!({ "window": a.window }),
                }),
                AnalysisStep::Compute(ComputeStep {
                    label: format!("线性外推 {} 期", a.horizon),
                    function: ComputeFn::LinearExtrapolation,
                    source_step: 0,
                    params: serde_json::json!({ "horizon": a.horizon, "granularity": a.granularity }),
                }),
            ],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Line,
                x: Some("bucket".into()),
                y: Some("value".into()),
            }),
            explanation: format!(
                "{}.{} 按{}聚合，移动均值+外推 {} 期",
                a.db, a.table, a.granularity, a.horizon
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    fn ctx() -> RetrievalContext {
        RetrievalContext {
            tables: vec![],
            columns: vec![],
            business_terms: vec![],
            few_shots: vec![],
        }
    }

    #[test]
    fn plan_has_three_steps() {
        let p = ForecastSimple
            .plan(
                &serde_json::json!({
                    "db":"d","table":"t","date_column":"d","measure_sql":"sum(x)",
                    "granularity":"day","window":7,"horizon":7
                }),
                &ctx(),
            )
            .unwrap();
        assert_eq!(p.steps.len(), 3);
        assert!(matches!(p.steps[0], AnalysisStep::Sql(_)));
        assert!(matches!(p.steps[1], AnalysisStep::Compute(_)));
        assert!(matches!(p.steps[2], AnalysisStep::Compute(_)));
    }
}
