//! cluster_kmeans：从一张表里抽 numeric 列，调 sidecar 跑 K-means。

use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, MlStep, SqlStep};
use crate::render::quote_ident;
use crate::{AnalysisSkill, SkillSchema};

pub struct ClusterKmeans;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    feature_columns: Vec<String>,
    #[serde(default = "default_n_clusters")]
    n_clusters: u32,
    #[serde(default)]
    where_clause: Option<String>,
    #[serde(default = "default_sample_limit")]
    sample_limit: u32,
}

fn default_n_clusters() -> u32 {
    3
}
fn default_sample_limit() -> u32 {
    5000
}

impl AnalysisSkill for ClusterKmeans {
    fn name(&self) -> &'static str {
        "cluster_kmeans"
    }
    fn description(&self) -> &'static str {
        "对指定数值列做 K-means 聚类。先 SQL 取样本，再调 sidecar 训练 + 预测，输出每行的 cluster 标签。"
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
                    "feature_columns": {"type":"array","items":{"type":"string"},"minItems":2,"maxItems":12},
                    "n_clusters":      {"type":"integer","default":3,"minimum":2,"maximum":20},
                    "sample_limit":    {"type":"integer","default":5000,"minimum":50,"maximum":100000},
                    "where_clause":    {"type":"string"}
                },
                "required": ["db","table","feature_columns"]
            }),
        }
    }
    fn plan(
        &self,
        args: &serde_json::Value,
        _ctx: &RetrievalContext,
    ) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("cluster_kmeans", e.to_string()))?;
        if a.feature_columns.len() < 2 {
            return Err(SkillError::InvalidArg(
                "feature_columns",
                "need >= 2".into(),
            ));
        }
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let select = a
            .feature_columns
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
                    label: format!("{}.{} 抽取特征", a.db, a.table),
                    sql,
                }),
                AnalysisStep::Ml(MlStep {
                    label: format!("K-means k={}", a.n_clusters),
                    task: "kmeans".into(),
                    source_step: 0,
                    feature_columns: a.feature_columns.clone(),
                    params: serde_json::json!({"n_clusters": a.n_clusters, "random_state": 42}),
                }),
            ],
            chart_hint: Some(ChartHint {
                kind: ChartKind::None,
                x: a.feature_columns.first().cloned(),
                y: a.feature_columns.get(1).cloned(),
            }),
            explanation: format!(
                "对 {} 跑 K-means k={}",
                a.feature_columns.join(","),
                a.n_clusters
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
    fn plan_has_sql_then_ml_step() {
        let p = ClusterKmeans
            .plan(
                &serde_json::json!({
                    "db":"d","table":"t","feature_columns":["x","y"],"n_clusters":3
                }),
                &ctx(),
            )
            .unwrap();
        assert_eq!(p.steps.len(), 2);
        assert!(matches!(p.steps[0], AnalysisStep::Sql(_)));
        assert!(matches!(p.steps[1], AnalysisStep::Ml(_)));
    }

    #[test]
    fn fewer_than_2_features_rejected() {
        let err = ClusterKmeans
            .plan(
                &serde_json::json!({
                    "db":"d","table":"t","feature_columns":["x"]
                }),
                &ctx(),
            )
            .unwrap_err();
        assert!(matches!(
            err,
            SkillError::InvalidArg("cluster_kmeans", _)
                | SkillError::InvalidArg("feature_columns", _)
        ));
    }
}
