use serde::{Deserialize, Serialize};

use sqlai_core::IntentDecision;
use sqlai_skills::{ChartHint, ChartKind, SqlStep};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSpec {
    pub kind: String,
    pub x: Option<String>,
    pub y: Option<String>,
}

impl From<&ChartHint> for ChartSpec {
    fn from(h: &ChartHint) -> Self {
        let kind = match h.kind {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Pie => "pie",
            ChartKind::None => "none",
        };
        Self {
            kind: kind.to_string(),
            x: h.x.clone(),
            y: h.y.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_index: usize,
    pub label: String,
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Value>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSnapshot {
    pub steps: Vec<SqlStepSnapshot>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlStepSnapshot {
    pub label: String,
    pub sql: String,
}

impl From<&SqlStep> for SqlStepSnapshot {
    fn from(s: &SqlStep) -> Self {
        Self {
            label: s.label.clone(),
            sql: s.sql.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRecommendation {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PipelineEvent {
    Intent(IntentDecision),
    SkillCall {
        skill: String,
        args: serde_json::Value,
        plan: PlanSnapshot,
    },
    Validate {
        passed: bool,
        retries: u32,
        error: Option<String>,
    },
    Rows(StepResult),
    Chart(ChartSpec),
    MetricsRecommend(Vec<MetricRecommendation>),
    Summary {
        text: String,
    },
    Done {
        latency_ms: u64,
    },
    Error {
        stage: String,
        code: String,
        message: String,
    },
}
