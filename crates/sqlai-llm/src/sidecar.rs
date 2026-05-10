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
    pub base_url: String,  // http://localhost:8081
    pub timeout_secs: u64, // 默认 30 (embed 本地 ~1-3s；ml 预留)
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
