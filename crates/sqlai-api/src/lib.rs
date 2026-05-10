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
        .route(
            "/healthz",
            get(|| async { axum::Json(json!({"ok": true})) }),
        )
        .route("/api/sessions", post(routes::sessions::create_session))
        .route(
            "/api/sessions/:session_id/messages",
            get(routes::sessions::list_messages),
        )
        .route("/api/sessions/:session_id/ask", post(routes::sessions::ask))
        .route(
            "/api/messages/:message_id/export.csv",
            get(routes::sessions::export_csv),
        )
        .route(
            "/api/admin/datasources",
            post(routes::admin::create_datasource).get(routes::admin::list_datasources),
        )
        .route(
            "/api/admin/business-terms",
            post(routes::admin::create_or_replace_term).get(routes::admin::list_terms),
        )
        .route(
            "/api/admin/business-terms/:term",
            axum::routing::delete(routes::admin::delete_term),
        )
        .route(
            "/api/admin/metrics",
            post(routes::admin::create_or_replace_metric).get(routes::admin::list_metrics),
        )
        .route(
            "/api/admin/metrics/:name",
            axum::routing::delete(routes::admin::delete_metric),
        )
        .route(
            "/api/admin/few-shots",
            post(routes::admin::create_few_shot).get(routes::admin::list_few_shots),
        )
        .route(
            "/api/admin/few-shots/:id/vote",
            post(routes::admin::vote_few_shot),
        )
        .route(
            "/api/admin/few-shots/:id",
            axum::routing::delete(routes::admin::delete_few_shot),
        )
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
