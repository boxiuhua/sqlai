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
    let pool = sqlai_store::connect(&pg_cfg).await?;

    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(SidecarEmbedder::new(SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600,
    })?);
    let llm: Arc<dyn LlmProvider> = Arc::new(DeepSeekProvider::new(DeepSeekConfig {
        base_url: std::env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".into()),
        api_key: std::env::var("DEEPSEEK_API_KEY").context("DEEPSEEK_API_KEY required")?,
        model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
        timeout_secs: 60,
    })?);
    let ml_client = Arc::new(SidecarMlClient::new(SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600,
    })?);

    let mut item_results = Vec::with_capacity(items.len());
    for item in &items {
        let ds = match sqlai_store::datasource::get_by_name(&pool, &item.datasource).await {
            Ok(d) => d,
            Err(e) => {
                item_results.push(ItemResult {
                    id: item.id.clone(),
                    passed: false,
                    skill_hit: false,
                    column_hit: false,
                    rows_hit: false,
                    picked_skill: None,
                    error: Some(format!("datasource '{}' not found: {e}", item.datasource)),
                });
                continue;
            }
        };
        let password = match ds.secret_ref.strip_prefix("env:") {
            Some(var) => std::env::var(var).unwrap_or_default(),
            None => String::new(),
        };
        let executor: Arc<dyn Executor> = Arc::new(ClickHouseExecutor::new(
            ReadonlyClickHouse::new(ReadonlyConfig {
                url: format!("http://{}:{}", ds.host, ds.port),
                user: ds.user_name.clone(),
                password,
                database: ds.db.clone(),
                max_execution_time_secs: 30,
                max_result_rows: 1000,
            })?,
        ));
        let pipeline = Pipeline {
            llm: llm.clone(),
            embedder: embedder.clone(),
            pool: pool.clone(),
            executor,
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
                    if got_columns.is_empty() {
                        got_columns = r.columns.clone();
                    }
                    got_rows += r.rows.len();
                }
                PipelineEvent::Error {
                    stage,
                    code,
                    message,
                } => {
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
            item.expected_columns
                .iter()
                .all(|c| got_columns.iter().any(|g| g == c))
        };
        let rows_hit = got_rows >= item.expected_min_rows;
        let passed = skill_hit && column_hit && rows_hit && error.is_none();

        item_results.push(ItemResult {
            id: item.id.clone(),
            passed,
            skill_hit,
            column_hit,
            rows_hit,
            picked_skill,
            error,
        });
    }

    let total = item_results.len();
    let passed = item_results.iter().filter(|r| r.passed).count();
    let skill_acc = if total == 0 {
        0.0
    } else {
        item_results.iter().filter(|r| r.skill_hit).count() as f64 / total as f64
    };
    let col_acc = if total == 0 {
        0.0
    } else {
        item_results.iter().filter(|r| r.column_hit).count() as f64 / total as f64
    };

    let report = Report {
        total,
        passed,
        skill_accuracy: skill_acc,
        column_accuracy: col_acc,
        items: item_results,
    };

    println!(
        "GoldenSet: {}/{} passed ({:.1}%); skill_acc={:.1}%; column_acc={:.1}%",
        report.passed,
        report.total,
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
        std::process::exit(1);
    }
    Ok(())
}
