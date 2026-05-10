//! sqlai sync-schema：从指定 datasource 拉 ClickHouse 元数据并 upsert 到 PG。

use anyhow::{Context, Result};
use clap::Args;

use sqlai_exec::{ReadonlyClickHouse, ReadonlyConfig};
use sqlai_llm::sidecar::{SidecarConfig, SidecarEmbedder};
use sqlai_llm::EmbeddingProvider;
use sqlai_store::{datasource, schema as store_schema, StoreConfig};

#[derive(Args, Debug)]
pub struct SyncArgs {
    /// 已注册的 datasource 名（PG datasource.name）
    #[arg(long)]
    pub datasource: String,

    /// 每列采样 distinct 值的数量
    #[arg(long, default_value_t = 8)]
    pub sample_size: u32,

    /// Sidecar /embed 端点
    #[arg(long, default_value = "http://127.0.0.1:8081")]
    pub sidecar_url: String,
}

pub async fn run(args: SyncArgs) -> Result<()> {
    let pg_cfg = StoreConfig::from_env().context("load PG config from env")?;
    let pool = sqlai_store::connect(&pg_cfg).await.context("connect PG")?;

    let ds = datasource::get_by_name(&pool, &args.datasource)
        .await
        .with_context(|| format!("datasource '{}' not found in PG", args.datasource))?;
    tracing::info!("syncing datasource={} db={}", ds.name, ds.db);

    // 解析 secret_ref：v1 唯一形式 'env:VAR_NAME'
    let password = if let Some(var) = ds.secret_ref.strip_prefix("env:") {
        std::env::var(var).with_context(|| format!("env {var} not set"))?
    } else {
        return Err(anyhow::anyhow!(
            "unsupported secret_ref scheme: {}",
            ds.secret_ref
        ));
    };

    let ch = ReadonlyClickHouse::new(ReadonlyConfig {
        url: format!("http://{}:{}", ds.host, ds.port),
        user: ds.user_name.clone(),
        password,
        database: ds.db.clone(),
        max_execution_time_secs: 30,
        max_result_rows: 5000,
    })
    .context("clickhouse client")?;

    let embedder = SidecarEmbedder::new(SidecarConfig {
        base_url: args.sidecar_url.clone(),
        timeout_secs: 600,
    })
    .context("sidecar embedder")?;

    let tables = ch.introspect_tables(&ds.db).await.context("list tables")?;
    tracing::info!("found {} tables", tables.len());

    let table_prompts: Vec<String> = tables
        .iter()
        .map(|t| {
            format!(
                "{}.{}: {} (engine={}, rows~{})",
                t.database,
                t.name,
                t.comment.clone().unwrap_or_default(),
                t.engine,
                t.total_rows.unwrap_or(0)
            )
        })
        .collect();

    let table_embs = if table_prompts.is_empty() {
        vec![]
    } else {
        embedder
            .embed(&table_prompts)
            .await
            .context("embed table prompts")?
    };

    for (info, emb) in tables.iter().zip(table_embs.iter()) {
        let t = store_schema::upsert_table(
            &pool,
            store_schema::UpsertTable {
                datasource_id: ds.id,
                db: &info.database,
                table_name: &info.name,
                comment: info.comment.as_deref(),
                row_count_est: info.total_rows.map(|n| n as i64),
                embedding: emb.clone(),
            },
        )
        .await
        .context("upsert table")?;

        let cols = ch
            .introspect_columns(&info.database, &info.name)
            .await
            .with_context(|| format!("columns of {}.{}", info.database, info.name))?;
        if cols.is_empty() {
            continue;
        }

        let mut col_with_samples = Vec::with_capacity(cols.len());
        for c in &cols {
            let samples = ch
                .sample_distinct(&info.database, &info.name, &c.name, args.sample_size)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "sample {}.{}.{} failed: {e}",
                        info.database,
                        info.name,
                        c.name
                    );
                    vec![]
                });
            col_with_samples.push((c, samples));
        }

        let col_prompts: Vec<String> = col_with_samples
            .iter()
            .map(|(c, samples)| {
                format!(
                    "{}.{}.{} ({}): {}; samples: {}",
                    info.database,
                    info.name,
                    c.name,
                    c.data_type,
                    c.comment.clone().unwrap_or_default(),
                    serde_json::Value::Array(samples.clone())
                )
            })
            .collect();
        let col_embs = embedder
            .embed(&col_prompts)
            .await
            .with_context(|| format!("embed columns of {}.{}", info.database, info.name))?;

        for ((c, samples), emb) in col_with_samples.iter().zip(col_embs.iter()) {
            store_schema::upsert_column(
                &pool,
                store_schema::UpsertColumn {
                    table_id: t.id,
                    name: &c.name,
                    data_type: &c.data_type,
                    comment: c.comment.as_deref(),
                    sample_values: serde_json::Value::Array(samples.clone()),
                    distinct_count_est: None,
                    embedding: emb.clone(),
                },
            )
            .await
            .context("upsert column")?;
        }

        tracing::info!(
            "synced {}.{}: {} columns",
            info.database,
            info.name,
            cols.len()
        );
    }

    Ok(())
}
