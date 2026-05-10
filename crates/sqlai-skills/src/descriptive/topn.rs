use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::{quote_ident, quote_lit};
use crate::{AnalysisSkill, SkillSchema};

pub struct TopN;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    dimension: String,
    measure_sql: String,
    #[serde(default = "default_n")]
    n: u32,
    #[serde(default)]
    where_clause: Option<String>,
}

fn default_n() -> u32 {
    10
}

impl AnalysisSkill for TopN {
    fn name(&self) -> &'static str {
        "topn"
    }
    fn description(&self) -> &'static str {
        "按某个维度做 Top-N 排名。适合 \"销售 Top10 商品\" \"成交额 Top5 渠道\"。"
    }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db":           { "type": "string" },
                    "table":        { "type": "string" },
                    "dimension":    { "type": "string", "description": "分组维度列名" },
                    "measure_sql":  { "type": "string", "description": "排序用的聚合表达式" },
                    "n":            { "type": "integer", "default": 10, "minimum": 1, "maximum": 1000 },
                    "where_clause": { "type": "string", "description": "可选过滤" }
                },
                "required": ["db", "table", "dimension", "measure_sql"]
            }),
        }
    }

    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("topn", e.to_string()))?;
        if a.n == 0 {
            return Err(SkillError::InvalidArg("n", "n must be >= 1".into()));
        }
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let dim = quote_ident(&a.dimension);
        let where_sql = match &a.where_clause {
            Some(w) if !w.trim().is_empty() => format!(" WHERE ({w})"),
            _ => String::new(),
        };
        let _ = quote_lit;
        let sql = format!(
            "SELECT {dim} AS dimension, {measure} AS value FROM {table}{where_sql} \
             GROUP BY {dim} ORDER BY value DESC LIMIT {n}",
            dim = dim,
            measure = a.measure_sql,
            table = table,
            where_sql = where_sql,
            n = a.n,
        );
        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!("{}.{} 按 {} Top {}", a.db, a.table, a.dimension, a.n),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Bar,
                x: Some("dimension".into()),
                y: Some("value".into()),
            }),
            explanation: format!("按 {} 排序展示前 {} 名", a.dimension, a.n),
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
    fn renders_topn_sql() {
        let p = TopN
            .plan(
                &serde_json::json!({
                    "db": "default", "table": "orders", "dimension": "channel",
                    "measure_sql": "sum(amount)", "n": 5
                }),
                &ctx(),
            )
            .unwrap();
        let AnalysisStep::Sql(s) = &p.steps[0] else {
            panic!("expected Sql step")
        };
        assert!(s.sql.contains("LIMIT 5"));
        assert!(s.sql.contains("ORDER BY value DESC"));
        assert!(s.sql.contains("`channel` AS dimension"));
        assert_eq!(p.chart_hint.as_ref().unwrap().kind, ChartKind::Bar);
    }

    #[test]
    fn n_zero_rejected() {
        let err = TopN
            .plan(
                &serde_json::json!({
                    "db":"d","table":"t","dimension":"c","measure_sql":"count()","n":0
                }),
                &ctx(),
            )
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidArg("n", _)));
    }
}
