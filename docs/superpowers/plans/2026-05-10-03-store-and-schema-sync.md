# 智能问数系统 v1.0 — 子计划 #3：sqlai-store + ClickHouse Schema 同步 CLI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `sqlai-store` 从占位 crate 升级为真正的 PostgreSQL + pgvector 持久化层；让 `sqlai-cli sync-schema` 能从一个真实 ClickHouse 数据源拉表/列元数据 + 样本值，调 sidecar `/embed` 生成 1024 维向量，幂等写入 PG。完成后，子计划 #4（pipeline + skill）就能直接从 PG 检索 schema/词表来源生成 SQL。

**Architecture:** `sqlai-store` 用 sqlx + `pgvector` crate（提供 `Vec<f32>` ↔ pgvector 类型转换），按"表"分模块（`datasource.rs / schema.rs / knowledge.rs`），每模块只暴露对该表的 CRUD + 向量检索。集成测试用 `testcontainers` 起临时 PG。`sqlai-cli sync-schema` 是新子命令，连 ClickHouse 拉 `system.tables` / `system.columns` + 采样，调 `SidecarEmbedder` 批量向量化，调 `sqlai-store` 幂等 upsert。

**Tech Stack:** sqlx 0.8（已在 workspace） + pgvector 0.4（新依赖，含 `sqlx` feature） + testcontainers 0.23 + testcontainers-modules 0.11（postgres）+ clap 4（已在 sqlai-cli） + sqlai-exec 已有的 ClickHouseExecutor + sqlai-llm 已有的 SidecarEmbedder。

**前置假设：**
- 子计划 #1、#2 已完成（22 commit；workspace 全绿；sidecar 容器可用，PG 已通过 docker-compose 起来后 migration 自动跑）。
- Docker Desktop 在运行，`docker compose up -d postgres` 可启动 PG（pg16 + pgvector）。
- 用户的 ClickHouse 仍在 `127.0.0.1:8123`，`admin / root23 / default`。

---

## File Structure

完成后新增 / 修改：

```
sqlai/
├── Cargo.toml                        # +pgvector workspace dep
├── crates/
│   ├── sqlai-store/
│   │   ├── Cargo.toml                # 大幅扩展依赖
│   │   ├── src/
│   │   │   ├── lib.rs                # 公开 Store + 子模块
│   │   │   ├── pool.rs               # PgPool 构造 + migration 跑通
│   │   │   ├── error.rs              # StoreError
│   │   │   ├── datasource.rs         # datasource 表 CRUD
│   │   │   ├── schema.rs             # table_meta + column_meta + 向量检索
│   │   │   └── knowledge.rs          # business_term + metric_def + 向量检索
│   │   └── tests/
│   │       └── store_integration.rs  # testcontainers PG 集成
│   ├── sqlai-exec/
│   │   ├── Cargo.toml                # （无需改）
│   │   └── src/
│   │       └── clickhouse.rs         # 加 introspect_tables / sample_values
│   └── sqlai-cli/
│       ├── Cargo.toml                # +sqlai-store +sqlai-exec +sqlai-llm
│       └── src/
│           ├── main.rs               # 加 sync-schema 子命令路由
│           └── sync_schema.rs        # sync-schema 主流程
└── docs/superpowers/plans/
    └── 2026-05-10-03-store-and-schema-sync.md   # 本文件
```

每个新文件的"做什么 / 依赖谁 / 暴露什么"：

| 文件 | 职责 | 暴露 |
|---|---|---|
| `sqlai-store/src/lib.rs` | 公开 `Store` 顶层入口（持有 PgPool）+ 子模块 re-export | `Store, StoreError, datasource::*, schema::*, knowledge::*` |
| `sqlai-store/src/pool.rs` | 从配置创建 `PgPool` + 启动时跑 `migrations/0001_init.sql` | `connect(cfg) -> PgPool`, `run_migrations(&PgPool)` |
| `sqlai-store/src/error.rs` | `StoreError` 枚举（`Sql, NotFound, Conflict`） | `StoreError` |
| `sqlai-store/src/datasource.rs` | `DatasourceRecord` + insert / get_by_name / list / update | typed CRUD |
| `sqlai-store/src/schema.rs` | `TableMetaRecord`, `ColumnMetaRecord` + upsert + `top_k_tables_by_embedding(emb, k)` | typed CRUD + vector search |
| `sqlai-store/src/knowledge.rs` | `BusinessTermRecord`, `MetricDefRecord` + CRUD + vector search | typed CRUD + vector search |
| `sqlai-exec/src/clickhouse.rs`（追加） | `introspect_tables(db)`, `introspect_columns(db, table)`, `sample_distinct(db, table, col, n)` | 三个新方法挂在 `ClickHouseExecutor` |
| `sqlai-cli/src/sync_schema.rs` | 编排：拉 CH 元数据 → 采样 → embed → upsert PG | `pub async fn run(args: SyncArgs)` |

---

## Task 1：sqlai-store 基础（Pool + datasource CRUD + testcontainers 集成测试）

**Files:**
- Modify: `Cargo.toml` （workspace 加 pgvector）
- Modify: `crates/sqlai-store/Cargo.toml`
- Create: `crates/sqlai-store/src/lib.rs`（覆盖原占位 lib.rs）
- Create: `crates/sqlai-store/src/pool.rs`
- Create: `crates/sqlai-store/src/error.rs`
- Create: `crates/sqlai-store/src/datasource.rs`
- Create: `crates/sqlai-store/tests/store_integration.rs`

- [ ] **Step 1：workspace 加 pgvector + testcontainers 依赖**

`Cargo.toml`（顶层）`[workspace.dependencies]` 追加：

```toml
pgvector              = { version = "0.4", features = ["sqlx"] }
testcontainers        = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres"] }
```

- [ ] **Step 2：sqlai-store 的 Cargo.toml**

```toml
[package]
name = "sqlai-store"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core  = { workspace = true }
serde       = { workspace = true }
serde_json  = { workspace = true }
async-trait = { workspace = true }
thiserror   = { workspace = true }
sqlx        = { workspace = true }
uuid        = { workspace = true }
chrono      = { workspace = true }
pgvector    = { workspace = true }
tracing     = { workspace = true }

[dev-dependencies]
tokio                  = { workspace = true }
testcontainers         = { workspace = true }
testcontainers-modules = { workspace = true }
```

- [ ] **Step 3：error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sql error: {0}")]
    Sql(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migrate(String),

    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),
}
```

- [ ] **Step 4：pool.rs**

```rust
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

use crate::error::StoreError;

#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub url: String,           // postgres://user:pass@host:5432/db
    pub max_connections: u32,  // 默认 10
}

impl StoreConfig {
    pub fn from_env() -> Result<Self, StoreError> {
        let url = std::env::var("SQLAI_PG_URL")
            .map_err(|_| StoreError::Migrate("SQLAI_PG_URL not set".into()))?;
        let max_connections = std::env::var("SQLAI_PG_MAX_CONN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        Ok(Self { url, max_connections })
    }
}

pub async fn connect(cfg: &StoreConfig) -> Result<PgPool, StoreError> {
    PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&cfg.url)
        .await
        .map_err(StoreError::Sql)
}

/// 跑同目录下 `migrations/` 中的 .sql 文件（按文件名升序，幂等）。
/// 调用前要确保数据库已存在；migration 自身用 `CREATE EXTENSION IF NOT EXISTS`、
/// `CREATE TABLE IF NOT EXISTS` 实现幂等。
pub async fn run_migrations(pool: &PgPool, dir: &std::path::Path) -> Result<(), StoreError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| StoreError::Migrate(format!("read_dir {dir:?}: {e}")))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let sql = std::fs::read_to_string(entry.path())
            .map_err(|e| StoreError::Migrate(format!("read {entry:?}: {e}")))?;
        sqlx::raw_sql(&sql).execute(pool).await.map_err(|e| {
            StoreError::Migrate(format!("apply {:?}: {e}", entry.file_name()))
        })?;
    }
    Ok(())
}
```

- [ ] **Step 5：datasource.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasourceRecord {
    pub id: Uuid,
    pub name: String,
    pub kind: String,        // 'clickhouse'
    pub host: String,
    pub port: i32,
    pub db: String,
    pub user_name: String,
    pub secret_ref: String,  // 引用 env / vault key
    pub readonly: bool,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewDatasource<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub host: &'a str,
    pub port: i32,
    pub db: &'a str,
    pub user_name: &'a str,
    pub secret_ref: &'a str,
    pub readonly: bool,
    pub settings: serde_json::Value,
}

pub async fn insert(pool: &PgPool, ds: NewDatasource<'_>) -> Result<DatasourceRecord, StoreError> {
    let row = sqlx::query_as!(
        DatasourceRecord,
        r#"
        INSERT INTO datasource (name, kind, host, port, db, user_name, secret_ref, readonly, settings)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, name, kind, host, port, db, user_name, secret_ref, readonly,
                  settings as "settings: serde_json::Value",
                  created_at, updated_at
        "#,
        ds.name, ds.kind, ds.host, ds.port, ds.db, ds.user_name, ds.secret_ref,
        ds.readonly, ds.settings
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<DatasourceRecord, StoreError> {
    sqlx::query_as!(
        DatasourceRecord,
        r#"
        SELECT id, name, kind, host, port, db, user_name, secret_ref, readonly,
               settings as "settings: serde_json::Value",
               created_at, updated_at
        FROM datasource WHERE name = $1
        "#,
        name
    )
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)
}

pub async fn list(pool: &PgPool) -> Result<Vec<DatasourceRecord>, StoreError> {
    let rows = sqlx::query_as!(
        DatasourceRecord,
        r#"
        SELECT id, name, kind, host, port, db, user_name, secret_ref, readonly,
               settings as "settings: serde_json::Value",
               created_at, updated_at
        FROM datasource ORDER BY name
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
```

> **Note on `query_as!`**: sqlx 的编译期验证默认要求连数据库。我们在 dev / CI 下不强制——通过 `SQLX_OFFLINE=true` + 提交 `.sqlx/` 缓存可以离线编译。但为了最简，本子计划全部使用运行时 `query_as` 而非宏。**修订 Step 5：把 `query_as!` 替换成 `query_as`**——
>
> 把上面 `datasource.rs` 中三处 `sqlx::query_as!` 全部改成 `sqlx::query_as` 并去掉 SQL 中的 `as "settings: serde_json::Value"` cast。同时给 `DatasourceRecord` 加 `#[derive(sqlx::FromRow)]`。最终模板如下：

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DatasourceRecord {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub host: String,
    pub port: i32,
    pub db: String,
    pub user_name: String,
    pub secret_ref: String,
    pub readonly: bool,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewDatasource<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub host: &'a str,
    pub port: i32,
    pub db: &'a str,
    pub user_name: &'a str,
    pub secret_ref: &'a str,
    pub readonly: bool,
    pub settings: serde_json::Value,
}

pub async fn insert(pool: &PgPool, ds: NewDatasource<'_>) -> Result<DatasourceRecord, StoreError> {
    sqlx::query_as::<_, DatasourceRecord>(
        r#"
        INSERT INTO datasource (name, kind, host, port, db, user_name, secret_ref, readonly, settings)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, name, kind, host, port, db, user_name, secret_ref, readonly,
                  settings, created_at, updated_at
        "#,
    )
    .bind(ds.name)
    .bind(ds.kind)
    .bind(ds.host)
    .bind(ds.port)
    .bind(ds.db)
    .bind(ds.user_name)
    .bind(ds.secret_ref)
    .bind(ds.readonly)
    .bind(&ds.settings)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<DatasourceRecord, StoreError> {
    sqlx::query_as::<_, DatasourceRecord>(
        "SELECT id, name, kind, host, port, db, user_name, secret_ref, readonly, settings, created_at, updated_at FROM datasource WHERE name = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)
}

pub async fn list(pool: &PgPool) -> Result<Vec<DatasourceRecord>, StoreError> {
    sqlx::query_as::<_, DatasourceRecord>(
        "SELECT id, name, kind, host, port, db, user_name, secret_ref, readonly, settings, created_at, updated_at FROM datasource ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)
}
```

- [ ] **Step 6：lib.rs 入口**

```rust
//! sqlai-store：PostgreSQL + pgvector 持久化。

pub mod datasource;
pub mod error;
pub mod pool;

pub use error::StoreError;
pub use pool::{connect, run_migrations, StoreConfig};
```

- [ ] **Step 7：testcontainers 集成测试**

`crates/sqlai-store/tests/store_integration.rs`:

```rust
//! 集成测试：每个 #[tokio::test] 启动一个临时 PG 容器（pgvector/pgvector:pg16），
//! 跑 migrations，然后断言 CRUD 行为。
//!
//! 默认 ignored —— 跑法：`docker info > /dev/null && cargo test -p sqlai-store --test store_integration -- --ignored`

use serde_json::json;
use sqlai_store::{
    connect, datasource,
    datasource::NewDatasource,
    run_migrations, StoreConfig, StoreError,
};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

async fn boot_pg() -> (testcontainers::ContainerAsync<Postgres>, sqlx::PgPool) {
    // pgvector 镜像替换默认 postgres 镜像。
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

    let got = datasource::get_by_name(&pool, "ch_dev")
        .await
        .expect("get");
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
```

- [ ] **Step 8：build + 单元 + 集成**

```
cargo build -p sqlai-store
cargo test -p sqlai-store --lib    # 0 passed (no unit tests yet)
cargo test -p sqlai-store --test store_integration -- --ignored    # 3 passed
```

预期：build 干净；3 个集成测试通过。每个测试 ~10s（容器启停 + migration）。

- [ ] **Step 9：commit**

```
git add Cargo.toml Cargo.lock crates/sqlai-store
git commit -m "feat(store): add sqlai-store with pool + datasource CRUD + testcontainers tests"
```

---

## Task 2：schema.rs（table_meta + column_meta + 向量检索）

**Files:**
- Create: `crates/sqlai-store/src/schema.rs`
- Modify: `crates/sqlai-store/src/lib.rs`（加 `pub mod schema;`）
- Modify: `crates/sqlai-store/tests/store_integration.rs`（追加测试）

- [ ] **Step 1：schema.rs**

```rust
use chrono::{DateTime, Utc};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TableMetaRecord {
    pub id: Uuid,
    pub datasource_id: Uuid,
    pub db: String,
    pub table_name: String,
    pub comment: Option<String>,
    pub row_count_est: Option<i64>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ColumnMetaRecord {
    pub id: Uuid,
    pub table_id: Uuid,
    pub name: String,
    pub data_type: String,
    pub comment: Option<String>,
    pub sample_values: serde_json::Value,
    pub distinct_count_est: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpsertTable<'a> {
    pub datasource_id: Uuid,
    pub db: &'a str,
    pub table_name: &'a str,
    pub comment: Option<&'a str>,
    pub row_count_est: Option<i64>,
    pub embedding: Vec<f32>, // 1024 维
}

#[derive(Debug, Clone)]
pub struct UpsertColumn<'a> {
    pub table_id: Uuid,
    pub name: &'a str,
    pub data_type: &'a str,
    pub comment: Option<&'a str>,
    pub sample_values: serde_json::Value,
    pub distinct_count_est: Option<i64>,
    pub embedding: Vec<f32>,
}

pub async fn upsert_table(pool: &PgPool, t: UpsertTable<'_>) -> Result<TableMetaRecord, StoreError> {
    let v = Vector::from(t.embedding);
    sqlx::query_as::<_, TableMetaRecord>(
        r#"
        INSERT INTO table_meta (datasource_id, db, table_name, comment, row_count_est, embedding, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, now())
        ON CONFLICT (datasource_id, db, table_name)
        DO UPDATE SET comment = EXCLUDED.comment,
                      row_count_est = EXCLUDED.row_count_est,
                      embedding = EXCLUDED.embedding,
                      updated_at = now()
        RETURNING id, datasource_id, db, table_name, comment, row_count_est, updated_at
        "#,
    )
    .bind(t.datasource_id)
    .bind(t.db)
    .bind(t.table_name)
    .bind(t.comment)
    .bind(t.row_count_est)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn upsert_column(pool: &PgPool, c: UpsertColumn<'_>) -> Result<ColumnMetaRecord, StoreError> {
    let v = Vector::from(c.embedding);
    sqlx::query_as::<_, ColumnMetaRecord>(
        r#"
        INSERT INTO column_meta (table_id, name, data_type, comment, sample_values, distinct_count_est, embedding)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (table_id, name)
        DO UPDATE SET data_type = EXCLUDED.data_type,
                      comment = EXCLUDED.comment,
                      sample_values = EXCLUDED.sample_values,
                      distinct_count_est = EXCLUDED.distinct_count_est,
                      embedding = EXCLUDED.embedding
        RETURNING id, table_id, name, data_type, comment, sample_values, distinct_count_est
        "#,
    )
    .bind(c.table_id)
    .bind(c.name)
    .bind(c.data_type)
    .bind(c.comment)
    .bind(&c.sample_values)
    .bind(c.distinct_count_est)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

/// 用 cosine 距离（`<=>`）找前 K 个相似表。返回 (record, distance)，distance 越小越相似。
pub async fn top_k_tables_by_embedding(
    pool: &PgPool,
    datasource_id: Uuid,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(TableMetaRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    let rows: Vec<(Uuid, Uuid, String, String, Option<String>, Option<i64>, DateTime<Utc>, f64)> =
        sqlx::query_as(
            r#"
            SELECT id, datasource_id, db, table_name, comment, row_count_est, updated_at,
                   (embedding <=> $2) AS distance
            FROM table_meta
            WHERE datasource_id = $1 AND embedding IS NOT NULL
            ORDER BY embedding <=> $2
            LIMIT $3
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
        .map(|(id, datasource_id, db, table_name, comment, row_count_est, updated_at, dist)| {
            (
                TableMetaRecord {
                    id,
                    datasource_id,
                    db,
                    table_name,
                    comment,
                    row_count_est,
                    updated_at,
                },
                dist,
            )
        })
        .collect())
}

pub async fn top_k_columns_by_embedding(
    pool: &PgPool,
    table_ids: &[Uuid],
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(ColumnMetaRecord, f64)>, StoreError> {
    if table_ids.is_empty() {
        return Ok(vec![]);
    }
    let v = Vector::from(query);
    let rows: Vec<(Uuid, Uuid, String, String, Option<String>, serde_json::Value, Option<i64>, f64)> =
        sqlx::query_as(
            r#"
            SELECT id, table_id, name, data_type, comment, sample_values, distinct_count_est,
                   (embedding <=> $2) AS distance
            FROM column_meta
            WHERE table_id = ANY($1) AND embedding IS NOT NULL
            ORDER BY embedding <=> $2
            LIMIT $3
            "#,
        )
        .bind(table_ids)
        .bind(&v)
        .bind(k)
        .fetch_all(pool)
        .await
        .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(|(id, table_id, name, data_type, comment, sample_values, distinct_count_est, dist)| {
            (
                ColumnMetaRecord {
                    id,
                    table_id,
                    name,
                    data_type,
                    comment,
                    sample_values,
                    distinct_count_est,
                },
                dist,
            )
        })
        .collect())
}
```

- [ ] **Step 2：lib.rs 加 `pub mod schema;`**

- [ ] **Step 3：在 `tests/store_integration.rs` 末尾追加**

```rust
use sqlai_store::schema;

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

    // 用三组单位向量充当 1024 维 embedding，做相似度可预测。
    let t_orders = schema::upsert_table(&pool, schema::UpsertTable {
        datasource_id: ds.id,
        db: "default",
        table_name: "orders",
        comment: Some("订单表"),
        row_count_est: Some(1_000_000),
        embedding: unit_vec_with_one_at(0, 1024),
    }).await.unwrap();

    let _t_products = schema::upsert_table(&pool, schema::UpsertTable {
        datasource_id: ds.id,
        db: "default",
        table_name: "products",
        comment: Some("商品表"),
        row_count_est: Some(50_000),
        embedding: unit_vec_with_one_at(1, 1024),
    }).await.unwrap();

    let _t_users = schema::upsert_table(&pool, schema::UpsertTable {
        datasource_id: ds.id,
        db: "default",
        table_name: "users",
        comment: Some("用户表"),
        row_count_est: Some(200_000),
        embedding: unit_vec_with_one_at(2, 1024),
    }).await.unwrap();

    // 幂等 upsert：同一表再写一次只更新，不报冲突
    let t_orders2 = schema::upsert_table(&pool, schema::UpsertTable {
        datasource_id: ds.id,
        db: "default",
        table_name: "orders",
        comment: Some("订单表 v2"),
        row_count_est: Some(1_100_000),
        embedding: unit_vec_with_one_at(0, 1024),
    }).await.unwrap();
    assert_eq!(t_orders.id, t_orders2.id);
    assert_eq!(t_orders2.comment.as_deref(), Some("订单表 v2"));

    // top-k：查询向量贴近 orders 的 embedding（idx=0）
    let res = schema::top_k_tables_by_embedding(
        &pool,
        ds.id,
        unit_vec_with_one_at(0, 1024),
        2,
    ).await.unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(res[0].0.table_name, "orders");
    // distance 越小越相似；orders 应当最小
    assert!(res[0].1 < res[1].1, "orders distance must be smallest");
}

#[ignore]
#[tokio::test]
async fn upsert_column_idempotent_and_top_k() {
    let (_c, pool) = boot_pg().await;
    let ds = datasource::insert(&pool, NewDatasource {
        name: "ch_dev", kind: "clickhouse", host: "127.0.0.1", port: 8123, db: "default",
        user_name: "admin", secret_ref: "env:X", readonly: true, settings: json!({}),
    }).await.unwrap();

    let t = schema::upsert_table(&pool, schema::UpsertTable {
        datasource_id: ds.id, db: "default", table_name: "orders",
        comment: None, row_count_est: None,
        embedding: unit_vec_with_one_at(0, 1024),
    }).await.unwrap();

    let c1 = schema::upsert_column(&pool, schema::UpsertColumn {
        table_id: t.id, name: "amount", data_type: "Decimal(18,2)",
        comment: Some("订单金额"), sample_values: json!([12.5, 33.0]),
        distinct_count_est: None,
        embedding: unit_vec_with_one_at(10, 1024),
    }).await.unwrap();

    let _c2 = schema::upsert_column(&pool, schema::UpsertColumn {
        table_id: t.id, name: "user_id", data_type: "UInt64",
        comment: Some("下单用户"), sample_values: json!([1,2,3]),
        distinct_count_est: None,
        embedding: unit_vec_with_one_at(20, 1024),
    }).await.unwrap();

    // 重复 upsert 不冲突
    let c1_again = schema::upsert_column(&pool, schema::UpsertColumn {
        table_id: t.id, name: "amount", data_type: "Decimal(18,2)",
        comment: Some("订单金额（修订）"), sample_values: json!([12.5]),
        distinct_count_est: Some(1234),
        embedding: unit_vec_with_one_at(10, 1024),
    }).await.unwrap();
    assert_eq!(c1.id, c1_again.id);
    assert_eq!(c1_again.distinct_count_est, Some(1234));

    let res = schema::top_k_columns_by_embedding(
        &pool, &[t.id], unit_vec_with_one_at(10, 1024), 1,
    ).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.name, "amount");
}
```

- [ ] **Step 4：跑测试**

```
cargo test -p sqlai-store --test store_integration -- --ignored
```

预期：5 passed（3 datasource + 2 schema）。

- [ ] **Step 5：commit**

```
git add crates/sqlai-store
git commit -m "feat(store): add table_meta/column_meta upsert and pgvector top-K search"
```

---

## Task 3：knowledge.rs（business_term + metric_def + 向量检索）

**Files:**
- Create: `crates/sqlai-store/src/knowledge.rs`
- Modify: `crates/sqlai-store/src/lib.rs`
- Modify: `crates/sqlai-store/tests/store_integration.rs`

- [ ] **Step 1：knowledge.rs**

```rust
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BusinessTermRecord {
    pub id: Uuid,
    pub term: String,
    pub aliases: Vec<String>,
    pub definition: String,
    pub formula: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpsertTerm<'a> {
    pub term: &'a str,
    pub aliases: &'a [String],
    pub definition: &'a str,
    pub formula: Option<&'a str>,
    pub embedding: Vec<f32>,
}

pub async fn upsert_term(pool: &PgPool, t: UpsertTerm<'_>) -> Result<BusinessTermRecord, StoreError> {
    let v = Vector::from(t.embedding);
    sqlx::query_as::<_, BusinessTermRecord>(
        r#"
        INSERT INTO business_term (term, aliases, definition, formula, embedding)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (term) DO UPDATE SET
            aliases = EXCLUDED.aliases,
            definition = EXCLUDED.definition,
            formula = EXCLUDED.formula,
            embedding = EXCLUDED.embedding
        RETURNING id, term, aliases, definition, formula
        "#,
    )
    .bind(t.term)
    .bind(t.aliases)
    .bind(t.definition)
    .bind(t.formula)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn top_k_terms(
    pool: &PgPool,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(BusinessTermRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    let rows: Vec<(Uuid, String, Vec<String>, String, Option<String>, f64)> =
        sqlx::query_as(
            r#"
            SELECT id, term, aliases, definition, formula, (embedding <=> $1) AS distance
            FROM business_term WHERE embedding IS NOT NULL
            ORDER BY embedding <=> $1 LIMIT $2
            "#,
        )
        .bind(&v)
        .bind(k)
        .fetch_all(pool)
        .await
        .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(|(id, term, aliases, definition, formula, dist)| {
            (
                BusinessTermRecord { id, term, aliases, definition, formula },
                dist,
            )
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MetricDefRecord {
    pub id: Uuid,
    pub name: String,
    pub dimension_keys: Vec<String>,
    pub measure_sql: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpsertMetric<'a> {
    pub name: &'a str,
    pub dimension_keys: &'a [String],
    pub measure_sql: &'a str,
    pub owner: Option<&'a str>,
    pub embedding: Vec<f32>,
}

pub async fn upsert_metric(pool: &PgPool, m: UpsertMetric<'_>) -> Result<MetricDefRecord, StoreError> {
    let v = Vector::from(m.embedding);
    sqlx::query_as::<_, MetricDefRecord>(
        r#"
        INSERT INTO metric_def (name, dimension_keys, measure_sql, owner, embedding)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (name) DO UPDATE SET
            dimension_keys = EXCLUDED.dimension_keys,
            measure_sql = EXCLUDED.measure_sql,
            owner = EXCLUDED.owner,
            embedding = EXCLUDED.embedding
        RETURNING id, name, dimension_keys, measure_sql, owner
        "#,
    )
    .bind(m.name)
    .bind(m.dimension_keys)
    .bind(m.measure_sql)
    .bind(m.owner)
    .bind(&v)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn top_k_metrics(
    pool: &PgPool,
    query: Vec<f32>,
    k: i64,
) -> Result<Vec<(MetricDefRecord, f64)>, StoreError> {
    let v = Vector::from(query);
    let rows: Vec<(Uuid, String, Vec<String>, String, Option<String>, f64)> =
        sqlx::query_as(
            r#"
            SELECT id, name, dimension_keys, measure_sql, owner, (embedding <=> $1) AS distance
            FROM metric_def WHERE embedding IS NOT NULL
            ORDER BY embedding <=> $1 LIMIT $2
            "#,
        )
        .bind(&v)
        .bind(k)
        .fetch_all(pool)
        .await
        .map_err(StoreError::Sql)?;
    Ok(rows
        .into_iter()
        .map(|(id, name, dimension_keys, measure_sql, owner, dist)| {
            (
                MetricDefRecord { id, name, dimension_keys, measure_sql, owner },
                dist,
            )
        })
        .collect())
}
```

- [ ] **Step 2：lib.rs 加 `pub mod knowledge;`**

- [ ] **Step 3：在 `store_integration.rs` 末尾追加**

```rust
use sqlai_store::knowledge;

#[ignore]
#[tokio::test]
async fn business_term_upsert_and_search() {
    let (_c, pool) = boot_pg().await;
    knowledge::upsert_term(&pool, knowledge::UpsertTerm {
        term: "GMV",
        aliases: &["成交额".into(), "总成交金额".into()],
        definition: "已支付订单金额合计",
        formula: Some("SUM(amount) WHERE status='paid'"),
        embedding: unit_vec_with_one_at(0, 1024),
    }).await.unwrap();

    knowledge::upsert_term(&pool, knowledge::UpsertTerm {
        term: "DAU",
        aliases: &["日活".into()],
        definition: "每日活跃用户数",
        formula: None,
        embedding: unit_vec_with_one_at(5, 1024),
    }).await.unwrap();

    let res = knowledge::top_k_terms(&pool, unit_vec_with_one_at(0, 1024), 1).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.term, "GMV");
}

#[ignore]
#[tokio::test]
async fn metric_def_upsert_and_search() {
    let (_c, pool) = boot_pg().await;
    knowledge::upsert_metric(&pool, knowledge::UpsertMetric {
        name: "daily_gmv",
        dimension_keys: &["date".into(), "channel".into()],
        measure_sql: "sum(amount)",
        owner: Some("data-team"),
        embedding: unit_vec_with_one_at(7, 1024),
    }).await.unwrap();

    let res = knowledge::top_k_metrics(&pool, unit_vec_with_one_at(7, 1024), 1).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].0.name, "daily_gmv");
    assert_eq!(res[0].0.dimension_keys, vec!["date".to_string(), "channel".to_string()]);
}
```

- [ ] **Step 4：跑测试**

预期：7 passed total（3 datasource + 2 schema + 2 knowledge）。

- [ ] **Step 5：commit**

```
git add crates/sqlai-store
git commit -m "feat(store): add business_term and metric_def CRUD with pgvector top-K search"
```

---

## Task 4：sqlai-exec ClickHouse 元数据 introspect 三个新方法

**Files:**
- Modify: `crates/sqlai-exec/src/clickhouse.rs`

- [ ] **Step 1：在 `ClickHouseExecutor` 之外、之上加三个独立方法**

把这块代码追加到 `crates/sqlai-exec/src/clickhouse.rs` 末尾（在 `#[cfg(test)] mod tests` 之前）：

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ChTableInfo {
    pub database: String,
    pub name: String,
    #[serde(default)]
    pub comment: Option<String>,
    pub total_rows: Option<u64>,
    pub engine: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChColumnInfo {
    pub database: String,
    pub table: String,
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default)]
    pub comment: Option<String>,
}

impl ReadonlyClickHouse {
    /// 列出指定 db 的所有表（排除 system / information_schema 等）。
    pub async fn introspect_tables(&self, db: &str) -> Result<Vec<ChTableInfo>, ExecError> {
        let sql = format!(
            "SELECT database, name, comment, total_rows, engine \
             FROM system.tables \
             WHERE database = '{}' AND engine NOT LIKE '%View%' \
             ORDER BY name FORMAT JSONEachRow",
            db.replace('\'', "''")
        );
        let raw = self.post_query(&sql).await?;
        parse_jsoneachrow(&raw)
    }

    pub async fn introspect_columns(
        &self,
        db: &str,
        table: &str,
    ) -> Result<Vec<ChColumnInfo>, ExecError> {
        let sql = format!(
            "SELECT database, table, name, type, comment \
             FROM system.columns \
             WHERE database = '{}' AND table = '{}' \
             ORDER BY position FORMAT JSONEachRow",
            db.replace('\'', "''"),
            table.replace('\'', "''"),
        );
        let raw = self.post_query(&sql).await?;
        parse_jsoneachrow(&raw)
    }

    /// 抽取某列的前 n 个 distinct 值，用于 schema linking 与脱敏检查。
    pub async fn sample_distinct(
        &self,
        db: &str,
        table: &str,
        column: &str,
        n: u32,
    ) -> Result<Vec<serde_json::Value>, ExecError> {
        // 反引号引用名字，防注入。
        let sql = format!(
            "SELECT DISTINCT `{}` AS v FROM `{}`.`{}` LIMIT {} FORMAT JSONEachRow",
            column.replace('`', "``"),
            db.replace('`', "``"),
            table.replace('`', "``"),
            n
        );
        let raw = self.post_query(&sql).await?;
        let rows: Vec<serde_json::Value> = parse_jsoneachrow_values(&raw)?;
        Ok(rows.into_iter().filter_map(|r| r.get("v").cloned()).collect())
    }
}

fn parse_jsoneachrow<T: for<'de> serde::Deserialize<'de>>(raw: &str) -> Result<Vec<T>, ExecError> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(line)
                .map_err(|e| ExecError::Engine(format!("introspect json: {e}")))?,
        );
    }
    Ok(out)
}

fn parse_jsoneachrow_values(raw: &str) -> Result<Vec<serde_json::Value>, ExecError> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(line)
                .map_err(|e| ExecError::Engine(format!("sample json: {e}")))?,
        );
    }
    Ok(out)
}
```

> 注意：`post_query` 当前是 `async fn`，且签名是 `pub(crate)` —— 它现在被同 module 内的 impl 复用，没问题。如果它当前是 `private`（既无 `pub` 也无 `pub(crate)`），把它改为 `pub(crate)` 也行，但因为 `introspect_*` 在同一文件 `impl ReadonlyClickHouse`，private 也能调到，不需要改。

- [ ] **Step 2：在 `crates/sqlai-exec/tests/clickhouse_integration.rs` 追加 ignored 测试**

```rust
use sqlai_exec::ReadonlyClickHouse;

#[ignore]
#[tokio::test]
async fn introspect_returns_at_least_system_table_count() {
    let cfg = sqlai_exec::ReadonlyConfig {
        url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into()),
        user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "admin".into()),
        password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "root23".into()),
        database: "default".into(),
        max_execution_time_secs: 30,
        max_result_rows: 1000,
    };
    let ch = ReadonlyClickHouse::new(cfg).unwrap();
    // 'system' 库一定存在，引擎里会有 system.tables 自身、system.columns 等
    let tables = ch.introspect_tables("system").await.unwrap();
    assert!(tables.iter().any(|t| t.name == "tables"), "system.tables should exist");
    let cols = ch.introspect_columns("system", "tables").await.unwrap();
    assert!(!cols.is_empty(), "system.tables must have columns");
    // sample_distinct: system.tables 里的 engine 列总有几种值
    let samples = ch.sample_distinct("system", "tables", "engine", 5).await.unwrap();
    assert!(!samples.is_empty());
}
```

- [ ] **Step 3：跑测试**

```
$env:CLICKHOUSE_URL="http://127.0.0.1:8123"; $env:CLICKHOUSE_USER="admin"; $env:CLICKHOUSE_PASSWORD="root23"; $env:CLICKHOUSE_DB="default"
cargo test -p sqlai-exec -- --ignored 2>&1 | tail -10
```

预期：4 ignored tests pass（原来的 3 + 这条新加的 1）。

- [ ] **Step 4：commit**

```
git add crates/sqlai-exec
git commit -m "feat(exec): introspect ClickHouse tables/columns and sample distinct values"
```

---

## Task 5：sqlai-cli `sync-schema` 命令（端到端）

**Files:**
- Modify: `crates/sqlai-cli/Cargo.toml`
- Create: `crates/sqlai-cli/src/sync_schema.rs`
- Modify: `crates/sqlai-cli/src/main.rs`

- [ ] **Step 1：扩展 cli 的 Cargo.toml**

```toml
[package]
name = "sqlai-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core      = { workspace = true }
sqlai-store     = { workspace = true }
sqlai-exec      = { workspace = true }
sqlai-llm       = { workspace = true }
sqlai-pipeline  = { workspace = true }
tokio           = { workspace = true }
clap            = { version = "4", features = ["derive"] }
anyhow          = { workspace = true }
tracing         = { workspace = true }
tracing-subscriber = { workspace = true }
uuid            = { workspace = true }
serde_json      = { workspace = true }

[[bin]]
name = "sqlai"
path = "src/main.rs"
```

- [ ] **Step 2：sync_schema.rs**

```rust
//! sqlai sync-schema：从指定 datasource 拉 ClickHouse 元数据并 upsert 到 PG。

use anyhow::{Context, Result};
use clap::Args;

use sqlai_exec::{ReadonlyClickHouse, ReadonlyConfig};
use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder};
use sqlai_llm::EmbeddingProvider;
use sqlai_store::{datasource, schema as store_schema, StoreConfig};

#[derive(Args, Debug)]
pub struct SyncArgs {
    /// 已注册的 datasource 名（PG datasource.name）
    #[arg(long)]
    pub datasource: String,

    /// 每列采样 distinct 值的数量
    #[arg(long, default_value_t = 8)]
    pub sample_size: u32,

    /// Sidecar /embed 端点
    #[arg(long, default_value = "http://127.0.0.1:8081")]
    pub sidecar_url: String,
}

pub async fn run(args: SyncArgs) -> Result<()> {
    let pg_cfg = StoreConfig::from_env().context("load PG config from env")?;
    let pool = sqlai_store::pool::connect(&pg_cfg).await.context("connect PG")?;

    let ds = datasource::get_by_name(&pool, &args.datasource)
        .await
        .with_context(|| format!("datasource '{}' not found in PG", args.datasource))?;
    tracing::info!("syncing datasource={} db={}", ds.name, ds.db);

    // 解析 secret_ref：支持 'env:VAR_NAME'（v1 唯一形式）。
    let password = if let Some(var) = ds.secret_ref.strip_prefix("env:") {
        std::env::var(var).with_context(|| format!("env {var} not set"))?
    } else {
        return Err(anyhow::anyhow!(
            "unsupported secret_ref scheme: {}",
            ds.secret_ref
        ));
    };

    let ch = ReadonlyClickHouse::new(ReadonlyConfig {
        url: format!("http://{}:{}", ds.host, ds.port),
        user: ds.user_name.clone(),
        password,
        database: ds.db.clone(),
        max_execution_time_secs: 30,
        max_result_rows: 5000,
    })
    .context("clickhouse client")?;

    let embedder = SidecarEmbedder::new(SidecarConfig {
        base_url: args.sidecar_url.clone(),
        timeout_secs: 600,
    })
    .context("sidecar embedder")?;

    let tables = ch
        .introspect_tables(&ds.db)
        .await
        .context("list tables")?;
    tracing::info!("found {} tables", tables.len());

    // 为减少 sidecar 往返，分两轮批量 embed：先所有表 prompt，再分表批量列 prompt。
    let table_prompts: Vec<String> = tables
        .iter()
        .map(|t| {
            format!(
                "{}.{}: {} (engine={}, rows~{})",
                t.database,
                t.name,
                t.comment.clone().unwrap_or_default(),
                t.engine,
                t.total_rows.unwrap_or(0)
            )
        })
        .collect();

    let table_embs = if table_prompts.is_empty() {
        vec![]
    } else {
        embedder
            .embed(&table_prompts)
            .await
            .context("embed table prompts")?
    };

    for (info, emb) in tables.iter().zip(table_embs.iter()) {
        let t = store_schema::upsert_table(
            &pool,
            store_schema::UpsertTable {
                datasource_id: ds.id,
                db: &info.database,
                table_name: &info.name,
                comment: info.comment.as_deref(),
                row_count_est: info.total_rows.map(|n| n as i64),
                embedding: emb.clone(),
            },
        )
        .await
        .context("upsert table")?;

        let cols = ch
            .introspect_columns(&info.database, &info.name)
            .await
            .with_context(|| format!("columns of {}.{}", info.database, info.name))?;
        if cols.is_empty() {
            continue;
        }

        // 按列采样
        let mut col_with_samples = Vec::with_capacity(cols.len());
        for c in &cols {
            let samples = ch
                .sample_distinct(&info.database, &info.name, &c.name, args.sample_size)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("sample {}.{}.{} failed: {e}", info.database, info.name, c.name);
                    vec![]
                });
            col_with_samples.push((c, samples));
        }

        let col_prompts: Vec<String> = col_with_samples
            .iter()
            .map(|(c, samples)| {
                format!(
                    "{}.{}.{} ({}): {}; samples: {}",
                    info.database,
                    info.name,
                    c.name,
                    c.data_type,
                    c.comment.clone().unwrap_or_default(),
                    serde_json::Value::Array(samples.clone())
                )
            })
            .collect();
        let col_embs = embedder
            .embed(&col_prompts)
            .await
            .with_context(|| format!("embed columns of {}.{}", info.database, info.name))?;

        for ((c, samples), emb) in col_with_samples.iter().zip(col_embs.iter()) {
            store_schema::upsert_column(
                &pool,
                store_schema::UpsertColumn {
                    table_id: t.id,
                    name: &c.name,
                    data_type: &c.data_type,
                    comment: c.comment.as_deref(),
                    sample_values: serde_json::Value::Array(samples.clone()),
                    distinct_count_est: None,
                    embedding: emb.clone(),
                },
            )
            .await
            .context("upsert column")?;
        }

        tracing::info!("synced {}.{}: {} columns", info.database, info.name, cols.len());
    }

    Ok(())
}
```

- [ ] **Step 3：main.rs 加子命令**

替换 `crates/sqlai-cli/src/main.rs`：

```rust
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
    /// 占位命令
    Hello,

    /// 把 ClickHouse schema 拉到 PG（含向量化）
    SyncSchema(sync_schema::SyncArgs),
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
        Cmd::Hello => {
            println!("sqlai-cli ready");
            Ok(())
        }
        Cmd::SyncSchema(args) => sync_schema::run(args).await,
    }
}
```

- [ ] **Step 4：build + 端到端跑通**

前提：
1. `docker compose up -d postgres sidecar`（PG 起来时 migration 自动跑；sidecar 跑起来）。
2. ClickHouse 在 `127.0.0.1:8123` 已经跑（用户已确认）。
3. 在 PG 里手动注册一条 datasource：

```bash
docker exec -i sqlai-pg psql -U sqlai -d sqlai <<'SQL'
INSERT INTO datasource (name, kind, host, port, db, user_name, secret_ref, readonly, settings)
VALUES ('ch_local', 'clickhouse', '127.0.0.1', 8123, 'default', 'admin', 'env:CLICKHOUSE_PASSWORD', TRUE, '{}'::jsonb)
ON CONFLICT (name) DO NOTHING;
SQL
```

跑：

```powershell
$env:SQLAI_PG_URL = "postgres://sqlai:sqlai@127.0.0.1:5432/sqlai"
$env:CLICKHOUSE_PASSWORD = "root23"
cargo run -p sqlai-cli -- sync-schema --datasource ch_local --sample-size 5
```

预期：日志显示 `syncing datasource=ch_local db=default; found N tables; synced default.<table>: M columns`，每张表都同步完成。

验证：

```bash
docker exec -i sqlai-pg psql -U sqlai -d sqlai -c "SELECT table_name, row_count_est FROM table_meta WHERE datasource_id=(SELECT id FROM datasource WHERE name='ch_local') ORDER BY table_name;"
docker exec -i sqlai-pg psql -U sqlai -d sqlai -c "SELECT t.table_name, c.name, c.data_type FROM column_meta c JOIN table_meta t ON c.table_id=t.id ORDER BY t.table_name, c.name LIMIT 20;"
```

- [ ] **Step 5：commit**

```
git add crates/sqlai-cli
git commit -m "feat(cli): add sync-schema command (ClickHouse -> embed -> upsert PG)"
```

---

## 验收清单（子计划 #3 完成时全部应可通过）

- [ ] `cargo build --workspace` ✅
- [ ] `cargo test --workspace` ✅ — 没有新增的非 ignored 单元测试，仍是 30 单元 + 9 ignored 集成（含 4 exec、4 sidecar/llm、新增 7 store + 1 exec）
- [ ] `cargo clippy --workspace -- -D warnings` ✅
- [ ] `cargo fmt --all -- --check` ✅
- [ ] `docker compose up -d postgres` 后 store 集成 7 ignored tests pass
- [ ] sync-schema 端到端跑通：`cargo run -p sqlai-cli -- sync-schema --datasource ch_local` 完成且 PG 中 `table_meta` / `column_meta` 有数据
- [ ] `git log` 至少 5 条本子计划新增 commit

---

## 进入下一份子计划

完成本计划后，下一份是 **#4：核心 pipeline + AnalysisSkill 抽象 + 描述/诊断 skill 实现**。
