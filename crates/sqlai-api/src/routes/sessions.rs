use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use sqlai_pipeline::{AskRequest, PipelineEvent};
use sqlai_store::session::NewMessage;
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;
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
    let r = session::create_session(
        &s.pool,
        &req.user_id,
        req.datasource_id,
        req.title.as_deref(),
    )
    .await?;
    Ok(Json(SessionResp {
        id: r.id,
        user_id: r.user_id,
        datasource_id: r.datasource_id,
        title: r.title,
    }))
}

pub async fn list_messages(
    State(s): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let msgs = session::list_messages(&s.pool, session_id).await?;
    Ok(Json(msgs))
}

#[derive(Debug, Deserialize)]
pub struct AskBody {
    pub question: String,
}

pub async fn ask(
    State(s): State<AppState>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<AskBody>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let sess: sqlai_store::session::SessionRecord =
        sqlx::query_as::<_, sqlai_store::session::SessionRecord>(
            "SELECT id, user_id, datasource_id, title, created_at, updated_at FROM session WHERE id = $1",
        )
        .bind(session_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or(ApiError::NotFound)?;

    let datasource_id = sess
        .datasource_id
        .ok_or_else(|| ApiError::BadRequest("session has no datasource_id".into()))?;

    // 1. 先拉历史（包含此前所有 user/assistant 文本），不含本轮问题
    let history_msgs = sqlai_store::session::list_messages(&s.pool, sess.id)
        .await
        .unwrap_or_default();
    let history: Vec<sqlai_llm::ChatMessage> = history_msgs
        .iter()
        .filter_map(|m| {
            let text = match m.role.as_str() {
                "user" => m
                    .content
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                "assistant" => m
                    .content
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                _ => None,
            };
            text.map(|content| sqlai_llm::ChatMessage {
                role: m.role.clone(),
                content,
            })
        })
        .collect();

    // 2. 再 insert 本轮 user_msg
    let user_msg = sqlai_store::session::append_message(
        &s.pool,
        NewMessage {
            session_id: sess.id,
            role: "user".into(),
            content: serde_json::json!({ "text": body.question }),
            plan: None,
            chart_spec: None,
            rows_returned: None,
            latency_ms: None,
            parent_id: None,
        },
    )
    .await?;

    let req = AskRequest {
        session_id: sess.id,
        datasource_id,
        question: body.question.clone(),
        history,
    };
    let mut rx = s.pipeline.ask(req);

    let pool = s.pool.clone();
    let session_id_for_sink = sess.id;
    let user_msg_id = user_msg.id;
    let (sse_tx, sse_rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    tokio::spawn(async move {
        let mut acc_plan: Option<serde_json::Value> = None;
        let mut acc_chart: Option<serde_json::Value> = None;
        let mut acc_rows: i32 = 0;
        let mut acc_first_columns: Option<Vec<String>> = None;
        let mut acc_first_rows: Vec<serde_json::Value> = Vec::new();
        let mut acc_summary: String = String::new();
        let mut latency_ms: Option<i32> = None;
        const MAX_PERSISTED_ROWS: usize = 1000;

        while let Some(ev) = rx.recv().await {
            match &ev {
                PipelineEvent::SkillCall { plan, .. } => {
                    acc_plan = Some(serde_json::to_value(plan).unwrap_or_default());
                }
                PipelineEvent::Chart(c) => {
                    acc_chart = Some(serde_json::to_value(c).unwrap_or_default())
                }
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
                PipelineEvent::Summary { text } => acc_summary = text.clone(),
                PipelineEvent::Done { latency_ms: l } => latency_ms = Some(*l as i32),
                _ => {}
            }
            // Serialize once to avoid borrow-after-move across match arms
            let data = serde_json::to_value(&ev).unwrap_or_default();
            let name = match &ev {
                PipelineEvent::Intent(_) => "intent",
                PipelineEvent::SkillCall { .. } => "skill_call",
                PipelineEvent::Validate { .. } => "validate",
                PipelineEvent::Rows(_) => "rows",
                PipelineEvent::Chart(_) => "chart",
                PipelineEvent::MetricsRecommend(_) => "metrics_recommend",
                PipelineEvent::Summary { .. } => "summary",
                PipelineEvent::Done { .. } => "done",
                PipelineEvent::Error { .. } => "error",
            };
            let evt = Event::default()
                .event(name)
                .json_data(data)
                .unwrap_or_else(|_| Event::default());
            if sse_tx.send(Ok(evt)).await.is_err() {
                break;
            }
        }

        let _ = sqlai_store::session::append_message(
            &pool,
            NewMessage {
                session_id: session_id_for_sink,
                role: "assistant".into(),
                content: serde_json::json!({
                    "summary": acc_summary,
                    "columns": acc_first_columns.unwrap_or_default(),
                    "rows": acc_first_rows,
                }),
                plan: acc_plan,
                chart_spec: acc_chart,
                rows_returned: Some(acc_rows),
                latency_ms,
                parent_id: Some(user_msg_id),
            },
        )
        .await;
        let _ = sqlai_store::session::touch_session(&pool, session_id_for_sink).await;
    });

    let stream = ReceiverStream::new(sse_rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

use axum::body::Body;
use axum::http::header;
use axum::response::Response;

pub async fn export_csv(
    State(s): State<AppState>,
    Path(message_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let m = sqlai_store::session::get_message(&s.pool, message_id).await?;
    let columns: Vec<String> = m
        .content
        .get("columns")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let rows: Vec<serde_json::Value> = m
        .content
        .get("rows")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut buf = String::new();
    buf.push_str(&columns.join(","));
    buf.push('\n');
    for r in rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|c| {
                let v = r.get(c).cloned().unwrap_or(serde_json::Value::Null);
                csv_cell(&v)
            })
            .collect();
        buf.push_str(&cells.join(","));
        buf.push('\n');
    }

    let body = Body::from(buf);
    Response::builder()
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"message-{}.csv\"", m.id),
        )
        .body(body)
        .map_err(|e| ApiError::Internal(e.to_string()))
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
