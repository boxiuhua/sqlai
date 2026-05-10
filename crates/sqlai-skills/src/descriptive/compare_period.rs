use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::{quote_ident, quote_lit};
use crate::{AnalysisSkill, SkillSchema};

pub struct ComparePeriod;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    date_column: String,
    measure_sql: String,
    current_start: String,
    current_end: String,
    baseline_start: String,
    baseline_end: String,
    #[serde(default)]
    dimension: Option<String>,
}

impl AnalysisSkill for ComparePeriod {
    fn name(&self) -> &'static str {
        "compare_period"
    }
    fn description(&self) -> &'static str {
        "对比两个时间窗口的指标值，常用于同比/环比/活动前后对比。"
    }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db":             { "type": "string" },
                    "table":          { "type": "string" },
                    "date_column":    { "type": "string" },
                    "measure_sql":    { "type": "string" },
                    "current_start":  { "type": "string", "description": "YYYY-MM-DD" },
                    "current_end":    { "type": "string" },
                    "baseline_start": { "type": "string" },
                    "baseline_end":   { "type": "string" },
                    "dimension":      { "type": "string", "description": "可选，传则按此维度对比" }
                },
                "required": ["db","table","date_column","measure_sql","current_start","current_end","baseline_start","baseline_end"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("compare_period", e.to_string()))?;
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let dc = quote_ident(&a.date_column);
        let cs = quote_lit(&a.current_start);
        let ce = quote_lit(&a.current_end);
        let bs = quote_lit(&a.baseline_start);
        let be = quote_lit(&a.baseline_end);

        let sql = if let Some(dim) = &a.dimension {
            let d = quote_ident(dim);
            format!(
                "WITH cur AS (SELECT {d} AS dim, {m} AS v FROM {t} WHERE {dc} BETWEEN {cs} AND {ce} GROUP BY dim), \
                 base AS (SELECT {d} AS dim, {m} AS v FROM {t} WHERE {dc} BETWEEN {bs} AND {be} GROUP BY dim) \
                 SELECT coalesce(cur.dim, base.dim) AS dimension, coalesce(cur.v, 0) AS current, \
                        coalesce(base.v, 0) AS baseline, coalesce(cur.v, 0) - coalesce(base.v, 0) AS delta \
                 FROM cur FULL OUTER JOIN base ON cur.dim = base.dim ORDER BY current DESC",
                d = d, m = a.measure_sql, t = table, dc = dc, cs = cs, ce = ce, bs = bs, be = be,
            )
        } else {
            format!(
                "SELECT \
                   (SELECT {m} FROM {t} WHERE {dc} BETWEEN {cs} AND {ce}) AS current, \
                   (SELECT {m} FROM {t} WHERE {dc} BETWEEN {bs} AND {be}) AS baseline",
                m = a.measure_sql,
                t = table,
                dc = dc,
                cs = cs,
                ce = ce,
                bs = bs,
                be = be,
            )
        };

        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!("{}.{} 时段对比", a.db, a.table),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Bar,
                x: a.dimension.clone().or(Some("metric".into())),
                y: Some("current".into()),
            }),
            explanation: format!(
                "对比 [{} ~ {}] 与 [{} ~ {}] 的 {}",
                a.current_start, a.current_end, a.baseline_start, a.baseline_end, a.measure_sql
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
    fn single_value_compare() {
        let p = ComparePeriod
            .plan(
                &serde_json::json!({
                    "db": "default", "table": "orders", "date_column": "d",
                    "measure_sql": "sum(amount)",
                    "current_start": "2025-02-01", "current_end": "2025-02-28",
                    "baseline_start": "2025-01-01", "baseline_end": "2025-01-31"
                }),
                &ctx(),
            )
            .unwrap();
        let AnalysisStep::Sql(s) = &p.steps[0] else {
            panic!("expected Sql step")
        };
        assert!(s.sql.contains("AS current"));
        assert!(s.sql.contains("AS baseline"));
        assert!(s.sql.contains("BETWEEN '2025-02-01' AND '2025-02-28'"));
    }

    #[test]
    fn dimension_compare_uses_full_outer_join() {
        let p = ComparePeriod
            .plan(
                &serde_json::json!({
                    "db": "default", "table": "orders", "date_column": "d",
                    "measure_sql": "sum(amount)", "dimension": "channel",
                    "current_start": "2025-02-01", "current_end": "2025-02-28",
                    "baseline_start": "2025-01-01", "baseline_end": "2025-01-31"
                }),
                &ctx(),
            )
            .unwrap();
        let AnalysisStep::Sql(s) = &p.steps[0] else {
            panic!("expected Sql step")
        };
        assert!(s.sql.contains("FULL OUTER JOIN"));
        assert!(s.sql.contains("delta"));
    }
}
