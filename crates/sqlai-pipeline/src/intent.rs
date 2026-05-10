//! 阶段 1：意图分类。

use sqlai_core::IntentDecision;
use sqlai_llm::{mask, ChatMessage, ChatRequest, LlmError, LlmProvider};
use std::sync::Arc;

const SYSTEM_PROMPT: &str =
    "你是一个 BI 数据分析助手。你只能基于结构化数据回答问题，不能编造数据。\n\
对每个用户问题，输出严格 JSON：\n\
- 如果问题清晰且属于 BI 数据查询，输出 {\"kind\":\"direct\",\"hint\":\"<对意图的简短复述>\"}\n\
- 如果问题歧义或缺关键信息，输出 {\"kind\":\"clarify\",\"prompt\":\"<反向澄清问题>\"}\n\
- 如果问题与数据查询无关，输出 {\"kind\":\"reject\",\"reason\":\"<原因>\"}";

pub async fn classify(
    llm: &Arc<dyn LlmProvider>,
    question: &str,
    history: &[ChatMessage],
) -> Result<IntentDecision, LlmError> {
    let mut messages: Vec<ChatMessage> = vec![ChatMessage {
        role: "system".into(),
        content: SYSTEM_PROMPT.into(),
    }];
    messages.extend_from_slice(history);
    messages.push(ChatMessage {
        role: "user".into(),
        content: question.to_string(),
    });

    let req = ChatRequest {
        messages,
        max_tokens: Some(256),
        temperature: Some(0.0),
        response_format_json: true,
        tools: vec![],
        tool_choice: None,
    };
    let ctx = mask(sqlai_core::RetrievalContext {
        tables: vec![],
        columns: vec![],
        business_terms: vec![],
        few_shots: vec![],
    });
    let resp = llm.complete(&ctx, req).await?;
    serde_json::from_str(&resp.content)
        .map_err(|e| LlmError::InvalidResponse(format!("intent json: {e}; body: {}", resp.content)))
}
