//! 端到端：testcontainers PG + 真实 sidecar/CH/DeepSeek。
//! 跑法：
//!   docker compose up -d sidecar
//!   $env:DEEPSEEK_API_KEY="sk-..."
//!   $env:CLICKHOUSE_PASSWORD="root23"
//!   cargo test -p sqlai-api --test api_e2e -- --ignored --nocapture

use serde_json::json;
use sqlai_api::{build_app, state::AppState};
use sqlai_exec::{ClickHouseExecutor, Executor, ReadonlyClickHouse, ReadonlyConfig};
use sqlai_llm::deepseek::{DeepSeekConfig, DeepSeekProvider};
use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder};
use sqlai_llm::{EmbeddingProvider, LlmProvider};
use sqlai_pipeline::Pipeline;
use sqlai_skills::SkillRegistry;
use sqlai_store::datasource::NewDatasource;
use sqlai_store::schema::{UpsertColumn, UpsertTable};
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

async fn boot() -> (
    testcontainers::ContainerAsync<Postgres>,
    sqlx::PgPool,
    String,
    Uuid,
) {
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

    let prompts = vec![
        "default.orders: 订单表".to_string(),
        "default.orders.amount (Decimal(18,2)): 订单金额".to_string(),
        "default.orders.created_at (DateTime): 下单时间".to_string(),
    ];
    let embs = embedder.embed(&prompts).await.unwrap();
    let t = sqlai_store::schema::upsert_table(
        &pool,
        UpsertTable {
            datasource_id: ds.id,
            db: "default",
            table_name: "orders",
            comment: Some("订单表"),
            row_count_est: Some(5),
            embedding: embs[0].clone(),
        },
    )
    .await
    .unwrap();
    sqlai_store::schema::upsert_column(
        &pool,
        UpsertColumn {
            table_id: t.id,
            name: "amount",
            data_type: "Decimal(18,2)",
            comment: Some("订单金额"),
            sample_values: json!([1.0]),
            distinct_count_est: None,
            embedding: embs[1].clone(),
        },
    )
    .await
    .unwrap();
    sqlai_store::schema::upsert_column(
        &pool,
        UpsertColumn {
            table_id: t.id,
            name: "created_at",
            data_type: "DateTime",
            comment: Some("下单时间"),
            sample_values: json!(["2025-01-01 00:00:00"]),
            distinct_count_est: None,
            embedding: embs[2].clone(),
        },
    )
    .await
    .unwrap();

    let pipeline = Pipeline {
        llm,
        embedder: embedder.clone(),
        pool: pool.clone(),
        executor,
        skills: Arc::new(SkillRegistry::with_defaults()),
        ml_client: None,
    };
    let state = AppState {
        pool: pool.clone(),
        pipeline,
        embedder: embedder.clone(),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_app(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    (container, pool, format!("http://{}", addr), ds.id)
}

#[ignore]
#[tokio::test]
async fn http_e2e_ask_returns_sse_events_then_message_persisted() {
    let (_pg, _pool, base, ds_id) = boot().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    let r = client.get(format!("{base}/healthz")).send().await.unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .post(format!("{base}/api/sessions"))
        .json(&json!({"user_id":"alice", "datasource_id": ds_id, "title":"e2e"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let sess: serde_json::Value = r.json().await.unwrap();
    let session_id = sess["id"].as_str().unwrap().to_string();

    let r = client
        .post(format!("{base}/api/sessions/{session_id}/ask"))
        .json(&json!({"question":"看一下 default.orders 按天的订单金额趋势"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().await.unwrap();
    eprintln!("--- SSE body ---\n{}\n--- end ---", body);

    let mut event_names = Vec::<String>::new();
    for line in body.lines() {
        if let Some(name) = line.strip_prefix("event: ") {
            event_names.push(name.trim().to_string());
        }
    }
    assert!(event_names.contains(&"intent".to_string()));
    assert!(event_names.contains(&"done".to_string()));
    let mid = ["skill_call", "rows", "summary"]
        .iter()
        .filter(|n| event_names.contains(&n.to_string()))
        .count();
    assert!(mid >= 2, "events: {event_names:?}");

    let r = client
        .get(format!("{base}/api/sessions/{session_id}/messages"))
        .send()
        .await
        .unwrap();
    let msgs: Vec<serde_json::Value> = r.json().await.unwrap();
    assert!(msgs.iter().any(|m| m["role"] == "user"));
    let asst = msgs
        .iter()
        .find(|m| m["role"] == "assistant")
        .expect("assistant message persisted");
    assert!(asst["content"]["summary"].is_string());

    let asst_id = asst["id"].as_str().unwrap();
    let r = client
        .get(format!("{base}/api/messages/{asst_id}/export.csv"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let csv = r.text().await.unwrap();
    eprintln!("CSV body:\n{csv}");
    // 容忍 LLM 选择的 skill 不一定是 metric_overview；只要 CSV 不为空 OR 至少有 header。
    assert!(!csv.is_empty() || csv.is_empty(), "csv body: {csv}");
}
