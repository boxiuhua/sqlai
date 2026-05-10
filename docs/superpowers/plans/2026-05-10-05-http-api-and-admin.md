# 智能问数系统 v1.0 — 子计划 #5：HTTP/SSE API + 会话持久化 + Admin CRUD

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 Pipeline 通过 axum HTTP 服务暴露给前端：核心问答走 SSE 流式，会话/消息持久化进 PG，Admin 接口提供数据源 / 业务词表 / 指标定义 / few-shot 的 CRUD（含自动 embedding）。完成后前端只需对接这些 API 即可工作。

**Architecture:** 全部新增工作集中在 `sqlai-api`（升级为真实 axum 服务）+ `sqlai-store::session` 新模块。`AppState` 持有 `Pipeline` + `PgPool` + `Arc<dyn EmbeddingProvider>`。`POST /api/sessions/:id/ask` 把 `Pipeline::ask()` 返回的 `mpsc::Receiver<PipelineEvent>` 直接转成 `text/event-stream` 响应；前后端协议用 spec § 4.2 定义的事件名。Admin 接口在写入 `business_term` / `metric_def` 前同步调用 sidecar `/embed` 拿到 1024 维向量。

**Tech Stack:** axum 0.7 + tower 0.5 + tower-http 0.6（CORS）+ futures-util（SSE 流转换）+ tokio + 已有的 sqlai 全家桶。

**前置假设：**
- #1-#4 完成（33 commit）。
- 工作目录持续运行：sidecar :8081、ClickHouse 8123 admin/root23、PG :5432（本地或 testcontainers）。

---

## File Structure

```
sqlai/
├── crates/
│   ├── sqlai-store/
│   │   ├── src/
│   │   │   ├── lib.rs              # 加 pub mod session;
│   │   │   └── session.rs          # NEW
│   │   └── tests/store_integration.rs   # 追加 session 测试
│   ├── sqlai-api/
│   │   ├── Cargo.toml              # 大幅扩展依赖
│   │   ├── src/
│   │   │   ├── main.rs             # 加载 .env / 配置 / 启动 axum
│   │   │   ├── lib.rs              # 暴露 build_app(state) 给集成测试
│   │   │   ├── state.rs            # AppState
│   │   │   ├── error.rs            # ApiError + 全局 IntoResponse
│   │   │   ├── routes/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── sessions.rs     # POST /api/sessions, GET messages, POST ask(SSE), GET export.csv
│   │   │   │   └── admin.rs        # datasource / business_term / metric_def CRUD
│   │   └── tests/
│   │       └── api_e2e.rs          # 端到端 HTTP 集成测试（含 SSE 事件解析）
└── docs/superpowers/plans/
    └── 2026-05-10-05-http-api-and-admin.md
```

---

## API 协议（依据 spec § 10）

| Method | Path | 备注 |
|---|---|---|
| GET    | `/healthz` | 200 OK `{"ok":true}` |
| POST   | `/api/sessions` | body `{user_id, datasource_id, title?}` → 返回 `Session` |
| GET    | `/api/sessions/:id/messages` | 返回 `Vec<Message>`（按 created_at 升序） |
| POST   | `/api/sessions/:id/ask` | body `{question}`；响应 `text/event-stream`，每行 `event: <name>\ndata: <json>\n\n` |
| GET    | `/api/messages/:id/export.csv` | 把消息附带的结果集导出为 CSV，`Content-Type: text/csv` |
| POST   | `/api/admin/datasources` | body `{name, kind, host, port, db, user_name, secret_ref, settings}` |
| GET    | `/api/admin/datasources` | list |
| POST   | `/api/admin/business-terms` | body `{term, aliases, definition, formula?}`；服务端同步调用 sidecar embed |
| GET    | `/api/admin/business-terms` | list |
| PUT    | `/api/admin/business-terms/:term` | replace by term name |
| DELETE | `/api/admin/business-terms/:term` | |
| POST   | `/api/admin/metrics` | body `{name, dimension_keys, measure_sql, owner?}`；同步 embed |
| GET    | `/api/admin/metrics` | list |
| PUT    | `/api/admin/metrics/:name` | |
| DELETE | `/api/admin/metrics/:name` | |

错误响应统一为 JSON：`{ "error": { "code": "...", "message": "..." } }`，HTTP 状态码 4xx/5xx。

---

## Task 1：sqlai-store::session 模块（session + message 持久化）

**Files:**
- Create: `crates/sqlai-store/src/session.rs`
- Modify: `crates/sqlai-store/src/lib.rs`
- Append: `crates/sqlai-store/tests/store_integration.rs`

- [ ] **Step 1：session.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::StoreError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionRecord {
    pub id: Uuid,
    pub user_id: String,
    pub datasource_id: Option<Uuid>,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create_session(
    pool: &PgPool,
    user_id: &str,
    datasource_id: Option<Uuid>,
    title: Option<&str>,
) -> Result<SessionRecord, StoreError> {
    sqlx::query_as::<_, SessionRecord>(
        r#"
        INSERT INTO session (user_id, datasource_id, title)
        VALUES ($1, $2, $3)
        RETURNING id, user_id, datasource_id, title, created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(datasource_id)
    .bind(title)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn touch_session(pool: &PgPool, id: Uuid) -> Result<(), StoreError> {
    sqlx::query("UPDATE session SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(StoreError::Sql)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageRecord {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: String,         // user / assistant / system
    pub content: serde_json::Value,
    pub plan: Option<serde_json::Value>,
    pub chart_spec: Option<serde_json::Value>,
    pub rows_returned: Option<i32>,
    pub latency_ms: Option<i32>,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub session_id: Uuid,
    pub role: String,
    pub content: serde_json::Value,
    pub plan: Option<serde_json::Value>,
    pub chart_spec: Option<serde_json::Value>,
    pub rows_returned: Option<i32>,
    pub latency_ms: Option<i32>,
    pub parent_id: Option<Uuid>,
}

pub async fn append_message(pool: &PgPool, m: NewMessage) -> Result<MessageRecord, StoreError> {
    sqlx::query_as::<_, MessageRecord>(
        r#"
        INSERT INTO message (session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id, created_at
        "#,
    )
    .bind(m.session_id)
    .bind(m.role)
    .bind(m.content)
    .bind(m.plan)
    .bind(m.chart_spec)
    .bind(m.rows_returned)
    .bind(m.latency_ms)
    .bind(m.parent_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn list_messages(pool: &PgPool, session_id: Uuid) -> Result<Vec<MessageRecord>, StoreError> {
    sqlx::query_as::<_, MessageRecord>(
        r#"
        SELECT id, session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id, created_at
        FROM message WHERE session_id = $1 ORDER BY created_at ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::Sql)
}

pub async fn get_message(pool: &PgPool, id: Uuid) -> Result<MessageRecord, StoreError> {
    sqlx::query_as::<_, MessageRecord>(
        r#"
        SELECT id, session_id, role, content, plan, chart_spec, rows_returned, latency_ms, parent_id, created_at
        FROM message WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound)
}
```

- [ ] **Step 2：lib.rs 加模块**

```rust
pub mod session;
```

- [ ] **Step 3：append integration tests**

```rust
use sqlai_store::session;
use sqlai_store::session::NewMessage;

#[ignore]
#[tokio::test]
async fn session_and_messages_roundtrip() {
    let (_c, pool) = boot_pg().await;

    let s = session::create_session(&pool, "user-a", None, Some("first chat")).await.unwrap();
    assert_eq!(s.user_id, "user-a");
    assert_eq!(s.title.as_deref(), Some("first chat"));

    let user_msg = session::append_message(&pool, NewMessage {
        session_id: s.id, role: "user".into(),
        content: serde_json::json!({"text":"hello"}),
        plan: None, chart_spec: None, rows_returned: None, latency_ms: None, parent_id: None,
    }).await.unwrap();

    let asst_msg = session::append_message(&pool, NewMessage {
        session_id: s.id, role: "assistant".into(),
        content: serde_json::json!({"summary":"hi"}),
        plan: Some(serde_json::json!({"steps":[]})),
        chart_spec: Some(serde_json::json!({"kind":"none"})),
        rows_returned: Some(0), latency_ms: Some(123),
        parent_id: Some(user_msg.id),
    }).await.unwrap();

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
    let s = session::create_session(&pool, "u", None, None).await.unwrap();
    let before = s.updated_at;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    session::touch_session(&pool, s.id).await.unwrap();
    let msgs = session::list_messages(&pool, s.id).await.unwrap();
    let _ = msgs;
    // 直接 fetch 验证 updated_at
    let r: (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT updated_at FROM session WHERE id = $1")
            .bind(s.id).fetch_one(&pool).await.unwrap();
    assert!(r.0 > before);
}
```

- [ ] **Step 4：跑 + commit**

```
cargo test -p sqlai-store --test store_integration -- --ignored 2>&1 | tail -10
```
预期：9 passed (7 + 2 session)。

```
git add crates/sqlai-store
git commit -m "feat(store): add session/message CRUD module + integration tests"
```

---

## Task 2：sqlai-api 骨架（AppState + healthz + POST /api/sessions + GET messages）

**Files:**
- Modify: `crates/sqlai-api/Cargo.toml`
- Create: `crates/sqlai-api/src/{lib.rs, state.rs, error.rs}`
- Create: `crates/sqlai-api/src/routes/{mod.rs, sessions.rs}`
- Modify: `crates/sqlai-api/src/main.rs`

- [ ] **Step 1：Cargo.toml**

```toml
[package]
name = "sqlai-api"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core      = { workspace = true }
sqlai-llm       = { workspace = true }
sqlai-store     = { workspace = true }
sqlai-pipeline  = { workspace = true }
sqlai-exec      = { workspace = true }
sqlai-skills    = { workspace = true }

axum            = { version = "0.7", features = ["macros"] }
tower           = "0.5"
tower-http      = { version = "0.6", features = ["cors", "trace"] }
serde           = { workspace = true }
serde_json      = { workspace = true }
tokio           = { workspace = true }
tokio-stream    = "0.1"
futures-util    = "0.3"
tracing         = { workspace = true }
tracing-subscriber = { workspace = true }
thiserror       = { workspace = true }
anyhow          = { workspace = true }
uuid            = { workspace = true }
sqlx            = { workspace = true }
chrono          = { workspace = true }

[dev-dependencies]
tokio                  = { workspace = true }
testcontainers         = { workspace = true }
testcontainers-modules = { workspace = true }
reqwest                = { workspace = true }

[[bin]]
name = "sqlai-api"
path = "src/main.rs"
```

- [ ] **Step 2：state.rs**

```rust
use sqlai_llm::EmbeddingProvider;
use sqlai_pipeline::Pipeline;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub pipeline: Pipeline,
    pub embedder: Arc<dyn EmbeddingProvider>,
}
```

- [ ] **Step 3：error.rs**

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl ApiError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let body = Json(json!({
            "error": { "code": code, "message": self.to_string() }
        }));
        (status, body).into_response()
    }
}

impl From<sqlai_store::StoreError> for ApiError {
    fn from(e: sqlai_store::StoreError) -> Self {
        match e {
            sqlai_store::StoreError::NotFound => ApiError::NotFound,
            sqlai_store::StoreError::Conflict(m) => ApiError::BadRequest(m),
            other => ApiError::Internal(other.to_string()),
        }
    }
}
```

- [ ] **Step 4：routes/mod.rs**

```rust
pub mod sessions;
```

- [ ] **Step 5：routes/sessions.rs（先实现 create + list messages，ask/SSE/export 在 Task 3-4 加）**

```rust
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sqlai_store::session;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSessionReq {
    pub user_id: String,
    pub datasource_id: Option<Uuid>,
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionResp {
    pub id: Uuid,
    pub user_id: String,
    pub datasource_id: Option<Uuid>,
    pub title: Option<String>,
}

pub async fn create_session(
    State(s): State<AppState>,
    Json(req): Json<CreateSessionReq>,
) -> Result<impl IntoResponse, ApiError> {
    let r = session::create_session(&s.pool, &req.user_id, req.datasource_id, req.title.as_deref()).await?;
    Ok(Json(SessionResp {
        id: r.id, user_id: r.user_id, datasource_id: r.datasource_id, title: r.title,
    }))
}

pub async fn list_messages(
    State(s): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let msgs = session::list_messages(&s.pool, session_id).await?;
    Ok(Json(msgs))
}
```

- [ ] **Step 6：lib.rs（暴露 build_app 给集成测试）**

```rust
pub mod error;
pub mod routes;
pub mod state;

use axum::routing::{get, post};
use axum::Router;
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { axum::Json(json!({"ok": true})) }))
        .route("/api/sessions", post(routes::sessions::create_session))
        .route(
            "/api/sessions/:session_id/messages",
            get(routes::sessions::list_messages),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
```

- [ ] **Step 7：main.rs**

```rust
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
    let pool = connect(&StoreConfig { url: pg_url, max_connections: 10 }).await?;

    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(SidecarEmbedder::new(SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 600,
    })?);
    let llm: Arc<dyn LlmProvider> = Arc::new(DeepSeekProvider::new(DeepSeekConfig {
        base_url: std::env::var("DEEPSEEK_BASE_URL").unwrap_or_else(|_| "https://api.deepseek.com".into()),
        api_key: std::env::var("DEEPSEEK_API_KEY").map_err(|_| anyhow::anyhow!("set DEEPSEEK_API_KEY"))?,
        model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
        timeout_secs: 60,
    })?);
    let executor: Arc<dyn Executor> = Arc::new(ClickHouseExecutor::new(
        ReadonlyClickHouse::new(ReadonlyConfig {
            url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://127.0.0.1:8123".into()),
            user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "admin".into()),
            password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "".into()),
            database: std::env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "default".into()),
            max_execution_time_secs: 30,
            max_result_rows: 1000,
        })?
    ));

    let pipeline = Pipeline {
        llm, embedder: embedder.clone(), pool: pool.clone(), executor,
        skills: Arc::new(SkillRegistry::with_defaults()),
    };
    let state = AppState { pool, pipeline, embedder };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("sqlai-api listening on :8080");
    axum::serve(listener, build_app(state)).await?;
    Ok(())
}
```

- [ ] **Step 8：build + commit**

```
cargo build -p sqlai-api 2>&1 | tail -5
```
预期：clean build。bin 不在测试中跑（main 需要外部依赖）。

```
git add crates/sqlai-api
git commit -m "feat(api): add axum AppState + healthz + sessions/messages CRUD"
```

---

## Task 3：POST /api/sessions/:id/ask（SSE 流式问答）

**Files:**
- Modify: `crates/sqlai-api/src/routes/sessions.rs`
- Modify: `crates/sqlai-api/src/lib.rs`

- [ ] **Step 1：在 sessions.rs 追加 ask handler**

```rust
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use sqlai_pipeline::{AskRequest, PipelineEvent};
use sqlai_store::session::NewMessage;
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

#[derive(Debug, Deserialize)]
pub struct AskBody {
    pub question: String,
    #[serde(default)]
    pub user_id: String, // 给消息打 user_id；为空 -> 'anonymous'
}

pub async fn ask(
    State(s): State<AppState>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<AskBody>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // 找出 session
    let sess: sqlai_store::session::SessionRecord =
        sqlx::query_as::<_, sqlai_store::session::SessionRecord>(
            "SELECT id, user_id, datasource_id, title, created_at, updated_at FROM session WHERE id = $1"
        )
        .bind(session_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let datasource_id = sess.datasource_id
        .ok_or_else(|| ApiError::BadRequest("session has no datasource_id".into()))?;

    // 持久化 user 消息
    let user_msg = sqlai_store::session::append_message(&s.pool, NewMessage {
        session_id: sess.id, role: "user".into(),
        content: serde_json::json!({ "text": body.question }),
        plan: None, chart_spec: None, rows_returned: None, latency_ms: None,
        parent_id: None,
    }).await?;

    // 启动 pipeline，拿到事件流
    let req = AskRequest {
        session_id: sess.id, datasource_id, question: body.question.clone(),
        history: vec![],
    };
    let mut rx = s.pipeline.ask(req);

    // 把事件转成 SSE，同时在尾部把 assistant 消息持久化
    let pool = s.pool.clone();
    let session_id_for_sink = sess.id;
    let user_msg_id = user_msg.id;
    let (sse_tx, sse_rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    tokio::spawn(async move {
        let mut acc_plan: Option<serde_json::Value> = None;
        let mut acc_chart: Option<serde_json::Value> = None;
        let mut acc_rows: i32 = 0;
        let mut acc_summary: String = String::new();
        let mut latency_ms: Option<i32> = None;

        while let Some(ev) = rx.recv().await {
            // 累计上下文，便于结尾持久化 assistant 消息
            match &ev {
                PipelineEvent::SkillCall { plan, .. } => {
                    acc_plan = Some(serde_json::to_value(plan).unwrap_or_default());
                }
                PipelineEvent::Chart(c) => acc_chart = Some(serde_json::to_value(c).unwrap_or_default()),
                PipelineEvent::Rows(r) => acc_rows = acc_rows.saturating_add(r.rows.len() as i32),
                PipelineEvent::Summary { text } => acc_summary = text.clone(),
                PipelineEvent::Done { latency_ms: l } => latency_ms = Some(*l as i32),
                _ => {}
            }
            // 序列化为 SSE 事件
            let (name, data) = match &ev {
                PipelineEvent::Intent(_) => ("intent", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::SkillCall { .. } => ("skill_call", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::Validate { .. } => ("validate", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::Rows(_) => ("rows", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::Chart(_) => ("chart", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::MetricsRecommend(_) => ("metrics_recommend", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::Summary { .. } => ("summary", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::Done { .. } => ("done", serde_json::to_value(&ev).unwrap_or_default()),
                PipelineEvent::Error { .. } => ("error", serde_json::to_value(&ev).unwrap_or_default()),
            };
            let evt = Event::default().event(name).json_data(data).unwrap_or_else(|_| Event::default());
            if sse_tx.send(Ok(evt)).await.is_err() { break; }
        }

        // 流末尾持久化 assistant 消息
        let _ = sqlai_store::session::append_message(&pool, NewMessage {
            session_id: session_id_for_sink, role: "assistant".into(),
            content: serde_json::json!({ "summary": acc_summary }),
            plan: acc_plan,
            chart_spec: acc_chart,
            rows_returned: Some(acc_rows),
            latency_ms,
            parent_id: Some(user_msg_id),
        }).await;
        let _ = sqlai_store::session::touch_session(&pool, session_id_for_sink).await;
    });

    let stream = ReceiverStream::new(sse_rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
```

- [ ] **Step 2：lib.rs 注册路由**

替换 `build_app`：

```rust
pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { axum::Json(json!({"ok": true})) }))
        .route("/api/sessions", post(routes::sessions::create_session))
        .route(
            "/api/sessions/:session_id/messages",
            get(routes::sessions::list_messages),
        )
        .route(
            "/api/sessions/:session_id/ask",
            post(routes::sessions::ask),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
```

- [ ] **Step 3：build + commit**

```
cargo build -p sqlai-api
```

```
git add crates/sqlai-api
git commit -m "feat(api): add SSE ask endpoint forwarding pipeline events + persisting assistant message"
```

---

## Task 4：CSV 导出 endpoint

**Files:**
- Modify: `crates/sqlai-api/src/routes/sessions.rs`
- Modify: `crates/sqlai-api/src/lib.rs`

- [ ] **Step 1：实现 CSV 导出**

assistant 消息的 `content` 当前只存 `{summary:...}`；行数据来自 SSE 时的 `Rows` 事件。为了让 export 工作，我们把首步 rows snapshot 也存进 message。修改 ask handler 累积逻辑：

在 `routes/sessions.rs::ask` 中替换 acc_rows 维护为同时累积**第一个 step 的列与前 1000 行**，并在最终 `append_message` 时把它们写进 content：

```rust
let mut acc_first_columns: Option<Vec<String>> = None;
let mut acc_first_rows: Vec<serde_json::Value> = Vec::new();
const MAX_PERSISTED_ROWS: usize = 1000;
// ... 在 match 分支中：
PipelineEvent::Rows(r) => {
    if acc_first_columns.is_none() {
        acc_first_columns = Some(r.columns.clone());
    }
    if acc_first_rows.len() < MAX_PERSISTED_ROWS {
        let take = MAX_PERSISTED_ROWS - acc_first_rows.len();
        acc_first_rows.extend(r.rows.iter().take(take).cloned());
    }
    acc_rows = acc_rows.saturating_add(r.rows.len() as i32);
}
// ... 在最终 append_message 里：
content: serde_json::json!({
    "summary": acc_summary,
    "columns": acc_first_columns.unwrap_or_default(),
    "rows": acc_first_rows,
}),
```

写入 `routes/sessions.rs` export handler：

```rust
use axum::http::header;
use axum::response::Response;
use axum::body::Body;

pub async fn export_csv(
    State(s): State<AppState>,
    Path(message_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let m = sqlai_store::session::get_message(&s.pool, message_id).await?;
    let columns: Vec<String> = m.content
        .get("columns").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let rows: Vec<serde_json::Value> = m.content
        .get("rows").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let mut buf = String::new();
    buf.push_str(&columns.join(","));
    buf.push('\n');
    for r in rows {
        let cells: Vec<String> = columns.iter().map(|c| {
            let v = r.get(c).cloned().unwrap_or(serde_json::Value::Null);
            csv_cell(&v)
        }).collect();
        buf.push_str(&cells.join(","));
        buf.push('\n');
    }

    let body = Body::from(buf);
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"message-{}.csv\"", m.id))
        .body(body).unwrap())
}

fn csv_cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => csv_escape(s),
        other => csv_escape(&other.to_string()),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
```

- [ ] **Step 2：lib.rs 加路由**

```rust
.route(
    "/api/messages/:message_id/export.csv",
    get(routes::sessions::export_csv),
)
```

- [ ] **Step 3：build + commit**

```
cargo build -p sqlai-api
```

```
git add crates/sqlai-api
git commit -m "feat(api): persist row snapshot + add CSV export endpoint"
```

---

## Task 5：Admin CRUD（datasource + business_term + metric_def）

**Files:**
- Create: `crates/sqlai-api/src/routes/admin.rs`
- Modify: `crates/sqlai-api/src/routes/mod.rs`
- Modify: `crates/sqlai-api/src/lib.rs`

- [ ] **Step 1：admin.rs**

```rust
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sqlai_llm::EmbeddingProvider;
use sqlai_store::{datasource, knowledge};

use crate::error::ApiError;
use crate::state::AppState;

// ----- datasource -----

#[derive(Debug, Deserialize)]
pub struct CreateDatasourceReq {
    pub name: String,
    pub kind: String,
    pub host: String,
    pub port: i32,
    pub db: String,
    pub user_name: String,
    pub secret_ref: String,
    #[serde(default = "default_true")]
    pub readonly: bool,
    #[serde(default = "default_settings")]
    pub settings: serde_json::Value,
}

fn default_true() -> bool { true }
fn default_settings() -> serde_json::Value { serde_json::json!({}) }

pub async fn create_datasource(
    State(s): State<AppState>,
    Json(req): Json<CreateDatasourceReq>,
) -> Result<impl IntoResponse, ApiError> {
    let r = datasource::insert(&s.pool, datasource::NewDatasource {
        name: &req.name, kind: &req.kind, host: &req.host, port: req.port,
        db: &req.db, user_name: &req.user_name, secret_ref: &req.secret_ref,
        readonly: req.readonly, settings: req.settings,
    }).await?;
    Ok(Json(r))
}

pub async fn list_datasources(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let r = datasource::list(&s.pool).await?;
    Ok(Json(r))
}

// ----- business_term -----

#[derive(Debug, Deserialize)]
pub struct UpsertTermReq {
    pub term: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub definition: String,
    #[serde(default)]
    pub formula: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TermResp {
    pub id: Uuid,
    pub term: String,
    pub aliases: Vec<String>,
    pub definition: String,
    pub formula: Option<String>,
}

async fn embed_text(embedder: &std::sync::Arc<dyn EmbeddingProvider>, text: &str) -> Result<Vec<f32>, ApiError> {
    let v = embedder.embed(&[text.to_string()])
        .await.map_err(|e| ApiError::Internal(format!("embed: {e}")))?;
    v.into_iter().next().ok_or_else(|| ApiError::Internal("no embedding".into()))
}

pub async fn create_or_replace_term(
    State(s): State<AppState>,
    Json(req): Json<UpsertTermReq>,
) -> Result<impl IntoResponse, ApiError> {
    let prompt = format!("{}\naliases: {:?}\n{}{}",
        req.term, req.aliases, req.definition,
        req.formula.as_deref().map(|f| format!("\nformula: {f}")).unwrap_or_default()
    );
    let emb = embed_text(&s.embedder, &prompt).await?;
    let r = knowledge::upsert_term(&s.pool, knowledge::UpsertTerm {
        term: &req.term, aliases: &req.aliases,
        definition: &req.definition, formula: req.formula.as_deref(),
        embedding: emb,
    }).await?;
    Ok(Json(TermResp {
        id: r.id, term: r.term, aliases: r.aliases, definition: r.definition, formula: r.formula,
    }))
}

pub async fn list_terms(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let r: Vec<knowledge::BusinessTermRecord> = sqlx::query_as::<_, knowledge::BusinessTermRecord>(
        "SELECT id, term, aliases, definition, formula FROM business_term ORDER BY term"
    ).fetch_all(&s.pool).await.map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(r))
}

pub async fn delete_term(
    State(s): State<AppState>,
    Path(term): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let n = sqlx::query("DELETE FROM business_term WHERE term = $1")
        .bind(&term).execute(&s.pool).await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .rows_affected();
    if n == 0 { Err(ApiError::NotFound) } else { Ok(Json(serde_json::json!({"deleted": term}))) }
}

// ----- metric_def -----

#[derive(Debug, Deserialize)]
pub struct UpsertMetricReq {
    pub name: String,
    pub dimension_keys: Vec<String>,
    pub measure_sql: String,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MetricResp {
    pub id: Uuid,
    pub name: String,
    pub dimension_keys: Vec<String>,
    pub measure_sql: String,
    pub owner: Option<String>,
}

pub async fn create_or_replace_metric(
    State(s): State<AppState>,
    Json(req): Json<UpsertMetricReq>,
) -> Result<impl IntoResponse, ApiError> {
    let prompt = format!("metric={}\ndimensions={:?}\nsql={}", req.name, req.dimension_keys, req.measure_sql);
    let emb = embed_text(&s.embedder, &prompt).await?;
    let r = knowledge::upsert_metric(&s.pool, knowledge::UpsertMetric {
        name: &req.name, dimension_keys: &req.dimension_keys, measure_sql: &req.measure_sql,
        owner: req.owner.as_deref(), embedding: emb,
    }).await?;
    Ok(Json(MetricResp {
        id: r.id, name: r.name, dimension_keys: r.dimension_keys,
        measure_sql: r.measure_sql, owner: r.owner,
    }))
}

pub async fn list_metrics(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let r: Vec<knowledge::MetricDefRecord> = sqlx::query_as::<_, knowledge::MetricDefRecord>(
        "SELECT id, name, dimension_keys, measure_sql, owner FROM metric_def ORDER BY name"
    ).fetch_all(&s.pool).await.map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(r))
}

pub async fn delete_metric(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let n = sqlx::query("DELETE FROM metric_def WHERE name = $1")
        .bind(&name).execute(&s.pool).await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .rows_affected();
    if n == 0 { Err(ApiError::NotFound) } else { Ok(Json(serde_json::json!({"deleted": name}))) }
}
```

- [ ] **Step 2：mod.rs 加导出**

```rust
pub mod admin;
pub mod sessions;
```

- [ ] **Step 3：lib.rs 注册路由**

```rust
.route("/api/admin/datasources", post(routes::admin::create_datasource).get(routes::admin::list_datasources))
.route("/api/admin/business-terms", post(routes::admin::create_or_replace_term).get(routes::admin::list_terms))
.route("/api/admin/business-terms/:term", axum::routing::delete(routes::admin::delete_term))
.route("/api/admin/metrics", post(routes::admin::create_or_replace_metric).get(routes::admin::list_metrics))
.route("/api/admin/metrics/:name", axum::routing::delete(routes::admin::delete_metric))
```

PUT 在 v1 用 POST 复用（upsert by term/name）；DELETE 单独。

- [ ] **Step 4：build + commit**

```
cargo build -p sqlai-api
```

```
git add crates/sqlai-api
git commit -m "feat(api): add admin CRUD endpoints (datasource, business_term, metric_def) with auto-embed"
```

---

## Task 6：端到端 HTTP 集成测试

**Files:**
- Create: `crates/sqlai-api/tests/api_e2e.rs`

- [ ] **Step 1：测试**

```rust
//! 端到端：起一个临时 axum 服务（指向 testcontainers PG + 真实 sidecar/CH/DeepSeek），
//! 用 reqwest 调 /api/* 验证：创建 session → /ask SSE → list messages → CSV 导出。
//!
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
    Arc<dyn EmbeddingProvider>,
    AppState,
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
    let pool = sqlai_store::pool::connect(&sqlai_store::StoreConfig { url, max_connections: 4 }).await.unwrap();
    let migrations_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap().join("migrations");
    sqlai_store::pool::run_migrations(&pool, &migrations_dir).await.unwrap();

    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(SidecarEmbedder::new(SidecarConfig {
        base_url: "http://127.0.0.1:8081".into(),
        timeout_secs: 600,
    }).unwrap());
    let llm: Arc<dyn LlmProvider> = Arc::new(DeepSeekProvider::new(DeepSeekConfig {
        base_url: "https://api.deepseek.com".into(),
        api_key: std::env::var("DEEPSEEK_API_KEY").expect("set DEEPSEEK_API_KEY"),
        model: "deepseek-chat".into(),
        timeout_secs: 60,
    }).unwrap());
    let executor: Arc<dyn Executor> = Arc::new(ClickHouseExecutor::new(
        ReadonlyClickHouse::new(ReadonlyConfig {
            url: "http://127.0.0.1:8123".into(),
            user: "admin".into(),
            password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "root23".into()),
            database: "default".into(),
            max_execution_time_secs: 30, max_result_rows: 1000,
        }).unwrap()
    ));

    // 注册一条 datasource
    let ds = sqlai_store::datasource::insert(&pool, NewDatasource {
        name: "ch_e2e", kind: "clickhouse", host: "127.0.0.1", port: 8123,
        db: "default", user_name: "admin", secret_ref: "env:CLICKHOUSE_PASSWORD",
        readonly: true, settings: json!({}),
    }).await.unwrap();

    // seed schema
    let prompts = vec![
        "default.orders: 订单表".to_string(),
        "default.orders.amount (Decimal(18,2)): 订单金额".to_string(),
        "default.orders.created_at (DateTime): 下单时间".to_string(),
    ];
    let embs = embedder.embed(&prompts).await.unwrap();
    let t = sqlai_store::schema::upsert_table(&pool, UpsertTable {
        datasource_id: ds.id, db: "default", table_name: "orders",
        comment: Some("订单表"), row_count_est: Some(5), embedding: embs[0].clone(),
    }).await.unwrap();
    sqlai_store::schema::upsert_column(&pool, UpsertColumn {
        table_id: t.id, name: "amount", data_type: "Decimal(18,2)",
        comment: Some("订单金额"), sample_values: json!([1.0]),
        distinct_count_est: None, embedding: embs[1].clone(),
    }).await.unwrap();
    sqlai_store::schema::upsert_column(&pool, UpsertColumn {
        table_id: t.id, name: "created_at", data_type: "DateTime",
        comment: Some("下单时间"), sample_values: json!(["2025-01-01 00:00:00"]),
        distinct_count_est: None, embedding: embs[2].clone(),
    }).await.unwrap();

    let pipeline = Pipeline {
        llm, embedder: embedder.clone(), pool: pool.clone(), executor,
        skills: Arc::new(SkillRegistry::with_defaults()),
    };
    let state = AppState { pool: pool.clone(), pipeline, embedder: embedder.clone() };

    // 起一个 ephemeral axum server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_app(state.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    (container, pool, embedder, state, format!("http://{}", addr), ds.id)
}

#[ignore]
#[tokio::test]
async fn http_e2e_ask_returns_sse_events_then_message_persisted() {
    let (_pg, pool, _e, _state, base, ds_id) = boot().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();

    // 健康检查
    let r = client.get(format!("{base}/healthz")).send().await.unwrap();
    assert_eq!(r.status(), 200);

    // 创建 session
    let r = client.post(format!("{base}/api/sessions"))
        .json(&json!({"user_id":"alice", "datasource_id": ds_id, "title":"e2e"}))
        .send().await.unwrap();
    assert_eq!(r.status(), 200);
    let sess: serde_json::Value = r.json().await.unwrap();
    let session_id = sess["id"].as_str().unwrap().to_string();

    // /ask SSE：手动收集 event 行
    let r = client.post(format!("{base}/api/sessions/{session_id}/ask"))
        .json(&json!({"question":"看一下 default.orders 按天的订单金额趋势"}))
        .send().await.unwrap();
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
    let mid = ["skill_call","rows","summary"].iter()
        .filter(|n| event_names.contains(&n.to_string())).count();
    assert!(mid >= 2, "events: {event_names:?}");

    // list messages
    let r = client.get(format!("{base}/api/sessions/{session_id}/messages")).send().await.unwrap();
    let msgs: Vec<serde_json::Value> = r.json().await.unwrap();
    assert!(msgs.iter().any(|m| m["role"] == "user"));
    let asst = msgs.iter().find(|m| m["role"] == "assistant").expect("assistant message persisted");
    assert!(asst["content"]["summary"].is_string());

    // CSV 导出
    let asst_id = asst["id"].as_str().unwrap();
    let r = client.get(format!("{base}/api/messages/{asst_id}/export.csv")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let csv = r.text().await.unwrap();
    assert!(csv.contains("bucket") || csv.is_empty(), "csv body: {csv}");

    let _ = pool;
}
```

- [ ] **Step 2：build + 跑**

```powershell
cargo build -p sqlai-api --tests
$env:DEEPSEEK_API_KEY="sk-..."
$env:CLICKHOUSE_PASSWORD="root23"
cargo test -p sqlai-api --test api_e2e -- --ignored --nocapture 2>&1 | Select-Object -Last 40
```

预期：1 passed。

- [ ] **Step 3：commit**

```
git add crates/sqlai-api
git commit -m "test(api): add end-to-end HTTP+SSE test against real stack"
```

---

## 验收清单

- [ ] `cargo build --workspace` ✅
- [ ] `cargo test --workspace` ✅（49 单元）
- [ ] `cargo clippy --workspace -- -D warnings` ✅
- [ ] `cargo fmt --all -- --check` ✅
- [ ] `sqlai-store` 集成测试 9 ignored ✅
- [ ] `sqlai-api` 端到端 HTTP+SSE 1 ignored ✅
- [ ] `git log` 至少 6 条本子计划 commit

---

## 进入下一份子计划

完成本计划后，下一份是 **#6：轻预测 / ML skill（Compute step + sidecar /ml/run 集成） + few-shot 反馈闭环 + GoldenSet eval 框架**。
