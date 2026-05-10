//! 阶段 3：function-calling 选 skill 并执行 skill.plan()。

use sqlai_core::RetrievalContext;
use sqlai_llm::{
    mask, ChatMessage, ChatRequest, ChatResponse, LlmError, LlmProvider, Tool, ToolFunction,
};
use sqlai_skills::{AnalysisPlan, SkillError, SkillRegistry};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum SelectError {
    #[error("llm: {0}")]
    Llm(#[from] LlmError),

    #[error("no tool_call returned")]
    NoToolCall,

    #[error("unknown skill: {0}")]
    UnknownSkill(String),

    #[error("invalid arguments: {0}")]
    BadArgs(String),

    #[error("skill: {0}")]
    Skill(#[from] SkillError),
}

const SYSTEM_PROMPT: &str =
    "你是一个 BI SQL 助手。结合下方提供的表结构与业务知识，挑选最合适的 tool 并填入参数。\n\
     - 表名/列名只能从提供的 schema 中选择。\n\
     - 不要生成原始 SQL；通过 tool_call 输出。";

pub async fn select_and_plan(
    llm: &Arc<dyn LlmProvider>,
    skills: &SkillRegistry,
    question: &str,
    ctx: &RetrievalContext,
    feedback: Option<&str>,
) -> Result<(String, serde_json::Value, AnalysisPlan), SelectError> {
    let tools: Vec<Tool> = skills
        .all_schemas()
        .into_iter()
        .map(|s| Tool {
            kind: "function".into(),
            function: ToolFunction {
                name: s.name,
                description: s.description,
                parameters: s.parameters,
            },
        })
        .collect();

    let schema_md = serialize_ctx_for_prompt(ctx);
    let mut messages: Vec<ChatMessage> = vec![
        ChatMessage {
            role: "system".into(),
            content: SYSTEM_PROMPT.into(),
        },
        ChatMessage {
            role: "system".into(),
            content: format!("可用 schema 与知识：\n{schema_md}"),
        },
    ];
    if let Some(err) = feedback {
        messages.push(ChatMessage {
            role: "system".into(),
            content: format!(
                "上一次尝试失败，错误信息：\n{}\n请基于该错误调整你的 tool_call 参数（例如改用提供的 schema 中真实存在的列名）。",
                err
            ),
        });
    }
    messages.push(ChatMessage {
        role: "user".into(),
        content: question.to_string(),
    });

    let req = ChatRequest {
        messages,
        max_tokens: Some(1024),
        temperature: Some(0.0),
        response_format_json: false,
        tools,
        tool_choice: Some(serde_json::json!("auto")),
    };

    let masked = mask(ctx.clone());
    let resp: ChatResponse = llm.complete(&masked, req).await?;
    let call = resp
        .tool_calls
        .into_iter()
        .next()
        .ok_or(SelectError::NoToolCall)?;
    let args: serde_json::Value = serde_json::from_str(&call.function.arguments)
        .map_err(|e| SelectError::BadArgs(e.to_string()))?;
    let skill = skills
        .get(&call.function.name)
        .ok_or_else(|| SelectError::UnknownSkill(call.function.name.clone()))?;
    let plan = skill.plan(&args, ctx)?;
    Ok((call.function.name, args, plan))
}

fn serialize_ctx_for_prompt(ctx: &RetrievalContext) -> String {
    let mut out = String::new();
    out.push_str("# 表\n");
    for t in &ctx.tables {
        out.push_str(&format!(
            "- {}.{}{}\n",
            t.db,
            t.table,
            t.comment
                .as_deref()
                .map(|c| format!("（{c}）"))
                .unwrap_or_default()
        ));
    }
    out.push_str("# 列\n");
    for c in &ctx.columns {
        out.push_str(&format!(
            "- table_id={} col={} type={} sample={}\n",
            c.table_id,
            c.name,
            c.data_type,
            serde_json::Value::Array(c.sample_values.clone())
        ));
    }
    if !ctx.business_terms.is_empty() {
        out.push_str("# 业务知识\n");
        for t in &ctx.business_terms {
            out.push_str(&format!(
                "- {}: {}{}\n",
                t.term,
                t.definition,
                t.formula
                    .as_deref()
                    .map(|f| format!("，公式: {f}"))
                    .unwrap_or_default()
            ));
        }
    }
    if !ctx.few_shots.is_empty() {
        out.push_str("# 历史问答（few-shot）\n");
        for (i, fs) in ctx.few_shots.iter().enumerate() {
            out.push_str(&format!(
                "## 例 {}\nQ: {}\nSQL:\n```sql\n{}\n```\n",
                i + 1,
                fs.question,
                fs.sql_text
            ));
        }
    }
    out
}
