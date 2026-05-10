# 智能问数系统 v1.0 — 子计划 #1：基础骨架 + 类型护栏

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 搭出 Rust workspace 骨架 + 三个核心防御类型（`ValidatedSql` / `ReadonlyClickHouse` / `MaskedContext`） + ClickHouse SELECT-only 校验与只读执行 + PG 元数据库迁移文件，作为后续所有子计划的地基。

**Architecture:** Cargo workspace，9 个职责单一的 crate（`sqlai-core / -llm / -dialect / -exec / -store / -skills / -pipeline / -api / -cli`）。三个 newtype 在编译期保证：未校验的 SQL 不可执行；ClickHouse 客户端不可绕过只读 settings；进入 LLM 的上下文必经脱敏。开发期数据库（PG + ClickHouse）通过 docker-compose 提供。

**Tech Stack:** Rust 1.78+ / cargo workspace / sqlparser-rs（SQL AST）/ reqwest（直连 ClickHouse HTTP 8123）/ sqlx + Postgres + pgvector / docker-compose（开发环境）。

> **关于 ClickHouse 客户端选型：** 不使用 `clickhouse` crate。原因：该 crate 的 `fetch_*` API 需要静态 row 类型（实现 `Row` trait），对动态结构（每个查询列结构都不同）不友好。我们通过 reqwest 直接 POST 到 ClickHouse HTTP 端点（`http://host:8123/?...`），用 `FORMAT JSONEachRow` 拿到 JSON 文本逐行 parse，能保留全部 settings 控制力（readonly / max_execution_time / max_result_rows 都通过 query string 注入）。

**前置假设：**
- 已安装 Rust（`rustup` 装的稳定版即可）、Docker Desktop、`cargo` 在 PATH。
- 工作目录 `D:\workspase\rust\sqlai`，目前为空。
- spec 文档已在 `docs/superpowers/specs/2026-05-09-smart-query-design.md`，本计划基于它。

---

## File Structure

本计划完成后的目录树：

```
sqlai/
├── Cargo.toml                        # workspace 根
├── rust-toolchain.toml               # 锁定工具链版本
├── .gitignore
├── docker-compose.yml                # 开发用 PG + ClickHouse
├── README.md
├── migrations/
│   └── 0001_init.sql                 # PG 元数据库全部表
├── crates/
│   ├── sqlai-core/                   # 领域类型（无 IO）
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── sqlai-dialect/                # Dialect trait + ClickHouseDialect + ValidatedSql + 校验器
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── clickhouse.rs
│   │       └── validator.rs
│   ├── sqlai-llm/                    # LlmProvider / EmbeddingProvider trait（占位）+ MaskedContext
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── desensitize.rs
│   ├── sqlai-exec/                   # Executor trait + ReadonlyClickHouse + ClickHouseExecutor
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── clickhouse.rs
│   ├── sqlai-store/                  # （占位 lib，后续子计划填实现）
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── sqlai-skills/                 # （占位）
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── sqlai-pipeline/               # （占位）
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── sqlai-api/                    # （占位 axum bin）
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── sqlai-cli/                    # （占位 clap bin）
│       ├── Cargo.toml
│       └── src/main.rs
└── docs/
    └── superpowers/
        ├── specs/2026-05-09-smart-query-design.md  # 已存在
        └── plans/2026-05-09-01-foundation-skeleton.md  # 本文件
```

每个 crate 的"做什么 / 依赖谁 / 暴露什么"：

| crate | 做什么 | 依赖 | 暴露 |
|---|---|---|---|
| `sqlai-core` | 领域类型（无 IO） | serde | `Question`, `Dialect`, `RetrievalContext`, `TableMeta`, `ColumnMeta`, `IntentDecision`, `SkillCall` |
| `sqlai-dialect` | SQL 解析、SELECT-only 校验、ClickHouse 提示片段 | core, sqlparser | `Dialect` trait, `ClickHouseDialect`, `ValidatedSql` newtype, `validate()` |
| `sqlai-llm` | LLM/Embedding trait + 脱敏 | core | `LlmProvider`, `EmbeddingProvider` trait, `MaskedContext` newtype, `mask()` |
| `sqlai-exec` | 只读 ClickHouse 客户端 + Executor trait | core, dialect, clickhouse | `Executor` trait, `ReadonlyClickHouse`, `ClickHouseExecutor` |
| `sqlai-store / -skills / -pipeline` | （占位 lib，后续子计划填） | — | — |
| `sqlai-api` | （占位 bin，后续子计划填） | — | — |
| `sqlai-cli` | （占位 bin，后续子计划填） | — | — |

---

## Task 1：初始化 workspace + git 仓库 + 顶层配置

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `.gitignore`
- Create: `README.md`

- [ ] **Step 1：创建 workspace 顶层 `Cargo.toml`**

写入 `D:\workspase\rust\sqlai\Cargo.toml`：

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
authors = ["bxh"]

[workspace.dependencies]
# 领域基础
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
thiserror   = "1"
anyhow      = "1"
uuid        = { version = "1", features = ["v4", "serde"] }
chrono      = { version = "0.4", features = ["serde"] }
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# 异步 / 网络
tokio       = { version = "1", features = ["full"] }
reqwest     = { version = "0.12", features = ["json", "stream"] }
async-trait = "0.1"

# SQL / DB
sqlparser   = "0.51"
url         = "2"
sqlx        = { version = "0.8", features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json"] }

# 内部 crate（path 依赖）
sqlai-core      = { path = "crates/sqlai-core" }
sqlai-dialect   = { path = "crates/sqlai-dialect" }
sqlai-llm       = { path = "crates/sqlai-llm" }
sqlai-exec      = { path = "crates/sqlai-exec" }
sqlai-store     = { path = "crates/sqlai-store" }
sqlai-skills    = { path = "crates/sqlai-skills" }
sqlai-pipeline  = { path = "crates/sqlai-pipeline" }

[profile.dev]
opt-level = 0

[profile.release]
opt-level = 3
lto = "thin"
```

- [ ] **Step 2：锁定 Rust 版本**

写入 `rust-toolchain.toml`：

```toml
[toolchain]
channel = "1.78.0"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3：创建 `.gitignore`**

写入 `.gitignore`：

```
/target
**/*.rs.bk
Cargo.lock.bak
.DS_Store
.idea/
.vscode/
*.log
.env
.env.local
```

注：`Cargo.lock` 对 binary 项目应当提交，不在 ignore 中。

- [ ] **Step 4：创建占位 README**

写入 `README.md`：

```markdown
# sqlai — 智能问数系统

Rust 后端 + Python ML sidecar + 独立前端的企业 BI 智能问数系统。

设计文档：`docs/superpowers/specs/2026-05-09-smart-query-design.md`
实现计划：`docs/superpowers/plans/`

## 开发环境

```
docker compose up -d        # 启动 Postgres(pgvector) 与 ClickHouse
cargo build --workspace
cargo test --workspace
```

## crate 索引

详见 `docs/superpowers/specs/2026-05-09-smart-query-design.md` §3.1。
```

- [ ] **Step 5：初始化 git 并首次提交**

```bash
cd D:\workspase\rust\sqlai
git init
git add Cargo.toml rust-toolchain.toml .gitignore README.md docs/
git commit -m "chore: bootstrap workspace skeleton and lock toolchain"
```

预期：成功创建仓库与首次 commit；`git log` 看得到一条记录。

---

## Task 2：sqlai-core 领域类型

**Files:**
- Create: `crates/sqlai-core/Cargo.toml`
- Create: `crates/sqlai-core/src/lib.rs`

- [ ] **Step 1：写测试（先失败）**

写入 `crates/sqlai-core/Cargo.toml`：

```toml
[package]
name = "sqlai-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde       = { workspace = true }
serde_json  = { workspace = true }
uuid        = { workspace = true }
chrono      = { workspace = true }
thiserror   = { workspace = true }
```

写入 `crates/sqlai-core/src/lib.rs`（先只放测试 + 类型声明的 `unimplemented!()` 桩，确认能 fail）：

```rust
//! sqlai-core: 领域类型，无 IO 副作用。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialectKind {
    ClickHouse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub session_id: Uuid,
    pub datasource_id: Uuid,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableMeta {
    pub id: Uuid,
    pub datasource_id: Uuid,
    pub db: String,
    pub table: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub id: Uuid,
    pub table_id: Uuid,
    pub name: String,
    pub data_type: String,
    pub comment: Option<String>,
    pub sample_values: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalContext {
    pub tables: Vec<TableMeta>,
    pub columns: Vec<ColumnMeta>,
    pub business_terms: Vec<BusinessTerm>,
    pub few_shots: Vec<FewShot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessTerm {
    pub term: String,
    pub aliases: Vec<String>,
    pub definition: String,
    pub formula: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FewShot {
    pub question: String,
    pub sql_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntentDecision {
    Direct { hint: String },
    Clarify { prompt: String },
    Reject { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCall {
    pub skill: String,
    pub args: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_serializes_with_kind_tag() {
        let d = IntentDecision::Direct { hint: "h".into() };
        let s = serde_json::to_string(&d).unwrap();
        assert!(s.contains("\"kind\":\"direct\""));
    }

    #[test]
    fn dialect_kind_serializes_snake_case() {
        let d = DialectKind::ClickHouse;
        assert_eq!(serde_json::to_string(&d).unwrap(), "\"click_house\"");
    }
}
```

- [ ] **Step 2：跑测试看通过（这两个测试都能直接通过，因为是序列化形态校验）**

```bash
cargo test -p sqlai-core
```

预期：`test tests::intent_serializes_with_kind_tag ... ok`、`test tests::dialect_kind_serializes_snake_case ... ok`、整体 `test result: ok. 2 passed`。

> 关于 `DialectKind::ClickHouse` 默认 snake_case 序列化为 `click_house` —— 这是 serde 的默认行为，对我们影响不大；如需 `clickhouse`，后续可改 `#[serde(rename = "clickhouse")]`。

- [ ] **Step 3：把 `DialectKind::ClickHouse` 序列化为 `"clickhouse"`（修订需求）**

修改 `lib.rs`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialectKind {
    #[serde(rename = "clickhouse")]
    ClickHouse,
}
```

并把测试断言改为：

```rust
assert_eq!(serde_json::to_string(&d).unwrap(), "\"clickhouse\"");
```

- [ ] **Step 4：跑测试通过**

```bash
cargo test -p sqlai-core
```

预期：2 passed。

- [ ] **Step 5：commit**

```bash
git add crates/sqlai-core
git commit -m "feat(core): add domain types for question / schema / intent"
```

---

## Task 3：sqlai-dialect — Dialect trait + ValidatedSql + ClickHouse 校验器

这是地基里最关键的安全边界：`ValidatedSql` 这个 newtype 没有公开构造函数，只有 `validator::validate()` 能产出。

**Files:**
- Create: `crates/sqlai-dialect/Cargo.toml`
- Create: `crates/sqlai-dialect/src/lib.rs`
- Create: `crates/sqlai-dialect/src/clickhouse.rs`
- Create: `crates/sqlai-dialect/src/validator.rs`

- [ ] **Step 1：写 Cargo.toml**

```toml
[package]
name = "sqlai-dialect"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core  = { workspace = true }
sqlparser   = { workspace = true }
serde       = { workspace = true }
thiserror   = { workspace = true }
```

- [ ] **Step 2：写测试（先失败）— `validator.rs` 测试**

写入 `crates/sqlai-dialect/src/validator.rs`：

```rust
//! SELECT-only 校验：基于 sqlparser-rs 的 AST，强制只接受只读语句。

use sqlparser::ast::Statement;
use sqlparser::dialect::ClickHouseDialect as ChDialect;
use sqlparser::parser::Parser;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ValidationError {
    #[error("parse error: {0}")]
    Parse(String),

    #[error("only SELECT/SHOW/EXPLAIN/DESCRIBE statements are allowed; got: {kind}")]
    NotReadOnly { kind: String },

    #[error("multiple statements are not allowed; got {count}")]
    MultiStatement { count: usize },
}

/// 已通过 SELECT-only 校验的 SQL。无公开构造函数 —— 只能由 `validate()` 产出。
#[derive(Debug, Clone)]
pub struct ValidatedSql(String);

impl ValidatedSql {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 把原始 SQL 校验为 ValidatedSql。
pub fn validate(sql: &str) -> Result<ValidatedSql, ValidationError> {
    let stmts = Parser::parse_sql(&ChDialect {}, sql)
        .map_err(|e| ValidationError::Parse(e.to_string()))?;

    if stmts.len() != 1 {
        return Err(ValidationError::MultiStatement { count: stmts.len() });
    }

    let stmt = &stmts[0];
    let kind = stmt_kind(stmt);
    if !is_readonly(stmt) {
        return Err(ValidationError::NotReadOnly { kind });
    }
    Ok(ValidatedSql(sql.to_string()))
}

fn is_readonly(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::Query(_)
            | Statement::ShowVariable { .. }
            | Statement::ShowVariables { .. }
            | Statement::ShowCreate { .. }
            | Statement::ShowColumns { .. }
            | Statement::ShowTables { .. }
            | Statement::ShowFunctions { .. }
            | Statement::ExplainTable { .. }
            | Statement::Explain { .. }
    )
}

fn stmt_kind(stmt: &Statement) -> String {
    let s = format!("{:?}", stmt);
    s.split_whitespace().next().unwrap_or("Unknown").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_passes() {
        assert!(validate("SELECT 1").is_ok());
        assert!(validate("SELECT a, b FROM t WHERE c > 1 LIMIT 10").is_ok());
    }

    #[test]
    fn explain_passes() {
        assert!(validate("EXPLAIN SELECT 1").is_ok());
    }

    #[test]
    fn show_passes() {
        assert!(validate("SHOW TABLES").is_ok());
    }

    #[test]
    fn insert_rejected() {
        let err = validate("INSERT INTO t VALUES (1)").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn update_rejected() {
        let err = validate("UPDATE t SET a = 1 WHERE b = 2").unwrap_err();
        // ClickHouse dialect 中 UPDATE 也会解析；要么被 sqlparser 解析为 Update（落入 NotReadOnly 分支），
        // 要么被解析失败（落入 Parse 分支）。两种都算合规拒绝。
        assert!(
            matches!(err, ValidationError::NotReadOnly { .. } | ValidationError::Parse(_)),
            "got {:?}", err
        );
    }

    #[test]
    fn delete_rejected() {
        let err = validate("DELETE FROM t WHERE a = 1").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. } | ValidationError::Parse(_)));
    }

    #[test]
    fn drop_rejected() {
        let err = validate("DROP TABLE t").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn alter_rejected() {
        let err = validate("ALTER TABLE t ADD COLUMN x Int32").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn create_rejected() {
        let err = validate("CREATE TABLE t (a Int32) ENGINE = Memory").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn truncate_rejected() {
        let err = validate("TRUNCATE TABLE t").unwrap_err();
        assert!(matches!(err, ValidationError::NotReadOnly { .. }));
    }

    #[test]
    fn multi_statement_rejected() {
        let err = validate("SELECT 1; SELECT 2").unwrap_err();
        assert!(matches!(err, ValidationError::MultiStatement { count: 2 }));
    }

    #[test]
    fn invalid_sql_returns_parse_error() {
        let err = validate("NOT_A_SQL_STATEMENT %%").unwrap_err();
        assert!(matches!(err, ValidationError::Parse(_)));
    }
}
```

- [ ] **Step 3：暂时把 `lib.rs` 写为最小可编译形态并跑测试看是否通过（应当通过）**

写入 `crates/sqlai-dialect/src/lib.rs`：

```rust
pub mod validator;
```

```bash
cargo test -p sqlai-dialect
```

预期：12 passed。如果某条 ClickHouse 特殊语法解析失败（如 `Memory` 引擎），是因 sqlparser 版本对 ClickHouse 引擎子句支持差异 —— 此时按测试中 fall-back 到 `Parse` 分支也算通过。

- [ ] **Step 4：补 `Dialect` trait 与 `ClickHouseDialect` 实现**

写入 `crates/sqlai-dialect/src/clickhouse.rs`：

```rust
use crate::Dialect;
use sqlai_core::DialectKind;

pub struct ClickHouseDialect;

impl Dialect for ClickHouseDialect {
    fn kind(&self) -> DialectKind {
        DialectKind::ClickHouse
    }

    fn limit_clause(&self, n: u64) -> String {
        format!(" LIMIT {}", n)
    }

    fn explain_prefix(&self) -> &'static str {
        "EXPLAIN SYNTAX "
    }

    fn prompt_hints(&self) -> &'static str {
        "ClickHouse 方言要点：使用 toDate/toDateTime 处理时间；聚合用 sum/avg/uniq/quantile；\
         窗口函数支持 over()；避免 SELECT *；大表查询应当带 PREWHERE/WHERE 与 LIMIT。"
    }
}
```

更新 `crates/sqlai-dialect/src/lib.rs`：

```rust
//! sqlai-dialect：方言抽象 + ClickHouse 实现 + SELECT-only 校验。

pub mod clickhouse;
pub mod validator;

pub use validator::{validate, ValidatedSql, ValidationError};

use sqlai_core::DialectKind;

pub trait Dialect: Send + Sync {
    fn kind(&self) -> DialectKind;
    fn limit_clause(&self, n: u64) -> String;
    fn explain_prefix(&self) -> &'static str;
    fn prompt_hints(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clickhouse::ClickHouseDialect;

    #[test]
    fn clickhouse_dialect_basics() {
        let d = ClickHouseDialect;
        assert_eq!(d.kind(), DialectKind::ClickHouse);
        assert_eq!(d.limit_clause(100), " LIMIT 100");
        assert_eq!(d.explain_prefix(), "EXPLAIN SYNTAX ");
        assert!(d.prompt_hints().contains("ClickHouse"));
    }
}
```

- [ ] **Step 5：跑测试通过 + commit**

```bash
cargo test -p sqlai-dialect
```

预期：13 passed（12 个 validator + 1 个 dialect basics）。

```bash
git add crates/sqlai-dialect
git commit -m "feat(dialect): add Dialect trait, ClickHouseDialect, ValidatedSql with SELECT-only validator"
```

---

## Task 4：sqlai-llm — LlmProvider / EmbeddingProvider trait + MaskedContext + 脱敏

**Files:**
- Create: `crates/sqlai-llm/Cargo.toml`
- Create: `crates/sqlai-llm/src/lib.rs`
- Create: `crates/sqlai-llm/src/desensitize.rs`

- [ ] **Step 1：写 Cargo.toml**

```toml
[package]
name = "sqlai-llm"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core  = { workspace = true }
serde       = { workspace = true }
serde_json  = { workspace = true }
async-trait = { workspace = true }
thiserror   = { workspace = true }
```

- [ ] **Step 2：写测试（先失败） — 脱敏行为**

写入 `crates/sqlai-llm/src/desensitize.rs`：

```rust
//! 进入 LLM 的上下文必经此层脱敏。

use sqlai_core::{ColumnMeta, RetrievalContext, TableMeta};
use serde_json::Value;

/// 已脱敏的上下文。无公开构造函数 —— 只能由 `mask()` 产出。
#[derive(Debug, Clone)]
pub struct MaskedContext {
    inner: RetrievalContext,
}

impl MaskedContext {
    pub fn as_ref(&self) -> &RetrievalContext {
        &self.inner
    }
}

/// 默认敏感列名规则（小写匹配）。后续可由配置覆盖。
const SENSITIVE_NAME_HINTS: &[&str] = &[
    "phone", "mobile", "tel",
    "email", "mail",
    "id_card", "idcard", "passport",
    "password", "passwd", "secret", "token",
    "address", "addr",
    "bank", "card_no", "cardno",
];

pub fn mask(ctx: RetrievalContext) -> MaskedContext {
    let RetrievalContext {
        tables,
        columns,
        business_terms,
        few_shots,
    } = ctx;

    let columns = columns.into_iter().map(mask_column).collect();
    MaskedContext {
        inner: RetrievalContext {
            tables: tables.into_iter().map(mask_table).collect(),
            columns,
            business_terms,
            few_shots,
        },
    }
}

fn mask_table(t: TableMeta) -> TableMeta {
    t // 表名暂不脱敏
}

fn mask_column(mut c: ColumnMeta) -> ColumnMeta {
    if is_sensitive(&c.name) {
        c.sample_values = c
            .sample_values
            .into_iter()
            .map(|v| mask_value(&v))
            .collect();
    }
    c
}

fn is_sensitive(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_NAME_HINTS.iter().any(|h| lower.contains(h))
}

fn mask_value(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(mask_string(s)),
        _ => Value::String("***".into()),
    }
}

fn mask_string(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    if n <= 2 {
        return "*".repeat(n);
    }
    let keep_head = chars[0];
    let keep_tail = chars[n - 1];
    format!("{}{}{}", keep_head, "*".repeat(n - 2), keep_tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn col(name: &str, sample: Vec<Value>) -> ColumnMeta {
        ColumnMeta {
            id: Uuid::new_v4(),
            table_id: Uuid::new_v4(),
            name: name.into(),
            data_type: "String".into(),
            comment: None,
            sample_values: sample,
        }
    }

    #[test]
    fn mask_string_keeps_head_and_tail() {
        assert_eq!(mask_string("alice"), "a***e");
        assert_eq!(mask_string("ab"), "**");
        assert_eq!(mask_string("a"), "*");
    }

    #[test]
    fn sensitive_columns_get_masked() {
        let ctx = RetrievalContext {
            tables: vec![],
            columns: vec![
                col("phone_number", vec![json!("13800138000"), json!("18811112222")]),
                col("user_name", vec![json!("alice"), json!("bob")]),
            ],
            business_terms: vec![],
            few_shots: vec![],
        };
        let m = mask(ctx);
        let inner = m.as_ref();

        let phone = inner.columns.iter().find(|c| c.name == "phone_number").unwrap();
        assert_eq!(phone.sample_values, vec![json!("1*********0"), json!("1*********2")]);

        let name = inner.columns.iter().find(|c| c.name == "user_name").unwrap();
        assert_eq!(name.sample_values, vec![json!("alice"), json!("bob")]);
    }

    #[test]
    fn non_string_sensitive_value_replaced_with_stars() {
        let ctx = RetrievalContext {
            tables: vec![],
            columns: vec![col("id_card", vec![json!(110101199001011234_i64)])],
            business_terms: vec![],
            few_shots: vec![],
        };
        let m = mask(ctx);
        assert_eq!(m.as_ref().columns[0].sample_values, vec![json!("***")]);
    }
}
```

- [ ] **Step 3：跑测试看通过**

更新 `crates/sqlai-llm/src/lib.rs`：

```rust
//! sqlai-llm：LLM/Embedding trait + 脱敏。

pub mod desensitize;

pub use desensitize::{mask, MaskedContext};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("rate limited")]
    RateLimited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // system / user / assistant
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub response_format_json: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(
        &self,
        ctx: &MaskedContext,
        req: ChatRequest,
    ) -> Result<ChatResponse, LlmError>;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, LlmError>;
}
```

```bash
cargo test -p sqlai-llm
```

预期：3 passed。

- [ ] **Step 4：（可选清理）跑 clippy**

```bash
cargo clippy -p sqlai-llm -- -D warnings
```

预期：no warnings。如果出现，按提示修复后重新跑。

- [ ] **Step 5：commit**

```bash
git add crates/sqlai-llm
git commit -m "feat(llm): add LlmProvider/EmbeddingProvider traits and MaskedContext desensitizer"
```

---

## Task 5：sqlai-exec — Executor trait + ReadonlyClickHouse + ClickHouseExecutor

这一步开始真的连 ClickHouse。我们用 `reqwest` 直连 ClickHouse HTTP 接口（默认 8123 端口），通过 query string 强制注入 `readonly=2` / `max_execution_time` / `max_result_rows`。`ReadonlyClickHouse` 这个 newtype 把 base URL、账号和这三个 settings 封死，外部代码拿不到一个能脱离只读约束的 reqwest Client。

**Files:**
- Create: `crates/sqlai-exec/Cargo.toml`
- Create: `crates/sqlai-exec/src/lib.rs`
- Create: `crates/sqlai-exec/src/clickhouse.rs`

- [ ] **Step 1：写 Cargo.toml**

```toml
[package]
name = "sqlai-exec"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core      = { workspace = true }
sqlai-dialect   = { workspace = true }
serde           = { workspace = true }
serde_json      = { workspace = true }
async-trait     = { workspace = true }
thiserror       = { workspace = true }
reqwest         = { workspace = true }
url             = { workspace = true }
tokio           = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
```

- [ ] **Step 2：写测试（先失败） — 单元测试 ReadonlyClickHouse 构造**

写入 `crates/sqlai-exec/src/clickhouse.rs`：

```rust
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
    base: Url,            // http://host:8123/
    user: String,
    password: String,
    database: String,
    max_execution_time_secs: u64,
    max_result_rows: u64,
    http: HttpClient,
}

#[derive(Debug, Clone)]
pub struct ReadonlyConfig {
    pub url: String,                    // e.g. http://localhost:8123
    pub user: String,
    pub password: String,
    pub database: String,
    pub max_execution_time_secs: u64,
    pub max_result_rows: u64,
}

impl ReadonlyClickHouse {
    pub fn new(cfg: ReadonlyConfig) -> Result<Self, ClickHouseConfigError> {
        let base = Url::parse(&cfg.url).map_err(|e| ClickHouseConfigError::InvalidUrl(e.to_string()))?;
        let http = HttpClient::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            // 整体请求超时给 settings 留余量 (settings 已经限制 max_execution_time)
            .timeout(std::time::Duration::from_secs(cfg.max_execution_time_secs + 10))
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
            .append_pair("user", &self.user)
            .append_pair("password", &self.password)
            // readonly=2：只读 + 允许会话级 settings 变更
            .append_pair("readonly", "2")
            .append_pair("max_execution_time", &self.max_execution_time_secs.to_string())
            .append_pair("max_result_rows", &self.max_result_rows.to_string());

        let resp = self
            .http
            .post(url)
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
```

写入 `crates/sqlai-exec/src/lib.rs`：

```rust
//! sqlai-exec：Executor trait + ClickHouse 只读执行实现。

pub mod clickhouse;

pub use crate::clickhouse::{ClickHouseExecutor, ReadonlyClickHouse, ReadonlyConfig};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use sqlai_dialect::ValidatedSql;

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("engine error: {0}")]
    Engine(String),

    #[error("timeout")]
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub columns: Vec<String>,
    pub rows: Vec<serde_json::Value>,
    pub truncated: bool,
}

#[async_trait]
pub trait Executor: Send + Sync {
    async fn explain(&self, sql: &ValidatedSql) -> Result<(), ExecError>;
    async fn run(&self, sql: &ValidatedSql) -> Result<ExecutionResult, ExecError>;
}
```

- [ ] **Step 3：跑单元测试通过**

```bash
cargo test -p sqlai-exec --lib
```

预期：2 passed（仅 newtype 构造单测，不连真实 ClickHouse）。

- [ ] **Step 4：写忽略式集成测试（连真实 CH，需要本地容器先起；不要求 CI 默认跑）**

写入 `crates/sqlai-exec/tests/clickhouse_integration.rs`：

```rust
//! 集成测试：需要本地 ClickHouse 在 8123 端口；默认 ignored，按需跑。
//! 跑法：`docker compose up -d clickhouse && cargo test -p sqlai-exec -- --ignored`

use sqlai_dialect::validate;
use sqlai_exec::{ClickHouseExecutor, Executor, ReadonlyClickHouse, ReadonlyConfig};

fn make_executor() -> ClickHouseExecutor {
    let cfg = ReadonlyConfig {
        url: std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into()),
        user: std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into()),
        password: std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_else(|_| "".into()),
        database: std::env::var("CLICKHOUSE_DB").unwrap_or_else(|_| "default".into()),
        max_execution_time_secs: 30,
        max_result_rows: 1000,
    };
    let client = ReadonlyClickHouse::new(cfg).expect("clickhouse client");
    ClickHouseExecutor::new(client)
}

#[ignore]
#[tokio::test]
async fn explain_select_one_works() {
    let exec = make_executor();
    let sql = validate("SELECT 1").unwrap();
    exec.explain(&sql).await.expect("explain should succeed");
}

#[ignore]
#[tokio::test]
async fn run_select_one_returns_one_row() {
    let exec = make_executor();
    let sql = validate("SELECT 1 AS x").unwrap();
    let r = exec.run(&sql).await.expect("run should succeed");
    assert_eq!(r.columns, vec!["x".to_string()]);
    assert_eq!(r.rows.len(), 1);
}

#[ignore]
#[tokio::test]
async fn run_insert_is_unreachable_due_to_validator() {
    // 这个测试演示：validator 已经拦截，executor 根本不会被调用。
    let err = validate("INSERT INTO t VALUES (1)").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("only SELECT"), "unexpected: {msg}");
}
```

跑（暂时不需要 ClickHouse 运行；ignored 测试默认跳过）：

```bash
cargo test -p sqlai-exec
```

预期：单元测试 2 passed；集成测试因 `#[ignore]` 不跑。

- [ ] **Step 5：commit**

```bash
git add crates/sqlai-exec
git commit -m "feat(exec): add Executor trait, ReadonlyClickHouse, ClickHouseExecutor with readonly=2 settings"
```

---

## Task 6：占位 crate — store / skills / pipeline / api / cli

这些 crate 在后续子计划填实现。本任务只让它们能 build，避免后续 task 遇到"找不到 crate"问题。

**Files:**
- Create: `crates/sqlai-store/Cargo.toml` + `src/lib.rs`
- Create: `crates/sqlai-skills/Cargo.toml` + `src/lib.rs`
- Create: `crates/sqlai-pipeline/Cargo.toml` + `src/lib.rs`
- Create: `crates/sqlai-api/Cargo.toml` + `src/main.rs`
- Create: `crates/sqlai-cli/Cargo.toml` + `src/main.rs`

- [ ] **Step 1：sqlai-store 占位**

`crates/sqlai-store/Cargo.toml`：
```toml
[package]
name = "sqlai-store"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core = { workspace = true }
```

`crates/sqlai-store/src/lib.rs`：
```rust
//! sqlai-store：PG + pgvector 持久化。后续子计划填实现。
```

- [ ] **Step 2：sqlai-skills 占位**

`crates/sqlai-skills/Cargo.toml`：
```toml
[package]
name = "sqlai-skills"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core      = { workspace = true }
sqlai-dialect   = { workspace = true }
sqlai-llm       = { workspace = true }
sqlai-exec      = { workspace = true }
```

`crates/sqlai-skills/src/lib.rs`：
```rust
//! sqlai-skills：AnalysisSkill 抽象与内置 skill 实现。后续子计划填。
```

- [ ] **Step 3：sqlai-pipeline 占位**

`crates/sqlai-pipeline/Cargo.toml`：
```toml
[package]
name = "sqlai-pipeline"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core      = { workspace = true }
sqlai-dialect   = { workspace = true }
sqlai-llm       = { workspace = true }
sqlai-exec      = { workspace = true }
sqlai-store     = { workspace = true }
sqlai-skills    = { workspace = true }
```

`crates/sqlai-pipeline/src/lib.rs`：
```rust
//! sqlai-pipeline：流水线编排 + SSE 事件流。后续子计划填。
```

- [ ] **Step 4：sqlai-api 占位（最小 axum hello）**

`crates/sqlai-api/Cargo.toml`：
```toml
[package]
name = "sqlai-api"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core      = { workspace = true }
sqlai-pipeline  = { workspace = true }
tokio           = { workspace = true }
tracing         = { workspace = true }
tracing-subscriber = { workspace = true }
axum            = "0.7"

[[bin]]
name = "sqlai-api"
path = "src/main.rs"
```

`crates/sqlai-api/src/main.rs`：
```rust
use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let app = Router::new().route("/healthz", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("sqlai-api listening on :8080");
    axum::serve(listener, app).await.unwrap();
}
```

- [ ] **Step 5：sqlai-cli 占位**

`crates/sqlai-cli/Cargo.toml`：
```toml
[package]
name = "sqlai-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
sqlai-core      = { workspace = true }
sqlai-pipeline  = { workspace = true }
tokio           = { workspace = true }
clap            = { version = "4", features = ["derive"] }

[[bin]]
name = "sqlai"
path = "src/main.rs"
```

`crates/sqlai-cli/src/main.rs`：
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sqlai", version, about = "智能问数 CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 占位命令；后续子计划填实现。
    Hello,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Hello => println!("sqlai-cli ready"),
    }
}
```

跑整套 build：

```bash
cargo build --workspace
cargo test --workspace
```

预期：`Finished` + `test result: ok` 累计 20 passed（core 2 + dialect 13 + llm 3 + exec 2）。

```bash
git add crates/sqlai-store crates/sqlai-skills crates/sqlai-pipeline crates/sqlai-api crates/sqlai-cli
git commit -m "feat(workspace): add placeholder crates for store/skills/pipeline/api/cli"
```

---

## Task 7：PG 元数据库迁移文件

写出 spec §6 的全部表 DDL，供后续 `sqlai-store` 与运行期使用。

**Files:**
- Create: `migrations/0001_init.sql`

- [ ] **Step 1：写 migration**

写入 `migrations/0001_init.sql`：

```sql
-- 启用 pgvector
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- 数据源
CREATE TABLE IF NOT EXISTS datasource (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL,           -- v1.0 仅 'clickhouse'
    host            TEXT NOT NULL,
    port            INT  NOT NULL,
    db              TEXT NOT NULL,
    user_name       TEXT NOT NULL,
    secret_ref      TEXT NOT NULL,           -- 引用环境变量 / vault key，不直接存密码
    readonly        BOOLEAN NOT NULL DEFAULT TRUE,
    settings        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (name)
);

-- schema 元数据：表
CREATE TABLE IF NOT EXISTS table_meta (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    datasource_id   UUID NOT NULL REFERENCES datasource(id) ON DELETE CASCADE,
    db              TEXT NOT NULL,
    table_name      TEXT NOT NULL,
    comment         TEXT,
    row_count_est   BIGINT,
    embedding       VECTOR(1024),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (datasource_id, db, table_name)
);
CREATE INDEX IF NOT EXISTS table_meta_embedding_idx
    ON table_meta USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- schema 元数据：列
CREATE TABLE IF NOT EXISTS column_meta (
    id                 UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    table_id           UUID NOT NULL REFERENCES table_meta(id) ON DELETE CASCADE,
    name               TEXT NOT NULL,
    data_type          TEXT NOT NULL,
    comment            TEXT,
    sample_values      JSONB NOT NULL DEFAULT '[]'::jsonb,
    distinct_count_est BIGINT,
    embedding          VECTOR(1024),
    UNIQUE (table_id, name)
);
CREATE INDEX IF NOT EXISTS column_meta_embedding_idx
    ON column_meta USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- 业务知识：词表
CREATE TABLE IF NOT EXISTS business_term (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    term        TEXT NOT NULL,
    aliases     TEXT[] NOT NULL DEFAULT '{}',
    definition  TEXT NOT NULL,
    formula     TEXT,
    embedding   VECTOR(1024),
    UNIQUE (term)
);
CREATE INDEX IF NOT EXISTS business_term_embedding_idx
    ON business_term USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- 业务知识：指标定义
CREATE TABLE IF NOT EXISTS metric_def (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    dimension_keys  TEXT[] NOT NULL DEFAULT '{}',
    measure_sql     TEXT NOT NULL,
    owner           TEXT,
    embedding       VECTOR(1024),
    UNIQUE (name)
);
CREATE INDEX IF NOT EXISTS metric_def_embedding_idx
    ON metric_def USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- 会话与历史
CREATE TABLE IF NOT EXISTS session (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         TEXT NOT NULL,
    datasource_id   UUID REFERENCES datasource(id),
    title           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS session_user_idx ON session(user_id);

CREATE TABLE IF NOT EXISTS message (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    session_id      UUID NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,            -- user / assistant / system
    content         JSONB NOT NULL,           -- 原始问题 / SkillCall / 解释 / 摘要
    plan            JSONB,                    -- AnalysisPlan
    chart_spec      JSONB,
    rows_returned   INT,
    latency_ms      INT,
    parent_id       UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS message_session_created_idx ON message(session_id, created_at);

-- few-shot
CREATE TABLE IF NOT EXISTS few_shot (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    question        TEXT NOT NULL,
    skill_call      JSONB NOT NULL,
    sql_text        TEXT NOT NULL,
    datasource_id   UUID REFERENCES datasource(id),
    vote            INT NOT NULL DEFAULT 0,
    embedding       VECTOR(1024),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS few_shot_embedding_idx
    ON few_shot USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
```

- [ ] **Step 2：commit migration（暂不实际执行，等 docker-compose 起 PG 后再跑）**

```bash
git add migrations/0001_init.sql
git commit -m "feat(store): add initial PG migration with pgvector schema"
```

---

## Task 8：docker-compose 开发环境

提供 PG（含 pgvector）+ ClickHouse 两个本地服务，用来跑集成测试与本地开发。

**Files:**
- Create: `docker-compose.yml`

- [ ] **Step 1：写 compose**

写入 `docker-compose.yml`：

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    container_name: sqlai-pg
    environment:
      POSTGRES_USER: sqlai
      POSTGRES_PASSWORD: sqlai
      POSTGRES_DB: sqlai
    ports:
      - "5432:5432"
    volumes:
      - sqlai_pg_data:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d:ro
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U sqlai"]
      interval: 5s
      timeout: 3s
      retries: 10

  clickhouse:
    image: clickhouse/clickhouse-server:24.8
    container_name: sqlai-ch
    ports:
      - "8123:8123"   # HTTP
      - "9000:9000"   # native
    ulimits:
      nofile:
        soft: 262144
        hard: 262144
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:8123/ping"]
      interval: 5s
      timeout: 3s
      retries: 10

volumes:
  sqlai_pg_data:
```

- [ ] **Step 2：手动验证 compose 启动 + migration 跑通 + ClickHouse 可达**

```bash
docker compose up -d
```

等 30 秒让两个容器都 healthy（`docker compose ps` 看 STATUS）。

PG 验证：

```bash
docker exec -i sqlai-pg psql -U sqlai -d sqlai -c "SELECT extname FROM pg_extension WHERE extname='vector';"
```

预期：返回一行 `vector`。

ClickHouse 验证：

```bash
curl -s "http://localhost:8123/?query=SELECT%201"
```

预期：输出 `1`。

- [ ] **Step 3：跑集成测试（连真实 ClickHouse）**

```bash
cargo test -p sqlai-exec -- --ignored
```

预期：3 ignored 集成测试全部 passed。

- [ ] **Step 4：清理（可选）**

```bash
docker compose down        # 停容器但保留数据卷
# 或 docker compose down -v 清掉数据
```

- [ ] **Step 5：commit**

```bash
git add docker-compose.yml
git commit -m "chore(devenv): add docker-compose with pgvector and clickhouse"
```

---

## Task 9：跑通 workspace 全量构建 + 写最小 README

**Files:**
- Modify: `README.md`

- [ ] **Step 1：全量构建**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

预期：全部 OK。如果 fmt 报告差异，跑 `cargo fmt --all` 修正后再 commit（commit 信息：`style: cargo fmt`）。

- [ ] **Step 2：扩充 README**

把 `README.md` 替换为：

```markdown
# sqlai — 智能问数系统

Rust 后端 + Python ML sidecar + 独立前端的企业 BI 智能问数系统。

- 设计文档：`docs/superpowers/specs/2026-05-09-smart-query-design.md`
- 实现计划：`docs/superpowers/plans/`

## 工作区

| crate | 职责 |
|---|---|
| `sqlai-core` | 领域类型（无 IO） |
| `sqlai-dialect` | Dialect trait + ClickHouseDialect + ValidatedSql + SELECT-only 校验 |
| `sqlai-llm` | LlmProvider/EmbeddingProvider trait + MaskedContext 脱敏 |
| `sqlai-exec` | Executor trait + ReadonlyClickHouse + ClickHouseExecutor |
| `sqlai-store` | PG + pgvector（占位） |
| `sqlai-skills` | AnalysisSkill 抽象（占位） |
| `sqlai-pipeline` | 流水线编排（占位） |
| `sqlai-api` | axum HTTP/SSE（占位） |
| `sqlai-cli` | 运维命令（占位） |

## 开发环境

需要 Rust 1.78、Docker Desktop。

```
docker compose up -d                    # 启动 PG(pgvector) + ClickHouse
cargo build --workspace
cargo test --workspace                  # 单元 + 契约
cargo test -p sqlai-exec -- --ignored   # 跑连真实 ClickHouse 的集成测试
```

## 关键安全边界

- `ValidatedSql`：无公开构造函数，唯一产出途径是 `sqlai_dialect::validate()`，强制 SELECT-only。
- `ReadonlyClickHouse`：构造时强制 `readonly=2` settings；`ClickHouseExecutor::run/explain` 均通过它发出请求。
- `MaskedContext`：进入 `LlmProvider::complete()` 的上下文必经 `mask()`。
```

- [ ] **Step 3：commit**

```bash
git add README.md
git commit -m "docs: extend README with crate index and dev quickstart"
```

---

## 验收清单（在第 1 份子计划完成时全部应能通过）

- [ ] `cargo build --workspace` 成功，无 warning
- [ ] `cargo test --workspace` 通过（20 个单元/契约测试：core 2 + dialect 13 + llm 3 + exec 2）
- [ ] `cargo clippy --workspace -- -D warnings` 通过
- [ ] `docker compose up -d` 起来后，`cargo test -p sqlai-exec -- --ignored` 3 个集成测试通过
- [ ] `git log` 至少 8 条 commit，每个 task 一条（或多条）
- [ ] 类型层防御边界三件套都已存在并被测试覆盖：`ValidatedSql` / `ReadonlyClickHouse` / `MaskedContext`

---

## 进入下一份子计划

完成本计划后，下一份是 **#2：LLM Provider（DeepSeek） + Python sidecar（/embed + /ml/run 桩） + 契约测试**。
