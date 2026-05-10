//! 集成测试：每个 #[tokio::test] 启动一个临时 PG 容器，跑 migrations，再 CRUD。
//! 默认 ignored —— 跑法：`cargo test -p sqlai-store --test store_integration -- --ignored`

use serde_json::json;
use sqlai_store::{
    connect, datasource, datasource::NewDatasource, run_migrations, schema, StoreConfig, StoreError,
};
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::postgres::Postgres;

async fn boot_pg() -> (testcontainers::ContainerAsync<Postgres>, sqlx::PgPool) {
    // 用 pgvector 镜像替换默认 postgres。如果本地缓存只有 pg17，可以把 tag 改成 pg17。
    let container = Postgres::default()
        .with_name("pgvector/pgvector")
        .with_tag("pg16")
        .start()
        .await
        .expect("start pg container");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = connect(&StoreConfig {
        url: url.clone(),
        max_connections: 4,
    })
    .await
    .expect("connect");
    let migrations_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations");
    run_migrations(&pool, &migrations_dir)
        .await
        .expect("run_migrations");
    (container, pool)
}

#[ignore]
#[tokio::test]
async fn datasource_insert_and_get_by_name() {
    let (_c, pool) = boot_pg().await;
    let ds = datasource::insert(
        &pool,
        NewDatasource {
            name: "ch_dev",
            kind: "clickhouse",
            host: "127.0.0.1",
            port: 8123,
            db: "default",
            user_name: "admin",
            secret_ref: "env:CLICKHOUSE_PASSWORD",
            readonly: true,
            settings: json!({"max_execution_time": 30}),
        },
    )
    .await
    .expect("insert");
    assert_eq!(ds.name, "ch_dev");
    assert_eq!(ds.port, 8123);

    let got = datasource::get_by_name(&pool, "ch_dev").await.expect("get");
    assert_eq!(got.id, ds.id);
    assert!(got.readonly);
    assert_eq!(got.settings["max_execution_time"], json!(30));
}

#[ignore]
#[tokio::test]
async fn datasource_list_returns_sorted_by_name() {
    let (_c, pool) = boot_pg().await;
    for name in ["zeta", "alpha", "mu"] {
        datasource::insert(
            &pool,
            NewDatasource {
                name,
                kind: "clickhouse",
                host: "127.0.0.1",
                port: 8123,
                db: "default",
                user_name: "admin",
                secret_ref: "env:X",
                readonly: true,
                settings: json!({}),
            },
        )
        .await
        .unwrap();
    }
    let names: Vec<String> = datasource::list(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, vec!["alpha", "mu", "zeta"]);
}

#[ignore]
#[tokio::test]
async fn datasource_get_missing_returns_not_found() {
    let (_c, pool) = boot_pg().await;
    let err = datasource::get_by_name(&pool, "nope").await.unwrap_err();
    assert!(matches!(err, StoreError::NotFound));
}

fn unit_vec_with_one_at(idx: usize, dim: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; dim];
    v[idx] = 1.0;
    v
}

#[ignore]
#[tokio::test]
async fn upsert_table_idempotent_and_top_k_returns_closest() {
    let (_c, pool) = boot_pg().await;
    let ds = datasource::insert(
        &pool,
        NewDatasource {
            name: "ch_dev",
            kind: "clickhouse",
            host: "127.0.0.1",
            port: 8123,
            db: "default",
            user_name: "admin",
            secret_ref: "env:X",
            readonly: true,
            settings: json!({}),
        },
    )
    .await
    .unwrap();

    let t_orders = schema::upsert_table(
        &pool,
        schema::UpsertTable {
            datasource_id: ds.id,
            db: "default",
            table_name: "orders",
            comment: Some("订单表"),
            row_count_est: Some(1_000_000),
            embedding: unit_vec_with_one_at(0, 1024),
        },
    )
    .await
    .unwrap();

    let _t_products = schema::upsert_table(
        &pool,
        schema::UpsertTable {
            datasource_id: ds.id,
            db: "default",
            table_name: "products",
            comment: Some("商品表"),
            row_count_est: Some(50_000),
            embedding: unit_vec_with_one_at(1, 1024),
        },
    )
    .await
    .unwrap();

    let _t_users = schema::upsert_table(
        &pool,
        schema::UpsertTable {
            datasource_id: ds.id,
            db: "default",
            table_name: "users",
            comment: Some("用户表"),
            row_count_est: Some(200_000),
            embedding: unit_vec_with_one_at(2, 1024),
        },
    )
    .await
    .unwrap();

    // 幂等 upsert
    let t_orders2 = schema::upsert_table(
        &pool,
        schema::UpsertTable {
            datasource_id: ds.id,
            db: "default",
            table_name: "orders",
            comment: Some("订单表 v2"),
            row_count_est: Some(1_100_000),
            embedding: unit_vec_with_one_at(0, 1024),
        },
    )
    .await
    .unwrap();
    assert_eq!(t_orders.id, t_orders2.id);
    assert_eq!(t_orders2.comment.as_deref(), Some("订单表 v2"));

    let res = schema::top_k_tables_by_embedding(&pool, ds.id, unit_vec_with_one_at(0, 1024), 2)
        .await
        .unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].0.table_name, "orders");
    assert!(res[0].1 < res[1].1, "orders distance must be smallest");
}

#[ignore]
#[tokio::test]
async fn upsert_column_idempotent_and_top_k() {
    let (_c, pool) = boot_pg().await;
    let ds = datasource::insert(
        &pool,
        NewDatasource {
            name: "ch_dev",
            kind: "clickhouse",
            host: "127.0.0.1",
            port: 8123,
            db: "default",
            user_name: "admin",
            secret_ref: "env:X",
            readonly: true,
            settings: json!({}),
        },
    )
    .await
    .unwrap();

    let t = schema::upsert_table(
        &pool,
        schema::UpsertTable {
            datasource_id: ds.id,
            db: "default",
            table_name: "orders",
            comment: None,
            row_count_est: None,
            embedding: unit_vec_with_one_at(0, 1024),
        },
    )
    .await
    .unwrap();

    let c1 = schema::upsert_column(
        &pool,
        schema::UpsertColumn {
            table_id: t.id,
            name: "amount",
            data_type: "Decimal(18,2)",
            comment: Some("订单金额"),
            sample_values: json!([12.5, 33.0]),
            distinct_count_est: None,
            embedding: unit_vec_with_one_at(10, 1024),
        },
    )
    .await
    .unwrap();

    let _c2 = schema::upsert_column(
        &pool,
        schema::UpsertColumn {
            table_id: t.id,
            name: "user_id",
            data_type: "UInt64",
            comment: Some("下单用户"),
            sample_values: json!([1, 2, 3]),
            distinct_count_est: None,
            embedding: unit_vec_with_one_at(20, 1024),
        },
    )
    .await
    .unwrap();

    let c1_again = schema::upsert_column(
        &pool,
        schema::UpsertColumn {
            table_id: t.id,
            name: "amount",
            data_type: "Decimal(18,2)",
            comment: Some("订单金额（修订）"),
            sample_values: json!([12.5]),
            distinct_count_est: Some(1234),
            embedding: unit_vec_with_one_at(10, 1024),
        },
    )
    .await
    .unwrap();
    assert_eq!(c1.id, c1_again.id);
    assert_eq!(c1_again.distinct_count_est, Some(1234));

    let res = schema::top_k_columns_by_embedding(&pool, &[t.id], unit_vec_with_one_at(10, 1024), 1)
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.name, "amount");
}

use sqlai_store::knowledge;
use sqlai_store::session;
use sqlai_store::session::NewMessage;

#[ignore]
#[tokio::test]
async fn business_term_upsert_and_search() {
    let (_c, pool) = boot_pg().await;
    knowledge::upsert_term(
        &pool,
        knowledge::UpsertTerm {
            term: "GMV",
            aliases: &["成交额".into(), "总成交金额".into()],
            definition: "已支付订单金额合计",
            formula: Some("SUM(amount) WHERE status='paid'"),
            embedding: unit_vec_with_one_at(0, 1024),
        },
    )
    .await
    .unwrap();

    knowledge::upsert_term(
        &pool,
        knowledge::UpsertTerm {
            term: "DAU",
            aliases: &["日活".into()],
            definition: "每日活跃用户数",
            formula: None,
            embedding: unit_vec_with_one_at(5, 1024),
        },
    )
    .await
    .unwrap();

    let res = knowledge::top_k_terms(&pool, unit_vec_with_one_at(0, 1024), 1)
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.term, "GMV");
}

#[ignore]
#[tokio::test]
async fn metric_def_upsert_and_search() {
    let (_c, pool) = boot_pg().await;
    knowledge::upsert_metric(
        &pool,
        knowledge::UpsertMetric {
            name: "daily_gmv",
            dimension_keys: &["date".into(), "channel".into()],
            measure_sql: "sum(amount)",
            owner: Some("data-team"),
            embedding: unit_vec_with_one_at(7, 1024),
        },
    )
    .await
    .unwrap();

    let res = knowledge::top_k_metrics(&pool, unit_vec_with_one_at(7, 1024), 1)
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.name, "daily_gmv");
    assert_eq!(
        res[0].0.dimension_keys,
        vec!["date".to_string(), "channel".to_string()]
    );
}

#[ignore]
#[tokio::test]
async fn session_and_messages_roundtrip() {
    let (_c, pool) = boot_pg().await;

    let s = session::create_session(&pool, "user-a", None, Some("first chat"))
        .await
        .unwrap();
    assert_eq!(s.user_id, "user-a");
    assert_eq!(s.title.as_deref(), Some("first chat"));

    let user_msg = session::append_message(
        &pool,
        NewMessage {
            session_id: s.id,
            role: "user".into(),
            content: serde_json::json!({"text":"hello"}),
            plan: None,
            chart_spec: None,
            rows_returned: None,
            latency_ms: None,
            parent_id: None,
        },
    )
    .await
    .unwrap();

    let asst_msg = session::append_message(
        &pool,
        NewMessage {
            session_id: s.id,
            role: "assistant".into(),
            content: serde_json::json!({"summary":"hi"}),
            plan: Some(serde_json::json!({"steps":[]})),
            chart_spec: Some(serde_json::json!({"kind":"none"})),
            rows_returned: Some(0),
            latency_ms: Some(123),
            parent_id: Some(user_msg.id),
        },
    )
    .await
    .unwrap();

    let msgs = session::list_messages(&pool, s.id).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].id, user_msg.id);
    assert_eq!(msgs[1].id, asst_msg.id);
    assert_eq!(msgs[1].latency_ms, Some(123));
}

#[ignore]
#[tokio::test]
async fn touch_session_updates_timestamp() {
    let (_c, pool) = boot_pg().await;
    let s = session::create_session(&pool, "u", None, None)
        .await
        .unwrap();
    let before = s.updated_at;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    session::touch_session(&pool, s.id).await.unwrap();
    let r: (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT updated_at FROM session WHERE id = $1")
            .bind(s.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(r.0 > before);
}

use sqlai_store::few_shot::{self, NewFewShot};

#[ignore]
#[tokio::test]
async fn few_shot_insert_vote_top_k_delete() {
    let (_c, pool) = boot_pg().await;
    let fs = few_shot::insert(
        &pool,
        NewFewShot {
            question: "GMV 走势",
            skill_call: serde_json::json!({"skill":"metric_overview"}),
            sql_text: "SELECT toStartOfDay(d), sum(amt) FROM o GROUP BY 1",
            datasource_id: None,
            embedding: unit_vec_with_one_at(0, 1024),
        },
    )
    .await
    .unwrap();
    assert_eq!(fs.vote, 0);

    let fs2 = few_shot::vote(&pool, fs.id, 3).await.unwrap();
    assert_eq!(fs2.vote, 3);

    let res = few_shot::top_k(&pool, None, unit_vec_with_one_at(0, 1024), 1)
        .await
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.id, fs.id);

    few_shot::delete(&pool, fs.id).await.unwrap();
    let res2 = few_shot::list(&pool, 10).await.unwrap();
    assert!(res2.iter().all(|r| r.id != fs.id));
}
