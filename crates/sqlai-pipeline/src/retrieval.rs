//! 阶段 2：从 PG 检索 top-K table/column/term/metric。

use sqlai_core::{
    BusinessTerm as CoreBusinessTerm, ColumnMeta as CoreColumnMeta, FewShot, RetrievalContext,
    TableMeta as CoreTableMeta,
};
use sqlai_llm::{EmbeddingProvider, LlmError};
use sqlai_store::{few_shot, knowledge, schema as store_schema};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RetrievalConfig {
    pub top_k_tables: i64,
    pub top_k_columns: i64,
    pub top_k_terms: i64,
    pub top_k_metrics: i64,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k_tables: 8,
            top_k_columns: 32,
            top_k_terms: 5,
            top_k_metrics: 5,
        }
    }
}

pub async fn collect(
    pool: &PgPool,
    embedder: &Arc<dyn EmbeddingProvider>,
    datasource_id: Uuid,
    question: &str,
    cfg: &RetrievalConfig,
) -> Result<RetrievalContext, LlmError> {
    let q_embs = embedder.embed(&[question.to_string()]).await?;
    let q = q_embs
        .into_iter()
        .next()
        .ok_or_else(|| LlmError::InvalidResponse("embedder returned no vector".into()))?;

    let q_for_fs = q.clone();

    let tables_with_dist =
        store_schema::top_k_tables_by_embedding(pool, datasource_id, q.clone(), cfg.top_k_tables)
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("pg: {e}")))?;

    let table_ids: Vec<Uuid> = tables_with_dist.iter().map(|(t, _)| t.id).collect();
    let cols_with_dist =
        store_schema::top_k_columns_by_embedding(pool, &table_ids, q.clone(), cfg.top_k_columns)
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("pg: {e}")))?;

    let terms_with_dist = knowledge::top_k_terms(pool, q.clone(), cfg.top_k_terms)
        .await
        .map_err(|e| LlmError::InvalidResponse(format!("pg: {e}")))?;
    let metrics_with_dist = knowledge::top_k_metrics(pool, q, cfg.top_k_metrics)
        .await
        .map_err(|e| LlmError::InvalidResponse(format!("pg: {e}")))?;

    let fs = few_shot::top_k(pool, Some(datasource_id), q_for_fs, 3)
        .await
        .map_err(|e| LlmError::InvalidResponse(format!("pg: {e}")))?;
    let few_shots: Vec<FewShot> = fs
        .into_iter()
        .map(|(r, _)| FewShot {
            question: r.question,
            sql_text: r.sql_text,
        })
        .collect();

    let tables: Vec<CoreTableMeta> = tables_with_dist
        .into_iter()
        .map(|(t, _)| CoreTableMeta {
            id: t.id,
            datasource_id: t.datasource_id,
            db: t.db,
            table: t.table_name,
            comment: t.comment,
        })
        .collect();

    let columns: Vec<CoreColumnMeta> = cols_with_dist
        .into_iter()
        .map(|(c, _)| CoreColumnMeta {
            id: c.id,
            table_id: c.table_id,
            name: c.name,
            data_type: c.data_type,
            comment: c.comment,
            sample_values: match c.sample_values {
                serde_json::Value::Array(arr) => arr,
                _ => vec![],
            },
        })
        .collect();

    let mut business_terms: Vec<CoreBusinessTerm> = terms_with_dist
        .into_iter()
        .map(|(t, _)| CoreBusinessTerm {
            term: t.term,
            aliases: t.aliases,
            definition: t.definition,
            formula: t.formula,
        })
        .collect();

    for (m, _) in metrics_with_dist {
        business_terms.push(CoreBusinessTerm {
            term: m.name,
            aliases: vec![],
            definition: format!(
                "metric: SQL=[{}], dims={:?}",
                m.measure_sql, m.dimension_keys
            ),
            formula: Some(m.measure_sql),
        });
    }

    Ok(RetrievalContext {
        tables,
        columns,
        business_terms,
        few_shots,
    })
}
