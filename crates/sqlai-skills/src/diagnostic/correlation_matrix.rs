use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, SqlStep};
use crate::render::quote_ident;
use crate::{AnalysisSkill, SkillSchema};

pub struct CorrelationMatrix;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    columns: Vec<String>,
    #[serde(default)]
    where_clause: Option<String>,
}

impl AnalysisSkill for CorrelationMatrix {
    fn name(&self) -> &'static str {
        "correlation_matrix"
    }
    fn description(&self) -> &'static str {
        "对一组数值列两两计算 Pearson 相关系数，长表输出 (col1, col2, corr)。"
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
                    "columns": {"type":"array","items":{"type":"string"},"minItems":2,"maxItems":12},
                    "where_clause": {"type":"string"}
                },
                "required": ["db","table","columns"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("correlation_matrix", e.to_string()))?;
        if a.columns.len() < 2 {
            return Err(SkillError::InvalidArg(
                "columns",
                "need at least 2 columns".into(),
            ));
        }
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let where_sql = match &a.where_clause {
            Some(w) if !w.trim().is_empty() => format!(" WHERE ({w})"),
            _ => String::new(),
        };
        let mut parts = Vec::new();
        for i in 0..a.columns.len() {
            for j in (i + 1)..a.columns.len() {
                let ci = quote_ident(&a.columns[i]);
                let cj = quote_ident(&a.columns[j]);
                let li = format!("'{}'", a.columns[i].replace('\'', "''"));
                let lj = format!("'{}'", a.columns[j].replace('\'', "''"));
                parts.push(format!(
                    "SELECT {li} AS col1, {lj} AS col2, corr({ci}, {cj}) AS corr FROM {table}{where_sql}"
                ));
            }
        }
        let sql = parts.join(" UNION ALL ");

        Ok(AnalysisPlan {
            steps: vec![AnalysisStep::Sql(SqlStep {
                label: format!("{}.{} 相关性矩阵", a.db, a.table),
                sql,
            })],
            chart_hint: Some(ChartHint {
                kind: ChartKind::None,
                x: None,
                y: None,
            }),
            explanation: format!("对 {} 列两两计算相关系数", a.columns.len()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    #[test]
    fn correlation_emits_n_choose_2_unions() {
        let p = CorrelationMatrix
            .plan(
                &serde_json::json!({"db":"d","table":"t","columns":["a","b","c"]}),
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
        assert_eq!(s.sql.matches("UNION ALL").count(), 2);
        assert!(s.sql.contains("corr(`a`, `b`)"));
        assert!(s.sql.contains("corr(`a`, `c`)"));
        assert!(s.sql.contains("corr(`b`, `c`)"));
    }

    #[test]
    fn fewer_than_2_columns_rejected() {
        let err = CorrelationMatrix
            .plan(
                &serde_json::json!({"db":"d","table":"t","columns":["a"]}),
                &RetrievalContext {
                    tables: vec![],
                    columns: vec![],
                    business_terms: vec![],
                    few_shots: vec![],
                },
            )
            .unwrap_err();
        assert!(matches!(
            err,
            SkillError::InvalidArg("correlation_matrix", _) | SkillError::InvalidArg("columns", _)
        ));
    }
}
