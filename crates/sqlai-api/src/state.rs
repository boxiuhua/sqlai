use sqlai_llm::EmbeddingProvider;
use sqlai_pipeline::Pipeline;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub pipeline: Pipeline,
    pub embedder: Arc<dyn EmbeddingProvider>,
}
