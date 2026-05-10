# 智能问数系统（sqlai）— v1.0 设计文档

- 作者：bxh
- 日期：2026-05-09
- 状态：草案，待审阅

## 1. 背景与目标

构建一个面向企业 BI 场景的智能问数系统：业务 / 运营人员用自然语言对企业 ClickHouse 数据仓库提问，系统自动完成 schema 理解、SQL 生成、校验、执行、结果呈现与解释，并支持多轮追问、图表推荐、指标推荐、数据挖掘类分析。

成功标准：

1. 业务用户在不写 SQL 的前提下，可对接入的 ClickHouse 数据源完成日常指标查询、同环比、Top-N、归因拆解、相关性、聚类 / 分类等典型分析任务。
2. 黄金问答集（≥ 100 题）上 SQL 等价或结果等价命中率达到设定基线，且每次 prompt / few-shot 改动可量化回归。
3. 后端只读账号 + SELECT-only 双重护栏，编译期类型保证未校验 SQL 不可被执行。
4. 单次问答端到端 P50 延迟 ≤ 8s（不含 ML 任务），ML 任务 ≤ 30s。

## 2. 范围

### 2.1 v1.0 范围

| 维度 | 选定范围 |
|---|---|
| 目标数据源 | ClickHouse（单一方言，但保留 Dialect trait 供后续扩展） |
| LLM | DeepSeek（`deepseek-chat`，OpenAI 兼容协议） |
| Embedding | BGE-M3（1024 维，开源），由 Python sidecar 通过 `/embed` endpoint 暴露；不出域 |
| 任务类型 | 描述性分析 + 诊断性分析 + 轻预测 + ML 任务（聚类 / 分类） |
| 输出 | SQL+解释、表格+分页+CSV 导出、ECharts 自动图表（柱 / 线 / 饼）、指标推荐、数据 / 字段级解释、多轮对话 |
| 安全 | 只读 + 单一只读账号 + SELECT-only 强制护栏 |
| 准确率能力 | Schema linking + 业务词表 / 指标知识库 + Few-shot 检索与反馈闭环 + EXPLAIN 校验与自修复 |
| 交付形态 | Rust（axum）HTTP/SSE 后端 + 独立前端仓库（React + ECharts + Monaco）+ Python FastAPI ML sidecar |
| 系统库 | PostgreSQL 15 + pgvector（schema 元数据 / 会话 / few-shot / 向量统一存储） |
| Admin UI | v1.0 含运营页面：数据源 / 词表 / 指标 / few-shot |
| 测试 | 单元 + 契约（fixture 回放）+ 集成（testcontainers）+ 准确率回归（GoldenSet ≥ 100 题）+ E2E（Playwright） |
| 部署 | docker-compose（开发 + 中小型生产同一份） |

### 2.2 显式非目标（v1.0 不做）

- 多 LLM Provider（保留 trait，仅实现 DeepSeek）
- 多方言（保留 Dialect trait，仅实现 ClickHouse）
- 行级 / 列级权限改写（依赖单一只读账号）
- 多租户 / SSO（v1.0 假设单组织内部署）
- ML 模型仓库与版本管理（v1.0 ML 任务每次"采样 + 即时训练 + 即时返回"，不持久化模型）
- 异步长任务队列（一切同步走完，超时即报错）

## 3. 总体架构

```
┌───────────────────────────────────────────────────────────────────────┐
│  前端（独立仓库 sqlai-web，React + ECharts + Monaco）                 │
│  Chat · SQL 折叠 · 表格分页 · 图表 · CSV 导出 · 指标推荐 · 解释面板  │
│  Admin：数据源 / 词表 / 指标 / few-shot 运营页面                      │
└──────────────────────────────────┬────────────────────────────────────┘
                                   │  HTTP + SSE
┌──────────────────────────────────┴────────────────────────────────────┐
│  Rust 后端（cargo workspace: sqlai）                                  │
│                                                                       │
│  sqlai-api ─ axum 路由 / 鉴权 / SSE                                  │
│      │                                                                │
│  sqlai-pipeline ─ intent → retrieval → generate(skill) →             │
│                   validate → execute → postprocess                   │
│      │                  │                  │                          │
│  sqlai-llm        sqlai-store         sqlai-exec / sqlai-dialect     │
│  (DeepSeek)       (PG + pgvector)     (ClickHouse readonly)          │
│                                                                       │
│  sqlai-core ─ 领域类型（无 IO）                                       │
│  sqlai-cli  ─ schema 同步 / few-shot 导入 / GoldenSet eval            │
└─────────────┬─────────────────────────────────────────────────────────┘
              │ HTTP（仅 ML skill 触发）
              ▼
┌─────────────────────────────────────────────────────┐
│  sqlai-sidecar（Python FastAPI）                    │
│  /embed     ─ BGE-M3 文本向量化（schema/词表/few-shot）│
│  /ml/run    ─ scikit-learn：K-means / 决策树 / LR    │
└─────────────────────────────────────────────────────┘

外部业务依赖：ClickHouse 集群（只读账号接入）
```

### 3.1 Rust workspace 拆分

| crate | 职责 | 主要依赖 |
|---|---|---|
| `sqlai-core` | 领域类型（Question / Intent / Plan / TableMeta / Dialect 枚举 / SkillSchema），无 IO | serde |
| `sqlai-llm` | `LlmProvider` trait + DeepSeek 实现；`EmbeddingProvider` trait + sidecar `/embed` 客户端实现；脱敏过滤 | core, reqwest |
| `sqlai-dialect` | `Dialect` trait + ClickHouse 实现（提示片段、AST 解析、SELECT-only 校验、LIMIT 注入） | core, sqlparser |
| `sqlai-exec` | `Executor` trait + ClickHouse 只读连接池 | core, dialect, clickhouse-rs |
| `sqlai-store` | PG 持久化（schema_meta / session / message / few_shot / business_term / metric_def），pgvector 检索 | core, sqlx |
| `sqlai-skills` | `AnalysisSkill` trait + 内置 skill 集 + sidecar 客户端 | core, llm, dialect, exec, reqwest |
| `sqlai-pipeline` | 控制流编排 + SSE 事件流 | 上述全部 |
| `sqlai-api` | axum HTTP / SSE / 鉴权 / Admin API | pipeline |
| `sqlai-cli` | schema 同步 / few-shot 导入 / GoldenSet eval | pipeline, store |

每个 crate 单一职责，能独立 build / test。

## 4. 控制流与数据流

### 4.1 主流水线（方案 C：流水线 + 局部 Agent 行为）

```
用户提问 q + session_id + datasource_id
        │
        ▼
[1] intent.classify(q, history)   ─── DeepSeek 调用 ───
    输出 IntentDecision:
      ├─ Direct(QueryHint)          → 进入 [2]
      ├─ Clarify(prompt)            → SSE 直接返回澄清问题，等用户补答
      └─ Reject(reason)             → 非数据问题 / 越权，结束
        │
        ▼
[2] retrieval.collect(hint, datasource_id)   ─── 无 LLM ───
    并发：
      ├─ 表 / 列向量召回（pgvector，topK=20）
      ├─ business_term / metric_def 命中（关键词 + 向量混合）
      └─ few_shot topK=3
    输出 RetrievalContext（结构化、token 预算受控）
        │
        ▼
[3] skills.select(q, ctx)   ─── DeepSeek 调用：function-calling ───
    LLM 选定 skill 并填参（不直接写 SQL），返回 SkillCall
        │
        ▼
    skill.plan(args, ctx) → AnalysisPlan
        │
        ▼
[4] validate.check(plan, dialect, datasource)
    对 AnalysisPlan 中的每个 Sql 步骤：
      a. 本地 sqlparser 解析 → SELECT-only 强制
      b. 远端 EXPLAIN SYNTAX 校验
    ┌─ 通过 ──────────────────────────────────► 进入 [5]
    └─ 失败：错误回喂 LLM，重新选择 skill / 修订参数（最多 2 次）
              └─ 仍失败 → SSE 返回错误 + 当前 plan，结束
        │
        ▼
[5] execute.run(plan, datasource)
    AnalysisStep 分派：
      ├─ Sql → ClickHouse 执行（强制 LIMIT + max_execution_time）
      ├─ Compute → Rust 内置（移动均值 / 线性外推 / IQR 异常）
      └─ MlTask → POST 到 sidecar（同步 ≤ 30s）
    流式返回首页 + cursor
        │
        ▼
[6] postprocess
    a. 图表推荐（按列类型 + 基数 + skill 输出 hint）
    b. 指标推荐（结果列与 business_term / metric_def 命中匹配）
    c. DeepSeek 调用：基于结果给出 1-2 句业务摘要
        │
        ▼
持久化 message（plan / sql 列表 / chart_spec / 行数 / 耗时 / 摘要）
```

LLM 调用次数上限：常规路径 3 次（意图 + skill 选择 + 摘要）；EXPLAIN 自修复回路最多额外 +2 次，单次问答硬上限 5 次。超出即降级为"不带 few-shot 也不再重试"完成本次问答。

### 4.2 SSE 事件流

| 事件 | 时机 | payload |
|---|---|---|
| `intent` | [1] 完成 | `{kind: "direct" \| "clarify" \| "reject", ...}` |
| `skill_call` | [3] 完成 | `{skill, args, sql_drafts, explanation}` |
| `validate` | [4] 完成 | `{passed, retries, error?}` |
| `rows` | [5] 首页就绪 | `{step_index, columns, rows, total?, cursor, truncated?}` |
| `chart` | [6] 完成 | `{type: "bar"\|"line"\|"pie"\|"none", encoding}` |
| `metrics_recommend` | [6] 完成 | `[{term, why}]` |
| `summary` | [6] 完成 | `{text}` |
| `done` | 收尾 | `{message_id, latency_ms}` |
| `error` | 任何阶段 | `{stage, code, message}` |

### 4.3 关键不变量

1. SQL 在执行前必须通过本地 SELECT-only 校验 + 远端 EXPLAIN，缺一不进 [5]。
2. 任何 LLM 调用前，schema 与样本值经过脱敏过滤（敏感列名 / 值掩码）。
3. 每次 LLM 调用都有 token 上限 + 超时；超限降级为"无 few-shot"再生成一次。
4. 一次请求 LLM 调用上限 5 次：意图 1 + skill 选择 1 + EXPLAIN 自修复 ≤2 + 摘要 1。

## 5. Analysis Skill 抽象

```rust
pub trait AnalysisSkill: Send + Sync {
    fn name(&self) -> &str;
    fn schema(&self) -> SkillSchema;            // JSON Schema，给 LLM function-calling
    fn plan(&self, args: SkillArgs, ctx: &RetrievalContext) -> Result<AnalysisPlan>;
}

pub enum AnalysisStep {
    Sql(SqlStep),                  // 多数 skill 落到这里
    Compute(ComputeFn, ComputeIn), // Rust 内置：MA / 线性外推 / IQR
    MlTask(MlSpec),                // 路由到 sidecar
}

pub struct AnalysisPlan {
    pub steps: Vec<AnalysisStep>,
    pub combine: Option<CombineSpec>, // 多步如何拼装成最终结果集
    pub chart_hint: Option<ChartHint>,
}
```

### v1.0 内置 Skill 清单

| 档位 | Skill | 实现路径 |
|---|---|---|
| 描述性 | `metric_overview` / `topn` / `compare_period` / `share_breakdown` / `trend_segment` | Sql |
| 诊断性 | `drill_down` / `correlation_matrix` / `distribution_shift` | Sql（多查询拼装） |
| 轻预测 | `forecast_simple`（移动均值 / 线性外推） | Compute（Rust 内置） |
| ML | `cluster_kmeans` | MlTask → sidecar K-means |
| ML | `classify` | 优先 ClickHouse `stochasticLogisticRegression`，复杂度高时回落到 sidecar 决策树 |

## 6. 数据存储模型

系统库 = PostgreSQL 15 + pgvector，单库统一管理。

```sql
-- 数据源
CREATE TABLE datasource (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,              -- v1.0 仅 'clickhouse'
  host TEXT, port INT, db TEXT,
  user_name TEXT, secret_ref TEXT, -- secret 走环境变量 / vault，不入库
  readonly BOOL NOT NULL DEFAULT TRUE,
  settings JSONB,
  created_at TIMESTAMPTZ DEFAULT now()
);

-- schema 元数据
CREATE TABLE table_meta (
  id UUID PRIMARY KEY,
  datasource_id UUID REFERENCES datasource(id),
  db TEXT, table_name TEXT,
  comment TEXT,
  row_count_est BIGINT,
  embedding VECTOR(1024),
  updated_at TIMESTAMPTZ
);

CREATE TABLE column_meta (
  id UUID PRIMARY KEY,
  table_id UUID REFERENCES table_meta(id) ON DELETE CASCADE,
  name TEXT, type TEXT,
  comment TEXT,
  sample_values JSONB,
  distinct_count_est BIGINT,
  embedding VECTOR(1024)
);
CREATE INDEX ON table_meta USING ivfflat (embedding vector_cosine_ops);
CREATE INDEX ON column_meta USING ivfflat (embedding vector_cosine_ops);

-- 业务知识
CREATE TABLE business_term (
  id UUID PRIMARY KEY,
  term TEXT NOT NULL,
  aliases TEXT[],
  definition TEXT,
  formula TEXT,
  embedding VECTOR(1024)
);

CREATE TABLE metric_def (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  dimension_keys TEXT[],
  measure_sql TEXT,
  owner TEXT,
  embedding VECTOR(1024)
);

-- 会话与历史
CREATE TABLE session (
  id UUID PRIMARY KEY,
  user_id TEXT,
  datasource_id UUID REFERENCES datasource(id),
  title TEXT,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE message (
  id UUID PRIMARY KEY,
  session_id UUID REFERENCES session(id) ON DELETE CASCADE,
  role TEXT,                      -- user / assistant / system
  content JSONB,                  -- 原始问 / SkillCall / 解释 / 摘要
  plan JSONB,                     -- AnalysisPlan 序列化
  chart_spec JSONB,
  rows_returned INT,
  latency_ms INT,
  parent_id UUID,
  created_at TIMESTAMPTZ DEFAULT now()
);

-- few-shot
CREATE TABLE few_shot (
  id UUID PRIMARY KEY,
  question TEXT,
  skill_call JSONB,
  sql_text TEXT,
  datasource_id UUID,
  vote INT DEFAULT 0,
  embedding VECTOR(1024)
);
```

执行结果不在 PG 中持久化大数据集——只存首页 + cursor，分页继续从 ClickHouse 拉。

## 7. 错误处理与防御边界

### 7.1 错误处理矩阵

| 错误来源 | 策略 | SSE 事件 |
|---|---|---|
| 意图阶段 LLM 失败 | 不重试，503 | `error{stage:"intent"}` |
| schema 检索为空 | 走兜底 prompt（精简全表），不报错 | 无 |
| LLM 生成超时 / JSON 解析失败 | 重试 1 次，仍失败报错 | `error{stage:"generate"}` |
| 本地 SELECT-only 校验失败 | 不重试（说明 LLM 越权），直接报错 | `error{stage:"validate", code:"non_select"}` |
| EXPLAIN 失败 | 错误回喂 LLM，最多重试 2 次 | `validate{retry:n}` |
| 执行超时 / OOM | 直接报错，附 SQL | `error{stage:"execute"}` |
| Sidecar 不可达 | 仅 ML skill 受影响：错误 + 建议改用描述性 | `error{stage:"ml"}` |
| 结果集太大 | 自动 LIMIT 1000 + 提示已截断 | `rows{truncated:true}` |

### 7.2 编译期防御边界（写在 Rust 类型里）

1. **`ReadonlyClickHouse`**：`clickhouse::Client` 的 newtype，构造函数强制 `readonly=2` settings + 拒绝 INSERT/ALTER/DROP；其它代码无法绕过它直接拿到原始 client。
2. **`ValidatedSql(String)`**：无公开构造函数的 newtype，只有 `validator::check()` 能产出。`Executor::run()` 入参是 `ValidatedSql` 而非 `String`，编译器保证未校验 SQL 不可被执行。
3. **`MaskedContext`**：传入 LLM 的所有上下文必须是 `MaskedContext` 类型，由 `desensitizer::mask()` 产出；`LlmProvider::complete()` 入参类型即 `MaskedContext`。

## 8. 测试策略

| 层级 | 范围 | 工具 | 频率 |
|---|---|---|---|
| 单元 | dialect 解析、SELECT-only 校验、Skill 参数 schema、图表 / 指标推荐规则、脱敏函数 | `cargo test` | 每次提交 |
| 契约 | LlmProvider trait（fixture 回放）、Sidecar HTTP 协议 | `wiremock` + JSON fixture | 每次提交 |
| 集成 | pipeline 全流程，ClickHouse / PG / Sidecar 全部容器 | `testcontainers-rs` | PR + 夜间 |
| 准确率回归 | GoldenSet（≥ 100 题）：SQL 等价 / 结果等价 / Top-N 命中 | `sqlai-cli eval` 自建 harness | 每周 + 每次 prompt / few-shot 改动 |
| E2E | 提问 → 图表 → 导出 → 追问 全链路 | Playwright（前端仓库） | 每次发版 |

GoldenSet 是 v1.0 的强制项，没有它无法量化 prompt / few-shot 改动。

## 9. 部署形态

```yaml
# docker-compose.yml（开发与中小型生产同一份）
services:
  sqlai-api:        # Rust axum, :8080
  sqlai-sidecar:    # Python FastAPI, :8081
  sqlai-meta:       # Postgres 15 + pgvector, :5432
  sqlai-frontend:   # Nginx 静态站, :80
```

ClickHouse 在 compose 之外，作为外部依赖通过配置接入。Secrets（DeepSeek API key、ClickHouse 只读账号密码、PG 密码）走环境变量，不入库。

## 10. HTTP API（最小集）

| Method | Path | 说明 |
|---|---|---|
| POST | `/api/sessions` | 创建会话 |
| POST | `/api/sessions/:id/ask` | 提问（**SSE 响应**） |
| GET  | `/api/sessions/:id/messages` | 拉历史消息 |
| GET  | `/api/messages/:id/rows?cursor=...` | 拉分页结果 |
| GET  | `/api/messages/:id/export.csv` | 导出 CSV（流式） |
| POST | `/api/admin/datasources` | 注册 / 更新 ClickHouse 数据源 |
| POST | `/api/admin/schema/sync/:ds_id` | 触发 schema 同步（拉 system.tables/columns + 采样 + 向量化） |
| GET / POST / PUT / DELETE | `/api/admin/business-terms` | 业务词表 CRUD |
| GET / POST / PUT / DELETE | `/api/admin/metrics` | 指标定义 CRUD |
| GET / POST | `/api/admin/few-shots` | few-shot 列表 / 入库 / 投票 |

## 11. 前端能力（独立仓库 sqlai-web）

- Chat 主界面：消息流、SSE 渐进渲染、澄清提问内联回答
- SQL 折叠面板：Monaco 只读 + 高亮，可一键复制
- 表格：虚拟滚动 + 分页 + 列排序 + CSV 导出
- 图表：ECharts 柱 / 线 / 饼，由后端 `chart_spec` 驱动渲染
- 指标推荐 panel：结合结果列与 `business_term` / `metric_def` 命中给出"你可能还想看…"
- 数据解释面板：LLM 摘要 + 字段级解释（hover 列名展示 `column_meta.comment`）
- Admin 二级页面：数据源管理 / 词表 CRUD / 指标 CRUD / few-shot 列表与投票

## 12. 关键决策与依据汇总

| 决策 | 选择 | 依据 |
|---|---|---|
| 控制流 | 流水线 + 局部 Agent（澄清 + 自修复回路） | BI 场景"答错代价 > 慢一点代价"，需可观测 + 鲁棒 |
| LLM | DeepSeek（OpenAI 兼容） | 用户指定；保留 Provider trait |
| 数据源 | ClickHouse | 用户指定；保留 Dialect trait |
| 系统库 | PG + pgvector 一库统管 | 单依赖、运维简单、能覆盖元数据 / 会话 / few-shot / 向量 |
| ML 形态 | Python FastAPI sidecar，模型不持久化 | YAGNI：避免引入模型仓库与版本管理 |
| Embedding | BGE-M3 在 sidecar 暴露 `/embed`，本地推理 | DeepSeek 不提供 embedding API；本地推理避免数据出域；与 ML 复用同一进程节省运维 |
| 安全 | 只读单账号 + SELECT-only + ValidatedSql 类型护栏 | 单账号场景下编译期保证最强约束 |
| 测试 | GoldenSet 写进 v1.0 | prompt 改动需可量化回归，否则准确率失控 |
| Admin UI | v1.0 含完整运营页面 | 词表 / 指标的运营是准确率长期提升的命脉 |

## 13. 开放项 / 后续工作

- v1.x：MySQL / PostgreSQL 方言扩展
- v1.x：多 LLM Provider（Claude / 通义 / 豆包）+ 路由策略
- v2.x：行级 / 列级权限改写（SQL AST 注入过滤）
- v2.x：ML 模型持久化与版本管理
- v2.x：多租户 / SSO
