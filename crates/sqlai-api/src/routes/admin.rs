use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use sqlai_llm::EmbeddingProvider;
use sqlai_store::{datasource, few_shot, knowledge};

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

fn default_true() -> bool {
    true
}
fn default_settings() -> serde_json::Value {
    serde_json::json!({})
}

pub async fn create_datasource(
    State(s): State<AppState>,
    Json(req): Json<CreateDatasourceReq>,
) -> Result<impl IntoResponse, ApiError> {
    let r = datasource::insert(
        &s.pool,
        datasource::NewDatasource {
            name: &req.name,
            kind: &req.kind,
            host: &req.host,
            port: req.port,
            db: &req.db,
            user_name: &req.user_name,
            secret_ref: &req.secret_ref,
            readonly: req.readonly,
            settings: req.settings,
        },
    )
    .await?;
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

async fn embed_text(
    embedder: &std::sync::Arc<dyn EmbeddingProvider>,
    text: &str,
) -> Result<Vec<f32>, ApiError> {
    let v = embedder
        .embed(&[text.to_string()])
        .await
        .map_err(|e| ApiError::Internal(format!("embed: {e}")))?;
    v.into_iter()
        .next()
        .ok_or_else(|| ApiError::Internal("no embedding".into()))
}

pub async fn create_or_replace_term(
    State(s): State<AppState>,
    Json(req): Json<UpsertTermReq>,
) -> Result<impl IntoResponse, ApiError> {
    let prompt = format!(
        "{}\naliases: {:?}\n{}{}",
        req.term,
        req.aliases,
        req.definition,
        req.formula
            .as_deref()
            .map(|f| format!("\nformula: {f}"))
            .unwrap_or_default()
    );
    let emb = embed_text(&s.embedder, &prompt).await?;
    let r = knowledge::upsert_term(
        &s.pool,
        knowledge::UpsertTerm {
            term: &req.term,
            aliases: &req.aliases,
            definition: &req.definition,
            formula: req.formula.as_deref(),
            embedding: emb,
        },
    )
    .await?;
    Ok(Json(TermResp {
        id: r.id,
        term: r.term,
        aliases: r.aliases,
        definition: r.definition,
        formula: r.formula,
    }))
}

pub async fn list_terms(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let r: Vec<knowledge::BusinessTermRecord> = sqlx::query_as::<_, knowledge::BusinessTermRecord>(
        "SELECT id, term, aliases, definition, formula FROM business_term ORDER BY term",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(r))
}

pub async fn delete_term(
    State(s): State<AppState>,
    Path(term): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let n = sqlx::query("DELETE FROM business_term WHERE term = $1")
        .bind(&term)
        .execute(&s.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .rows_affected();
    if n == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(Json(serde_json::json!({"deleted": term})))
    }
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
    let prompt = format!(
        "metric={}\ndimensions={:?}\nsql={}",
        req.name, req.dimension_keys, req.measure_sql
    );
    let emb = embed_text(&s.embedder, &prompt).await?;
    let r = knowledge::upsert_metric(
        &s.pool,
        knowledge::UpsertMetric {
            name: &req.name,
            dimension_keys: &req.dimension_keys,
            measure_sql: &req.measure_sql,
            owner: req.owner.as_deref(),
            embedding: emb,
        },
    )
    .await?;
    Ok(Json(MetricResp {
        id: r.id,
        name: r.name,
        dimension_keys: r.dimension_keys,
        measure_sql: r.measure_sql,
        owner: r.owner,
    }))
}

pub async fn list_metrics(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let r: Vec<knowledge::MetricDefRecord> = sqlx::query_as::<_, knowledge::MetricDefRecord>(
        "SELECT id, name, dimension_keys, measure_sql, owner FROM metric_def ORDER BY name",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(r))
}

pub async fn delete_metric(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let n = sqlx::query("DELETE FROM metric_def WHERE name = $1")
        .bind(&name)
        .execute(&s.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .rows_affected();
    if n == 0 {
        Err(ApiError::NotFound)
    } else {
        Ok(Json(serde_json::json!({"deleted": name})))
    }
}

// ----- few_shot -----

#[derive(Debug, Deserialize)]
pub struct CreateFewShotReq {
    pub question: String,
    pub skill_call: serde_json::Value,
    pub sql_text: String,
    #[serde(default)]
    pub datasource_id: Option<Uuid>,
}

pub async fn create_few_shot(
    State(s): State<AppState>,
    Json(req): Json<CreateFewShotReq>,
) -> Result<impl IntoResponse, ApiError> {
    let prompt = format!("{}\nSQL: {}", req.question, req.sql_text);
    let emb = embed_text(&s.embedder, &prompt).await?;
    let r = few_shot::insert(
        &s.pool,
        few_shot::NewFewShot {
            question: &req.question,
            skill_call: req.skill_call,
            sql_text: &req.sql_text,
            datasource_id: req.datasource_id,
            embedding: emb,
        },
    )
    .await?;
    Ok(Json(r))
}

pub async fn list_few_shots(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let r = few_shot::list(&s.pool, 200).await?;
    Ok(Json(r))
}

#[derive(Debug, Deserialize)]
pub struct VoteReq {
    pub delta: i32,
}

pub async fn vote_few_shot(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<VoteReq>,
) -> Result<impl IntoResponse, ApiError> {
    if req.delta.abs() > 5 {
        return Err(ApiError::BadRequest("delta out of range".into()));
    }
    let r = few_shot::vote(&s.pool, id, req.delta).await?;
    Ok(Json(r))
}

pub async fn delete_few_shot(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    few_shot::delete(&s.pool, id).await?;
    Ok(Json(serde_json::json!({"deleted": id})))
}
