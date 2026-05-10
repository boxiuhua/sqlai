//! 阶段 4-5：本地校验 + 远端 EXPLAIN + 真正执行（含 Compute / Ml 步骤）。

use sqlai_dialect::{validate, ValidationError};
use sqlai_exec::{ExecError, ExecutionResult, Executor};
use sqlai_llm::sidecar::SidecarMlClient;
use sqlai_skills::{AnalysisPlan, AnalysisStep};
use std::sync::Arc;

use crate::compute::{run_compute, ComputeError};

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("validation failed at step {step_idx}: {err}")]
    Validate { step_idx: usize, err: String },
    #[error("explain failed at step {step_idx}: {err}")]
    Explain { step_idx: usize, err: String },
    #[error("execute failed at step {step_idx}: {err}")]
    Execute { step_idx: usize, err: String },
    #[error("compute failed at step {step_idx}: {err}")]
    Compute { step_idx: usize, err: String },
    #[error("ml not configured but plan has Ml step at {step_idx}")]
    MlNotAvailable { step_idx: usize },
    #[error("ml step {step_idx}: {err}")]
    Ml { step_idx: usize, err: String },
    #[error("invalid step reference: step {step_idx} cites source_step {src} which doesn't exist")]
    BadSourceStep { step_idx: usize, src: usize },
}

pub struct StepRun {
    pub label: String,
    pub result: ExecutionResult,
}

/// 只做本地 SELECT-only 校验 + 远端 EXPLAIN，不执行。
/// 让 pipeline 在 select 阶段就能感知 SQL 是否合法，做 EXPLAIN 自修复。
pub async fn validate_plan_only(
    executor: &Arc<dyn Executor>,
    plan: &AnalysisPlan,
) -> Result<(), RunnerError> {
    for (idx, step) in plan.steps.iter().enumerate() {
        if let AnalysisStep::Sql(s) = step {
            let validated =
                validate(&s.sql).map_err(|e: ValidationError| RunnerError::Validate {
                    step_idx: idx,
                    err: e.to_string(),
                })?;
            executor
                .explain(&validated)
                .await
                .map_err(|e: ExecError| RunnerError::Explain {
                    step_idx: idx,
                    err: e.to_string(),
                })?;
        }
    }
    Ok(())
}

pub async fn validate_and_run(
    executor: &Arc<dyn Executor>,
    ml: Option<&Arc<SidecarMlClient>>,
    plan: &AnalysisPlan,
) -> Result<Vec<StepRun>, RunnerError> {
    let mut out: Vec<StepRun> = Vec::new();
    for (idx, step) in plan.steps.iter().enumerate() {
        match step {
            AnalysisStep::Sql(s) => {
                let validated =
                    validate(&s.sql).map_err(|e: ValidationError| RunnerError::Validate {
                        step_idx: idx,
                        err: e.to_string(),
                    })?;
                executor.explain(&validated).await.map_err(|e: ExecError| {
                    RunnerError::Explain {
                        step_idx: idx,
                        err: e.to_string(),
                    }
                })?;
                let r = executor
                    .run(&validated)
                    .await
                    .map_err(|e| RunnerError::Execute {
                        step_idx: idx,
                        err: e.to_string(),
                    })?;
                out.push(StepRun {
                    label: s.label.clone(),
                    result: r,
                });
            }
            AnalysisStep::Compute(c) => {
                let src = out.get(c.source_step).ok_or(RunnerError::BadSourceStep {
                    step_idx: idx,
                    src: c.source_step,
                })?;
                let r = run_compute(c.function, &c.params, &src.result).map_err(
                    |e: ComputeError| RunnerError::Compute {
                        step_idx: idx,
                        err: e.to_string(),
                    },
                )?;
                out.push(StepRun {
                    label: c.label.clone(),
                    result: r,
                });
            }
            AnalysisStep::Ml(m) => {
                let ml = ml.ok_or(RunnerError::MlNotAvailable { step_idx: idx })?;
                let src = out.get(m.source_step).ok_or(RunnerError::BadSourceStep {
                    step_idx: idx,
                    src: m.source_step,
                })?;
                let body = serde_json::json!({
                    "task": m.task,
                    "params": m.params,
                    "data": project_features(&src.result, &m.feature_columns).map_err(|e| {
                        RunnerError::Ml { step_idx: idx, err: e }
                    })?,
                });
                let res = ml.run(&body).await.map_err(|e| RunnerError::Ml {
                    step_idx: idx,
                    err: e.to_string(),
                })?;
                let merged =
                    merge_ml_into_rows(&src.result, &res, &m.feature_columns).map_err(|e| {
                        RunnerError::Ml {
                            step_idx: idx,
                            err: e,
                        }
                    })?;
                out.push(StepRun {
                    label: m.label.clone(),
                    result: merged,
                });
            }
        }
    }
    Ok(out)
}

fn project_features(input: &ExecutionResult, cols: &[String]) -> Result<serde_json::Value, String> {
    let mut data = Vec::with_capacity(input.rows.len());
    for r in &input.rows {
        let mut row = Vec::with_capacity(cols.len());
        for c in cols {
            let v = r
                .get(c)
                .and_then(|v| v.as_f64())
                .ok_or_else(|| format!("non-numeric or missing column '{c}' in source rows"))?;
            row.push(v);
        }
        data.push(row);
    }
    Ok(serde_json::to_value(&data).unwrap_or_default())
}

fn merge_ml_into_rows(
    src: &ExecutionResult,
    ml_resp: &serde_json::Value,
    feature_cols: &[String],
) -> Result<ExecutionResult, String> {
    let task = ml_resp.get("task").and_then(|v| v.as_str()).unwrap_or("");
    let result = ml_resp.get("result").ok_or("ml resp missing 'result'")?;

    if task == "classify_logreg" {
        // 单行报表：accuracy / n_train / n_test
        let acc = result
            .get("accuracy")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let n_train = result.get("n_train").and_then(|v| v.as_i64()).unwrap_or(0);
        let n_test = result.get("n_test").and_then(|v| v.as_i64()).unwrap_or(0);
        let row = serde_json::json!({
            "accuracy": acc,
            "n_train": n_train,
            "n_test": n_test,
        });
        return Ok(ExecutionResult {
            columns: vec!["accuracy".into(), "n_train".into(), "n_test".into()],
            rows: vec![row],
            truncated: false,
        });
    }

    // kmeans 等带 labels 的：把 cluster 列追加到原行
    let labels: Vec<i64> = result
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();
    if labels.len() != src.rows.len() {
        return Err(format!(
            "ml labels.len()={} doesn't match rows.len()={}",
            labels.len(),
            src.rows.len()
        ));
    }
    let mut new_cols = feature_cols.to_vec();
    new_cols.push("cluster".into());
    let new_rows: Vec<serde_json::Value> = src
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut o = serde_json::Map::new();
            for c in feature_cols {
                o.insert(
                    c.clone(),
                    r.get(c).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            o.insert("cluster".into(), serde_json::Value::from(labels[i]));
            serde_json::Value::Object(o)
        })
        .collect();
    Ok(ExecutionResult {
        columns: new_cols,
        rows: new_rows,
        truncated: false,
    })
}
