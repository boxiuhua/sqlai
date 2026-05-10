use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::{quote_ident, quote_lit};
use crate::{AnalysisSkill, SkillSchema};

pub struct DistributionShift;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    value_column: String,
    date_column: String,
    current_start: String,
    current_end: String,
    baseline_start: String,
    baseline_end: String,
    #[serde(default = "default_bins")]
    bins: u32,
}

fn default_bins() -> u32 {
    10
}

impl AnalysisSkill for DistributionShift {
    fn name(&self) -> &'static str {
        "distribution_shift"
    }
    fn description(&self) -> &'static str {
        "对比两个时段单一数值列的分布。返回每个分位段在 current/baseline 下的频次。"
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
                    "value_column": {"type":"string"},
                    "date_column": {"type":"string"},
                    "current_start": {"type":"string"},
                    "current_end": {"type":"string"},
                    "baseline_start": {"type":"string"},
                    "baseline_end": {"type":"string"},
                    "bins": {"type":"integer","minimum":2,"maximum":100,"default":10}
                },
                "required": ["db","table","value_column","date_column","current_start","current_end","baseline_start","baseline_end"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("distribution_shift", e.to_string()))?;
        if a.bins < 2 {
            return Err(SkillError::InvalidArg("bins", "must be >= 2".into()));
        }
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let vc = quote_ident(&a.value_column);
        let dc = quote_ident(&a.date_column);
        let cs = quote_lit(&a.current_start);
        let ce = quote_lit(&a.current_end);
        let bs = quote_lit(&a.baseline_start);
        let be = quote_lit(&a.baseline_end);
        let bins = a.bins;

        // 简化版本：用 ClickHouse histogram 函数对 current 和 baseline 两段分别建直方图，再 UNION。
        // histogram(N)(v) 返回 Tuple(low, high, count) 的数组；arrayJoin 展开。
        let sql = format!(
            "SELECT 'current' AS period, lo, hi, c FROM ( \
                 SELECT arrayJoin(histogram({bins})({vc})) AS h, h.1 AS lo, h.2 AS hi, h.3 AS c \
                 FROM {t} WHERE {dc} BETWEEN {cs} AND {ce} \
             ) \
             UNION ALL \
             SELECT 'baseline' AS period, lo, hi, c FROM ( \
                 SELECT arrayJoin(histogram({bins})({vc})) AS h, h.1 AS lo, h.2 AS hi, h.3 AS c \
                 FROM {t} WHERE {dc} BETWEEN {bs} AND {be} \
             ) \
             ORDER BY period, lo",
            bins = bins,
            vc = vc,
            t = table,
            dc = dc,
            cs = cs,
            ce = ce,
            bs = bs,
            be = be,
        );

        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!("{}.{} 分布对比", a.db, a.table),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Bar,
                x: Some("lo".into()),
                y: Some("c".into()),
            }),
            explanation: format!("把 {} 切成 {} 段做时段分布差", a.value_column, a.bins),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    #[test]
    fn renders_default_bins_10() {
        let p = DistributionShift
            .plan(
                &serde_json::json!({
                    "db":"d","table":"t","value_column":"v","date_column":"dc",
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
        assert!(s.sql.contains("histogram(10)"));
        assert!(s.sql.contains("'current' AS period"));
        assert!(s.sql.contains("'baseline' AS period"));
    }
}
