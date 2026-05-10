//! DeepSeek（OpenAI 兼容）的 LlmProvider 实现。

use async_trait::async_trait;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};

use crate::{ChatRequest, ChatResponse, LlmError, LlmProvider, MaskedContext};

#[derive(Debug, Clone)]
pub struct DeepSeekConfig {
    pub base_url: String, // https://api.deepseek.com
    pub api_key: String,
    pub model: String, // deepseek-chat
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: &'a Vec<crate::Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: &'a Option<serde_json::Value>,
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
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<crate::ToolCall>,
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
                Some(ResponseFormat {
                    kind: "json_object",
                })
            } else {
                None
            },
            tools: &req.tools,
            tool_choice: &req.tool_choice,
        };

        let url = format!(
            "{}/chat/completions",
            self.cfg.base_url.trim_end_matches('/')
        );
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
        let msg = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message)
            .ok_or_else(|| LlmError::InvalidResponse("no choices in response".into()))?;
        Ok(ChatResponse {
            content: msg.content.unwrap_or_default(),
            tool_calls: msg.tool_calls,
        })
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
            tools: vec![],
            tool_choice: None,
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
