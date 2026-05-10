//! 阶段 6：图表/指标推荐 + LLM 摘要。

use sqlai_llm::{mask, ChatMessage, ChatRequest, LlmError, LlmProvider, MaskedContext};
use std::sync::Arc;

use crate::event::{ChartSpec, MetricRecommendation};
use crate::runner::StepRun;
use sqlai_skills::ChartHint;

pub fn chart_spec_for(plan_hint: Option<&ChartHint>) -> ChartSpec {
    match plan_hint {
        Some(h) => h.into(),
        None => ChartSpec {
            kind: "none".into(),
            x: None,
            y: None,
        },
    }
}

pub fn metric_recommendations(_runs: &[StepRun]) -> Vec<MetricRecommendation> {
    // v1.0 占位：留给子计划 #5 真正连 PG metric_def 取数。
    vec![]
}

pub async fn summarize(
    llm: &Arc<dyn LlmProvider>,
    question: &str,
    runs: &[StepRun],
) -> Result<String, LlmError> {
    let preview = runs
        .iter()
        .take(2)
        .map(|r| {
            let cols = r.result.columns.join(", ");
            let rows_preview =
                serde_json::Value::Array(r.result.rows.iter().take(5).cloned().collect());
            format!(
                "[{}]\ncols: {}\nrows(first 5): {}",
                r.label, cols, rows_preview
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let req = ChatRequest {
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content: "你是 BI 助手。基于下面的查询结果，用 1-2 句中文给出业务摘要。不要使用 markdown。".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: format!("问题：{question}\n查询结果：\n{preview}"),
            },
        ],
        max_tokens: Some(200),
        temperature: Some(0.2),
        response_format_json: false,
        tools: vec![],
        tool_choice: None,
    };

    let empty: MaskedContext = mask(sqlai_core::RetrievalContext {
        tables: vec![],
        columns: vec![],
        business_terms: vec![],
        few_shots: vec![],
    });
    let resp = llm.complete(&empty, req).await?;
    Ok(resp.content)
}
