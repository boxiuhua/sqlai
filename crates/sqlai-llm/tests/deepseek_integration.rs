//! 集成测试：连真实 DeepSeek API。
//! 跑法：
//!   $env:DEEPSEEK_API_KEY="sk-..."; cargo test -p sqlai-llm --test deepseek_integration -- --ignored

use sqlai_core::RetrievalContext;
use sqlai_llm::deepseek::{DeepSeekConfig, DeepSeekProvider};
use sqlai_llm::{mask, ChatMessage, ChatRequest, LlmProvider};

fn cfg() -> DeepSeekConfig {
    DeepSeekConfig {
        base_url: std::env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".into()),
        api_key: std::env::var("DEEPSEEK_API_KEY")
            .expect("set DEEPSEEK_API_KEY before running this ignored test"),
        model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
        timeout_secs: 60,
    }
}

fn empty_masked() -> sqlai_llm::MaskedContext {
    mask(RetrievalContext {
        tables: vec![],
        columns: vec![],
        business_terms: vec![],
        few_shots: vec![],
    })
}

#[ignore]
#[tokio::test]
async fn deepseek_responds_with_short_message() {
    let p = DeepSeekProvider::new(cfg()).unwrap();
    let req = ChatRequest {
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "Reply with exactly the word: pong".into(),
        }],
        max_tokens: Some(8),
        temperature: Some(0.0),
        response_format_json: false,
        tools: vec![],
        tool_choice: None,
    };
    let r = p.complete(&empty_masked(), req).await.unwrap();
    assert!(!r.content.is_empty(), "response should be non-empty");
    // 不强求模型一字不差返回 "pong"，避免因模型小幅波动导致测试 flaky；
    // 只验证有内容且包含字母 p（pong / Pong / pong! 等都接受）。
    assert!(
        r.content.to_lowercase().contains("pong"),
        "got: {}",
        r.content
    );
}

#[ignore]
#[tokio::test]
async fn deepseek_json_mode_returns_valid_json_object() {
    let p = DeepSeekProvider::new(cfg()).unwrap();
    let req = ChatRequest {
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "You output strict JSON.".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: "Output a JSON object with field 'ok' set to true. Nothing else.".into(),
            },
        ],
        max_tokens: Some(64),
        temperature: Some(0.0),
        response_format_json: true,
        tools: vec![],
        tool_choice: None,
    };
    let r = p.complete(&empty_masked(), req).await.unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&r.content).expect(&format!("not valid JSON: {}", r.content));
    assert_eq!(v["ok"], serde_json::Value::Bool(true));
}
