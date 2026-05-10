//! classify_logreg：从一张表抽数值特征 + 整数标签，调 sidecar 跑逻辑回归。

use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, MlStep, SqlStep};
use crate::render::quote_ident;
use crate::{AnalysisSkill, SkillSchema};

pub struct ClassifyLogreg;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    feature_columns: Vec<String>,
    label_column: String,
    #[serde(default)]
    where_clause: Option<String>,
    #[serde(default = "default_sample_limit")]
    sample_limit: u32,
    #[serde(default = "default_test_size")]
    test_size: f32,
}
fn default_sample_limit() -> u32 {
    5000
}
fn default_test_size() -> f32 {
    0.2
}

impl AnalysisSkill for ClassifyLogreg {
    fn name(&self) -> &'static str {
        "classify_logreg"
    }
    fn description(&self) -> &'static str {
        "对一张表的一组数值特征列 + 一个整数标签列做逻辑回归分类。返回训练集大小、测试集大小与 accuracy。"
    }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db":              {"type":"string"},
                    "table":           {"type":"string"},
                    "feature_columns": {"type":"array","items":{"type":"string"},"minItems":1,"maxItems":12},
                    "label_column":    {"type":"string"},
                    "test_size":       {"type":"number","default":0.2,"minimum":0.05,"maximum":0.5},
                    "sample_limit":    {"type":"integer","default":5000,"minimum":50,"maximum":100000},
                    "where_clause":    {"type":"string"}
                },
                "required": ["db","table","feature_columns","label_column"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("classify_logreg", e.to_string()))?;
        if a.feature_columns.is_empty() {
            return Err(SkillError::InvalidArg(
                "feature_columns",
                "need >= 1".into(),
            ));
        }

        // sidecar's _logreg expects last column as label, so append label_column to features
        let mut all = a.feature_columns.clone();
        all.push(a.label_column.clone());
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let select = all
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let where_sql = match &a.where_clause {
            Some(w) if !w.trim().is_empty() => format!(" WHERE ({w})"),
            _ => String::new(),
        };
        let sql = format!(
            "SELECT {select} FROM {table}{where_sql} LIMIT {n}",
            n = a.sample_limit
        );

        Ok(AnalysisPlan {
            steps: vec![
                AnalysisStep::Sql(SqlStep {
                    label: format!("{}.{} 抽特征+标签", a.db, a.table),
                    sql,
                }),
                AnalysisStep::Ml(MlStep {
                    label: "logistic regression".into(),
                    task: "classify_logreg".into(),
                    source_step: 0,
                    feature_columns: all,
                    params: serde_json::json!({"test_size": a.test_size, "random_state": 42}),
                }),
            ],
            chart_hint: Some(ChartHint {
                kind: ChartKind::None,
                x: None,
                y: None,
            }),
            explanation: format!(
                "用 {} 个特征预测 {}，测试集占比 {}",
                a.feature_columns.len(),
                a.label_column,
                a.test_size
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
    fn plan_includes_label_as_last_feature_column() {
        let p = ClassifyLogreg
            .plan(
                &serde_json::json!({
                    "db":"d","table":"t","feature_columns":["x","y"],"label_column":"is_paid"
                }),
                &ctx(),
            )
            .unwrap();
        assert_eq!(p.steps.len(), 2);
        if let AnalysisStep::Ml(m) = &p.steps[1] {
            assert_eq!(
                m.feature_columns,
                vec!["x".to_string(), "y".to_string(), "is_paid".to_string()]
            );
            assert_eq!(m.task, "classify_logreg");
        } else {
            panic!("expected Ml step");
        }
    }
}
