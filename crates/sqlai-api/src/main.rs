use std::sync::Arc;

use sqlai_api::{build_app, state::AppState};
use sqlai_exec::{ClickHouseExecutor, Executor, ReadonlyClickHouse, ReadonlyConfig};
use sqlai_llm::deepseek::{DeepSeekConfig, DeepSeekProvider};
use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder};
use sqlai_llm::{EmbeddingProvider, LlmProvider};
use sqlai_pipeline::Pipeline;
use sqlai_skills::SkillRegistry;
use sqlai_store::{connect, StoreConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sqlai=debug")),
        )
        .init();

    let pg_url = std::env::var("SQLAI_PG_URL")
        .unwrap_or_else(|_| "postgres://sqlai:sqlai@127.0.0.1:5432/sqlai".into());
    let pool = connect(&StoreConfig {
        url: pg_url,
        max_connections: 10,
    })
    .await?;

    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(SidecarEmbedder::new(SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600,
    })?);
    let ml_client = std::sync::Arc::new(sqlai_llm::sidecar::SidecarMlClient::new(SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600,
    })?);
    let llm: Arc<dyn LlmProvider> = Arc::new(DeepSeekProvider::new(DeepSeekConfig {
        base_url: std::env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".into()),
        api_key: std::env::var("DEEPSEEK_API_KEY")
            .map_err(|_| anyhow::anyhow!("set DEEPSEEK_API_KEY"))?,
        model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
        timeout_secs: 60,
    })?);
    let executor: Arc<dyn Executor> = Arc::new(ClickHouseExecutor::new(ReadonlyClickHouse::new(
        ReadonlyConfig {
            url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into()),
            user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "admin".into()),
            password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "".into()),
            database: std::env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "default".into()),
            max_execution_time_secs: 30,
            max_result_rows: 1000,
        },
    )?));

    let pipeline = Pipeline {
        llm,
        embedder: embedder.clone(),
        pool: pool.clone(),
        executor,
        skills: Arc::new(SkillRegistry::with_defaults()),
        ml_client: Some(ml_client),
    };
    let state = AppState {
        pool,
        pipeline,
        embedder,
    };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("sqlai-api listening on :8080");
    axum::serve(listener, build_app(state)).await?;
    Ok(())
}
