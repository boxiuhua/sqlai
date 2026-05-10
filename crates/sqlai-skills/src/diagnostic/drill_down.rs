use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::{quote_ident, quote_lit};
use crate::{AnalysisSkill, SkillSchema};

pub struct DrillDown;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    date_column: String,
    measure_sql: String,
    dimensions: Vec<String>,
    current_start: String,
    current_end: String,
    baseline_start: String,
    baseline_end: String,
}

impl AnalysisSkill for DrillDown {
    fn name(&self) -> &'static str {
        "drill_down"
    }
    fn description(&self) -> &'static str {
        "按一组维度对比两个时段的指标差异，找贡献度最大的维度组合。"
    }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db": {"type":"string"},
                    "table": {"type":"string"},
                    "date_column": {"type":"string"},
                    "measure_sql": {"type":"string"},
                    "dimensions": {"type":"array","items":{"type":"string"},"minItems":1,"maxItems":4},
                    "current_start": {"type":"string"},
                    "current_end": {"type":"string"},
                    "baseline_start": {"type":"string"},
                    "baseline_end": {"type":"string"}
                },
                "required": ["db","table","date_column","measure_sql","dimensions","current_start","current_end","baseline_start","baseline_end"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("drill_down", e.to_string()))?;
        if a.dimensions.is_empty() {
            return Err(SkillError::InvalidArg(
                "dimensions",
                "must have at least 1 dimension".into(),
            ));
        }
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let dc = quote_ident(&a.date_column);
        let dims: Vec<String> = a.dimensions.iter().map(|d| quote_ident(d)).collect();
        let dim_select: String = dims
            .iter()
            .enumerate()
            .map(|(i, q)| format!("{q} AS dim{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let dim_group: String = (0..dims.len())
            .map(|i| format!("dim{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let cs = quote_lit(&a.current_start);
        let ce = quote_lit(&a.current_end);
        let bs = quote_lit(&a.baseline_start);
        let be = quote_lit(&a.baseline_end);
        let join_dims = (0..dims.len())
            .map(|i| format!("coalesce(cur.dim{i}, base.dim{i}) AS dim{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let join_cond = (0..dims.len())
            .map(|i| format!("cur.dim{i} = base.dim{i}"))
            .collect::<Vec<_>>()
            .join(" AND ");
        let sql = format!(
            "WITH cur AS (SELECT {ds}, {m} AS v FROM {t} WHERE {dc} BETWEEN {cs} AND {ce} GROUP BY {dg}), \
             base AS (SELECT {ds}, {m} AS v FROM {t} WHERE {dc} BETWEEN {bs} AND {be} GROUP BY {dg}) \
             SELECT {join_dims}, coalesce(cur.v, 0) AS current, coalesce(base.v, 0) AS baseline, \
                    coalesce(cur.v, 0) - coalesce(base.v, 0) AS delta \
             FROM cur FULL OUTER JOIN base ON {join_cond} \
             ORDER BY abs(delta) DESC LIMIT 200",
            ds = dim_select, m = a.measure_sql, t = table, dc = dc,
            cs = cs, ce = ce, bs = bs, be = be, dg = dim_group,
            join_dims = join_dims, join_cond = join_cond,
        );
        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!(
                    "{}.{} 按 {} 维度归因",
                    a.db,
                    a.table,
                    a.dimensions.join("/")
                ),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Bar,
                x: Some("dim0".into()),
                y: Some("delta".into()),
            }),
            explanation: format!("按 {:?} 拆解期间差异", a.dimensions),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    #[test]
    fn renders_drill_down_sql() {
        let p = DrillDown
            .plan(
                &serde_json::json!({
                    "db":"default","table":"orders","date_column":"d","measure_sql":"sum(amount)",
                    "dimensions":["channel","city"],
                    "current_start":"2025-02-01","current_end":"2025-02-28",
                    "baseline_start":"2025-01-01","baseline_end":"2025-01-31"
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
        assert!(s.sql.contains("dim0"));
        assert!(s.sql.contains("dim1"));
        assert!(s.sql.contains("FULL OUTER JOIN"));
        assert!(s.sql.contains("ORDER BY abs(delta) DESC"));
    }
}
