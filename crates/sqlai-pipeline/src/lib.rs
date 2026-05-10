//! sqlai-pipeline：v1.0 核心问答流水线。

pub mod compute;
pub mod event;
pub mod intent;
pub mod postprocess;
pub mod retrieval;
pub mod runner;
pub mod selector;

pub use event::{
    ChartSpec, MetricRecommendation, PipelineEvent, PlanSnapshot, SqlStepSnapshot, StepResult,
};

use sqlai_core::IntentDecision;
use sqlai_exec::Executor;
use sqlai_llm::{ChatMessage, EmbeddingProvider, LlmProvider};
use sqlai_skills::SkillRegistry;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone)]
pub struct Pipeline {
    pub llm: Arc<dyn LlmProvider>,
    pub embedder: Arc<dyn EmbeddingProvider>,
    pub pool: PgPool,
    pub executor: Arc<dyn Executor>,
    pub skills: Arc<SkillRegistry>,
    pub ml_client: Option<Arc<sqlai_llm::sidecar::SidecarMlClient>>,
}

#[derive(Debug, Clone)]
pub struct AskRequest {
    pub session_id: Uuid,
    pub datasource_id: Uuid,
    pub question: String,
    pub history: Vec<ChatMessage>,
}

impl Pipeline {
    /// 启动一次问答。返回事件流 channel。
    pub fn ask(&self, req: AskRequest) -> mpsc::Receiver<PipelineEvent> {
        let (tx, rx) = mpsc::channel(64);
        let me = self.clone();
        tokio::spawn(async move {
            let started = Instant::now();
            if let Err(e) = me.drive(&req, &tx).await {
                let _ = tx
                    .send(PipelineEvent::Error {
                        stage: e.stage.to_string(),
                        code: e.code.to_string(),
                        message: e.message,
                    })
                    .await;
            }
            let _ = tx
                .send(PipelineEvent::Done {
                    latency_ms: started.elapsed().as_millis() as u64,
                })
                .await;
        });
        rx
    }

    async fn drive(
        &self,
        req: &AskRequest,
        tx: &mpsc::Sender<PipelineEvent>,
    ) -> Result<(), StageErr> {
        // [1] 意图
        let intent = intent::classify(&self.llm, &req.question, &req.history)
            .await
            .map_err(|e| StageErr::new("intent", "llm", e.to_string()))?;
        let _ = tx.send(PipelineEvent::Intent(intent.clone())).await;
        match intent {
            IntentDecision::Direct { .. } => { /* 继续 */ }
            IntentDecision::Clarify { .. } | IntentDecision::Reject { .. } => return Ok(()),
        }

        // [2] 检索
        let cfg = retrieval::RetrievalConfig::default();
        let ctx = retrieval::collect(
            &self.pool,
            &self.embedder,
            req.datasource_id,
            &req.question,
            &cfg,
        )
        .await
        .map_err(|e| StageErr::new("retrieval", "store", e.to_string()))?;

        // [3] 选 skill（含 EXPLAIN 自修复，最多重试 2 次）
        const MAX_RETRIES: u32 = 2;
        let mut last_err: Option<String> = None;
        let mut chosen: Option<(String, serde_json::Value, sqlai_skills::AnalysisPlan)> = None;
        let mut retries: u32 = 0;
        for attempt in 0..=MAX_RETRIES {
            let (skill_name, args, plan) = selector::select_and_plan(
                &self.llm,
                &self.skills,
                &req.question,
                &ctx,
                last_err.as_deref(),
            )
            .await
            .map_err(|e| StageErr::new("select", "skill", e.to_string()))?;
            match runner::validate_plan_only(&self.executor, &plan).await {
                Ok(()) => {
                    chosen = Some((skill_name, args, plan));
                    break;
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    retries = attempt + 1;
                    let _ = tx
                        .send(PipelineEvent::Validate {
                            passed: false,
                            retries,
                            error: Some(e.to_string()),
                        })
                        .await;
                    if attempt == MAX_RETRIES {
                        return Err(StageErr::new("validate", "exec", e.to_string()));
                    }
                }
            }
        }
        let (skill_name, args, plan) = chosen.expect("loop exited without choice or error");

        let _ = tx
            .send(PipelineEvent::SkillCall {
                skill: skill_name,
                args,
                plan: PlanSnapshot {
                    steps: plan
                        .steps
                        .iter()
                        .map(|s| match s {
                            sqlai_skills::AnalysisStep::Sql(x) => SqlStepSnapshot::from(x),
                            sqlai_skills::AnalysisStep::Compute(c) => SqlStepSnapshot {
                                label: c.label.clone(),
                                sql: format!("[compute:{:?}]", c.function),
                            },
                            sqlai_skills::AnalysisStep::Ml(m) => SqlStepSnapshot {
                                label: m.label.clone(),
                                sql: format!("[ml:{}]", m.task),
                            },
                        })
                        .collect(),
                    explanation: plan.explanation.clone(),
                },
            })
            .await;

        // [4-5] 校验 + 执行
        let runs = runner::validate_and_run(&self.executor, self.ml_client.as_ref(), &plan)
            .await
            .map_err(|e| {
                let stage = match &e {
                    runner::RunnerError::Validate { .. } | runner::RunnerError::Explain { .. } => {
                        "validate"
                    }
                    runner::RunnerError::Execute { .. } => "execute",
                    _ => "execute",
                };
                StageErr::new(stage, "exec", e.to_string())
            })?;
        let _ = tx
            .send(PipelineEvent::Validate {
                passed: true,
                retries,
                error: None,
            })
            .await;
        for (i, r) in runs.iter().enumerate() {
            let _ = tx
                .send(PipelineEvent::Rows(StepResult {
                    step_index: i,
                    label: r.label.clone(),
                    columns: r.result.columns.clone(),
                    rows: r.result.rows.clone(),
                    truncated: r.result.truncated,
                }))
                .await;
        }

        // [6] 后处理
        let chart = postprocess::chart_spec_for(plan.chart_hint.as_ref());
        let _ = tx.send(PipelineEvent::Chart(chart)).await;
        let recs = postprocess::metric_recommendations(&runs);
        let _ = tx.send(PipelineEvent::MetricsRecommend(recs)).await;
        let summary = postprocess::summarize(&self.llm, &req.question, &runs)
            .await
            .map_err(|e| StageErr::new("postprocess", "llm", e.to_string()))?;
        let _ = tx.send(PipelineEvent::Summary { text: summary }).await;
        Ok(())
    }
}

struct StageErr {
    stage: &'static str,
    code: &'static str,
    message: String,
}
impl StageErr {
    fn new(stage: &'static str, code: &'static str, message: String) -> Self {
        Self {
            stage,
            code,
            message,
        }
    }
}
