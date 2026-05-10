use async_trait::async_trait;
use reqwest::Client as HttpClient;
use serde_json::Value;
use thiserror::Error;
use url::Url;

use sqlai_dialect::ValidatedSql;

use crate::{ExecError, ExecutionResult, Executor};

#[derive(Debug, Error)]
pub enum ClickHouseConfigError {
    #[error("invalid url: {0}")]
    InvalidUrl(String),
}

/// 已通过只读 settings 校验的 ClickHouse HTTP 端点。
/// 任何代码无法绕过此 newtype 直接构造一个不带 readonly 的 endpoint。
#[derive(Clone, Debug)]
pub struct ReadonlyClickHouse {
    base: Url, // http://host:8123/
    user: String,
    password: String,
    database: String,
    max_execution_time_secs: u64,
    max_result_rows: u64,
    http: HttpClient,
}

#[derive(Debug, Clone)]
pub struct ReadonlyConfig {
    pub url: String, // e.g. http://localhost:8123
    pub user: String,
    pub password: String,
    pub database: String,
    pub max_execution_time_secs: u64,
    pub max_result_rows: u64,
}

impl ReadonlyClickHouse {
    pub fn new(cfg: ReadonlyConfig) -> Result<Self, ClickHouseConfigError> {
        let base =
            Url::parse(&cfg.url).map_err(|e| ClickHouseConfigError::InvalidUrl(e.to_string()))?;
        let http = HttpClient::builder()
            // 数据源是内部服务，永远不走出站 HTTP 代理 —— 否则环境里的 http_proxy
            // 会让 127.0.0.1 / 内网地址也被路由到代理，握手挂起。
            .no_proxy()
            .connect_timeout(std::time::Duration::from_secs(5))
            // 整体请求超时给 settings 留余量 (settings 已经限制 max_execution_time)
            .timeout(std::time::Duration::from_secs(
                cfg.max_execution_time_secs + 10,
            ))
            .build()
            .map_err(|e| ClickHouseConfigError::InvalidUrl(e.to_string()))?;
        Ok(Self {
            base,
            user: cfg.user,
            password: cfg.password,
            database: cfg.database,
            max_execution_time_secs: cfg.max_execution_time_secs,
            max_result_rows: cfg.max_result_rows,
            http,
        })
    }

    /// 强制注入只读 settings 后发起一次 query。返回响应文本。
    async fn post_query(&self, sql: &str) -> Result<String, ExecError> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| ExecError::Engine("base url cannot be path-extended".into()))?
            .pop_if_empty();

        url.query_pairs_mut()
            .append_pair("database", &self.database)
            // readonly=2：只读 + 允许会话级 settings 变更
            .append_pair("readonly", "2")
            .append_pair(
                "max_execution_time",
                &self.max_execution_time_secs.to_string(),
            )
            .append_pair("max_result_rows", &self.max_result_rows.to_string());

        let resp = self
            .http
            .post(url)
            .header("X-ClickHouse-User", &self.user)
            .header("X-ClickHouse-Key", &self.password)
            .body(sql.to_string())
            .send()
            .await
            .map_err(|e| ExecError::Engine(format!("transport: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ExecError::Engine(format!("body: {e}")))?;
        if !status.is_success() {
            return Err(ExecError::Engine(format!(
                "clickhouse {}: {}",
                status,
                text.lines().take(3).collect::<Vec<_>>().join(" | ")
            )));
        }
        Ok(text)
    }
}

pub struct ClickHouseExecutor {
    client: ReadonlyClickHouse,
}

impl ClickHouseExecutor {
    pub fn new(client: ReadonlyClickHouse) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Executor for ClickHouseExecutor {
    async fn explain(&self, sql: &ValidatedSql) -> Result<(), ExecError> {
        let stmt = format!("EXPLAIN SYNTAX {}", sql.as_str());
        self.client.post_query(&stmt).await?;
        Ok(())
    }

    async fn run(&self, sql: &ValidatedSql) -> Result<ExecutionResult, ExecError> {
        // FORMAT JSONEachRow：每行一个 JSON 对象，便于动态 schema。
        let stmt = format!("{} FORMAT JSONEachRow", sql.as_str());
        let raw = self.client.post_query(&stmt).await?;

        let mut rows = Vec::new();
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(line)
                .map_err(|e| ExecError::Engine(format!("json parse: {e}")))?;
            rows.push(v);
        }

        let columns = rows
            .first()
            .and_then(|r| r.as_object())
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        Ok(ExecutionResult {
            columns,
            rows,
            truncated: false,
        })
    }
}

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ChTableInfo {
    pub database: String,
    pub name: String,
    #[serde(default)]
    pub comment: Option<String>,
    pub total_rows: Option<u64>,
    pub engine: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChColumnInfo {
    pub database: String,
    pub table: String,
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default)]
    pub comment: Option<String>,
}

impl ReadonlyClickHouse {
    /// 列出指定 db 的所有表（排除 view 类引擎）。
    pub async fn introspect_tables(&self, db: &str) -> Result<Vec<ChTableInfo>, ExecError> {
        let sql = format!(
            "SELECT database, name, comment, total_rows, engine \
             FROM system.tables \
             WHERE database = '{}' AND engine NOT LIKE '%View%' \
             ORDER BY name FORMAT JSONEachRow",
            db.replace('\'', "''")
        );
        let raw = self.post_query(&sql).await?;
        parse_jsoneachrow(&raw)
    }

    pub async fn introspect_columns(
        &self,
        db: &str,
        table: &str,
    ) -> Result<Vec<ChColumnInfo>, ExecError> {
        let sql = format!(
            "SELECT database, table, name, type, comment \
             FROM system.columns \
             WHERE database = '{}' AND table = '{}' \
             ORDER BY position FORMAT JSONEachRow",
            db.replace('\'', "''"),
            table.replace('\'', "''"),
        );
        let raw = self.post_query(&sql).await?;
        parse_jsoneachrow(&raw)
    }

    /// 抽取某列的前 n 个 distinct 值，用于 schema linking 与脱敏检查。
    pub async fn sample_distinct(
        &self,
        db: &str,
        table: &str,
        column: &str,
        n: u32,
    ) -> Result<Vec<serde_json::Value>, ExecError> {
        let sql = format!(
            "SELECT DISTINCT `{}` AS v FROM `{}`.`{}` LIMIT {} FORMAT JSONEachRow",
            column.replace('`', "``"),
            db.replace('`', "``"),
            table.replace('`', "``"),
            n
        );
        let raw = self.post_query(&sql).await?;
        let rows: Vec<serde_json::Value> = parse_jsoneachrow_values(&raw)?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.get("v").cloned())
            .collect())
    }
}

fn parse_jsoneachrow<T: for<'de> serde::Deserialize<'de>>(raw: &str) -> Result<Vec<T>, ExecError> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(line)
                .map_err(|e| ExecError::Engine(format!("introspect json: {e}")))?,
        );
    }
    Ok(out)
}

fn parse_jsoneachrow_values(raw: &str) -> Result<Vec<serde_json::Value>, ExecError> {
    let mut out = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(line)
                .map_err(|e| ExecError::Engine(format!("sample json: {e}")))?,
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_url_is_rejected() {
        let cfg = ReadonlyConfig {
            url: "not a url".into(),
            user: "default".into(),
            password: "".into(),
            database: "default".into(),
            max_execution_time_secs: 30,
            max_result_rows: 1000,
        };
        assert!(matches!(
            ReadonlyClickHouse::new(cfg),
            Err(ClickHouseConfigError::InvalidUrl(_))
        ));
    }

    #[test]
    fn good_url_constructs_ok() {
        let cfg = ReadonlyConfig {
            url: "http://localhost:8123".into(),
            user: "default".into(),
            password: "".into(),
            database: "default".into(),
            max_execution_time_secs: 30,
            max_result_rows: 1000,
        };
        assert!(ReadonlyClickHouse::new(cfg).is_ok());
    }
}
