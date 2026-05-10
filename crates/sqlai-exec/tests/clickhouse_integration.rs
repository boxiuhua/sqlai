//! 集成测试：需要本地 ClickHouse 在 8123 端口；默认 ignored，按需跑。
//! 跑法：`docker compose up -d clickhouse && cargo test -p sqlai-exec -- --ignored`

use sqlai_dialect::validate;
use sqlai_exec::{ClickHouseExecutor, Executor, ReadonlyClickHouse, ReadonlyConfig};

fn make_executor() -> ClickHouseExecutor {
    let cfg = ReadonlyConfig {
        url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into()),
        user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into()),
        password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "".into()),
        database: std::env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "default".into()),
        max_execution_time_secs: 30,
        max_result_rows: 1000,
    };
    let client = ReadonlyClickHouse::new(cfg).expect("clickhouse client");
    ClickHouseExecutor::new(client)
}

#[ignore]
#[tokio::test]
async fn explain_select_one_works() {
    let exec = make_executor();
    let sql = validate("SELECT 1").unwrap();
    exec.explain(&sql).await.expect("explain should succeed");
}

#[ignore]
#[tokio::test]
async fn run_select_one_returns_one_row() {
    let exec = make_executor();
    let sql = validate("SELECT 1 AS x").unwrap();
    let r = exec.run(&sql).await.expect("run should succeed");
    assert_eq!(r.columns, vec!["x".to_string()]);
    assert_eq!(r.rows.len(), 1);
}

#[ignore]
#[tokio::test]
async fn run_insert_is_unreachable_due_to_validator() {
    // 这个测试演示：validator 已经拦截，executor 根本不会被调用。
    let err = validate("INSERT INTO t VALUES (1)").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("only SELECT"), "unexpected: {msg}");
}

#[ignore]
#[tokio::test]
async fn introspect_returns_at_least_system_table_count() {
    let cfg = ReadonlyConfig {
        url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into()),
        user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "admin".into()),
        password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "root23".into()),
        database: "default".into(),
        max_execution_time_secs: 30,
        max_result_rows: 1000,
    };
    let ch = ReadonlyClickHouse::new(cfg).unwrap();
    let tables = ch.introspect_tables("system").await.unwrap();
    assert!(
        tables.iter().any(|t| t.name == "tables"),
        "system.tables should exist"
    );
    let cols = ch.introspect_columns("system", "tables").await.unwrap();
    assert!(!cols.is_empty(), "system.tables must have columns");
    let samples = ch
        .sample_distinct("system", "tables", "engine", 5)
        .await
        .unwrap();
    assert!(!samples.is_empty());
}
