# 智能问数系统 v1.0 — 子计划 #2：DeepSeek Provider + Python Sidecar

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `LlmProvider` 与 `EmbeddingProvider` 两个 trait 真正用真实服务连通：DeepSeek（OpenAI 兼容 chat completions）+ Python FastAPI sidecar（`/embed` 走 BGE-M3，`/ml/run` 走 scikit-learn）。Rust 侧用 `wiremock` 做契约回放测试，Python 侧用 `pytest` 验证端到端行为。

**Architecture:** Rust 客户端实现两条 trait —— `DeepSeekProvider` 通过 reqwest 调 DeepSeek HTTP API；`SidecarClient`（同一文件中暴露 `SidecarEmbedder` 和 `SidecarMlRunner`）通过 reqwest 调本地 sidecar HTTP。Python sidecar 用 FastAPI + uvicorn，BGE-M3 走 `sentence-transformers` 懒加载，K-means / 逻辑回归走 `scikit-learn`。Rust 与 Python 之间的协议固化为两套 schema（`/embed` 和 `/ml/run`），双侧都按 schema 实现 + 测试。

**Tech Stack:** reqwest（已在 workspace） + wiremock 0.6（Rust 契约测试） / FastAPI 0.115 + uvicorn + sentence-transformers + BAAI/bge-m3 + scikit-learn 1.5 + pydantic v2 + pytest 8.

**前置假设：**
- 子计划 #1 已完成（10+ commit，工作区可 build/test 通过；`sqlai-llm` 已含 `LlmProvider` / `EmbeddingProvider` trait + `MaskedContext`）。
- 已安装 Docker Desktop 与 Python 3.11+。
- DeepSeek API key 由用户在执行 Task 7（端到端验证）时提供；Tasks 1–6 不需要真实 key（mock 即可）。

---

## File Structure

本计划完成后新增/修改的目录：

```
sqlai/
├── crates/sqlai-llm/
│   ├── Cargo.toml                     # +reqwest +tokio + wiremock(dev)
│   └── src/
│       ├── lib.rs                     # 增加 pub mod deepseek; pub mod sidecar;
│       ├── deepseek.rs                # NEW
│       └── sidecar.rs                 # NEW
├── sidecar/                           # NEW Python project
│   ├── pyproject.toml
│   ├── README.md
│   ├── app/
│   │   ├── __init__.py
│   │   ├── main.py                    # FastAPI app + /healthz
│   │   ├── embed.py                   # /embed endpoint + lazy BGE-M3 loader
│   │   ├── ml.py                      # /ml/run endpoint (kmeans / logreg)
│   │   └── schema.py                  # pydantic 请求/响应类型
│   ├── tests/
│   │   ├── __init__.py
│   │   ├── conftest.py
│   │   ├── test_healthz.py
│   │   ├── test_embed.py
│   │   └── test_ml.py
│   └── Dockerfile
└── docker-compose.yml                 # 修改：加入 sqlai-sidecar 服务
```

每个文件的"做什么 / 依赖谁 / 暴露什么"：

| 文件 | 做什么 |
|---|---|
| `sqlai-llm/src/deepseek.rs` | `DeepSeekConfig` + `DeepSeekProvider`（impl `LlmProvider`），转换 `(MaskedContext, ChatRequest)` ↔ DeepSeek Chat Completions wire 格式 |
| `sqlai-llm/src/sidecar.rs` | `SidecarConfig` + `SidecarEmbedder`（impl `EmbeddingProvider`），调 `/embed`；预留 `SidecarMlClient` 的简单 HTTP 封装（具体 ML 调用留到子计划 #5） |
| `sidecar/app/main.py` | FastAPI app 装载，挂 `/healthz` `/embed` `/ml/run` |
| `sidecar/app/embed.py` | BGE-M3 懒加载（首次调用前不占显存）；`/embed` 接收 `texts`，返回 `embeddings` 1024 维 |
| `sidecar/app/ml.py` | `/ml/run` 派发：`task: "kmeans" \| "classify_logreg"` 路由到 sklearn 实现 |
| `sidecar/app/schema.py` | pydantic v2 request/response 模型 |
| `sidecar/tests/*` | pytest：health / embed（mock model）/ ml（小数据真跑） |
| `sidecar/Dockerfile` | python:3.11-slim 基础镜像 + 依赖 + uvicorn 启动 |

---

## Wire 协议（双侧依据）

### `/embed`

请求：
```json
POST /embed
{
  "texts": ["第一句", "第二句"]
}
```

响应：
```json
{
  "embeddings": [[0.01, -0.02, ...], [...]],
  "model": "BAAI/bge-m3",
  "dim": 1024
}
```

错误：`400` 空 texts；`500` 模型加载失败；`503` 模型暂不可用。

### `/ml/run`

请求：
```json
POST /ml/run
{
  "task": "kmeans",
  "params": { "n_clusters": 3, "random_state": 42 },
  "data": [[1.0, 2.0], [1.1, 2.1], [9.0, 8.0]]
}
```

响应（K-means）：
```json
{
  "task": "kmeans",
  "result": {
    "labels": [0, 0, 1],
    "centroids": [[1.05, 2.05], [9.0, 8.0]],
    "inertia": 0.005
  }
}
```

请求（逻辑回归）：
```json
{
  "task": "classify_logreg",
  "params": { "test_size": 0.2, "random_state": 42 },
  "data": [[1.0, 2.0, 0], [1.1, 2.1, 0], [9.0, 8.0, 1]]
}
```
（最后一列是 label。）

响应：
```json
{
  "task": "classify_logreg",
  "result": {
    "accuracy": 1.0,
    "n_train": 2,
    "n_test": 1,
    "predictions": [1]
  }
}
```

错误：`400` 未知 task / 数据形状错；`500` sklearn 异常。

### DeepSeek 上游（OpenAI 兼容）

```
POST https://api.deepseek.com/chat/completions
Authorization: Bearer <api_key>
Content-Type: application/json

{
  "model": "deepseek-chat",
  "messages": [...],
  "max_tokens": 1024,
  "temperature": 0.2,
  "response_format": { "type": "json_object" }   // 仅当 ChatRequest.response_format_json=true
}
```

响应：
```json
{
  "choices": [
    { "message": { "role": "assistant", "content": "...response..." } }
  ]
}
```

---

## Task 1：sqlai-llm 接 DeepSeek（含 wiremock 契约测试）

**Files:**
- Modify: `crates/sqlai-llm/Cargo.toml`
- Modify: `crates/sqlai-llm/src/lib.rs`
- Create: `crates/sqlai-llm/src/deepseek.rs`

- [ ] **Step 1：扩展 Cargo.toml**

替换 `[dependencies]` 与 `[dev-dependencies]`：

```toml
[dependencies]
sqlai-core  = { workspace = true }
serde       = { workspace = true }
serde_json  = { workspace = true }
async-trait = { workspace = true }
thiserror   = { workspace = true }
reqwest     = { workspace = true }
tokio       = { workspace = true }
tracing     = { workspace = true }

[dev-dependencies]
uuid     = { workspace = true }
wiremock = "0.6"
tokio    = { workspace = true }
```

- [ ] **Step 2：写 deepseek.rs（含失败测试 + 实现）**

```rust
//! DeepSeek（OpenAI 兼容）的 LlmProvider 实现。

use async_trait::async_trait;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};

use crate::{ChatRequest, ChatResponse, LlmError, LlmProvider, MaskedContext};

#[derive(Debug, Clone)]
pub struct DeepSeekConfig {
    pub base_url: String,    // https://api.deepseek.com
    pub api_key: String,
    pub model: String,       // deepseek-chat
    pub timeout_secs: u64,
}

impl Default for DeepSeekConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com".into(),
            api_key: String::new(),
            model: "deepseek-chat".into(),
            timeout_secs: 60,
        }
    }
}

pub struct DeepSeekProvider {
    cfg: DeepSeekConfig,
    http: HttpClient,
}

impl DeepSeekProvider {
    pub fn new(cfg: DeepSeekConfig) -> Result<Self, LlmError> {
        if cfg.api_key.is_empty() {
            return Err(LlmError::InvalidResponse("api_key is required".into()));
        }
        let http = HttpClient::builder()
            .no_proxy() // 与 sqlai-exec 同样的策略：不走出站代理
            .timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .build()
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        Ok(Self { cfg, http })
    }
}

#[derive(Debug, Serialize)]
struct DeepSeekRequest<'a> {
    model: &'a str,
    messages: &'a Vec<crate::ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str, // "json_object"
}

#[derive(Debug, Deserialize)]
struct DeepSeekResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    content: String,
}

#[async_trait]
impl LlmProvider for DeepSeekProvider {
    async fn complete(
        &self,
        _ctx: &MaskedContext,
        req: ChatRequest,
    ) -> Result<ChatResponse, LlmError> {
        let body = DeepSeekRequest {
            model: &self.cfg.model,
            messages: &req.messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            response_format: if req.response_format_json {
                Some(ResponseFormat { kind: "json_object" })
            } else {
                None
            },
        };

        let url = format!("{}/chat/completions", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LlmError::RateLimited);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::InvalidResponse(format!(
                "deepseek http {}: {}",
                status,
                text.chars().take(300).collect::<String>()
            )));
        }
        let parsed: DeepSeekResponse = serde_json::from_str(&text)
            .map_err(|e| LlmError::InvalidResponse(format!("json: {e}; body: {}", text)))?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::InvalidResponse("no choices in response".into()))?;
        Ok(ChatResponse { content })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mask, ChatMessage};
    use sqlai_core::RetrievalContext;
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn empty_masked() -> MaskedContext {
        mask(RetrievalContext {
            tables: vec![],
            columns: vec![],
            business_terms: vec![],
            few_shots: vec![],
        })
    }

    fn req(json: bool) -> ChatRequest {
        ChatRequest {
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
            max_tokens: Some(64),
            temperature: Some(0.0),
            response_format_json: json,
        }
    }

    #[test]
    fn empty_api_key_rejected() {
        let p = DeepSeekProvider::new(DeepSeekConfig::default());
        assert!(matches!(p, Err(LlmError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn complete_returns_message_content_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header_exists("authorization"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [
                    { "message": { "role": "assistant", "content": "hi from mock" } }
                ]
            })))
            .mount(&server)
            .await;

        let p = DeepSeekProvider::new(DeepSeekConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            model: "deepseek-chat".into(),
            timeout_secs: 5,
        })
        .unwrap();

        let r = p.complete(&empty_masked(), req(false)).await.unwrap();
        assert_eq!(r.content, "hi from mock");
    }

    #[tokio::test]
    async fn rate_limited_status_maps_to_rate_limited_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let p = DeepSeekProvider::new(DeepSeekConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            model: "deepseek-chat".into(),
            timeout_secs: 5,
        })
        .unwrap();

        let err = p.complete(&empty_masked(), req(false)).await.unwrap_err();
        assert!(matches!(err, LlmError::RateLimited));
    }

    #[tokio::test]
    async fn http_error_status_returns_invalid_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let p = DeepSeekProvider::new(DeepSeekConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            model: "deepseek-chat".into(),
            timeout_secs: 5,
        })
        .unwrap();

        let err = p.complete(&empty_masked(), req(false)).await.unwrap_err();
        match err {
            LlmError::InvalidResponse(msg) => assert!(msg.contains("500"), "msg: {msg}"),
            e => panic!("unexpected: {e:?}"),
        }
    }

    #[tokio::test]
    async fn json_response_format_is_set_when_requested() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({
                "response_format": { "type": "json_object" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [
                    { "message": { "role": "assistant", "content": "{}" } }
                ]
            })))
            .mount(&server)
            .await;

        let p = DeepSeekProvider::new(DeepSeekConfig {
            base_url: server.uri(),
            api_key: "test-key".into(),
            model: "deepseek-chat".into(),
            timeout_secs: 5,
        })
        .unwrap();

        let r = p.complete(&empty_masked(), req(true)).await.unwrap();
        assert_eq!(r.content, "{}");
    }
}
```

- [ ] **Step 3：在 lib.rs 中暴露 deepseek 模块**

`crates/sqlai-llm/src/lib.rs` 在已有内容上加：

```rust
pub mod deepseek;
```

放在 `pub mod desensitize;` 之后即可。

- [ ] **Step 4：跑测试 + 提交**

```
cargo test -p sqlai-llm 2>&1 | tail -15
```

预期：8 passed（3 原有 + 5 新 deepseek 测试）。前台跑，timeout 300s。

如有 wiremock 0.6 API 变动（如 matcher 名称），按编译错误调整即可，不要为绕错而改弱测试。

```
git add crates/sqlai-llm Cargo.lock
git commit -m "feat(llm): add DeepSeekProvider with wiremock contract tests"
```

---

## Task 2：sqlai-llm 接 Sidecar（EmbeddingProvider 实现 + 契约测试）

**Files:**
- Create: `crates/sqlai-llm/src/sidecar.rs`
- Modify: `crates/sqlai-llm/src/lib.rs`

- [ ] **Step 1：写 sidecar.rs**

```rust
//! Python sidecar 客户端：embed + ml/run。
//!
//! 与 DeepSeekProvider 的不同：sidecar 是同 VPC / 同主机的内部服务，
//! 因此使用 no_proxy 并允许较短超时。

use async_trait::async_trait;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};

use crate::{EmbeddingProvider, LlmError};

#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub base_url: String,        // http://localhost:8081
    pub timeout_secs: u64,       // 默认 30 (embed 本地 ~1-3s；ml 预留)
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8081".into(),
            timeout_secs: 30,
        }
    }
}

pub struct SidecarEmbedder {
    cfg: SidecarConfig,
    http: HttpClient,
}

impl SidecarEmbedder {
    pub fn new(cfg: SidecarConfig) -> Result<Self, LlmError> {
        let http = HttpClient::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .build()
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        Ok(Self { cfg, http })
    }
}

#[derive(Debug, Serialize)]
struct EmbedRequest<'a> {
    texts: &'a [String],
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    dim: u32,
}

#[async_trait]
impl EmbeddingProvider for SidecarEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let url = format!("{}/embed", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(&EmbedRequest { texts })
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::InvalidResponse(format!(
                "sidecar /embed http {}: {}",
                status,
                text.chars().take(200).collect::<String>()
            )));
        }
        let parsed: EmbedResponse = serde_json::from_str(&text)
            .map_err(|e| LlmError::InvalidResponse(format!("embed json: {e}; body: {text}")))?;
        Ok(parsed.embeddings)
    }
}

/// Sidecar /ml/run 的薄封装。具体 ML skill 调用在子计划 #5 用到。
pub struct SidecarMlClient {
    cfg: SidecarConfig,
    http: HttpClient,
}

impl SidecarMlClient {
    pub fn new(cfg: SidecarConfig) -> Result<Self, LlmError> {
        let http = HttpClient::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(cfg.timeout_secs))
            .build()
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        Ok(Self { cfg, http })
    }

    pub async fn run(&self, body: &serde_json::Value) -> Result<serde_json::Value, LlmError> {
        let url = format!("{}/ml/run", self.cfg.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::InvalidResponse(format!(
                "sidecar /ml/run http {}: {}",
                status,
                text.chars().take(200).collect::<String>()
            )));
        }
        serde_json::from_str(&text)
            .map_err(|e| LlmError::InvalidResponse(format!("ml/run json: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn embed_roundtrip() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2], [0.3, 0.4]],
                "model": "BAAI/bge-m3",
                "dim": 1024
            })))
            .mount(&server)
            .await;

        let e = SidecarEmbedder::new(SidecarConfig {
            base_url: server.uri(),
            timeout_secs: 5,
        })
        .unwrap();

        let r = e.embed(&["a".into(), "b".into()]).await.unwrap();
        assert_eq!(r, vec![vec![0.1, 0.2], vec![0.3, 0.4]]);
    }

    #[tokio::test]
    async fn embed_empty_input_returns_empty_without_call() {
        // 没有 mock 也能通过：空输入应短路，不发出请求。
        let e = SidecarEmbedder::new(SidecarConfig {
            base_url: "http://invalid.invalid:9".into(),
            timeout_secs: 1,
        })
        .unwrap();
        let r = e.embed(&[]).await.unwrap();
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn embed_500_maps_to_invalid_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/embed"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let e = SidecarEmbedder::new(SidecarConfig {
            base_url: server.uri(),
            timeout_secs: 5,
        })
        .unwrap();
        let err = e.embed(&["x".into()]).await.unwrap_err();
        match err {
            LlmError::InvalidResponse(msg) => assert!(msg.contains("500"), "msg: {msg}"),
            e => panic!("unexpected: {e:?}"),
        }
    }

    #[tokio::test]
    async fn ml_run_roundtrip() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ml/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "task": "kmeans",
                "result": { "labels": [0, 0, 1] }
            })))
            .mount(&server)
            .await;
        let c = SidecarMlClient::new(SidecarConfig {
            base_url: server.uri(),
            timeout_secs: 5,
        })
        .unwrap();
        let body = serde_json::json!({
            "task": "kmeans",
            "params": { "n_clusters": 2 },
            "data": [[0.0], [0.1], [9.0]]
        });
        let r = c.run(&body).await.unwrap();
        assert_eq!(r["task"], "kmeans");
        assert!(r["result"]["labels"].is_array());
    }
}
```

- [ ] **Step 2：暴露 sidecar 模块**

`crates/sqlai-llm/src/lib.rs` 加：
```rust
pub mod sidecar;
```

- [ ] **Step 3：跑测试 + 提交**

```
cargo test -p sqlai-llm 2>&1 | tail -15
```

预期：12 passed（3 desensitize + 5 deepseek + 4 sidecar）。

```
git add crates/sqlai-llm
git commit -m "feat(llm): add SidecarEmbedder + SidecarMlClient with wiremock contract tests"
```

---

## Task 3：Python sidecar 项目骨架 + /healthz

**Files:**
- Create: `sidecar/pyproject.toml`
- Create: `sidecar/README.md`
- Create: `sidecar/app/__init__.py`
- Create: `sidecar/app/main.py`
- Create: `sidecar/app/schema.py`
- Create: `sidecar/tests/__init__.py`
- Create: `sidecar/tests/conftest.py`
- Create: `sidecar/tests/test_healthz.py`

- [ ] **Step 1：pyproject.toml**

```toml
[project]
name = "sqlai-sidecar"
version = "0.1.0"
description = "sqlai Python sidecar: BGE-M3 embedding + sklearn ML"
requires-python = ">=3.11"
dependencies = [
    "fastapi>=0.115,<0.120",
    "uvicorn[standard]>=0.30,<0.40",
    "pydantic>=2.7,<3",
    "numpy>=1.26,<3",
    "scikit-learn>=1.5,<2",
    "sentence-transformers>=3.0,<4",
]

[project.optional-dependencies]
dev = [
    "pytest>=8,<9",
    "httpx>=0.27,<1",        # FastAPI TestClient 依赖
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["app"]

[tool.pytest.ini_options]
testpaths = ["tests"]
addopts = "-q"
```

- [ ] **Step 2：README**

```markdown
# sqlai-sidecar

Python FastAPI sidecar serving BGE-M3 embeddings and sklearn ML tasks for the sqlai backend.

## Endpoints

- `GET /healthz` → `{"ok": true}`
- `POST /embed` → BGE-M3 vectors (1024-dim)
- `POST /ml/run` → K-means / logistic regression tasks

## Local dev

```
cd sidecar
python -m venv .venv
. .venv/Scripts/activate         # Windows
# . .venv/bin/activate           # *nix
pip install -e ".[dev]"
pytest
uvicorn app.main:app --host 0.0.0.0 --port 8081
```

## Embedding model

First request to `/embed` lazy-loads `BAAI/bge-m3` (~2.3 GB). Subsequent requests reuse the loaded instance. To preload at startup, set `SIDECAR_PRELOAD_EMBED=1`.
```

- [ ] **Step 3：app/__init__.py**

```python
"""sqlai sidecar package."""
```

- [ ] **Step 4：schema.py**

```python
"""Pydantic v2 request/response schemas shared by all endpoints."""
from __future__ import annotations

from typing import Any, Literal
from pydantic import BaseModel, Field


class EmbedRequest(BaseModel):
    texts: list[str] = Field(min_length=1)


class EmbedResponse(BaseModel):
    embeddings: list[list[float]]
    model: str
    dim: int


class MlRequest(BaseModel):
    task: Literal["kmeans", "classify_logreg"]
    params: dict[str, Any] = Field(default_factory=dict)
    data: list[list[float]] = Field(min_length=1)


class MlResponse(BaseModel):
    task: str
    result: dict[str, Any]
```

- [ ] **Step 5：main.py（先只挂 /healthz；embed 与 ml 在后续 Task 加）**

```python
"""FastAPI entrypoint."""
from __future__ import annotations

import os

from fastapi import FastAPI


def create_app() -> FastAPI:
    app = FastAPI(title="sqlai-sidecar", version="0.1.0")

    @app.get("/healthz")
    def healthz() -> dict[str, bool]:
        return {"ok": True}

    return app


app = create_app()


if os.environ.get("SIDECAR_PRELOAD_EMBED") == "1":
    # 这里只是占位；实际 preload 在 Task 4 接入后填充。
    pass
```

- [ ] **Step 6：测试**

`tests/__init__.py`：空文件。

`tests/conftest.py`：
```python
"""Shared pytest fixtures."""
from __future__ import annotations

import pytest
from fastapi.testclient import TestClient

from app.main import create_app


@pytest.fixture()
def client() -> TestClient:
    return TestClient(create_app())
```

`tests/test_healthz.py`：
```python
def test_healthz_returns_ok(client):
    r = client.get("/healthz")
    assert r.status_code == 200
    assert r.json() == {"ok": True}
```

- [ ] **Step 7：本地装 + 跑测试**

```bash
cd sidecar
python -m venv .venv
.venv/Scripts/pip install -e ".[dev]"
.venv/Scripts/pytest -q
```

预期：1 passed。

> 如果 BGE-M3 模型相关下载在 `pip install` 时拉慢，那是 `sentence-transformers` 触发的——它在导入时不会自动下载模型；只在首次调用 `SentenceTransformer("BAAI/bge-m3")` 时下载。所以 `pip install` 阶段不会触发模型下载。

- [ ] **Step 8：commit**

```
cd D:\workspase\rust\sqlai
git add sidecar
git commit -m "feat(sidecar): add Python FastAPI skeleton with /healthz endpoint"
```

---

## Task 4：Sidecar `/embed` endpoint（BGE-M3 懒加载）

**Files:**
- Create: `sidecar/app/embed.py`
- Modify: `sidecar/app/main.py`
- Create: `sidecar/tests/test_embed.py`

- [ ] **Step 1：embed.py**

```python
"""POST /embed: BGE-M3 lazy-loaded embeddings."""
from __future__ import annotations

import threading
from typing import TYPE_CHECKING

from fastapi import APIRouter, HTTPException

from app.schema import EmbedRequest, EmbedResponse

if TYPE_CHECKING:
    from sentence_transformers import SentenceTransformer


router = APIRouter()

_MODEL_NAME = "BAAI/bge-m3"
_EMBED_DIM = 1024
_model: "SentenceTransformer | None" = None
_lock = threading.Lock()


def _get_model() -> "SentenceTransformer":
    global _model
    if _model is not None:
        return _model
    with _lock:
        if _model is None:
            from sentence_transformers import SentenceTransformer

            _model = SentenceTransformer(_MODEL_NAME)
    return _model


def reset_model_for_tests() -> None:
    """Test-only hook to clear the lazy-loaded model."""
    global _model
    with _lock:
        _model = None


@router.post("/embed", response_model=EmbedResponse)
def embed(req: EmbedRequest) -> EmbedResponse:
    if not req.texts:
        # min_length=1 already enforces this, but double-guard for safety.
        raise HTTPException(status_code=400, detail="texts must be non-empty")
    try:
        model = _get_model()
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=503, detail=f"model unavailable: {exc!s}") from exc

    vectors = model.encode(req.texts, normalize_embeddings=True).tolist()
    return EmbedResponse(embeddings=vectors, model=_MODEL_NAME, dim=_EMBED_DIM)
```

- [ ] **Step 2：main.py 挂载 router**

替换原 main.py 的 `create_app()`：

```python
def create_app() -> FastAPI:
    app = FastAPI(title="sqlai-sidecar", version="0.1.0")

    from app.embed import router as embed_router
    app.include_router(embed_router)

    @app.get("/healthz")
    def healthz() -> dict[str, bool]:
        return {"ok": True}

    return app
```

- [ ] **Step 3：test_embed.py（用 monkeypatch 把模型替换为确定性 stub）**

```python
"""/embed: 不真的下载 BGE-M3；用 stub 模型走完管线。"""
from __future__ import annotations

import numpy as np
import pytest

from app import embed as embed_module


class _StubModel:
    """A deterministic replacement for SentenceTransformer used in tests."""

    def encode(self, texts, normalize_embeddings: bool = False):  # noqa: ARG002
        # 用文本长度种子产生确定向量；维度 1024 与生产一致。
        rng = np.random.default_rng(seed=42)
        return np.stack([rng.standard_normal(1024) for _ in texts])


@pytest.fixture(autouse=True)
def _stub_bge(monkeypatch):
    embed_module.reset_model_for_tests()
    monkeypatch.setattr(embed_module, "_get_model", lambda: _StubModel())
    yield
    embed_module.reset_model_for_tests()


def test_embed_two_texts_returns_two_vectors(client):
    r = client.post("/embed", json={"texts": ["第一句", "第二句"]})
    assert r.status_code == 200
    body = r.json()
    assert body["model"] == "BAAI/bge-m3"
    assert body["dim"] == 1024
    assert len(body["embeddings"]) == 2
    assert all(len(v) == 1024 for v in body["embeddings"])


def test_embed_empty_texts_rejected(client):
    r = client.post("/embed", json={"texts": []})
    # pydantic min_length=1 → 422
    assert r.status_code == 422
```

- [ ] **Step 4：跑测试**

```
cd sidecar
.venv/Scripts/pytest -q
```

预期：3 passed（healthz 1 + embed 2）。

- [ ] **Step 5：commit**

```
cd D:\workspase\rust\sqlai
git add sidecar
git commit -m "feat(sidecar): add /embed endpoint with BGE-M3 lazy load + stubbed tests"
```

---

## Task 5：Sidecar `/ml/run` endpoint（kmeans + logreg）

**Files:**
- Create: `sidecar/app/ml.py`
- Modify: `sidecar/app/main.py`
- Create: `sidecar/tests/test_ml.py`

- [ ] **Step 1：ml.py**

```python
"""POST /ml/run: K-means and logistic regression via scikit-learn."""
from __future__ import annotations

from typing import Any

import numpy as np
from fastapi import APIRouter, HTTPException
from sklearn.cluster import KMeans
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split

from app.schema import MlRequest, MlResponse


router = APIRouter()


@router.post("/ml/run", response_model=MlResponse)
def run(req: MlRequest) -> MlResponse:
    if req.task == "kmeans":
        return _kmeans(req)
    if req.task == "classify_logreg":
        return _logreg(req)
    # pydantic Literal already restricts; this is defensive
    raise HTTPException(status_code=400, detail=f"unknown task: {req.task}")


def _kmeans(req: MlRequest) -> MlResponse:
    n_clusters = int(req.params.get("n_clusters", 3))
    random_state = int(req.params.get("random_state", 42))
    if n_clusters < 1:
        raise HTTPException(status_code=400, detail="n_clusters must be >= 1")
    data = np.asarray(req.data, dtype=float)
    if data.ndim != 2:
        raise HTTPException(status_code=400, detail="data must be 2-D matrix")
    if data.shape[0] < n_clusters:
        raise HTTPException(
            status_code=400, detail=f"need >= {n_clusters} rows, got {data.shape[0]}"
        )
    try:
        km = KMeans(n_clusters=n_clusters, n_init="auto", random_state=random_state)
        labels = km.fit_predict(data)
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=500, detail=f"kmeans failed: {exc!s}") from exc
    return MlResponse(
        task="kmeans",
        result={
            "labels": labels.tolist(),
            "centroids": km.cluster_centers_.tolist(),
            "inertia": float(km.inertia_),
        },
    )


def _logreg(req: MlRequest) -> MlResponse:
    test_size = float(req.params.get("test_size", 0.2))
    random_state = int(req.params.get("random_state", 42))
    arr = np.asarray(req.data, dtype=float)
    if arr.ndim != 2 or arr.shape[1] < 2:
        raise HTTPException(
            status_code=400, detail="data must be 2-D with at least one feature column + label column"
        )
    x = arr[:, :-1]
    y = arr[:, -1].astype(int)
    if len(set(y.tolist())) < 2:
        raise HTTPException(status_code=400, detail="need at least 2 distinct labels")
    if arr.shape[0] < 4:
        raise HTTPException(status_code=400, detail="need >= 4 rows for split")

    try:
        x_train, x_test, y_train, y_test = train_test_split(
            x, y, test_size=test_size, random_state=random_state, stratify=y
        )
        clf = LogisticRegression(max_iter=200)
        clf.fit(x_train, y_train)
        preds: Any = clf.predict(x_test)
        acc = float((preds == y_test).mean())
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=500, detail=f"logreg failed: {exc!s}") from exc

    return MlResponse(
        task="classify_logreg",
        result={
            "accuracy": acc,
            "n_train": int(len(y_train)),
            "n_test": int(len(y_test)),
            "predictions": preds.tolist(),
        },
    )
```

- [ ] **Step 2：main.py 挂 router**

```python
def create_app() -> FastAPI:
    app = FastAPI(title="sqlai-sidecar", version="0.1.0")

    from app.embed import router as embed_router
    from app.ml import router as ml_router
    app.include_router(embed_router)
    app.include_router(ml_router)

    @app.get("/healthz")
    def healthz() -> dict[str, bool]:
        return {"ok": True}

    return app
```

- [ ] **Step 3：test_ml.py**

```python
def test_kmeans_clusters_well_separated_data(client):
    data = [[0.0, 0.0], [0.1, 0.0], [0.0, 0.1], [10.0, 10.0], [10.1, 10.0], [10.0, 10.1]]
    r = client.post("/ml/run", json={
        "task": "kmeans",
        "params": {"n_clusters": 2, "random_state": 0},
        "data": data,
    })
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["task"] == "kmeans"
    labels = body["result"]["labels"]
    # 前 3 行应同簇，后 3 行同簇（两簇本身可能 0/1 互换）
    assert labels[0] == labels[1] == labels[2]
    assert labels[3] == labels[4] == labels[5]
    assert labels[0] != labels[3]


def test_kmeans_too_few_rows_rejected(client):
    r = client.post("/ml/run", json={
        "task": "kmeans",
        "params": {"n_clusters": 3},
        "data": [[1.0], [2.0]],  # 只有 2 行 < n_clusters
    })
    assert r.status_code == 400


def test_classify_logreg_smoke(client):
    # 8 行，2 类 (label is last column)，足够 train_test_split + stratify
    data = [
        [0.0, 0.0, 0],
        [0.1, 0.0, 0],
        [0.0, 0.1, 0],
        [0.05, 0.05, 0],
        [10.0, 10.0, 1],
        [10.1, 10.0, 1],
        [10.0, 10.1, 1],
        [10.05, 10.05, 1],
    ]
    r = client.post("/ml/run", json={
        "task": "classify_logreg",
        "params": {"test_size": 0.5, "random_state": 0},
        "data": data,
    })
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["task"] == "classify_logreg"
    assert 0.0 <= body["result"]["accuracy"] <= 1.0


def test_unknown_task_rejected(client):
    r = client.post("/ml/run", json={
        "task": "what",
        "params": {},
        "data": [[1.0]],
    })
    # pydantic Literal validation → 422
    assert r.status_code == 422
```

- [ ] **Step 4：跑测试**

```
cd sidecar
.venv/Scripts/pytest -q
```

预期：7 passed（healthz 1 + embed 2 + ml 4）。

- [ ] **Step 5：commit**

```
cd D:\workspase\rust\sqlai
git add sidecar
git commit -m "feat(sidecar): add /ml/run endpoint with kmeans + logreg via scikit-learn"
```

---

## Task 6：Sidecar Dockerfile + docker-compose 集成

**Files:**
- Create: `sidecar/Dockerfile`
- Create: `sidecar/.dockerignore`
- Modify: `docker-compose.yml`

- [ ] **Step 1：Dockerfile**

```dockerfile
# CPU-only torch；不带 CUDA，镜像更小。
FROM python:3.11-slim AS base

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PIP_NO_CACHE_DIR=1

WORKDIR /opt/sidecar

# 系统依赖：libgomp 是 sklearn / torch CPU 的运行时依赖。
RUN apt-get update \
 && apt-get install -y --no-install-recommends libgomp1 \
 && rm -rf /var/lib/apt/lists/*

# 先装依赖（利用 layer 缓存）
COPY pyproject.toml ./
RUN pip install --upgrade pip \
 && pip install \
      "fastapi>=0.115,<0.120" \
      "uvicorn[standard]>=0.30,<0.40" \
      "pydantic>=2.7,<3" \
      "numpy>=1.26,<3" \
      "scikit-learn>=1.5,<2" \
      "torch>=2.3,<3" --index-url https://download.pytorch.org/whl/cpu \
 && pip install \
      "sentence-transformers>=3.0,<4"

COPY app ./app

EXPOSE 8081

CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8081"]
```

- [ ] **Step 2：.dockerignore**

```
.venv/
__pycache__/
*.pyc
tests/
*.egg-info/
.pytest_cache/
```

- [ ] **Step 3：修改 docker-compose.yml，加 sqlai-sidecar 服务**

在现有 `services:` 下追加：

```yaml
  sidecar:
    build: ./sidecar
    container_name: sqlai-sidecar
    ports:
      - "8081:8081"
    environment:
      # 设为 1 时容器启动即下载 BGE-M3（首次启动慢，但可避免首请求 5-10 分钟懒加载）。
      SIDECAR_PRELOAD_EMBED: "0"
    healthcheck:
      test: ["CMD", "python", "-c", "import urllib.request; urllib.request.urlopen('http://localhost:8081/healthz')"]
      interval: 10s
      timeout: 5s
      retries: 10
```

完整文件最终样貌：

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    container_name: sqlai-pg
    environment:
      POSTGRES_USER: sqlai
      POSTGRES_PASSWORD: sqlai
      POSTGRES_DB: sqlai
    ports:
      - "5432:5432"
    volumes:
      - sqlai_pg_data:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d:ro
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U sqlai"]
      interval: 5s
      timeout: 3s
      retries: 10

  clickhouse:
    image: clickhouse/clickhouse-server:24.8
    container_name: sqlai-ch
    ports:
      - "8123:8123"
      - "9000:9000"
    ulimits:
      nofile:
        soft: 262144
        hard: 262144
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:8123/ping"]
      interval: 5s
      timeout: 3s
      retries: 10

  sidecar:
    build: ./sidecar
    container_name: sqlai-sidecar
    ports:
      - "8081:8081"
    environment:
      SIDECAR_PRELOAD_EMBED: "0"
    healthcheck:
      test: ["CMD", "python", "-c", "import urllib.request; urllib.request.urlopen('http://localhost:8081/healthz')"]
      interval: 10s
      timeout: 5s
      retries: 10

volumes:
  sqlai_pg_data:
```

- [ ] **Step 4：build + healthz 验证**

```
docker compose build sidecar
docker compose up -d sidecar
```

等 30s 后：

```
curl.exe http://127.0.0.1:8081/healthz
```

预期：`{"ok":true}`。

注意：torch CPU wheel 较大（~250MB），首次 build 可能 3-5 分钟。如果用户在境内拉镜像慢，可在 `Dockerfile` 中 `RUN pip install` 前加 `RUN pip config set global.index-url https://pypi.tuna.tsinghua.edu.cn/simple`，但默认配置先用官方 index。

- [ ] **Step 5：commit**

```
git add sidecar/Dockerfile sidecar/.dockerignore docker-compose.yml
git commit -m "chore(devenv): containerize sidecar and wire into docker-compose"
```

---

## Task 7：Rust ↔ 真实 sidecar 端到端集成测试

**Files:**
- Create: `crates/sqlai-llm/tests/sidecar_integration.rs`

- [ ] **Step 1：写 ignored 集成测试**

```rust
//! 集成测试：需要本地 sqlai-sidecar 在 :8081 端口运行。
//! 跑法：`docker compose up -d sidecar && cargo test -p sqlai-llm -- --ignored`

use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder, SidecarMlClient};
use sqlai_llm::EmbeddingProvider;

fn cfg() -> SidecarConfig {
    SidecarConfig {
        base_url: std::env::var("SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
        timeout_secs: 60,
    }
}

#[ignore]
#[tokio::test]
async fn embed_real_sidecar_returns_1024_dim_vectors() {
    let e = SidecarEmbedder::new(cfg()).unwrap();
    let r = e.embed(&["你好".into(), "world".into()]).await.unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].len(), 1024);
    assert_eq!(r[1].len(), 1024);
    // sanity：两个不同输入的向量不应完全相同
    assert_ne!(r[0], r[1]);
}

#[ignore]
#[tokio::test]
async fn ml_kmeans_real_sidecar() {
    let c = SidecarMlClient::new(cfg()).unwrap();
    let body = serde_json::json!({
        "task": "kmeans",
        "params": {"n_clusters": 2, "random_state": 0},
        "data": [
            [0.0, 0.0], [0.1, 0.0], [0.0, 0.1],
            [10.0, 10.0], [10.1, 10.0], [10.0, 10.1]
        ]
    });
    let r = c.run(&body).await.unwrap();
    assert_eq!(r["task"], "kmeans");
    let labels = r["result"]["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 6);
}
```

- [ ] **Step 2：跑（前提：sidecar 容器已跑起来）**

```
docker compose up -d sidecar
# 第一次调 /embed 会触发 BGE-M3 下载（约 2-3 GB），耐心等待 5-10 分钟。
cargo test -p sqlai-llm -- --ignored
```

预期：2 ignored tests pass。如果 BGE-M3 下载在国内慢，建议提前用 `huggingface-cli download BAAI/bge-m3` 把模型缓存到 ~/.cache/huggingface 后再跑。

- [ ] **Step 3：commit**

```
git add crates/sqlai-llm/tests
git commit -m "test(llm): add ignored integration tests for real sidecar (embed + ml/run)"
```

---

## 验收清单（子计划 #2 完成时全部应可通过）

- [ ] `cargo build --workspace` ✅
- [ ] `cargo test --workspace` ✅ 12 passed in `sqlai-llm`（3 desensitize + 5 deepseek + 4 sidecar），加上 #1 已有的 24 个，合计 36 passed
- [ ] `cargo clippy --workspace -- -D warnings` ✅
- [ ] `cargo fmt --all -- --check` ✅
- [ ] Python sidecar：`pytest` 7 passed（healthz 1 + embed 2 + ml 4）
- [ ] `docker compose up -d sidecar` 后，`curl http://localhost:8081/healthz` 返回 `{"ok":true}`
- [ ] `cargo test -p sqlai-llm -- --ignored` 在 sidecar 启动后 2 ignored tests pass
- [ ] `git log` 至少 7 条本子计划新增 commit

---

## 进入下一份子计划

完成本计划后，下一份是 **#3：sqlai-store（PG + pgvector 持久化） + sqlai-cli schema 同步命令**。
