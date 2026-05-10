use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::quote_ident;
use crate::{AnalysisSkill, SkillSchema};

pub struct ShareBreakdown;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    dimension: String,
    measure_sql: String,
    #[serde(default)]
    where_clause: Option<String>,
}

impl AnalysisSkill for ShareBreakdown {
    fn name(&self) -> &'static str {
        "share_breakdown"
    }
    fn description(&self) -> &'static str {
        "按维度做占比分析，输出每项的绝对值与占总和的份额。"
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
                    "measure_sql":  {"type":"string"},
                    "where_clause": {"type":"string"}
                },
                "required": ["db","table","dimension","measure_sql"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("share_breakdown", e.to_string()))?;
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let dim = quote_ident(&a.dimension);
        let where_sql = match &a.where_clause {
            Some(w) if !w.trim().is_empty() => format!(" WHERE ({w})"),
            _ => String::new(),
        };
        let sql = format!(
            "SELECT {dim} AS dimension, {m} AS value, \
                    {m} / sum({m}) OVER () AS share \
             FROM {t}{w} GROUP BY {dim} ORDER BY value DESC",
            dim = dim,
            m = a.measure_sql,
            t = table,
            w = where_sql,
        );
        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!("{}.{} 按 {} 占比", a.db, a.table, a.dimension),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Pie,
                x: Some("dimension".into()),
                y: Some("value".into()),
            }),
            explanation: format!("按 {} 看 {} 的占比构成", a.dimension, a.measure_sql),
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
    fn renders_share_sql() {
        let p = ShareBreakdown
            .plan(
                &serde_json::json!({
                    "db": "default", "table": "orders", "dimension": "channel",
                    "measure_sql": "sum(amount)"
                }),
                &ctx(),
            )
            .unwrap();
        let AnalysisStep::Sql(s) = &p.steps[0] else {
            panic!("expected Sql step")
        };
        assert!(s.sql.contains("OVER ()"));
        assert!(s.sql.contains("AS share"));
        assert_eq!(p.chart_hint.as_ref().unwrap().kind, ChartKind::Pie);
    }
}
