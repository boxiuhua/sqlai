//! 端到端：真实 PG (testcontainers) + 真实 ClickHouse + 真实 sidecar + 真实 DeepSeek。
//!
//! 跑法：
//!   docker compose up -d sidecar
//!   $env:DEEPSEEK_API_KEY="sk-..."
//!   $env:CLICKHOUSE_PASSWORD="root23"
//!   cargo test -p sqlai-pipeline --test pipeline_e2e -- --ignored --nocapture

use serde_json::json;
use sqlai_exec::{ClickHouseExecutor, Executor, ReadonlyClickHouse, ReadonlyConfig};
use sqlai_llm::deepseek::{DeepSeekConfig, DeepSeekProvider};
use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder};
use sqlai_llm::{EmbeddingProvider, LlmProvider};
use sqlai_pipeline::{AskRequest, Pipeline, PipelineEvent};
use sqlai_skills::SkillRegistry;
use sqlai_store::datasource::NewDatasource;
use sqlai_store::schema::{UpsertColumn, UpsertTable};
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

async fn boot_pg() -> (testcontainers::ContainerAsync<Postgres>, sqlx::PgPool) {
    let container = Postgres::default()
        .with_name("pgvector/pgvector")
        .with_tag("pg16")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = sqlai_store::pool::connect(&sqlai_store::StoreConfig {
        url,
        max_connections: 4,
    })
    .await
    .unwrap();
    let migrations_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations");
    sqlai_store::pool::run_migrations(&pool, &migrations_dir)
        .await
        .unwrap();
    (container, pool)
}

async fn seed_minimal_schema(
    pool: &sqlx::PgPool,
    ds_id: Uuid,
    embedder: &Arc<dyn EmbeddingProvider>,
) {
    let prompts = vec![
        "default.orders: 订单表".to_string(),
        "default.products: 商品表".to_string(),
        "default.orders.amount (Decimal(18,2)): 订单金额; samples: [1.0]".to_string(),
        "default.orders.created_at (DateTime): 下单时间; samples: [\"2025-01-01 00:00:00\"]"
            .to_string(),
        "default.products.id (UInt32): 商品ID".to_string(),
    ];
    let embs = embedder.embed(&prompts).await.unwrap();

    let t_orders = sqlai_store::schema::upsert_table(
        pool,
        UpsertTable {
            datasource_id: ds_id,
            db: "default",
            table_name: "orders",
            comment: Some("订单表"),
            row_count_est: Some(5),
            embedding: embs[0].clone(),
        },
    )
    .await
    .unwrap();
    let _t_products = sqlai_store::schema::upsert_table(
        pool,
        UpsertTable {
            datasource_id: ds_id,
            db: "default",
            table_name: "products",
            comment: Some("商品表"),
            row_count_est: Some(5),
            embedding: embs[1].clone(),
        },
    )
    .await
    .unwrap();

    sqlai_store::schema::upsert_column(
        pool,
        UpsertColumn {
            table_id: t_orders.id,
            name: "amount",
            data_type: "Decimal(18,2)",
            comment: Some("订单金额"),
            sample_values: json!([1.0]),
            distinct_count_est: None,
            embedding: embs[2].clone(),
        },
    )
    .await
    .unwrap();
    sqlai_store::schema::upsert_column(
        pool,
        UpsertColumn {
            table_id: t_orders.id,
            name: "created_at",
            data_type: "DateTime",
            comment: Some("下单时间"),
            sample_values: json!(["2025-01-01 00:00:00"]),
            distinct_count_est: None,
            embedding: embs[3].clone(),
        },
    )
    .await
    .unwrap();
}

#[ignore]
#[tokio::test]
async fn end_to_end_metric_overview_against_real_stack() {
    let (_pg, pool) = boot_pg().await;

    let ds = sqlai_store::datasource::insert(
        &pool,
        NewDatasource {
            name: "ch_e2e",
            kind: "clickhouse",
            host: "127.0.0.1",
            port: 8123,
            db: "default",
            user_name: "admin",
            secret_ref: "env:CLICKHOUSE_PASSWORD",
            readonly: true,
            settings: json!({}),
        },
    )
    .await
    .unwrap();

    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(
        SidecarEmbedder::new(SidecarConfig {
            base_url: "http://127.0.0.1:8081".into(),
            timeout_secs: 600,
        })
        .unwrap(),
    );
    let llm: Arc<dyn LlmProvider> = Arc::new(
        DeepSeekProvider::new(DeepSeekConfig {
            base_url: "https://api.deepseek.com".into(),
            api_key: std::env::var("DEEPSEEK_API_KEY").expect("set DEEPSEEK_API_KEY"),
            model: "deepseek-chat".into(),
            timeout_secs: 60,
        })
        .unwrap(),
    );
    let executor: Arc<dyn Executor> = Arc::new(ClickHouseExecutor::new(
        ReadonlyClickHouse::new(ReadonlyConfig {
            url: "http://127.0.0.1:8123".into(),
            user: "admin".into(),
            password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "root23".into()),
            database: "default".into(),
            max_execution_time_secs: 30,
            max_result_rows: 1000,
        })
        .unwrap(),
    ));

    seed_minimal_schema(&pool, ds.id, &embedder).await;

    let pipeline = Pipeline {
        llm: llm.clone(),
        embedder: embedder.clone(),
        pool: pool.clone(),
        executor: executor.clone(),
        skills: Arc::new(SkillRegistry::with_defaults()),
        ml_client: None,
    };
    let mut rx = pipeline.ask(AskRequest {
        session_id: Uuid::new_v4(),
        datasource_id: ds.id,
        question: "看一下 default.orders 按天的订单金额趋势".to_string(),
        history: vec![],
    });

    let mut got_intent = false;
    let mut got_skill_call = false;
    let mut got_rows = false;
    let mut got_summary = false;
    let mut got_done = false;
    while let Some(ev) = rx.recv().await {
        eprintln!("event: {:?}", ev);
        match ev {
            PipelineEvent::Intent(_) => got_intent = true,
            PipelineEvent::SkillCall { .. } => got_skill_call = true,
            PipelineEvent::Rows(_) => got_rows = true,
            PipelineEvent::Summary { .. } => got_summary = true,
            PipelineEvent::Done { .. } => got_done = true,
            PipelineEvent::Error {
                stage,
                code,
                message,
            } => eprintln!("error in {stage}/{code}: {message}"),
            _ => {}
        }
    }
    assert!(got_intent, "no Intent event");
    assert!(got_done, "no Done event");
    let mid_events = (got_skill_call as u32) + (got_rows as u32) + (got_summary as u32);
    assert!(
        mid_events >= 2,
        "got too few mid-stage events: skill_call={got_skill_call} rows={got_rows} summary={got_summary}"
    );
}
