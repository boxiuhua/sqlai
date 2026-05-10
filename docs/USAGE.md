# sqlai · 智能问数系统 — 使用手册

适用版本：v1.0（含 Top-5 修复，commit `2237302`+）。

---

## 0. 30 秒概念

sqlai 是一个企业 BI 智能问数后端 + 前端：业务人员用自然语言提问 → DeepSeek 判定意图 → 从 PG 元数据库召回 schema → DeepSeek function-calling 选 skill 并填参 → SQL 在 sqlparser 与 ClickHouse `EXPLAIN SYNTAX` 双重校验后真正执行 → 结果以 SSE 流式推回前端，含表格、ECharts 图表、CSV 导出与中文摘要。

支持 8+2 个分析 skill：描述（趋势/Top-N/同比/占比/分组趋势）+ 诊断（归因/相关性/分布对比）+ 轻预测（移动均值/线性外推）+ ML（K-means / 逻辑回归）。

---

## 1. 服务拓扑与端口

| 服务 | 端口 | 容器名 | 用途 |
|---|---:|---|---|
| 前端 nginx | 80 | sqlai-web | SPA 静态站 + `/api/*` 反代到 api |
| Rust 后端 | 8080 | sqlai-api | HTTP + SSE，pipeline 编排 |
| Python sidecar | 8081 | sqlai-sidecar | BGE-M3 embed + sklearn ML |
| PostgreSQL | 5432 | sqlai-pg | 元数据库（schema / 会话 / few-shot / 向量） |
| ClickHouse | 8123 | （外部） | 业务数据源；不在 compose 里 |
| DeepSeek | https | （外部） | LLM；OpenAI 兼容 chat completions |

数据流：浏览器 → 80（前端）→ 8080（api）→ 5432（PG）+ 8081（sidecar）+ 8123（业务 CH）+ DeepSeek。

---

## 2. 启动栈

### 2.1 前提

- Docker Desktop（Linux / Mac / Windows 均可）
- ClickHouse 已经在 `127.0.0.1:8123` 跑（业务数据），admin 账号有读权限
- DeepSeek API key（注册地址 https://platform.deepseek.com/）

### 2.2 写 `.env`

在仓库根创建 `.env`：

```ini
DEEPSEEK_API_KEY=sk-xxxxxxxxxxxxxxxxx
CLICKHOUSE_USER=admin
CLICKHOUSE_PASSWORD=xxxxxxxx
CLICKHOUSE_DB=default
```

> `.env` 已在 `.gitignore`，不会被 commit。

### 2.3 一键起栈

```
docker compose up -d
```

预期输出 4 个容器（pg / clickhouse / sidecar / api / frontend）健康。`clickhouse` 服务在 compose 里只是占位，业务 CH 走外部地址 `host.docker.internal:8123`。

### 2.4 自检

```bash
curl http://127.0.0.1/             # 前端 SPA
curl http://127.0.0.1:8080/healthz # api → {"ok":true}
curl http://127.0.0.1:8081/healthz # sidecar → {"ok":true}
docker exec sqlai-pg psql -U sqlai -d sqlai -c "SELECT count(*) FROM datasource;"
```

> 首次访问 `/embed` 会触发 BGE-M3 模型下载（约 2.3 GB，国内 5-10 min）。compose 已经把 `HF_ENDPOINT` 指向 `hf-mirror.com` 加速。

---

## 3. 注册数据源 + 同步 schema

### 3.1 通过 Web Admin 注册

打开 http://127.0.0.1/admin/datasources

填表：
- name：`ch_local`（任意名）
- host：`host.docker.internal`（容器内访问主机 CH）或 `127.0.0.1`（host 直连）
- port：`8123`
- db：`default`
- user_name：`admin`
- secret_ref：`env:CLICKHOUSE_PASSWORD`（密码从环境变量取，不入库）

> ⚠️ 密码本身不在数据库里；服务启动时按 `secret_ref` 解析的 env var 读。

### 3.2 同步表结构 + 列样本 + 向量化

通过 CLI（容器外）：

```bash
$env:SQLAI_PG_URL="postgres://sqlai:sqlai@127.0.0.1:5432/sqlai"
$env:CLICKHOUSE_PASSWORD="root23"   # 与 .env 一致
cargo run -p sqlai-cli -- sync-schema --datasource ch_local --sample-size 5
```

或在容器内：

```bash
docker exec sqlai-api sqlai-api  # 这是后端 server，CLI 没装在容器里
# CLI 目前需在 host 跑（v1.x 可加进 api 镜像）
```

预期输出：
```
syncing datasource=ch_local db=default
found 2 tables
synced default.orders: 5 columns
synced default.products: 5 columns
```

之后 PG `table_meta` / `column_meta` 里会有该数据源的所有表/列含 1024 维向量。

### 3.3 验证

```bash
docker exec sqlai-pg psql -U sqlai -d sqlai -c \
  "SELECT t.table_name, count(c.id) AS cols FROM table_meta t LEFT JOIN column_meta c ON c.table_id=t.id GROUP BY t.table_name;"
```

---

## 4. Chat 使用

打开 http://127.0.0.1/chat

1. 顶栏选数据源（默认第一条）
2. 输入框问问题，回车 / 点"发送"
3. 渐进展示：
   - **意图判定**（直接 / 反问澄清 / 拒绝）
   - **Skill 选择**（折叠的 SQL 面板，Monaco 高亮）
   - **校验状态**（含自修复 retry 信息）
   - **表格结果**（前 200 行；超出有截断提示）
   - **图表**（ECharts 柱/线/饼，由后端 chart_spec 决定）
   - **业务摘要**（DeepSeek 一句话总结）
   - **导出 CSV**（流末尾出现链接）

### 4.1 提问示例（针对 default.orders）

| 问句 | 命中 skill |
|---|---|
| 看一下 default.orders 按天的订单金额趋势 | metric_overview |
| default.orders 销售额 Top 5 商品 | topn |
| 对比 1 月与 2 月 default.orders 总金额 | compare_period |
| default.orders 各商品销售额占比 | share_breakdown |
| default.orders 按渠道按天的趋势 | trend_segment |
| 为什么 GMV 下降 —— 按品类拆解 | drill_down |
| default.orders 各列之间的相关性 | correlation_matrix |
| 未来 7 天 default.orders 销售额预估 | forecast_simple（含外推）|
| 对 amount + quantity 做 K-means k=3 | cluster_kmeans |
| 用 amount,quantity 预测 is_paid | classify_logreg |

### 4.2 多轮追问

同一会话内的下一问会带上历史。例：
- Q1：上周 GMV 多少
- Q2：再看看上上周 ← 系统会理解"也是 GMV"

### 4.3 失败行为

- LLM 选错 skill / SQL 跑不通：自修复回路最多再试 2 次（用上一次错误反馈给 LLM）
- 仍失败：SSE 流出现 `event: error`，UI 显示红色错误条
- ClickHouse 不存在的列名：当前 `EXPLAIN SYNTAX` 不解析 identifier，要到执行时才报；建议先用 sync-schema 把 schema 刷进 PG

---

## 5. Admin 运营

http://127.0.0.1/admin

### 5.1 数据源（/admin/datasources）

新增 / 列出。修改请删后重建（v1 没有 update 表单）。

### 5.2 业务词表（/admin/terms）

例：
- term：`GMV`
- aliases：`成交额, 总成交`
- definition：`已支付订单金额合计`
- formula：`SUM(amount) WHERE status='paid'`

入库时自动调 sidecar `/embed` 算 1024 维向量。pipeline 检索时会拉 top-5 词表注入 prompt，提高 LLM 对业务术语的理解。

### 5.3 指标定义（/admin/metrics）

例：
- name：`daily_gmv`
- dimension_keys：`date, channel`
- measure_sql：`sum(amount)`
- owner：`data-team`

同样自动 embed。

### 5.4 Few-shot（/admin/few-shots）

展示已收录的"问题 → SQL"示例 + 投票（👍 / 👎 / 删除）。投票分 ≥ 0 的样本会进入检索。新增 few-shot 走 `POST /api/admin/few-shots`（可由前端改造或在采纳一次对话时插入）。

---

## 6. CLI 命令

CLI binary 名是 `sqlai`（package `sqlai-cli`）。

### 6.1 sync-schema

```
sqlai sync-schema --datasource <name> [--sample-size 8] [--sidecar-url http://...]
```

按 `--datasource` 在 PG 里查到的 datasource 配置连 CH，拉 `system.tables` / `system.columns` / 列样本，调 sidecar 向量化，幂等 upsert 进 PG。

### 6.2 eval（GoldenSet 准确率回归）

```
sqlai eval --goldenset docs/superpowers/specs/golden-set-example.json [--report report.json]
```

跑题库、统计 skill_acc / column_acc / 通过率。**任意题失败退出码非零**，便于 CI 用。

题库 schema：
```json
{
  "id":"D001",
  "question":"...",
  "datasource":"ch_local",
  "expected_skill":"metric_overview",
  "expected_columns":["bucket","value"],
  "expected_min_rows":1
}
```

每题都跑完整 pipeline（真实 LLM + 真实 CH）。

---

## 7. 核心 HTTP API

完整列表在 spec § 10；常用接口：

| Method | Path | 说明 |
|---|---|---|
| POST | `/api/sessions` | body: `{user_id, datasource_id, title?}` |
| POST | `/api/sessions/:id/ask` | **SSE**；body: `{question}` |
| GET  | `/api/sessions/:id/messages` | 拉历史 |
| GET  | `/api/messages/:id/export.csv` | 流式 CSV |
| POST | `/api/admin/datasources` | 注册数据源 |
| POST | `/api/admin/business-terms` | 自动 embed 并 upsert |
| POST | `/api/admin/metrics` | 同上 |
| POST | `/api/admin/few-shots` | 同上 |
| POST | `/api/admin/few-shots/:id/vote` | body: `{delta: ±1}` |

### 7.1 SSE 事件序列（按时间）

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

### 7.2 调用示例（curl）

```bash
# 创建 session
SID=$(curl -s -X POST http://127.0.0.1/api/sessions \
  -H 'Content-Type: application/json' \
  -d '{"user_id":"u1","datasource_id":"<uuid>"}' | jq -r .id)

# SSE 提问
curl -N -X POST http://127.0.0.1/api/sessions/$SID/ask \
  -H 'Content-Type: application/json' \
  -d '{"question":"看一下 default.orders 按天的订单金额趋势"}'
```

---

## 8. 关键安全约定

| 边界 | 实现 | 位置 |
|---|---|---|
| SELECT-only SQL 校验 | `ValidatedSql` newtype（无公开构造函数） | sqlai-dialect/validator.rs |
| ClickHouse 只读会话 | `readonly=2` query string 注入 | sqlai-exec/clickhouse.rs |
| LLM 上下文脱敏 | `MaskedContext` newtype + 敏感列名规则 | sqlai-llm/desensitize.rs |
| EXPLAIN-DROP 旁路修复 | `is_readonly()` 递归进 `Statement::Explain.statement` | sqlai-dialect/validator.rs |
| 内部服务不走 HTTP 代理 | reqwest `.no_proxy()` | 4 个 HTTP 客户端 |

**敏感列默认匹配规则**（`SENSITIVE_NAME_HINTS`）：
`phone, mobile, tel, email, mail, id_card, idcard, passport, password, passwd, secret, token, address, addr, bank, card_no, cardno`

包含上述任一关键词的列名，其样本值进 LLM 之前会被掩码（`a***e` 形式）。

---

## 9. 配置项一览（环境变量）

| 变量 | 默认 | 用途 |
|---|---|---|
| `SQLAI_PG_URL` | `postgres://sqlai:sqlai@127.0.0.1:5432/sqlai` | 元数据库 |
| `SQLAI_PG_MAX_CONN` | 10 | PG 连接池上限 |
| `DEEPSEEK_API_KEY` | （必填） | DeepSeek 鉴权 |
| `DEEPSEEK_BASE_URL` | `https://api.deepseek.com` | API 端点（私有部署可改） |
| `DEEPSEEK_MODEL` | `deepseek-chat` | 模型名 |
| `SIDECAR_URL` | `http://127.0.0.1:8081` | sidecar 端点 |
| `CLICKHOUSE_URL` | `http://127.0.0.1:8123` | 默认 CH（按 datasource 覆盖）|
| `CLICKHOUSE_USER` | `admin` | 默认 CH 账号 |
| `CLICKHOUSE_PASSWORD` | （必填） | 默认 CH 密码 |
| `CLICKHOUSE_DB` | `default` | 默认 CH 库 |
| `RUST_LOG` | `info,sqlai=debug` | 日志级别 |
| `HF_ENDPOINT`（容器） | `https://hf-mirror.com` | HuggingFace 镜像，加速 BGE-M3 下载 |
| `SIDECAR_PRELOAD_EMBED` | `0` | 预留字段，v1.0 是 no-op |

---

## 10. 常见问题

### 10.1 首次问答超时 / 返回空

- 多半是 sidecar 还没加载 BGE-M3。看 `docker logs sqlai-sidecar`。下载完成会有日志。
- DeepSeek 请求被代理拦截：检查容器 / host `http_proxy` 环境变量。我们的 reqwest client 已经 `.no_proxy()`，但若 sidecar 容器内部连 HF Hub 走代理失败，也会出问题。

### 10.2 SSE 一上来就 done 没有别的事件

- LLM 把意图判成 `reject` 或 `clarify`：看 SSE 第一条 `intent` 事件 payload；如 `kind=clarify` 应继续追问 / 改问法。

### 10.3 跑出来 SQL 列不存在

- 这是 v1.0 已知限制：`EXPLAIN SYNTAX` 不解析 identifier。先 `sqlai sync-schema` 把列同步进 PG 后，LLM 拿到准确 schema 选 skill 命中率会显著提高。

### 10.4 docker pull 401

国内拉公共镜像偶发 401 / SSL 握手失败。手动：
```
docker pull m.daocloud.io/docker.io/library/<image>:<tag>
docker tag m.daocloud.io/docker.io/library/<image>:<tag> <image>:<tag>
```

### 10.5 ClickHouse 连不上

如果 api 容器报 `transport: ...` 错误，检查 datasource 里的 host：
- 容器跑 api，访问 host 上的 CH 用 `host.docker.internal`
- 你直接 `cargo run` 起 api，访问本机 CH 用 `127.0.0.1`

### 10.6 多轮失忆

确保 session 是同一个：前端默认每次刷新页面**会创建新 session**。在同一会话内追问才有 history。

---

## 11. 已知限制 / 路线图

v1.0 阻塞已修复，剩余 v1.1+ 候选：

- **API 鉴权**：当前无 auth；CorsLayer permissive。生产前需加 Bearer / OAuth。
- **EXPLAIN 改用 PLAN/AST**：当前只校验语法，schema 错（列不存在）漏到 execute 才报。
- **introspection SQL 参数化**：当前用字符串 escape；admin 可控的 `db`/`table` 名是注入面（admin 接口又无 auth）。
- **审计日志**：所有 LLM/SQL/admin 操作目前只走 tracing，不结构化落库。
- **LLM 速率限制 / 退避重试**：当前 429 直接抛错。
- **前端 Playwright E2E**：当前没有自动化 UI 回归。
- **GoldenSet 扩到 100+**：当前 20 题；准确率基线初步建立但样本仍偏少。
- **schema sync HTTP endpoint**：当前只 CLI，运维不便。

---

## 12. 仓库索引

| 路径 | 内容 |
|---|---|
| `crates/sqlai-*` | 9 个 Rust crate |
| `sidecar/` | Python FastAPI sidecar |
| `web/` | React 前端 |
| `migrations/` | PG schema migration |
| `docker-compose.yml` | 全栈编排 |
| `docs/superpowers/specs/` | 设计 spec + GoldenSet 题库 |
| `docs/superpowers/plans/` | 8 份子计划 |
| `docs/USAGE.md` | 本文件 |
| `README.md` | 项目入口 |

完整设计：`docs/superpowers/specs/2026-05-09-smart-query-design.md`。
