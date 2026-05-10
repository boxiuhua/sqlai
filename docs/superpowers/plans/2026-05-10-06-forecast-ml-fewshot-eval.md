# 智能问数系统 v1.0 — 子计划 #6：轻预测 + ML skill + few-shot 反馈闭环 + GoldenSet eval

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 v1.0 spec 中尚未实现的核心：轻预测（Compute step）+ ML skill（sidecar `/ml/run`）+ few-shot 反馈与检索 + GoldenSet 准确率回归框架，全部补齐。

**Architecture:** 扩展 `AnalysisStep` 枚举至 3 种变体（Sql / Compute / Ml）；`Compute` 在 Rust 内做移动均值 / 线性外推；`Ml` 走 `SidecarMlClient::run`。pipeline 的 runner 按 step 顺序执行，后置步骤可以引用前置步骤的结果。few-shot 用一张表（migration 已有）+ 检索阶段拉 top-K，selector 阶段塞进 prompt。GoldenSet eval 是 `sqlai-cli eval` 的子命令，跑一份 JSON 题库后输出准确率报表。

**Tech Stack:** 全部沿用已有依赖；不引入新外部 crate（forecast 用纯 Rust 写）。

**前置假设：**
- #1-#5 完成（39 commit）。
- sidecar `/ml/run` 已经在 #2 实现（kmeans + logreg），可直接调用。

---

## File Structure

```
sqlai/
├── crates/
│   ├── sqlai-skills/
│   │   └── src/
│   │       ├── plan.rs                # 扩展 AnalysisStep + 新增 ComputeStep / MlStep
│   │       ├── lib.rs                 # 注册 forecast_simple + cluster_kmeans
│   │       ├── compute/               # NEW
│   │       │   ├── mod.rs
│   │       │   └── forecast_simple.rs
│   │       └── ml/
│   │           ├── mod.rs
│   │           └── cluster_kmeans.rs
│   ├── sqlai-pipeline/
│   │   ├── Cargo.toml                 # （无需改）
│   │   └── src/
│   │       ├── runner.rs              # 增加 Compute / Ml 分支
│   │       ├── compute.rs             # NEW：Rust 内置计算函数
│   │       ├── retrieval.rs           # 加 few_shot top-K
│   │       ├── selector.rs            # 把 few_shot 注入 prompt
│   │       └── lib.rs                 # AppState 加 ml_client
│   ├── sqlai-store/
│   │   └── src/
│   │       ├── lib.rs                 # +pub mod few_shot
│   │       └── few_shot.rs            # NEW：CRUD + 向量检索
│   ├── sqlai-api/
│   │   └── src/routes/admin.rs        # +few_shot CRUD endpoints
│   └── sqlai-cli/
│       ├── src/
│       │   ├── main.rs                # +eval 子命令
│       │   └── eval.rs                # NEW：GoldenSet 评测器
│       └── tests/
│           └── eval_smoke.rs          # NEW（可选）
└── docs/superpowers/specs/
    └── golden-set-example.json        # 题库示例
```

---

## Task 1：AnalysisStep 扩展 + Compute/Ml 类型 + 跑通 forecast 单元测试

**Files:**
- Modify: `crates/sqlai-skills/src/plan.rs`
- Create: `crates/sqlai-skills/src/compute/mod.rs`
- Create: `crates/sqlai-skills/src/compute/forecast_simple.rs`
- Create: `crates/sqlai-skills/src/ml/mod.rs`（占位）
- Modify: `crates/sqlai-skills/src/lib.rs`

- [ ] **Step 1：扩展 plan.rs**

```rust
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
    pub source_step: usize,       // 取上一步结果做输入
    pub params: serde_json::Value, // 函数特定参数
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
    pub task: String,            // "kmeans" / 后续可加 "classify_logreg"
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
```

- [ ] **Step 2：现有 skills 改成显式匹配**

由于 `AnalysisStep` 现在有 3 个变体，所有 skill 测试中的 `let AnalysisStep::Sql(s) = &plan.steps[0];` 写法仍合法（不可反驳模式从单一变体变成可反驳，但 `let ... = ... else { ... }` 在 Rust 1.65+ 的 if-let 写法仍可。但 `let X(s) = expr;` 在多变体 enum 上是 refutable，需要改成 `let X(s) = expr else { panic!() };` 或 `match`。）

**修复方法：** 在所有 skill 单元测试里把
```rust
let AnalysisStep::Sql(s) = &plan.steps[0];
```
替换为
```rust
let AnalysisStep::Sql(s) = &plan.steps[0] else { panic!("expected Sql step") };
```

涉及文件：
- `descriptive/metric_overview.rs`
- `descriptive/topn.rs`
- `descriptive/compare_period.rs`
- `descriptive/share_breakdown.rs`
- `descriptive/trend_segment.rs`
- `diagnostic/drill_down.rs`
- `diagnostic/correlation_matrix.rs`
- `diagnostic/distribution_shift.rs`

- [ ] **Step 3：compute/mod.rs（占位 + forecast_simple 模块声明）**

```rust
pub mod forecast_simple;
```

- [ ] **Step 4：compute/forecast_simple.rs**

```rust
//! forecast_simple：把上一步 SQL 出来的（bucket, value）时间序列做：
//! 1. 移动均值平滑
//! 2. 线性外推 N 期
//! 输出仍然是 (bucket, value, kind) 三列；kind ∈ {"actual","ma","forecast"}。

use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, ComputeFn, ComputeStep, SqlStep};
use crate::render::{quote_ident, quote_lit, time_bucket_clickhouse};
use crate::{AnalysisSkill, SkillSchema};

pub struct ForecastSimple;

#[derive(Debug, Deserialize)]
struct Args {
    db: String,
    table: String,
    date_column: String,
    measure_sql: String,
    granularity: String,
    #[serde(default = "default_window")]
    window: u32,
    #[serde(default = "default_horizon")]
    horizon: u32,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
}
fn default_window() -> u32 { 7 }
fn default_horizon() -> u32 { 7 }

impl AnalysisSkill for ForecastSimple {
    fn name(&self) -> &'static str { "forecast_simple" }
    fn description(&self) -> &'static str {
        "对 (date, measure) 时间序列做移动均值平滑 + 线性外推 N 期。\
         适合 \"未来 7 天 GMV 预估\" \"按周 DAU 走势预测\"。"
    }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "db":          { "type":"string" },
                    "table":       { "type":"string" },
                    "date_column": { "type":"string" },
                    "measure_sql": { "type":"string" },
                    "granularity": { "type":"string", "enum":["day","week","month"] },
                    "window":      { "type":"integer", "default":7, "minimum":2, "maximum":60 },
                    "horizon":     { "type":"integer", "default":7, "minimum":1, "maximum":30 },
                    "start_date":  { "type":"string" },
                    "end_date":    { "type":"string" }
                },
                "required": ["db","table","date_column","measure_sql","granularity"]
            }),
        }
    }
    fn plan(&self, args: &serde_json::Value, _ctx: &RetrievalContext) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("forecast_simple", e.to_string()))?;
        let bucket = time_bucket_clickhouse(&a.date_column, &a.granularity)
            .map_err(|e| SkillError::InvalidArg("granularity", e))?;
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let mut wh: Vec<String> = vec![];
        if let Some(s) = &a.start_date { wh.push(format!("{} >= {}", quote_ident(&a.date_column), quote_lit(s))); }
        if let Some(s) = &a.end_date { wh.push(format!("{} <= {}", quote_ident(&a.date_column), quote_lit(s))); }
        let where_sql = if wh.is_empty() { String::new() } else { format!(" WHERE {}", wh.join(" AND ")) };

        let sql = format!(
            "SELECT {bucket} AS bucket, {m} AS value FROM {t}{w} GROUP BY bucket ORDER BY bucket",
            bucket = bucket, m = a.measure_sql, t = table, w = where_sql,
        );

        let plan = AnalysisPlan {
            steps: vec![
                AnalysisStep::Sql(SqlStep {
                    label: format!("{}.{} 历史聚合", a.db, a.table),
                    sql,
                }),
                AnalysisStep::Compute(ComputeStep {
                    label: format!("移动均值 (window={})", a.window),
                    function: ComputeFn::MovingAverage,
                    source_step: 0,
                    params: serde_json::json!({ "window": a.window }),
                }),
                AnalysisStep::Compute(ComputeStep {
                    label: format!("线性外推 {} 期", a.horizon),
                    function: ComputeFn::LinearExtrapolation,
                    source_step: 0,
                    params: serde_json::json!({
                        "horizon": a.horizon,
                        "granularity": a.granularity
                    }),
                }),
            ],
            chart_hint: Some(ChartHint {
                kind: ChartKind::Line,
                x: Some("bucket".into()),
                y: Some("value".into()),
            }),
            explanation: format!(
                "{}.{} 按{}聚合，移动均值平滑 + 外推 {} 期",
                a.db, a.table, a.granularity, a.horizon
            ),
        };
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    fn ctx() -> RetrievalContext {
        RetrievalContext { tables: vec![], columns: vec![], business_terms: vec![], few_shots: vec![] }
    }

    #[test]
    fn plan_has_three_steps() {
        let p = ForecastSimple.plan(&serde_json::json!({
            "db":"d","table":"t","date_column":"d","measure_sql":"sum(x)",
            "granularity":"day","window":7,"horizon":7
        }), &ctx()).unwrap();
        assert_eq!(p.steps.len(), 3);
        assert!(matches!(p.steps[0], AnalysisStep::Sql(_)));
        assert!(matches!(p.steps[1], AnalysisStep::Compute(_)));
        assert!(matches!(p.steps[2], AnalysisStep::Compute(_)));
    }
}
```

- [ ] **Step 5：ml/mod.rs（占位）**

```rust
pub mod cluster_kmeans;
```

并创建 stub `ml/cluster_kmeans.rs`：

```rust
// 实现在 Task 3。
```

实际上为了让 `pub mod ml` 干净编译，给一个最简实现：

```rust
//! cluster_kmeans skill：详见 Task 3 完整实现。

use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::AnalysisPlan;
use crate::{AnalysisSkill, SkillSchema};

pub struct ClusterKmeans;

impl AnalysisSkill for ClusterKmeans {
    fn name(&self) -> &'static str { "cluster_kmeans" }
    fn description(&self) -> &'static str { "TBD in Task 3" }
    fn schema(&self) -> SkillSchema {
        SkillSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: serde_json::json!({"type":"object"}),
        }
    }
    fn plan(&self, _args: &serde_json::Value, _ctx: &RetrievalContext) -> Result<AnalysisPlan, SkillError> {
        Err(SkillError::Render("cluster_kmeans not yet implemented (Task 3)".into()))
    }
}
```

- [ ] **Step 6：lib.rs 注册**

```rust
pub mod compute;
pub mod ml;
```

并在 `with_defaults` 末尾追加：

```rust
r.register(Arc::new(compute::forecast_simple::ForecastSimple));
// cluster_kmeans 在 Task 3 完整实现后再注册
```

- [ ] **Step 7：跑测试 + commit**

```
cargo test -p sqlai-skills 2>&1 | tail -10
```
预期：20 passed（19 + 1 forecast_simple），加上原 19 中所有 `let AnalysisStep::Sql(s) = ...else{...};` 修订后仍通过。

```
git add crates/sqlai-skills
git commit -m "feat(skills): extend AnalysisStep with Compute+Ml variants; add forecast_simple skill"
```

---

## Task 2：Pipeline runner 支持 Compute step

**Files:**
- Modify: `crates/sqlai-pipeline/Cargo.toml`（确认无需）
- Create: `crates/sqlai-pipeline/src/compute.rs`
- Modify: `crates/sqlai-pipeline/src/runner.rs`
- Modify: `crates/sqlai-pipeline/src/lib.rs`

- [ ] **Step 1：compute.rs（纯 Rust 计算）**

```rust
//! Rust 内置计算：moving average + linear extrapolation。
//!
//! 输入：上一步 SQL 出来的 ExecutionResult，结构假设是 (bucket: String, value: number)。
//! 输出：同形 ExecutionResult，但额外加一列 `kind` ∈ {"actual","ma","forecast"}。

use serde_json::Value;
use sqlai_exec::ExecutionResult;
use sqlai_skills::ComputeFn;

#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    #[error("missing column '{0}' in input rows")]
    MissingColumn(String),

    #[error("invalid param: {0}")]
    InvalidParam(String),

    #[error("non-numeric value at row {0}")]
    NonNumeric(usize),
}

pub fn run_compute(
    function: ComputeFn,
    params: &Value,
    input: &ExecutionResult,
) -> Result<ExecutionResult, ComputeError> {
    let buckets = collect_string_col(input, "bucket")?;
    let values = collect_number_col(input, "value")?;

    match function {
        ComputeFn::MovingAverage => {
            let window = params
                .get("window")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| ComputeError::InvalidParam("window".into()))? as usize;
            if window < 2 {
                return Err(ComputeError::InvalidParam("window must be >= 2".into()));
            }
            let mut rows = Vec::with_capacity(values.len() * 2);
            for (i, b) in buckets.iter().enumerate() {
                rows.push(make_row(b, values[i], "actual"));
            }
            for (i, b) in buckets.iter().enumerate() {
                if i + 1 < window {
                    continue;
                }
                let avg: f64 = values[(i + 1 - window)..=i].iter().sum::<f64>() / window as f64;
                rows.push(make_row(b, avg, "ma"));
            }
            Ok(ExecutionResult {
                columns: vec!["bucket".into(), "value".into(), "kind".into()],
                rows,
                truncated: false,
            })
        }
        ComputeFn::LinearExtrapolation => {
            let horizon = params
                .get("horizon")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| ComputeError::InvalidParam("horizon".into()))?
                as usize;
            let granularity = params
                .get("granularity")
                .and_then(|v| v.as_str())
                .unwrap_or("day");

            // 用所有历史点做最小二乘
            let n = values.len();
            if n < 2 {
                return Err(ComputeError::InvalidParam("need >= 2 points to fit".into()));
            }
            let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
            let (slope, intercept) = lin_reg(&xs, &values);

            let mut rows = Vec::with_capacity(n + horizon);
            for (i, b) in buckets.iter().enumerate() {
                rows.push(make_row(b, values[i], "actual"));
            }
            // 用最后一个 bucket 字符串解析为日期；失败则用占位字符串。
            let last_bucket = buckets.last().cloned().unwrap_or_default();
            for k in 1..=horizon {
                let predicted = intercept + slope * ((n - 1 + k) as f64);
                let label = next_bucket_label(&last_bucket, k, granularity);
                rows.push(make_row(&label, predicted, "forecast"));
            }
            Ok(ExecutionResult {
                columns: vec!["bucket".into(), "value".into(), "kind".into()],
                rows,
                truncated: false,
            })
        }
    }
}

fn make_row(bucket: &str, value: f64, kind: &str) -> Value {
    serde_json::json!({
        "bucket": bucket,
        "value": value,
        "kind": kind,
    })
}

fn collect_string_col(input: &ExecutionResult, name: &str) -> Result<Vec<String>, ComputeError> {
    if !input.columns.iter().any(|c| c == name) {
        return Err(ComputeError::MissingColumn(name.into()));
    }
    Ok(input
        .rows
        .iter()
        .map(|r| {
            r.get(name)
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default()
        })
        .collect())
}

fn collect_number_col(input: &ExecutionResult, name: &str) -> Result<Vec<f64>, ComputeError> {
    if !input.columns.iter().any(|c| c == name) {
        return Err(ComputeError::MissingColumn(name.into()));
    }
    let mut out = Vec::with_capacity(input.rows.len());
    for (i, r) in input.rows.iter().enumerate() {
        let v = r.get(name).and_then(|v| v.as_f64()).ok_or(ComputeError::NonNumeric(i))?;
        out.push(v);
    }
    Ok(out)
}

fn lin_reg(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        num += (x - mean_x) * (y - mean_y);
        den += (x - mean_x).powi(2);
    }
    let slope = if den == 0.0 { 0.0 } else { num / den };
    let intercept = mean_y - slope * mean_x;
    (slope, intercept)
}

fn next_bucket_label(last: &str, k: usize, granularity: &str) -> String {
    // 解析 "YYYY-MM-DD HH:MM:SS" 或 "YYYY-MM-DD"；失败时回退占位。
    use chrono::NaiveDateTime;
    let parsed = NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(&format!("{} 00:00:00", last), "%Y-%m-%d %H:%M:%S"));
    if let Ok(dt) = parsed {
        let next = match granularity {
            "day" => dt + chrono::Duration::days(k as i64),
            "week" => dt + chrono::Duration::weeks(k as i64),
            "month" => {
                // 简化：30 天近似；后续可换 chronoutil。
                dt + chrono::Duration::days((30 * k) as i64)
            }
            _ => dt + chrono::Duration::days(k as i64),
        };
        next.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        format!("{}+{}{}", last, k, granularity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(buckets: &[&str], values: &[f64]) -> ExecutionResult {
        let rows: Vec<Value> = buckets
            .iter()
            .zip(values.iter())
            .map(|(b, v)| serde_json::json!({"bucket": b, "value": v}))
            .collect();
        ExecutionResult {
            columns: vec!["bucket".into(), "value".into()],
            rows,
            truncated: false,
        }
    }

    #[test]
    fn moving_average_window_3() {
        let input = make_input(
            &["2025-01-01","2025-01-02","2025-01-03","2025-01-04","2025-01-05"],
            &[10.0, 20.0, 30.0, 40.0, 50.0],
        );
        let out = run_compute(ComputeFn::MovingAverage, &serde_json::json!({"window":3}), &input).unwrap();
        // 5 actual + 3 ma rows
        assert_eq!(out.rows.len(), 8);
        let ma_rows: Vec<f64> = out.rows.iter().filter(|r| r["kind"]=="ma")
            .map(|r| r["value"].as_f64().unwrap()).collect();
        assert_eq!(ma_rows, vec![20.0, 30.0, 40.0]);
    }

    #[test]
    fn linear_extrapolation_perfect_line() {
        let input = make_input(
            &["2025-01-01","2025-01-02","2025-01-03","2025-01-04","2025-01-05"],
            &[10.0, 20.0, 30.0, 40.0, 50.0],
        );
        let out = run_compute(ComputeFn::LinearExtrapolation,
            &serde_json::json!({"horizon":2,"granularity":"day"}),
            &input).unwrap();
        // 5 actual + 2 forecast
        assert_eq!(out.rows.len(), 7);
        let forecasts: Vec<f64> = out.rows.iter().filter(|r| r["kind"]=="forecast")
            .map(|r| r["value"].as_f64().unwrap()).collect();
        // 完美线性，预测应当是 60, 70（容差 1e-9）
        assert!((forecasts[0] - 60.0).abs() < 1e-9);
        assert!((forecasts[1] - 70.0).abs() < 1e-9);
    }

    #[test]
    fn missing_value_column_errors() {
        let input = ExecutionResult {
            columns: vec!["bucket".into()],
            rows: vec![serde_json::json!({"bucket":"2025-01-01"})],
            truncated: false,
        };
        let err = run_compute(ComputeFn::MovingAverage, &serde_json::json!({"window":2}), &input).unwrap_err();
        assert!(matches!(err, ComputeError::MissingColumn(_)));
    }
}
```

- [ ] **Step 2：扩展 runner.rs**

```rust
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

pub async fn validate_and_run(
    executor: &Arc<dyn Executor>,
    ml: Option<&Arc<SidecarMlClient>>,
    plan: &AnalysisPlan,
) -> Result<Vec<StepRun>, RunnerError> {
    let mut out: Vec<StepRun> = Vec::new();
    for (idx, step) in plan.steps.iter().enumerate() {
        match step {
            AnalysisStep::Sql(s) => {
                let validated = validate(&s.sql).map_err(|e: ValidationError| RunnerError::Validate {
                    step_idx: idx, err: e.to_string(),
                })?;
                executor.explain(&validated).await.map_err(|e: ExecError| RunnerError::Explain {
                    step_idx: idx, err: e.to_string(),
                })?;
                let r = executor.run(&validated).await.map_err(|e| RunnerError::Execute {
                    step_idx: idx, err: e.to_string(),
                })?;
                out.push(StepRun { label: s.label.clone(), result: r });
            }
            AnalysisStep::Compute(c) => {
                let src = out.get(c.source_step).ok_or(RunnerError::BadSourceStep {
                    step_idx: idx, src: c.source_step,
                })?;
                let r = run_compute(c.function, &c.params, &src.result)
                    .map_err(|e: ComputeError| RunnerError::Compute { step_idx: idx, err: e.to_string() })?;
                out.push(StepRun { label: c.label.clone(), result: r });
            }
            AnalysisStep::Ml(m) => {
                let ml = ml.ok_or(RunnerError::MlNotAvailable { step_idx: idx })?;
                let src = out.get(m.source_step).ok_or(RunnerError::BadSourceStep {
                    step_idx: idx, src: m.source_step,
                })?;
                let body = serde_json::json!({
                    "task": m.task,
                    "params": m.params,
                    "data": project_features(&src.result, &m.feature_columns).map_err(|e| {
                        RunnerError::Ml { step_idx: idx, err: e }
                    })?,
                });
                let res = ml.run(&body).await.map_err(|e| RunnerError::Ml {
                    step_idx: idx, err: e.to_string(),
                })?;
                let merged = merge_ml_into_rows(&src.result, &res, &m.feature_columns)
                    .map_err(|e| RunnerError::Ml { step_idx: idx, err: e })?;
                out.push(StepRun { label: m.label.clone(), result: merged });
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
            let v = r.get(c).and_then(|v| v.as_f64())
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
    let labels: Vec<i64> = result
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();
    if labels.len() != src.rows.len() {
        return Err(format!(
            "ml labels.len()={} doesn't match rows.len()={}",
            labels.len(), src.rows.len()
        ));
    }
    let mut new_cols = feature_cols.to_vec();
    new_cols.push("cluster".into());
    let new_rows: Vec<serde_json::Value> = src.rows.iter().enumerate().map(|(i, r)| {
        let mut o = serde_json::Map::new();
        for c in feature_cols {
            o.insert(c.clone(), r.get(c).cloned().unwrap_or(serde_json::Value::Null));
        }
        o.insert("cluster".into(), serde_json::Value::from(labels[i]));
        o.insert("_task".into(), serde_json::Value::from(task));
        serde_json::Value::Object(o)
    }).collect();
    Ok(ExecutionResult {
        columns: new_cols,
        rows: new_rows,
        truncated: false,
    })
}
```

- [ ] **Step 3：Pipeline 加 ml_client 字段**

`crates/sqlai-pipeline/src/lib.rs`：

```rust
#[derive(Clone)]
pub struct Pipeline {
    pub llm: Arc<dyn LlmProvider>,
    pub embedder: Arc<dyn EmbeddingProvider>,
    pub pool: PgPool,
    pub executor: Arc<dyn Executor>,
    pub skills: Arc<SkillRegistry>,
    pub ml_client: Option<Arc<sqlai_llm::sidecar::SidecarMlClient>>,
}
```

并在 `drive` 中把 `runner::validate_and_run(&self.executor, &plan)` 改为 `runner::validate_and_run(&self.executor, self.ml_client.as_ref(), &plan)`。

- [ ] **Step 4：lib.rs 暴露 compute 模块**

```rust
pub mod compute;
```

- [ ] **Step 5：sqlai-api 的 main.rs 与 e2e 测试构造 Pipeline 时补 `ml_client: None`**

修改：
- `crates/sqlai-api/src/main.rs`：
  ```rust
  let ml_client = Arc::new(sqlai_llm::sidecar::SidecarMlClient::new(SidecarConfig {
      base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
      timeout_secs: 600,
  })?);
  let pipeline = Pipeline {
      llm, embedder: embedder.clone(), pool: pool.clone(), executor,
      skills: Arc::new(SkillRegistry::with_defaults()),
      ml_client: Some(ml_client),
  };
  ```
- `crates/sqlai-api/tests/api_e2e.rs`：构造 Pipeline 时加 `ml_client: None`（或 Some，看测试是否触发 ML skill）。
- `crates/sqlai-pipeline/tests/pipeline_e2e.rs`：同样补 `ml_client: None`（或 Some）。

- [ ] **Step 6：跑所有测试 + commit**

```
cargo build --workspace 2>&1 | tail -10
cargo test --workspace 2>&1 | tail -10
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

预期：单元测试新增 3（compute）+ 1（forecast plan）= 53 passed。

```
git add crates/sqlai-skills crates/sqlai-pipeline crates/sqlai-api
git commit -m "feat(pipeline,skills): add Compute step (forecast_simple) + runner support"
```

---

## Task 3：cluster_kmeans skill 完整实现 + 端到端 ML 集成

**Files:**
- Modify: `crates/sqlai-skills/src/ml/cluster_kmeans.rs`（替换 stub）
- Modify: `crates/sqlai-skills/src/lib.rs`（注册）
- Append: `crates/sqlai-pipeline/tests/pipeline_e2e.rs`（一个 ML 集成测试）

- [ ] **Step 1：cluster_kmeans 完整实现**

```rust
//! cluster_kmeans：从一张表里抽 numeric 列，调 sidecar 跑 K-means。

use serde::Deserialize;
use sqlai_core::RetrievalContext;

use crate::error::SkillError;
use crate::plan::{AnalysisPlan, AnalysisStep, ChartHint, ChartKind, MlStep, SqlStep};
use crate::render::{quote_ident, quote_lit};
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

fn default_n_clusters() -> u32 { 3 }
fn default_sample_limit() -> u32 { 5000 }

impl AnalysisSkill for ClusterKmeans {
    fn name(&self) -> &'static str { "cluster_kmeans" }
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
    fn plan(&self, args: &serde_json::Value, _ctx: &RetrievalContext) -> Result<AnalysisPlan, SkillError> {
        let a: Args = serde_json::from_value(args.clone())
            .map_err(|e| SkillError::InvalidArg("cluster_kmeans", e.to_string()))?;
        if a.feature_columns.len() < 2 {
            return Err(SkillError::InvalidArg("feature_columns", "need >= 2".into()));
        }
        let table = format!("{}.{}", quote_ident(&a.db), quote_ident(&a.table));
        let select = a.feature_columns.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
        let where_sql = match &a.where_clause {
            Some(w) if !w.trim().is_empty() => format!(" WHERE ({w})"),
            _ => String::new(),
        };
        let _ = quote_lit;
        let sql = format!("SELECT {select} FROM {table}{where_sql} LIMIT {n}", n = a.sample_limit);

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
            explanation: format!("对 {} 跑 K-means k={}", a.feature_columns.join(","), a.n_clusters),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlai_core::RetrievalContext;

    #[test]
    fn plan_has_sql_then_ml_step() {
        let p = ClusterKmeans.plan(&serde_json::json!({
            "db":"d","table":"t","feature_columns":["x","y"],"n_clusters":3
        }), &RetrievalContext { tables: vec![], columns: vec![], business_terms: vec![], few_shots: vec![] }).unwrap();
        assert_eq!(p.steps.len(), 2);
        assert!(matches!(p.steps[0], AnalysisStep::Sql(_)));
        assert!(matches!(p.steps[1], AnalysisStep::Ml(_)));
    }

    #[test]
    fn fewer_than_2_features_rejected() {
        let err = ClusterKmeans.plan(&serde_json::json!({
            "db":"d","table":"t","feature_columns":["x"]
        }), &RetrievalContext { tables: vec![], columns: vec![], business_terms: vec![], few_shots: vec![] }).unwrap_err();
        assert!(matches!(err, SkillError::InvalidArg("cluster_kmeans", _) | SkillError::InvalidArg("feature_columns", _)));
    }
}
```

- [ ] **Step 2：lib.rs 注册**

```rust
r.register(Arc::new(ml::cluster_kmeans::ClusterKmeans));
```

- [ ] **Step 3：跑测试 + commit**

```
cargo test -p sqlai-skills 2>&1 | tail -10
```
预期：22 passed（20 + 2 cluster_kmeans）。

```
git add crates/sqlai-skills
git commit -m "feat(skills): cluster_kmeans skill (Sql sample + Ml step via sidecar)"
```

---

## Task 4：few-shot store + 检索注入 + Admin CRUD

**Files:**
- Create: `crates/sqlai-store/src/few_shot.rs`
- Modify: `crates/sqlai-store/src/lib.rs`
- Modify: `crates/sqlai-pipeline/src/retrieval.rs`
- Modify: `crates/sqlai-pipeline/src/selector.rs`
- Modify: `crates/sqlai-api/src/routes/admin.rs`
- Modify: `crates/sqlai-api/src/lib.rs`
- Append: `crates/sqlai-store/tests/store_integration.rs`

- [ ] **Step 1：few_shot.rs**

```rust
use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FewShotRecord {
    pub id: Uuid,
    pub question: String,
    pub skill_call: serde_json::Value,
    pub sql_text: String,
    pub datasource_id: Option<Uuid>,
    pub vote: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewFewShot<'a> {
    pub question: &'a str,
    pub skill_call: serde_json::Value,
    pub sql_text: &'a str,
    pub datasource_id: Option<Uuid>,
    pub embedding: Vec<f32>,
}

pub async fn insert(pool: &PgPool, fs: NewFewShot<'_>) -> Result<FewShotRecord, StoreError> {
    let v = Vector::from(fs.embedding);
    sqlx::query_as::<_, FewShotRecord>(
        r#"
        INSERT INTO few_shot (question, skill_call, sql_text, datasource_id, embedding)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, question, skill_call, sql_text, datasource_id, vote, created_at
        "#,
    )
    .bind(fs.question)
    .bind(fs.skill_call)
    .bind(fs.sql_text)
    .bind(fs.datasource_id)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn vote(pool: &PgPool, id: Uuid, delta: i32) -> Result<FewShotRecord, StoreError> {
    sqlx::query_as::<_, FewShotRecord>(
        r#"
        UPDATE few_shot SET vote = vote + $2 WHERE id = $1
        RETURNING id, question, skill_call, sql_text, datasource_id, vote, created_at
        "#,
    )
    .bind(id)
    .bind(delta)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), StoreError> {
    let n = sqlx::query("DELETE FROM few_shot WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(StoreError::Sql)?
        .rows_affected();
    if n == 0 { Err(StoreError::NotFound) } else { Ok(()) }
}

pub async fn list(pool: &PgPool, limit: i64) -> Result<Vec<FewShotRecord>, StoreError> {
    sqlx::query_as::<_, FewShotRecord>(
        r#"
        SELECT id, question, skill_call, sql_text, datasource_id, vote, created_at
        FROM few_shot ORDER BY vote DESC, created_at DESC LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)
}

#[allow(clippy::type_complexity)]
pub async fn top_k(
    pool: &PgPool,
    datasource_id: Option<Uuid>,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(FewShotRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    // 优先匹配同 datasource，但 datasource_id IS NULL 也算泛用样例。
    let rows: Vec<(Uuid, String, serde_json::Value, String, Option<Uuid>, i32, DateTime<Utc>, f64)> =
        sqlx::query_as(
            r#"
            SELECT id, question, skill_call, sql_text, datasource_id, vote, created_at,
                   (embedding <=> $2) AS distance
            FROM few_shot
            WHERE embedding IS NOT NULL
              AND ($1::uuid IS NULL OR datasource_id IS NULL OR datasource_id = $1)
              AND vote >= 0
            ORDER BY embedding <=> $2 LIMIT $3
            "#,
        )
        .bind(datasource_id)
        .bind(&v)
        .bind(k)
        .fetch_all(pool)
        .await
        .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(|(id, question, skill_call, sql_text, datasource_id, vote, created_at, dist)| {
            (
                FewShotRecord { id, question, skill_call, sql_text, datasource_id, vote, created_at },
                dist,
            )
        })
        .collect())
}
```

- [ ] **Step 2：lib.rs 加模块**

```rust
pub mod few_shot;
```

- [ ] **Step 3：retrieval.rs 调 top_k**

把 `RetrievalContext` 的 `few_shots` 字段填上：

```rust
// 在 collect 里，q 复用之前的；datasource_id 已经在签名里
let fs = sqlai_store::few_shot::top_k(pool, Some(datasource_id), q.clone(), 3)
    .await
    .map_err(|e| LlmError::InvalidResponse(format!("pg: {e}")))?;
let few_shots: Vec<FewShot> = fs.into_iter().map(|(r, _)| FewShot {
    question: r.question,
    sql_text: r.sql_text,
}).collect();

// 把 few_shots 放进返回的 RetrievalContext（替换原来的 vec![]）
```

> 注意：`q` 在原代码里被 move 到 `top_k_metrics`；调整为先 `let q_for_fs = q.clone();` 再使用，或调整调用顺序。

- [ ] **Step 4：selector.rs 把 few_shots 注入 prompt**

在 `serialize_ctx_for_prompt` 末尾追加：

```rust
if !ctx.few_shots.is_empty() {
    out.push_str("# 历史问答（few-shot）\n");
    for (i, fs) in ctx.few_shots.iter().enumerate() {
        out.push_str(&format!(
            "## 例 {}\nQ: {}\nSQL:\n```sql\n{}\n```\n",
            i + 1, fs.question, fs.sql_text
        ));
    }
}
```

- [ ] **Step 5：sqlai-api 的 admin.rs 加 few-shot endpoints**

```rust
use sqlai_store::few_shot;

#[derive(Debug, Deserialize)]
pub struct CreateFewShotReq {
    pub question: String,
    pub skill_call: serde_json::Value,
    pub sql_text: String,
    #[serde(default)]
    pub datasource_id: Option<Uuid>,
}

pub async fn create_few_shot(
    State(s): State<AppState>,
    Json(req): Json<CreateFewShotReq>,
) -> Result<impl IntoResponse, ApiError> {
    let prompt = format!("{}\nSQL: {}", req.question, req.sql_text);
    let emb = embed_text(&s.embedder, &prompt).await?;
    let r = few_shot::insert(&s.pool, few_shot::NewFewShot {
        question: &req.question,
        skill_call: req.skill_call,
        sql_text: &req.sql_text,
        datasource_id: req.datasource_id,
        embedding: emb,
    }).await?;
    Ok(Json(r))
}

pub async fn list_few_shots(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let r = few_shot::list(&s.pool, 200).await?;
    Ok(Json(r))
}

#[derive(Debug, Deserialize)]
pub struct VoteReq {
    pub delta: i32, // +1 或 -1
}

pub async fn vote_few_shot(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<VoteReq>,
) -> Result<impl IntoResponse, ApiError> {
    if req.delta.abs() > 5 { return Err(ApiError::BadRequest("delta out of range".into())); }
    let r = few_shot::vote(&s.pool, id, req.delta).await?;
    Ok(Json(r))
}

pub async fn delete_few_shot(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    few_shot::delete(&s.pool, id).await?;
    Ok(Json(serde_json::json!({"deleted": id})))
}
```

- [ ] **Step 6：lib.rs 加路由**

```rust
.route("/api/admin/few-shots", post(routes::admin::create_few_shot).get(routes::admin::list_few_shots))
.route("/api/admin/few-shots/:id/vote", post(routes::admin::vote_few_shot))
.route("/api/admin/few-shots/:id", axum::routing::delete(routes::admin::delete_few_shot))
```

- [ ] **Step 7：append store 集成测试**

```rust
use sqlai_store::few_shot::{self, NewFewShot};

#[ignore]
#[tokio::test]
async fn few_shot_insert_vote_top_k() {
    let (_c, pool) = boot_pg().await;
    let fs = few_shot::insert(&pool, NewFewShot {
        question: "GMV 走势",
        skill_call: serde_json::json!({"skill":"metric_overview"}),
        sql_text: "SELECT toStartOfDay(d), sum(amt) FROM o GROUP BY 1",
        datasource_id: None,
        embedding: unit_vec_with_one_at(0, 1024),
    }).await.unwrap();
    assert_eq!(fs.vote, 0);

    let fs2 = few_shot::vote(&pool, fs.id, 3).await.unwrap();
    assert_eq!(fs2.vote, 3);

    let res = few_shot::top_k(&pool, None, unit_vec_with_one_at(0, 1024), 1).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.id, fs.id);

    few_shot::delete(&pool, fs.id).await.unwrap();
    let res2 = few_shot::list(&pool, 10).await.unwrap();
    assert!(res2.iter().all(|r| r.id != fs.id));
}
```

- [ ] **Step 8：跑全套 + commit**

```
cargo test -p sqlai-store --test store_integration -- --ignored 2>&1 | tail -10
cargo build --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

预期：store 集成 10 passed（9 + 1 few_shot）。

```
git add .
git commit -m "feat(store,api,pipeline): few-shot CRUD + retrieval injection + admin endpoints"
```

---

## Task 5：GoldenSet eval 框架（sqlai-cli eval 子命令）

**Files:**
- Create: `crates/sqlai-cli/src/eval.rs`
- Modify: `crates/sqlai-cli/src/main.rs`
- Create: `docs/superpowers/specs/golden-set-example.json`

- [ ] **Step 1：题库 JSON 示例**

`docs/superpowers/specs/golden-set-example.json`：

```json
[
  {
    "id": "G001",
    "question": "看一下 default.orders 按天的订单金额趋势",
    "datasource": "ch_local",
    "expected_skill": "metric_overview",
    "expected_columns": ["bucket", "value"],
    "expected_min_rows": 1
  },
  {
    "id": "G002",
    "question": "default.orders 按渠道排序的销售 Top10",
    "datasource": "ch_local",
    "expected_skill": "topn",
    "expected_columns": ["dimension", "value"],
    "expected_min_rows": 1
  }
]
```

- [ ] **Step 2：eval.rs**

```rust
//! sqlai eval：跑题库 JSON，统计 skill 命中率 + 列匹配 + 行数下限。

use anyhow::{Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use sqlai_exec::{ClickHouseExecutor, Executor, ReadonlyClickHouse, ReadonlyConfig};
use sqlai_llm::deepseek::{DeepSeekConfig, DeepSeekProvider};
use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder, SidecarMlClient};
use sqlai_llm::{EmbeddingProvider, LlmProvider};
use sqlai_pipeline::{AskRequest, Pipeline, PipelineEvent};
use sqlai_skills::SkillRegistry;
use sqlai_store::StoreConfig;

#[derive(Args, Debug)]
pub struct EvalArgs {
    /// 题库 JSON 路径
    #[arg(long)]
    pub goldenset: String,

    /// 报表 JSON 输出路径（可选）
    #[arg(long)]
    pub report: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoldenItem {
    pub id: String,
    pub question: String,
    pub datasource: String,
    #[serde(default)]
    pub expected_skill: Option<String>,
    #[serde(default)]
    pub expected_columns: Vec<String>,
    #[serde(default)]
    pub expected_min_rows: usize,
}

#[derive(Debug, Serialize)]
pub struct ItemResult {
    pub id: String,
    pub passed: bool,
    pub skill_hit: bool,
    pub column_hit: bool,
    pub rows_hit: bool,
    pub picked_skill: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub total: usize,
    pub passed: usize,
    pub skill_accuracy: f64,
    pub column_accuracy: f64,
    pub items: Vec<ItemResult>,
}

pub async fn run(args: EvalArgs) -> Result<()> {
    let body = std::fs::read_to_string(&args.goldenset)
        .with_context(|| format!("read goldenset {}", args.goldenset))?;
    let items: Vec<GoldenItem> = serde_json::from_str(&body).context("parse goldenset")?;

    let pg_cfg = StoreConfig::from_env().context("load PG config")?;
    let pool = sqlai_store::pool::connect(&pg_cfg).await?;

    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(SidecarEmbedder::new(SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600,
    })?);
    let llm: Arc<dyn LlmProvider> = Arc::new(DeepSeekProvider::new(DeepSeekConfig {
        base_url: std::env::var("DEEPSEEK_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com".into()),
        api_key: std::env::var("DEEPSEEK_API_KEY").context("DEEPSEEK_API_KEY required")?,
        model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
        timeout_secs: 60,
    })?);
    // 简化：题库里所有 item 共用一个 datasource 名 → 实际跑时会按名查
    let ml_client = Arc::new(SidecarMlClient::new(SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600,
    })?);

    let mut item_results = Vec::with_capacity(items.len());
    for item in &items {
        // 拿 datasource_id
        let ds = match sqlai_store::datasource::get_by_name(&pool, &item.datasource).await {
            Ok(d) => d,
            Err(e) => {
                item_results.push(ItemResult {
                    id: item.id.clone(), passed: false, skill_hit: false,
                    column_hit: false, rows_hit: false, picked_skill: None,
                    error: Some(format!("datasource '{}' not found: {e}", item.datasource)),
                });
                continue;
            }
        };
        // 资源（每题重新构造 executor，因为 password 来自 env per-datasource）
        let password = match ds.secret_ref.strip_prefix("env:") {
            Some(var) => std::env::var(var).unwrap_or_default(),
            None => String::new(),
        };
        let executor: Arc<dyn Executor> = Arc::new(ClickHouseExecutor::new(
            ReadonlyClickHouse::new(ReadonlyConfig {
                url: format!("http://{}:{}", ds.host, ds.port),
                user: ds.user_name.clone(), password,
                database: ds.db.clone(),
                max_execution_time_secs: 30, max_result_rows: 1000,
            })?
        ));
        let pipeline = Pipeline {
            llm: llm.clone(), embedder: embedder.clone(),
            pool: pool.clone(), executor,
            skills: Arc::new(SkillRegistry::with_defaults()),
            ml_client: Some(ml_client.clone()),
        };
        let mut rx = pipeline.ask(AskRequest {
            session_id: Uuid::new_v4(),
            datasource_id: ds.id,
            question: item.question.clone(),
            history: vec![],
        });

        let mut picked_skill: Option<String> = None;
        let mut got_columns: Vec<String> = vec![];
        let mut got_rows = 0usize;
        let mut error: Option<String> = None;
        while let Some(ev) = rx.recv().await {
            match ev {
                PipelineEvent::SkillCall { skill, .. } => picked_skill = Some(skill),
                PipelineEvent::Rows(r) => {
                    if got_columns.is_empty() { got_columns = r.columns.clone(); }
                    got_rows += r.rows.len();
                }
                PipelineEvent::Error { stage, code, message } => {
                    error = Some(format!("{stage}/{code}: {message}"));
                }
                _ => {}
            }
        }

        let skill_hit = match (&item.expected_skill, &picked_skill) {
            (Some(e), Some(p)) => e == p,
            (None, _) => true,
            _ => false,
        };
        let column_hit = if item.expected_columns.is_empty() {
            true
        } else {
            item.expected_columns.iter().all(|c| got_columns.iter().any(|g| g == c))
        };
        let rows_hit = got_rows >= item.expected_min_rows;
        let passed = skill_hit && column_hit && rows_hit && error.is_none();

        item_results.push(ItemResult {
            id: item.id.clone(),
            passed, skill_hit, column_hit, rows_hit,
            picked_skill, error,
        });
    }

    let total = item_results.len();
    let passed = item_results.iter().filter(|r| r.passed).count();
    let skill_acc = if total == 0 { 0.0 } else {
        item_results.iter().filter(|r| r.skill_hit).count() as f64 / total as f64
    };
    let col_acc = if total == 0 { 0.0 } else {
        item_results.iter().filter(|r| r.column_hit).count() as f64 / total as f64
    };

    let report = Report {
        total, passed,
        skill_accuracy: skill_acc,
        column_accuracy: col_acc,
        items: item_results,
    };

    println!(
        "GoldenSet: {}/{} passed ({:.1}%); skill_acc={:.1}%; column_acc={:.1}%",
        report.passed, report.total,
        100.0 * report.passed as f64 / report.total.max(1) as f64,
        100.0 * report.skill_accuracy,
        100.0 * report.column_accuracy,
    );
    for r in &report.items {
        println!(
            "- {}: passed={} skill_hit={} col_hit={} rows_hit={} picked={:?} err={:?}",
            r.id, r.passed, r.skill_hit, r.column_hit, r.rows_hit, r.picked_skill, r.error
        );
    }

    if let Some(path) = args.report {
        std::fs::write(&path, serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("write report {path}"))?;
    }

    if report.passed != report.total {
        // 非零退出码，便于 CI 用。
        std::process::exit(1);
    }
    Ok(())
}
```

- [ ] **Step 3：main.rs 加子命令**

```rust
mod eval;
mod sync_schema;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sqlai", version, about = "智能问数 CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Hello,
    SyncSchema(sync_schema::SyncArgs),
    Eval(eval::EvalArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Hello => { println!("sqlai-cli ready"); Ok(()) }
        Cmd::SyncSchema(args) => sync_schema::run(args).await,
        Cmd::Eval(args) => eval::run(args).await,
    }
}
```

- [ ] **Step 4：跑一次真实 eval（用 ch_local datasource，需要前提：sidecar/CH/PG/sync-schema 都准备好）**

```powershell
$env:SQLAI_PG_URL="postgres://sqlai:sqlai@127.0.0.1:5432/sqlai"
$env:DEEPSEEK_API_KEY="sk-..."
$env:CLICKHOUSE_PASSWORD="root23"
cargo run -p sqlai-cli -- eval --goldenset docs/superpowers/specs/golden-set-example.json --report eval-report.json 2>&1 | Select-Object -Last 30
```

预期：题库 2 题至少有一题 passed=true；输出 `GoldenSet: N/2 passed`。

如果两题都失败：检查 datasource ch_local 是否在 PG 中存在（#3 已建过），并且 schema 已同步。

- [ ] **Step 5：commit**

```
git add crates/sqlai-cli docs/superpowers/specs/golden-set-example.json
git commit -m "feat(cli): add eval subcommand for GoldenSet accuracy regression"
```

---

## 验收清单

- [ ] `cargo build --workspace` ✅
- [ ] `cargo test --workspace` ✅
- [ ] `cargo clippy --workspace -- -D warnings` ✅
- [ ] `cargo fmt --all -- --check` ✅
- [ ] `sqlai-store` 集成 10 ignored ✅
- [ ] `sqlai-skills` 22 单元测试 ✅
- [ ] `cargo run -p sqlai-cli -- eval --goldenset ...` 成功打印报表 ✅
- [ ] `git log` 至少 5 条本子计划 commit

---

## v1.0 全部完成；下一份子计划是 **#7：前端独立仓库（Chat UI + Admin UI）**。
