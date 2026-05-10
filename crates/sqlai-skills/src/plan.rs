use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisPlan {
    pub steps: Vec<AnalysisStep>,
    pub chart_hint: Option<ChartHint>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalysisStep {
    Sql(SqlStep),
    Compute(ComputeStep),
    Ml(MlStep),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlStep {
    pub label: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeStep {
    pub label: String,
    pub function: ComputeFn,
    pub source_step: usize,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComputeFn {
    MovingAverage,
    LinearExtrapolation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlStep {
    pub label: String,
    pub task: String,
    pub source_step: usize,
    pub feature_columns: Vec<String>,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartHint {
    pub kind: ChartKind,
    pub x: Option<String>,
    pub y: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
    None,
}
