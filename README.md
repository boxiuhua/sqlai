# sqlai · 智能问数系统

> 企业 BI 自然语言查询：业务用户用一句话问数据 → DeepSeek 检索 schema → 选择分析 skill → SQL 双重校验 → ClickHouse 执行 → 表格 / 图表 / 摘要 SSE 流式回传。

Rust 后端（axum + sqlx + pgvector）+ Python ML sidecar（FastAPI + BGE-M3 + scikit-learn）+ React 前端（Tailwind + ECharts + Monaco）。

---

## 目录

- [一、整体架构](#一整体架构)
- [二、快速部署](#二快速部署)
- [三、首次使用](#三首次使用)
- [四、Chat 使用](#四chat-使用)
- [五、运营 Admin](#五运营-admin)
- [六、CLI 命令](#六cli-命令)
- [七、HTTP API](#七http-api)
- [八、配置项](#八配置项)
- [九、安全约定](#九安全约定)
- [十、常见问题](#十常见问题)
- [十一、开发指南](#十一开发指南)
- [十二、路线图](#十二路线图)

---

## 一、整体架构

| 服务 | 端口 | 容器名 | 用途 |
|---|---:|---|---|
| 前端 nginx | 80 | sqlai-web | SPA 静态站 + `/api/*` 反代到 api |
| Rust 后端 | 8080 | sqlai-api | HTTP + SSE，pipeline 编排 |
| Python sidecar | 8081 | sqlai-sidecar | BGE-M3 embedding + sklearn ML |
| PostgreSQL | 5432 | sqlai-pg | 元数据库（schema / 会话 / few-shot / 向量） |
| ClickHouse | 8123 | （外部） | 业务数据源；不在 compose 里 |
| DeepSeek | https | （外部） | LLM；OpenAI 兼容 chat completions |

**数据流：**

```
浏览器 (80)
   │
   ↓  /api/*  (nginx 反代)
sqlai-api (8080)
   │ ├─ 元数据库 (PG :5432)
   │ ├─ 向量化 (sidecar :8081 → BGE-M3)
   │ ├─ 业务数据 (ClickHouse :8123, readonly)
   │ └─ LLM      (DeepSeek API)
```

**核心子模块（Rust workspace 9 crate）：**

| crate | 职责 |
|---|---|
| `sqlai-core` | 领域类型，无 IO |
| `sqlai-dialect` | Dialect trait + ClickHouseDialect + `ValidatedSql` newtype + SELECT-only 校验 |
| `sqlai-llm` | LlmProvider / EmbeddingProvider trait + DeepSeek + sidecar 客户端 + `MaskedContext` 脱敏 |
| `sqlai-exec` | Executor trait + `ReadonlyClickHouse` newtype（强制 readonly=2） |
| `sqlai-store` | PG + pgvector（datasource / schema / knowledge / session / few_shot） |
| `sqlai-skills` | `AnalysisSkill` trait + 11 个内置 skill（5 描述 + 3 诊断 + forecast + kmeans + classify） |
| `sqlai-pipeline` | 6 阶段流水线 + EXPLAIN 自修复回路 + SSE 事件流 |
| `sqlai-api` | axum HTTP / SSE / Admin CRUD |
| `sqlai-cli` | `sync-schema` / `eval` 子命令 |

---

## 二、快速部署

### 2.1 前提

- **Docker Desktop**（Linux / Mac / Windows 均可）
- **ClickHouse** 已在 `127.0.0.1:8123` 跑（业务数据），有只读账号
- **DeepSeek API key**（注册：https://platform.deepseek.com/）
- 主机能访问外网（拉镜像 + 调 DeepSeek API）

### 2.2 一键起栈

```powershell
# 1) 克隆仓库
git clone <repo-url> sqlai
cd sqlai

# 2) 准备 .env
cp .env.example .env
# 编辑 .env 填入：
#   DEEPSEEK_API_KEY=sk-xxx
#   CLICKHOUSE_USER=admin
#   CLICKHOUSE_PASSWORD=your-readonly-password
#   CLICKHOUSE_DB=default

# 3) 一键起栈
docker compose up -d

# 4) 等 sidecar / pg / api 健康（约 30s）
docker compose ps
```

> ⚠️ **国内用户**：首次构建可能因镜像 401 失败，先手动拉关键镜像：
>
> ```bash
> docker pull m.daocloud.io/docker.io/library/rust:1.83-bookworm
> docker pull m.daocloud.io/docker.io/library/debian:bookworm-slim
> docker pull m.daocloud.io/docker.io/library/node:20-alpine
> docker pull m.daocloud.io/docker.io/library/nginx:alpine
> docker pull m.daocloud.io/docker.io/library/python:3.11-slim
> docker pull m.daocloud.io/docker.io/library/pgvector/pgvector:pg16  # 部分场景
> ```

### 2.3 自检

```bash
curl http://127.0.0.1/                     # 前端 SPA
curl http://127.0.0.1:8080/healthz         # api → {"ok":true}
curl http://127.0.0.1:8081/healthz         # sidecar → {"ok":true}

docker exec sqlai-pg psql -U sqlai -d sqlai \
  -c "SELECT count(*) FROM datasource;"
```

四项全 OK 后浏览器打开 **http://127.0.0.1/** 即可使用。

> **首次访问 `/embed` 会触发 BGE-M3 模型下载（约 2.3 GB，国内 5–10 min）**。compose 已把 `HF_ENDPOINT` 指向 `hf-mirror.com` 加速。

---

## 三、首次使用

### 3.1 注册数据源

打开 **http://127.0.0.1/admin/datasources**，填写：

| 字段 | 示例值 | 说明 |
|---|---|---|
| `name` | `ch_local` | 唯一标识，CLI / API 通过它指代数据源 |
| `kind` | `clickhouse` | v1.0 仅支持 ClickHouse |
| `host` | `host.docker.internal` | 容器内访问主机 CH；本地 cargo 跑用 `127.0.0.1` |
| `port` | `8123` | ClickHouse HTTP 端口 |
| `db` | `default` | 默认数据库 |
| `user_name` | `admin` | 只读账号 |
| `secret_ref` | `env:CLICKHOUSE_PASSWORD` | 密码从 env 取，**不入库** |

> ⚠️ 密码不入数据库；启动时按 `secret_ref` 解析的环境变量名读。

### 3.2 同步 schema 到 PG

CLI 命令（在 host 上跑）：

```powershell
$env:SQLAI_PG_URL="postgres://sqlai:sqlai@127.0.0.1:5432/sqlai"
$env:CLICKHOUSE_PASSWORD="your-readonly-password"
cargo run -p sqlai-cli -- sync-schema --datasource ch_local --sample-size 5
```

预期输出：

```
syncing datasource=ch_local db=default
found 12 tables
synced default.orders: 8 columns
synced default.products: 5 columns
...
```

之后 PG 里有该数据源所有表 / 列含 1024 维 BGE-M3 向量，pipeline 检索阶段就能召回到。

### 3.3 验证

```bash
docker exec sqlai-pg psql -U sqlai -d sqlai -c \
  "SELECT t.table_name, count(c.id) AS cols
   FROM table_meta t LEFT JOIN column_meta c ON c.table_id=t.id
   GROUP BY t.table_name;"
```

---

## 四、Chat 使用

打开 **http://127.0.0.1/chat**：

1. 顶栏选数据源
2. 输入框打问题（或点空态的示例卡片）
3. SSE 渐进展示：
   - **意图判定**（直接 / 反问澄清 / 拒绝）
   - **Skill 选择**（折叠的 SQL 面板）
   - **校验状态**（含自修复 retry 信息）
   - **表格**（前 200 行 + 截断提示）
   - **图表**（ECharts 柱 / 线 / 饼，由后端 `chart_spec` 决定）
   - **摘要**（DeepSeek 一句话总结）
   - **CSV 导出链接**
   - **推荐询问**（横向跨 skill 的 BI 探索）
   - **数据挖掘**（forecast / cluster / correlation / distribution / classify / anomaly / drilldown）

### 4.1 提问示例

| 问句 | 命中 skill |
|---|---|
| 看一下 default.orders 按天的订单金额趋势 | metric_overview |
| default.orders 销售额 Top 5 商品 | topn |
| 对比 1 月与 2 月 default.orders 总金额 | compare_period |
| default.orders 各商品销售额占比 | share_breakdown |
| default.orders 按渠道按天的趋势 | trend_segment |
| 为什么 GMV 下降 —— 按品类拆解 | drill_down |
| default.orders 各列之间的相关性 | correlation_matrix |
| 未来 7 天 default.orders 销售额预估 | forecast_simple |
| 对 amount + quantity 做 K-means k=3 | cluster_kmeans |
| 用 amount,quantity 预测 is_paid | classify_logreg |

### 4.2 多轮追问

同一会话内的下一问会带历史。例：

- Q1：上周 GMV
- Q2：再看看上上周 ← 系统理解"也是 GMV"

### 4.3 失败行为

- LLM 选错 skill / SQL 跑不通：**EXPLAIN 自修复回路最多再试 2 次**（用上一次错误反馈给 LLM）
- 仍失败：SSE 流出现 `event: error`，UI 显示朱红错误条
- ClickHouse 不存在的列名：当前 `EXPLAIN SYNTAX` 不解析 identifier，只能到执行时才报；建议先 `sync-schema` 把 schema 刷进 PG

---

## 五、运营 Admin

**http://127.0.0.1/admin**

| Tab | 功能 |
|---|---|
| 数据源 | 注册 ClickHouse 数据源；启动时按 `secret_ref` 解析密码 |
| 业务词表 | 业务术语 + 别名 + 定义（+公式可选）；服务端自动调 sidecar embed |
| 指标 | 命名指标 + dimension_keys + measure_sql + owner |
| Few-shot | 已采纳的"问题 → SQL"示例；支持 👍/👎 投票 + 删除；vote ≥ 0 进检索 |

**安全细节：**
- 删除用**两段确认**：第一次点变 "确认删除？" 朱红高亮 + 2.5s 后自动撤销，第二次点才真删
- 表单**保存时禁用按钮 / 错误统一在 `ErrorBanner`**

---

## 六、CLI 命令

CLI binary 名是 `sqlai`（package `sqlai-cli`）。本地需要 Rust stable 工具链（≥ 1.85）。

### 6.1 sync-schema · 同步 schema 到 PG

```powershell
$env:SQLAI_PG_URL="postgres://sqlai:sqlai@127.0.0.1:5432/sqlai"
$env:CLICKHOUSE_PASSWORD="root23"
cargo run -p sqlai-cli -- sync-schema --datasource <name> [--sample-size 8]
```

幂等 upsert：拉 ClickHouse `system.tables` / `system.columns` + 列 N 个 distinct 样本 + sidecar 向量化 → PG。

### 6.2 eval · GoldenSet 准确率回归

```powershell
$env:SQLAI_PG_URL="postgres://sqlai:sqlai@127.0.0.1:5432/sqlai"
$env:DEEPSEEK_API_KEY="sk-..."
$env:CLICKHOUSE_PASSWORD="root23"
cargo run -p sqlai-cli -- eval \
  --goldenset docs/superpowers/specs/golden-set-example.json \
  --report eval-report.json
```

跑题库统计 `skill_acc` / `column_acc` / 通过率。任意题失败退出码非零（CI 友好）。

题库 JSON schema：

```json
{
  "id": "D001",
  "question": "...",
  "datasource": "ch_local",
  "expected_skill": "metric_overview",
  "expected_columns": ["bucket", "value"],
  "expected_min_rows": 1
}
```

---

## 七、HTTP API

| Method | Path | 说明 |
|---|---|---|
| GET | `/healthz` | 200 OK `{"ok":true}` |
| POST | `/api/sessions` | body `{user_id, datasource_id, title?}` → `Session` |
| GET | `/api/sessions/:id/messages` | 历史消息 |
| **POST** | **`/api/sessions/:id/ask`** | body `{question}`；**响应 `text/event-stream`** |
| GET | `/api/messages/:id/export.csv` | 流式 CSV |
| POST | `/api/admin/datasources` | 注册 / 更新 |
| GET | `/api/admin/datasources` | list |
| POST | `/api/admin/business-terms` | 自动 embed + upsert |
| DELETE | `/api/admin/business-terms/:term` | |
| POST | `/api/admin/metrics` | 自动 embed + upsert |
| DELETE | `/api/admin/metrics/:name` | |
| GET | `/api/admin/few-shots` | |
| POST | `/api/admin/few-shots` | 自动 embed + upsert |
| POST | `/api/admin/few-shots/:id/vote` | body `{delta: ±1}` |
| DELETE | `/api/admin/few-shots/:id` | |

### 7.1 SSE 事件序列

按时间发生：

```
event: intent          # direct / clarify / reject
event: validate?       # 仅当出现自修复 retry 时
event: skill_call      # 选定的 skill + 参数 + plan
event: validate        # passed=true, retries=N
event: rows            # 每个 SQL step 一条
event: chart           # bar/line/pie/none + x/y
event: metrics_recommend  # v1.0 占位空数组
event: summary         # LLM 一句话摘要
event: done            # latency_ms
```

异常时会出 `event: error { stage, code, message }`。

### 7.2 调用示例

```bash
# 列出数据源
curl http://127.0.0.1:8080/api/admin/datasources

# 创建 session
SID=$(curl -s -X POST http://127.0.0.1:8080/api/sessions \
  -H 'Content-Type: application/json' \
  -d '{"user_id":"u1","datasource_id":"<uuid>"}' | jq -r .id)

# SSE 提问
curl -N -X POST http://127.0.0.1:8080/api/sessions/$SID/ask \
  -H 'Content-Type: application/json' \
  -d '{"question":"看一下 default.orders 按天的订单金额趋势"}'
```

---

## 八、配置项

| 变量 | 默认 | 用途 |
|---|---|---|
| `SQLAI_PG_URL` | `postgres://sqlai:sqlai@127.0.0.1:5432/sqlai` | 元数据库 |
| `SQLAI_PG_MAX_CONN` | 10 | PG 连接池 |
| `DEEPSEEK_API_KEY` | （必填） | DeepSeek 鉴权 |
| `DEEPSEEK_BASE_URL` | `https://api.deepseek.com` | API 端点（私有部署可改） |
| `DEEPSEEK_MODEL` | `deepseek-chat` | 模型名 |
| `SIDECAR_URL` | `http://127.0.0.1:8081` | sidecar 端点 |
| `CLICKHOUSE_URL` | `http://127.0.0.1:8123` | 默认 CH |
| `CLICKHOUSE_USER` | `admin` | 默认账号 |
| `CLICKHOUSE_PASSWORD` | （必填） | 默认密码 |
| `CLICKHOUSE_DB` | `default` | 默认库 |
| `RUST_LOG` | `info,sqlai=debug` | 日志 |
| `HF_ENDPOINT` | `https://hf-mirror.com` | sidecar HuggingFace 镜像 |

---

## 九、安全约定

**编译期保证**（不可绕过）：

| 边界 | 实现 |
|---|---|
| **`ValidatedSql`** | 无公开构造函数，唯一产出 = `validate()`；`Executor::run/explain` 入参即此 newtype |
| **`ReadonlyClickHouse`** | 构造时强制 `readonly=2 / max_execution_time / max_result_rows` |
| **`MaskedContext`** | 进 LLM 的上下文必经 `mask()`；敏感列名样本值自动掩码 |
| **EXPLAIN 旁路修复** | `is_readonly()` 递归校验 `Statement::Explain.statement` |
| **内部服务 `no_proxy()`** | 所有 reqwest 客户端禁用出站代理 |

**敏感列名规则**（默认匹配，自动掩码）：

```
phone, mobile, tel, email, mail, id_card, idcard, passport,
password, passwd, secret, token, address, addr, bank, card_no, cardno
```

匹配的列样本值进 LLM 之前掩码为 `a***e` 形式。

---

## 十、常见问题

### 首次问答超时 / 返回空

多半是 sidecar 还没加载 BGE-M3。看 `docker logs sqlai-sidecar`，下载完会有日志。模型在容器内缓存，重启不会重下。

### SSE 直接 done 没别的事件

LLM 把意图判成 `reject` 或 `clarify`：看 SSE 第一条 `intent` 事件 payload；如 `kind=clarify` 应继续追问 / 改问法。

### SQL 列不存在 (UNKNOWN_IDENTIFIER)

v1.0 已知限制：`EXPLAIN SYNTAX` 不解析 identifier。先 `sync-schema` 同步 schema 后命中率显著提高。

### docker pull 401 / SSL 失败

国内拉公共镜像偶发问题。手动用 daocloud 镜像：

```bash
docker pull m.daocloud.io/docker.io/library/<image>:<tag>
docker tag m.daocloud.io/docker.io/library/<image>:<tag> <image>:<tag>
```

### ClickHouse 连不上

检查 datasource 的 `host`：

- **api 跑在容器里**：用 `host.docker.internal`
- **api 跑在 host (cargo run)**：用 `127.0.0.1`

### 多轮失忆

确保是同一个 session：刷新页面会创建新 session，需在同一会话内追问才有 history。

---

## 十一、开发指南

### 11.1 本地全量测试

```bash
# 1) 起依赖
docker compose up -d postgres sidecar clickhouse  # CH 用你自己的

# 2) Rust 单元 + 契约
cargo test --workspace

# 3) Rust 集成（连真实服务）
$env:DEEPSEEK_API_KEY="sk-..."
$env:CLICKHOUSE_PASSWORD="root23"
cargo test --workspace -- --ignored

# 4) Python sidecar
cd sidecar && pytest

# 5) 前端构建
cd web && npm install && npm run build
```

### 11.2 测试统计

| 范畴 | 数量 |
|---|---:|
| Rust 单元 / 契约 | **52** |
| Python sidecar | **7** |
| Rust 集成（ignored） | **20+** |
| GoldenSet eval | **20 题**（baseline：skill 80% / 通过率 30%）|

### 11.3 仓库索引

| 路径 | 内容 |
|---|---|
| `crates/sqlai-*` | 9 个 Rust crate |
| `sidecar/` | Python FastAPI sidecar |
| `web/` | React 前端 |
| `migrations/` | PG schema migration |
| `docker-compose.yml` | 全栈编排 |
| `docs/superpowers/specs/` | 设计 spec + GoldenSet 题库 |
| `docs/superpowers/plans/` | 8 份子计划 |
| `docs/USAGE.md` | 详细使用手册（本 README 是精简版） |
| `.env.example` | 环境变量模板 |

---

## 十二、路线图

v1.0 + Top-5 上线前阻塞已修（详见 `docs/superpowers/plans/2026-05-10-08-top5-fixes.md`）。

**v1.1 候选：**

- API 鉴权（Bearer / OAuth）+ 审计日志
- EXPLAIN 改用 `EXPLAIN PLAN` / `EXPLAIN AST` 真正解析 identifier
- introspection SQL 改用参数化（消除字符串拼接注入面）
- LLM 速率限制 / 退避重试（429 当前直接抛错）
- 前端 Playwright E2E 自动化
- GoldenSet 扩到 100+
- 多 LLM Provider 路由（Claude / 通义 / 豆包）
- 多方言（MySQL / PostgreSQL）

---

## 许可

MIT
