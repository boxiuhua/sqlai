# 智能问数系统 v1.0 — 子计划 #7：前端 Web（Chat + Admin）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 v1.0 后端 HTTP/SSE API 包成可演示的前端：Chat 主界面（SSE 流式问答 + SQL 折叠 + 表格 + ECharts 图表 + 指标推荐 + 摘要 + CSV 导出）+ Admin 运营页面（数据源 / 业务词表 / 指标 / few-shot CRUD）。Vite + React + TypeScript + Tailwind + ECharts + Monaco。

**Architecture:** 单页面 React 应用，路由分两块：`/chat`（默认）+ `/admin`。`web/` 目录在主仓库内，独立 `package.json`。所有数据交互走 axios（GET/POST）+ `@microsoft/fetch-event-source`（SSE）。Tailwind v4（使用 CSS-first 配置） + 少量手写组件，不引大型 UI 库以减小体积。

**Tech Stack:** Node 20+ / Vite 7 / React 19 / TypeScript 5.6 / Tailwind 4 / ECharts 5 / @monaco-editor/react / axios / react-router 6 / @microsoft/fetch-event-source.

**前置假设：**
- #1-#6 完成（44 commit）。
- 后端能在 `http://127.0.0.1:8080` 跑（`cargo run -p sqlai-api`）。
- 前端开发时通过 Vite 代理把 `/api` 转发到 8080，避免 CORS。

---

## File Structure

```
sqlai/
├── web/                                  # NEW
│   ├── package.json
│   ├── tsconfig.json
│   ├── tsconfig.node.json
│   ├── vite.config.ts
│   ├── index.html
│   ├── postcss.config.js                 # Tailwind v4 不需要 postcss，但保留兜底
│   ├── .gitignore
│   ├── Dockerfile
│   ├── nginx.conf                        # 生产镜像 nginx 静态站
│   └── src/
│       ├── main.tsx                      # 入口 + Router
│       ├── App.tsx                       # 顶层 layout + tabs
│       ├── index.css                     # Tailwind import
│       ├── api/
│       │   ├── client.ts                 # axios 实例 + 类型化函数
│       │   ├── types.ts                  # 后端 DTO 类型
│       │   └── sse.ts                    # 流式 ask
│       ├── pages/
│       │   ├── Chat.tsx
│       │   └── Admin.tsx
│       ├── components/
│       │   ├── chat/
│       │   │   ├── MessageList.tsx
│       │   │   ├── MessageBubble.tsx
│       │   │   ├── AssistantPanel.tsx    # SQL + Table + Chart + Summary 复合
│       │   │   ├── SqlPanel.tsx          # Monaco 只读
│       │   │   ├── DataTable.tsx
│       │   │   ├── ChartView.tsx         # ECharts
│       │   │   └── ChatInput.tsx
│       │   └── admin/
│       │       ├── DatasourceTab.tsx
│       │       ├── TermTab.tsx
│       │       ├── MetricTab.tsx
│       │       └── FewShotTab.tsx
│       └── lib/
│           └── classnames.ts
├── docker-compose.yml                    # 加 frontend 服务
└── docs/superpowers/plans/
    └── 2026-05-10-07-frontend-web.md
```

---

## Task 1：Vite 项目骨架（依赖安装 + dev server 跑通）

**Files:** 上面 File Structure 中除了 src/ 与 Dockerfile 之外的所有顶层文件。

- [ ] **Step 1：在 `D:\workspase\rust\sqlai\web` 下手工创建脚手架**

```
mkdir D:\workspase\rust\sqlai\web
cd D:\workspase\rust\sqlai\web
```

写 `package.json`：

```json
{
  "name": "sqlai-web",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview --port 4173"
  },
  "dependencies": {
    "@microsoft/fetch-event-source": "^2.0.1",
    "@monaco-editor/react": "^4.6.0",
    "axios": "^1.7.9",
    "echarts": "^5.5.1",
    "echarts-for-react": "^3.0.2",
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "react-router-dom": "^6.28.1"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.4",
    "tailwindcss": "^4.0.0",
    "@tailwindcss/vite": "^4.0.0",
    "typescript": "~5.6.2",
    "vite": "^7.0.0"
  }
}
```

写 `tsconfig.json`：

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

`tsconfig.node.json`:
```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "strict": true
  },
  "include": ["vite.config.ts"]
}
```

`vite.config.ts`:
```ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/healthz': 'http://127.0.0.1:8080',
    },
  },
})
```

`index.html`:
```html
<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>sqlai · 智能问数</title>
  </head>
  <body class="h-screen">
    <div id="root" class="h-full"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`.gitignore`:
```
node_modules
dist
.vite
*.local
.env
.env.local
```

- [ ] **Step 2：安装依赖**

```powershell
cd D:\workspase\rust\sqlai\web
npm install --registry=https://registry.npmmirror.com 2>&1 | Select-Object -Last 10
```

CN 镜像加速。如果 `npm` 不在 PATH，先安装 Node 20+：用户已确认机器有 Node。预期：1-3 分钟内装完。

- [ ] **Step 3：写最小可跑的 src/**

`src/index.css`:
```css
@import "tailwindcss";

html, body, #root { height: 100%; }
body { font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", "PingFang SC", "Microsoft Yahei", sans-serif; }
```

`src/main.tsx`:
```tsx
import React from 'react'
import ReactDOM from 'react-dom/client'
import { BrowserRouter, Route, Routes, Navigate } from 'react-router-dom'
import App from './App'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<App />}>
          <Route index element={<Navigate to="/chat" replace />} />
          <Route path="chat" element={<div>chat (Task 3)</div>} />
          <Route path="admin/*" element={<div>admin (Task 5)</div>} />
        </Route>
      </Routes>
    </BrowserRouter>
  </React.StrictMode>,
)
```

`src/App.tsx`:
```tsx
import { Outlet, NavLink } from 'react-router-dom'

export default function App() {
  return (
    <div className="flex h-full flex-col">
      <header className="border-b bg-white px-4 py-2 shadow-sm">
        <div className="flex items-center gap-6">
          <span className="text-lg font-semibold text-slate-900">sqlai · 智能问数</span>
          <nav className="flex gap-3 text-sm">
            <NavLink
              to="/chat"
              className={({ isActive }) =>
                'px-2 py-1 rounded ' + (isActive ? 'bg-slate-900 text-white' : 'text-slate-700 hover:bg-slate-100')
              }
            >
              问答
            </NavLink>
            <NavLink
              to="/admin"
              className={({ isActive }) =>
                'px-2 py-1 rounded ' + (isActive ? 'bg-slate-900 text-white' : 'text-slate-700 hover:bg-slate-100')
              }
            >
              运营
            </NavLink>
          </nav>
        </div>
      </header>
      <main className="flex-1 overflow-hidden bg-slate-50">
        <Outlet />
      </main>
    </div>
  )
}
```

- [ ] **Step 4：跑 dev server 验证**

```powershell
cd D:\workspase\rust\sqlai\web
npm run dev 2>&1 | Select-Object -First 10
```
预期：输出 `Local:   http://localhost:5173/`。手动打开浏览器看 navbar 与"chat (Task 3)"占位是否显示。

> **跑法（agent）：** 用 Bash `npm run dev` + 启动后立即用 curl 验证 `http://localhost:5173/` 返回 200，然后 kill 进程；不依赖人工浏览器观察。

- [ ] **Step 5：build 验证**

```powershell
cd D:\workspase\rust\sqlai\web
npm run build 2>&1 | Select-Object -Last 15
```
预期：`dist/` 输出，无 TS 编译错误。

- [ ] **Step 6：commit**

```
cd D:\workspase\rust\sqlai
git add web/.gitignore web/package.json web/package-lock.json web/tsconfig.json web/tsconfig.node.json web/vite.config.ts web/index.html web/src
git commit -m "feat(web): vite + react + tailwind + router skeleton"
```

> 如果 `package-lock.json` 太大不想提交，可在 `.gitignore` 里加，但建议提交以保证复现。

---

## Task 2：API 客户端（typed + SSE）

**Files:**
- Create: `web/src/api/types.ts`
- Create: `web/src/api/client.ts`
- Create: `web/src/api/sse.ts`

- [ ] **Step 1：types.ts**

```ts
export type Uuid = string

export interface Datasource {
  id: Uuid
  name: string
  kind: string
  host: string
  port: number
  db: string
  user_name: string
  secret_ref: string
  readonly: boolean
  settings: Record<string, unknown>
  created_at: string
  updated_at: string
}

export interface Session {
  id: Uuid
  user_id: string
  datasource_id: Uuid | null
  title: string | null
}

export interface Message {
  id: Uuid
  session_id: Uuid
  role: 'user' | 'assistant' | 'system'
  content: any
  plan?: any
  chart_spec?: any
  rows_returned?: number
  latency_ms?: number
  parent_id?: Uuid
  created_at: string
}

export interface BusinessTerm {
  id: Uuid
  term: string
  aliases: string[]
  definition: string
  formula: string | null
}

export interface MetricDef {
  id: Uuid
  name: string
  dimension_keys: string[]
  measure_sql: string
  owner: string | null
}

export interface FewShot {
  id: Uuid
  question: string
  skill_call: any
  sql_text: string
  datasource_id: Uuid | null
  vote: number
  created_at: string
}

// SSE events（与后端 PipelineEvent 对齐）
export interface IntentEvent {
  event: 'intent'
  kind: 'direct' | 'clarify' | 'reject'
  hint?: string
  prompt?: string
  reason?: string
}

export interface SkillCallEvent {
  event: 'skill_call'
  skill: string
  args: any
  plan: { steps: { label: string; sql: string }[]; explanation: string }
}

export interface ValidateEvent {
  event: 'validate'
  passed: boolean
  retries: number
  error?: string
}

export interface RowsEvent {
  event: 'rows'
  step_index: number
  label: string
  columns: string[]
  rows: any[]
  truncated: boolean
}

export interface ChartEvent {
  event: 'chart'
  kind: 'bar' | 'line' | 'pie' | 'none'
  x?: string
  y?: string
}

export interface SummaryEvent {
  event: 'summary'
  text: string
}

export interface DoneEvent {
  event: 'done'
  latency_ms: number
}

export interface ErrorEvent {
  event: 'error'
  stage: string
  code: string
  message: string
}

export type PipelineEvent =
  | IntentEvent
  | SkillCallEvent
  | ValidateEvent
  | RowsEvent
  | ChartEvent
  | SummaryEvent
  | DoneEvent
  | ErrorEvent
  | { event: 'metrics_recommend'; data?: any[] }
```

- [ ] **Step 2：client.ts**

```ts
import axios from 'axios'
import type {
  BusinessTerm, Datasource, FewShot, Message, MetricDef, Session, Uuid,
} from './types'

export const http = axios.create({
  baseURL: '/',
  timeout: 30000,
})

// ----- sessions -----
export async function createSession(p: { user_id: string; datasource_id: Uuid; title?: string }) {
  const r = await http.post<Session>('/api/sessions', p)
  return r.data
}
export async function listMessages(session_id: Uuid) {
  const r = await http.get<Message[]>(`/api/sessions/${session_id}/messages`)
  return r.data
}

// ----- admin: datasource -----
export async function listDatasources() {
  const r = await http.get<Datasource[]>('/api/admin/datasources')
  return r.data
}
export async function createDatasource(p: Partial<Datasource> & { name: string; kind: string; host: string; port: number; db: string; user_name: string; secret_ref: string }) {
  const r = await http.post<Datasource>('/api/admin/datasources', p)
  return r.data
}

// ----- admin: business term -----
export async function listTerms() {
  const r = await http.get<BusinessTerm[]>('/api/admin/business-terms')
  return r.data
}
export async function upsertTerm(p: { term: string; aliases: string[]; definition: string; formula?: string }) {
  const r = await http.post<BusinessTerm>('/api/admin/business-terms', p)
  return r.data
}
export async function deleteTerm(term: string) {
  return http.delete(`/api/admin/business-terms/${encodeURIComponent(term)}`)
}

// ----- admin: metric -----
export async function listMetrics() {
  const r = await http.get<MetricDef[]>('/api/admin/metrics')
  return r.data
}
export async function upsertMetric(p: { name: string; dimension_keys: string[]; measure_sql: string; owner?: string }) {
  const r = await http.post<MetricDef>('/api/admin/metrics', p)
  return r.data
}
export async function deleteMetric(name: string) {
  return http.delete(`/api/admin/metrics/${encodeURIComponent(name)}`)
}

// ----- admin: few-shot -----
export async function listFewShots() {
  const r = await http.get<FewShot[]>('/api/admin/few-shots')
  return r.data
}
export async function createFewShot(p: { question: string; skill_call: any; sql_text: string; datasource_id?: Uuid }) {
  const r = await http.post<FewShot>('/api/admin/few-shots', p)
  return r.data
}
export async function voteFewShot(id: Uuid, delta: number) {
  const r = await http.post<FewShot>(`/api/admin/few-shots/${id}/vote`, { delta })
  return r.data
}
export async function deleteFewShot(id: Uuid) {
  return http.delete(`/api/admin/few-shots/${id}`)
}

// CSV export URL
export function csvExportUrl(message_id: Uuid): string {
  return `/api/messages/${message_id}/export.csv`
}
```

- [ ] **Step 3：sse.ts**

```ts
import { fetchEventSource } from '@microsoft/fetch-event-source'
import type { PipelineEvent, Uuid } from './types'

export interface AskParams {
  session_id: Uuid
  question: string
}

/** 启动一次 ask SSE。返回中断函数。 */
export function ask(p: AskParams, onEvent: (e: PipelineEvent) => void, onClose?: () => void): () => void {
  const ctrl = new AbortController()
  fetchEventSource(`/api/sessions/${p.session_id}/ask`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ question: p.question }),
    signal: ctrl.signal,
    openWhenHidden: true,
    onmessage(ev) {
      if (!ev.event) return
      try {
        const data = ev.data ? JSON.parse(ev.data) : {}
        onEvent({ ...data, event: ev.event } as PipelineEvent)
      } catch {
        // ignore malformed event
      }
    },
    onclose() {
      onClose?.()
    },
    onerror(err) {
      throw err
    },
  })
  return () => ctrl.abort()
}
```

- [ ] **Step 4：build 验证（确保 import 都解析）**

```
cd D:\workspase\rust\sqlai\web
npm run build 2>&1 | Select-Object -Last 10
```
预期：成功（即便目前没人引用这些 api 函数，TS noUnusedLocals 也只对模块内未用变量报错；模块被 export 的不算）。

- [ ] **Step 5：commit**

```
cd D:\workspase\rust\sqlai
git add web/src/api
git commit -m "feat(web): typed API client + SSE consumer"
```

---

## Task 3：Chat 主界面

**Files:**
- Create: `web/src/pages/Chat.tsx`
- Create: `web/src/components/chat/{MessageList,MessageBubble,AssistantPanel,SqlPanel,DataTable,ChartView,ChatInput}.tsx`
- Create: `web/src/lib/classnames.ts`
- Modify: `web/src/main.tsx`（route 接 Chat）

- [ ] **Step 1：classnames.ts**

```ts
export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(' ')
}
```

- [ ] **Step 2：组件骨架（按依赖序）**

`SqlPanel.tsx`:
```tsx
import { lazy, Suspense, useState } from 'react'

const Editor = lazy(() => import('@monaco-editor/react'))

interface Props { sql: string; label: string }

export function SqlPanel({ sql, label }: Props) {
  const [open, setOpen] = useState(false)
  return (
    <div className="rounded border bg-slate-50">
      <button
        className="flex w-full items-center justify-between px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100"
        onClick={() => setOpen(!open)}
      >
        <span>SQL · {label}</span>
        <span>{open ? '▾' : '▸'}</span>
      </button>
      {open && (
        <Suspense fallback={<div className="p-3 text-sm text-slate-500">loading editor…</div>}>
          <Editor
            height="220px"
            defaultLanguage="sql"
            value={sql}
            theme="vs"
            options={{ readOnly: true, minimap: { enabled: false }, fontSize: 13, wordWrap: 'on' }}
          />
        </Suspense>
      )}
    </div>
  )
}
```

`DataTable.tsx`:
```tsx
interface Props { columns: string[]; rows: any[] }

export function DataTable({ columns, rows }: Props) {
  if (columns.length === 0 || rows.length === 0) {
    return <div className="rounded border bg-slate-50 p-3 text-sm text-slate-500">无结果行</div>
  }
  return (
    <div className="overflow-auto rounded border bg-white">
      <table className="min-w-full text-sm">
        <thead className="bg-slate-100 text-slate-700">
          <tr>
            {columns.map((c) => (
              <th key={c} className="px-3 py-2 text-left font-medium">{c}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.slice(0, 200).map((r, i) => (
            <tr key={i} className="border-t border-slate-100">
              {columns.map((c) => (
                <td key={c} className="px-3 py-1.5 text-slate-800">
                  {fmt(r?.[c])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length > 200 && (
        <div className="border-t bg-slate-50 px-3 py-1.5 text-xs text-slate-500">
          仅展示前 200 / {rows.length} 行
        </div>
      )}
    </div>
  )
}

function fmt(v: any): string {
  if (v == null) return ''
  if (typeof v === 'object') return JSON.stringify(v)
  return String(v)
}
```

`ChartView.tsx`:
```tsx
import ReactECharts from 'echarts-for-react'

interface Props {
  spec: { kind: 'bar' | 'line' | 'pie' | 'none'; x?: string; y?: string }
  rows: any[]
}

export function ChartView({ spec, rows }: Props) {
  if (spec.kind === 'none' || !spec.x || !spec.y || rows.length === 0) return null
  const xs = rows.map((r) => String(r[spec.x!] ?? ''))
  const ys = rows.map((r) => Number(r[spec.y!] ?? 0))

  let option: any
  if (spec.kind === 'pie') {
    option = {
      tooltip: { trigger: 'item' },
      legend: { bottom: 0 },
      series: [{
        type: 'pie',
        radius: ['40%', '70%'],
        data: rows.map((r) => ({ name: String(r[spec.x!]), value: Number(r[spec.y!]) })),
      }],
    }
  } else {
    option = {
      tooltip: { trigger: 'axis' },
      grid: { left: 40, right: 16, top: 24, bottom: 32 },
      xAxis: { type: 'category', data: xs, axisLabel: { rotate: xs.length > 12 ? 45 : 0 } },
      yAxis: { type: 'value' },
      series: [{ type: spec.kind, data: ys }],
    }
  }
  return (
    <div className="rounded border bg-white p-2">
      <ReactECharts option={option} style={{ height: 280 }} />
    </div>
  )
}
```

`AssistantPanel.tsx`:
```tsx
import type { ChartEvent, RowsEvent, SkillCallEvent } from '../../api/types'
import { csvExportUrl } from '../../api/client'
import { SqlPanel } from './SqlPanel'
import { DataTable } from './DataTable'
import { ChartView } from './ChartView'

interface Props {
  messageId?: string
  skillCall?: SkillCallEvent | null
  rows?: RowsEvent | null
  chart?: ChartEvent | null
  summary?: string | null
}

export function AssistantPanel({ messageId, skillCall, rows, chart, summary }: Props) {
  return (
    <div className="space-y-3">
      {summary && (
        <div className="rounded border bg-amber-50 p-3 text-sm text-amber-900">
          {summary}
        </div>
      )}
      {chart && rows && (
        <ChartView spec={{ kind: chart.kind, x: chart.x, y: chart.y }} rows={rows.rows} />
      )}
      {rows && (
        <div className="space-y-1">
          <div className="flex items-center justify-between">
            <div className="text-sm font-medium text-slate-700">{rows.label}</div>
            {messageId && (
              <a
                href={csvExportUrl(messageId)}
                className="text-xs text-blue-600 hover:underline"
                download
              >
                导出 CSV
              </a>
            )}
          </div>
          <DataTable columns={rows.columns} rows={rows.rows} />
        </div>
      )}
      {skillCall && (
        <div className="space-y-2">
          <div className="text-xs text-slate-500">
            skill: <code className="rounded bg-slate-200 px-1">{skillCall.skill}</code>
            {skillCall.plan?.explanation && <span className="ml-2 text-slate-600">{skillCall.plan.explanation}</span>}
          </div>
          {skillCall.plan?.steps?.map((s, i) => (
            <SqlPanel key={i} sql={s.sql} label={s.label} />
          ))}
        </div>
      )}
    </div>
  )
}
```

`MessageBubble.tsx`:
```tsx
import type { ReactNode } from 'react'
import { cx } from '../../lib/classnames'

interface Props { role: 'user' | 'assistant'; children: ReactNode }

export function MessageBubble({ role, children }: Props) {
  return (
    <div className={cx('flex gap-3', role === 'user' && 'justify-end')}>
      <div className={cx(
        'max-w-[80%] rounded-lg p-3',
        role === 'user' ? 'bg-slate-900 text-white' : 'bg-white border'
      )}>
        {children}
      </div>
    </div>
  )
}
```

`MessageList.tsx`:
```tsx
import type { ReactNode } from 'react'

interface Props { children: ReactNode }

export function MessageList({ children }: Props) {
  return <div className="space-y-4">{children}</div>
}
```

`ChatInput.tsx`:
```tsx
import { useState } from 'react'

interface Props { onSubmit: (q: string) => void; disabled?: boolean }

export function ChatInput({ onSubmit, disabled }: Props) {
  const [v, setV] = useState('')
  return (
    <form
      className="flex items-center gap-2 border-t bg-white p-3"
      onSubmit={(e) => {
        e.preventDefault()
        const q = v.trim()
        if (!q) return
        onSubmit(q)
        setV('')
      }}
    >
      <input
        className="flex-1 rounded border px-3 py-2 text-sm focus:border-slate-900 focus:outline-none"
        placeholder="问点什么…  例如：看一下 default.orders 按天的订单金额趋势"
        value={v}
        onChange={(e) => setV(e.target.value)}
        disabled={disabled}
      />
      <button
        type="submit"
        className="rounded bg-slate-900 px-4 py-2 text-sm font-medium text-white hover:bg-slate-800 disabled:opacity-50"
        disabled={disabled}
      >
        发送
      </button>
    </form>
  )
}
```

`pages/Chat.tsx`:
```tsx
import { useEffect, useState } from 'react'
import { ask } from '../api/sse'
import { createSession, listDatasources } from '../api/client'
import type { ChartEvent, IntentEvent, RowsEvent, SkillCallEvent, Uuid } from '../api/types'
import { MessageBubble } from '../components/chat/MessageBubble'
import { MessageList } from '../components/chat/MessageList'
import { AssistantPanel } from '../components/chat/AssistantPanel'
import { ChatInput } from '../components/chat/ChatInput'

interface Turn {
  user: string
  intent?: IntentEvent
  skillCall?: SkillCallEvent
  rows?: RowsEvent
  chart?: ChartEvent
  summary?: string
  error?: string
  done?: boolean
}

export default function Chat() {
  const [datasourceId, setDatasourceId] = useState<Uuid | null>(null)
  const [datasources, setDatasources] = useState<{ id: Uuid; name: string }[]>([])
  const [sessionId, setSessionId] = useState<Uuid | null>(null)
  const [turns, setTurns] = useState<Turn[]>([])
  const [pending, setPending] = useState(false)

  useEffect(() => {
    listDatasources().then((ds) => {
      setDatasources(ds.map((d) => ({ id: d.id, name: d.name })))
      if (ds.length > 0) setDatasourceId(ds[0].id)
    }).catch(() => {})
  }, [])

  useEffect(() => {
    if (!datasourceId) return
    createSession({ user_id: 'web', datasource_id: datasourceId, title: '新会话' })
      .then((s) => setSessionId(s.id))
      .catch(() => {})
  }, [datasourceId])

  function send(q: string) {
    if (!sessionId) return
    setPending(true)
    setTurns((t) => [...t, { user: q }])
    const idx = turns.length
    ask({ session_id: sessionId, question: q }, (ev) => {
      setTurns((curr) => {
        const next = [...curr]
        const cur = next[idx] ?? { user: q }
        switch (ev.event) {
          case 'intent': cur.intent = ev as IntentEvent; break
          case 'skill_call': cur.skillCall = ev as SkillCallEvent; break
          case 'rows': cur.rows = ev as RowsEvent; break
          case 'chart': cur.chart = ev as ChartEvent; break
          case 'summary': cur.summary = (ev as any).text; break
          case 'error': cur.error = (ev as any).message; break
          case 'done': cur.done = true; break
        }
        next[idx] = cur
        return next
      })
    }, () => setPending(false))
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b bg-white px-4 py-2 text-sm">
        <span className="text-slate-600">数据源：</span>
        <select
          className="rounded border px-2 py-1"
          value={datasourceId ?? ''}
          onChange={(e) => setDatasourceId(e.target.value || null)}
        >
          {datasources.map((d) => (
            <option key={d.id} value={d.id}>{d.name}</option>
          ))}
        </select>
        {sessionId && <span className="text-slate-400">session={sessionId.slice(0, 8)}…</span>}
      </div>
      <div className="flex-1 space-y-4 overflow-auto p-4">
        <MessageList>
          {turns.map((t, i) => (
            <div key={i} className="space-y-3">
              <MessageBubble role="user">{t.user}</MessageBubble>
              <MessageBubble role="assistant">
                {!t.done && !t.error && (
                  <div className="text-xs text-slate-500">
                    {!t.intent && '正在判定意图…'}
                    {t.intent && !t.skillCall && '正在选择 skill…'}
                    {t.skillCall && !t.rows && '正在执行 SQL…'}
                    {t.rows && !t.summary && '正在生成摘要…'}
                  </div>
                )}
                {t.intent?.kind === 'clarify' && (
                  <div className="text-sm text-amber-700">{t.intent.prompt}</div>
                )}
                {t.intent?.kind === 'reject' && (
                  <div className="text-sm text-rose-700">这个问题暂时不能回答：{t.intent.reason}</div>
                )}
                <AssistantPanel
                  skillCall={t.skillCall}
                  rows={t.rows}
                  chart={t.chart}
                  summary={t.summary}
                />
                {t.error && <div className="text-sm text-rose-700">出错了：{t.error}</div>}
              </MessageBubble>
            </div>
          ))}
        </MessageList>
      </div>
      <ChatInput onSubmit={send} disabled={pending || !sessionId} />
    </div>
  )
}
```

- [ ] **Step 3：main.tsx 接 Chat**

```tsx
import Chat from './pages/Chat'
// ... 把 <Route path="chat" element={<div>chat (Task 3)</div>} /> 替换为：
<Route path="chat" element={<Chat />} />
```

- [ ] **Step 4：build 验证**

```
cd D:\workspase\rust\sqlai\web
npm run build 2>&1 | Select-Object -Last 10
```

- [ ] **Step 5：commit**

```
cd D:\workspase\rust\sqlai
git add web/src
git commit -m "feat(web): chat page with SSE-driven assistant panel (SQL/Table/Chart/Summary)"
```

---

## Task 4：Admin 页面（datasource / term / metric / few-shot）

**Files:**
- Create: `web/src/pages/Admin.tsx`
- Create: `web/src/components/admin/{DatasourceTab,TermTab,MetricTab,FewShotTab}.tsx`
- Modify: `web/src/main.tsx`（嵌套路由）

- [ ] **Step 1：Admin.tsx + 嵌套 tabs**

`pages/Admin.tsx`:
```tsx
import { NavLink, Outlet } from 'react-router-dom'

const tabs = [
  { to: 'datasources', label: '数据源' },
  { to: 'terms',       label: '业务词表' },
  { to: 'metrics',     label: '指标' },
  { to: 'few-shots',   label: 'Few-shot' },
]

export default function Admin() {
  return (
    <div className="flex h-full">
      <aside className="w-44 border-r bg-white p-3">
        <div className="space-y-1">
          {tabs.map((t) => (
            <NavLink
              key={t.to}
              to={t.to}
              className={({ isActive }) =>
                'block rounded px-3 py-1.5 text-sm ' +
                (isActive ? 'bg-slate-900 text-white' : 'text-slate-700 hover:bg-slate-100')
              }
            >
              {t.label}
            </NavLink>
          ))}
        </div>
      </aside>
      <section className="flex-1 overflow-auto p-4">
        <Outlet />
      </section>
    </div>
  )
}
```

`components/admin/DatasourceTab.tsx`:
```tsx
import { useEffect, useState } from 'react'
import { createDatasource, listDatasources } from '../../api/client'
import type { Datasource } from '../../api/types'

export function DatasourceTab() {
  const [list, setList] = useState<Datasource[]>([])
  const [form, setForm] = useState({
    name: '', kind: 'clickhouse', host: '127.0.0.1', port: 8123, db: 'default',
    user_name: 'admin', secret_ref: 'env:CLICKHOUSE_PASSWORD',
  })
  const [err, setErr] = useState<string | null>(null)

  async function refresh() {
    try { setList(await listDatasources()) } catch (e: any) { setErr(String(e?.message ?? e)) }
  }
  useEffect(() => { refresh() }, [])

  async function submit() {
    try {
      await createDatasource(form)
      setErr(null)
      refresh()
    } catch (e: any) { setErr(e?.response?.data?.error?.message ?? String(e?.message ?? e)) }
  }

  return (
    <div className="space-y-4">
      <h2 className="text-lg font-semibold">数据源</h2>
      <div className="rounded border bg-white p-3 text-sm">
        <div className="grid grid-cols-2 gap-2">
          <Input label="name" value={form.name} onChange={(v) => setForm({ ...form, name: v })} />
          <Input label="host" value={form.host} onChange={(v) => setForm({ ...form, host: v })} />
          <Input label="port" value={String(form.port)} onChange={(v) => setForm({ ...form, port: Number(v) || 0 })} />
          <Input label="db" value={form.db} onChange={(v) => setForm({ ...form, db: v })} />
          <Input label="user_name" value={form.user_name} onChange={(v) => setForm({ ...form, user_name: v })} />
          <Input label="secret_ref" value={form.secret_ref} onChange={(v) => setForm({ ...form, secret_ref: v })} />
        </div>
        <div className="mt-2 flex items-center gap-3">
          <button onClick={submit} className="rounded bg-slate-900 px-3 py-1 text-white">添加 / 更新</button>
          {err && <span className="text-rose-700">{err}</span>}
        </div>
      </div>
      <table className="min-w-full rounded border bg-white text-sm">
        <thead className="bg-slate-100"><tr>
          <Th>name</Th><Th>kind</Th><Th>host:port</Th><Th>db</Th><Th>user</Th>
        </tr></thead>
        <tbody>
          {list.map((d) => (
            <tr key={d.id} className="border-t">
              <Td>{d.name}</Td><Td>{d.kind}</Td><Td>{d.host}:{d.port}</Td><Td>{d.db}</Td><Td>{d.user_name}</Td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function Input({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="text-xs text-slate-600">
      <div>{label}</div>
      <input className="mt-1 w-full rounded border px-2 py-1 text-sm" value={value} onChange={(e) => onChange(e.target.value)} />
    </label>
  )
}
function Th({ children }: { children: React.ReactNode }) { return <th className="px-3 py-2 text-left">{children}</th> }
function Td({ children }: { children: React.ReactNode }) { return <td className="px-3 py-1.5">{children}</td> }
```

`components/admin/TermTab.tsx`:
```tsx
import { useEffect, useState } from 'react'
import { deleteTerm, listTerms, upsertTerm } from '../../api/client'
import type { BusinessTerm } from '../../api/types'

export function TermTab() {
  const [list, setList] = useState<BusinessTerm[]>([])
  const [form, setForm] = useState({ term: '', aliases: '', definition: '', formula: '' })
  const [err, setErr] = useState<string | null>(null)

  async function refresh() { try { setList(await listTerms()) } catch (e: any) { setErr(String(e)) } }
  useEffect(() => { refresh() }, [])

  async function submit() {
    try {
      await upsertTerm({
        term: form.term,
        aliases: form.aliases.split(',').map((s) => s.trim()).filter(Boolean),
        definition: form.definition,
        formula: form.formula || undefined,
      })
      setErr(null); setForm({ term: '', aliases: '', definition: '', formula: '' }); refresh()
    } catch (e: any) { setErr(e?.response?.data?.error?.message ?? String(e)) }
  }
  async function rm(t: string) { try { await deleteTerm(t); refresh() } catch (e: any) { setErr(String(e)) } }

  return (
    <div className="space-y-4">
      <h2 className="text-lg font-semibold">业务词表</h2>
      <div className="rounded border bg-white p-3 text-sm space-y-2">
        <input className="w-full rounded border px-2 py-1" placeholder="术语 (例：GMV)" value={form.term} onChange={(e) => setForm({ ...form, term: e.target.value })} />
        <input className="w-full rounded border px-2 py-1" placeholder="别名（逗号分隔）" value={form.aliases} onChange={(e) => setForm({ ...form, aliases: e.target.value })} />
        <textarea className="w-full rounded border px-2 py-1" placeholder="定义" rows={2} value={form.definition} onChange={(e) => setForm({ ...form, definition: e.target.value })} />
        <input className="w-full rounded border px-2 py-1" placeholder="公式（可选，例：sum(amount) where status='paid'）" value={form.formula} onChange={(e) => setForm({ ...form, formula: e.target.value })} />
        <div className="flex items-center gap-3">
          <button onClick={submit} className="rounded bg-slate-900 px-3 py-1 text-white">保存</button>
          {err && <span className="text-rose-700">{err}</span>}
        </div>
      </div>
      <table className="min-w-full rounded border bg-white text-sm">
        <thead className="bg-slate-100"><tr><th className="px-3 py-2 text-left">term</th><th className="px-3 py-2 text-left">aliases</th><th className="px-3 py-2 text-left">definition</th><th></th></tr></thead>
        <tbody>
          {list.map((t) => (
            <tr key={t.id} className="border-t">
              <td className="px-3 py-1.5 font-medium">{t.term}</td>
              <td className="px-3 py-1.5 text-slate-500">{t.aliases.join(', ')}</td>
              <td className="px-3 py-1.5">{t.definition}</td>
              <td className="px-3 py-1.5 text-right">
                <button className="text-rose-600 hover:underline" onClick={() => rm(t.term)}>删除</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
```

`components/admin/MetricTab.tsx`:
```tsx
import { useEffect, useState } from 'react'
import { deleteMetric, listMetrics, upsertMetric } from '../../api/client'
import type { MetricDef } from '../../api/types'

export function MetricTab() {
  const [list, setList] = useState<MetricDef[]>([])
  const [form, setForm] = useState({ name: '', dimension_keys: '', measure_sql: '', owner: '' })
  const [err, setErr] = useState<string | null>(null)

  async function refresh() { try { setList(await listMetrics()) } catch { /* ignore */ } }
  useEffect(() => { refresh() }, [])

  async function submit() {
    try {
      await upsertMetric({
        name: form.name,
        dimension_keys: form.dimension_keys.split(',').map((s) => s.trim()).filter(Boolean),
        measure_sql: form.measure_sql,
        owner: form.owner || undefined,
      })
      setErr(null); setForm({ name: '', dimension_keys: '', measure_sql: '', owner: '' }); refresh()
    } catch (e: any) { setErr(e?.response?.data?.error?.message ?? String(e)) }
  }
  async function rm(n: string) { try { await deleteMetric(n); refresh() } catch { /* ignore */ } }

  return (
    <div className="space-y-4">
      <h2 className="text-lg font-semibold">指标定义</h2>
      <div className="rounded border bg-white p-3 text-sm space-y-2">
        <input className="w-full rounded border px-2 py-1" placeholder="name (例：daily_gmv)" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
        <input className="w-full rounded border px-2 py-1" placeholder="dimensions（逗号分隔，例：date,channel）" value={form.dimension_keys} onChange={(e) => setForm({ ...form, dimension_keys: e.target.value })} />
        <input className="w-full rounded border px-2 py-1" placeholder="measure_sql (例：sum(amount))" value={form.measure_sql} onChange={(e) => setForm({ ...form, measure_sql: e.target.value })} />
        <input className="w-full rounded border px-2 py-1" placeholder="owner（可选）" value={form.owner} onChange={(e) => setForm({ ...form, owner: e.target.value })} />
        <div className="flex items-center gap-3">
          <button onClick={submit} className="rounded bg-slate-900 px-3 py-1 text-white">保存</button>
          {err && <span className="text-rose-700">{err}</span>}
        </div>
      </div>
      <table className="min-w-full rounded border bg-white text-sm">
        <thead className="bg-slate-100"><tr><th className="px-3 py-2 text-left">name</th><th className="px-3 py-2 text-left">dimensions</th><th className="px-3 py-2 text-left">measure_sql</th><th className="px-3 py-2 text-left">owner</th><th></th></tr></thead>
        <tbody>
          {list.map((m) => (
            <tr key={m.id} className="border-t">
              <td className="px-3 py-1.5 font-medium">{m.name}</td>
              <td className="px-3 py-1.5 text-slate-500">{m.dimension_keys.join(', ')}</td>
              <td className="px-3 py-1.5 font-mono text-xs">{m.measure_sql}</td>
              <td className="px-3 py-1.5">{m.owner ?? ''}</td>
              <td className="px-3 py-1.5 text-right">
                <button className="text-rose-600 hover:underline" onClick={() => rm(m.name)}>删除</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
```

`components/admin/FewShotTab.tsx`:
```tsx
import { useEffect, useState } from 'react'
import { deleteFewShot, listFewShots, voteFewShot } from '../../api/client'
import type { FewShot } from '../../api/types'

export function FewShotTab() {
  const [list, setList] = useState<FewShot[]>([])

  async function refresh() { try { setList(await listFewShots()) } catch { /* ignore */ } }
  useEffect(() => { refresh() }, [])

  async function vote(id: string, delta: number) {
    try { await voteFewShot(id, delta); refresh() } catch { /* ignore */ }
  }
  async function rm(id: string) { try { await deleteFewShot(id); refresh() } catch { /* ignore */ } }

  return (
    <div className="space-y-4">
      <h2 className="text-lg font-semibold">Few-shot</h2>
      <div className="text-xs text-slate-500">
        通过 POST <code>/api/admin/few-shots</code> 入库（例如把已采纳的对话写入）；这里只展示与投票/删除。
      </div>
      <table className="min-w-full rounded border bg-white text-sm">
        <thead className="bg-slate-100"><tr>
          <th className="px-3 py-2 text-left">question</th>
          <th className="px-3 py-2 text-left">sql</th>
          <th className="px-3 py-2 text-left">vote</th>
          <th></th>
        </tr></thead>
        <tbody>
          {list.map((f) => (
            <tr key={f.id} className="border-t">
              <td className="px-3 py-1.5 max-w-xs">{f.question}</td>
              <td className="px-3 py-1.5 font-mono text-xs"><code>{f.sql_text.slice(0, 120)}{f.sql_text.length > 120 ? '…' : ''}</code></td>
              <td className="px-3 py-1.5">{f.vote}</td>
              <td className="px-3 py-1.5 text-right space-x-2">
                <button className="rounded border px-2 py-0.5 text-xs" onClick={() => vote(f.id, +1)}>👍</button>
                <button className="rounded border px-2 py-0.5 text-xs" onClick={() => vote(f.id, -1)}>👎</button>
                <button className="rounded border border-rose-300 px-2 py-0.5 text-xs text-rose-700" onClick={() => rm(f.id)}>删除</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
```

- [ ] **Step 2：main.tsx 嵌套路由**

```tsx
import Admin from './pages/Admin'
import { DatasourceTab } from './components/admin/DatasourceTab'
import { TermTab } from './components/admin/TermTab'
import { MetricTab } from './components/admin/MetricTab'
import { FewShotTab } from './components/admin/FewShotTab'

// 路由替换：
<Route path="admin" element={<Admin />}>
  <Route index element={<DatasourceTab />} />
  <Route path="datasources" element={<DatasourceTab />} />
  <Route path="terms" element={<TermTab />} />
  <Route path="metrics" element={<MetricTab />} />
  <Route path="few-shots" element={<FewShotTab />} />
</Route>
```

- [ ] **Step 3：build + commit**

```
cd D:\workspase\rust\sqlai\web
npm run build 2>&1 | Select-Object -Last 10
cd ..
git add web/src
git commit -m "feat(web): admin pages (datasource / business-term / metric / few-shot)"
```

---

## Task 5：前端 Dockerfile + docker-compose 集成

**Files:**
- Create: `web/Dockerfile`
- Create: `web/nginx.conf`
- Modify: `docker-compose.yml`

- [ ] **Step 1：Dockerfile**

```dockerfile
# 多阶段：build → nginx
FROM m.daocloud.io/docker.io/library/node:20-alpine AS build

WORKDIR /opt/web

# 复用 npm 中国镜像加速（境外用户可改回默认）
RUN npm config set registry https://registry.npmmirror.com

COPY package.json package-lock.json* ./
RUN npm install

COPY tsconfig.json tsconfig.node.json vite.config.ts index.html ./
COPY src ./src

RUN npm run build


FROM m.daocloud.io/docker.io/library/nginx:alpine

COPY --from=build /opt/web/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
```

- [ ] **Step 2：nginx.conf**

```nginx
server {
    listen 80;
    server_name _;

    root /usr/share/nginx/html;
    index index.html;

    # SPA fallback
    location / {
        try_files $uri /index.html;
    }

    # API 转发到后端容器（docker-compose 的 sqlai-api，需要后端服务化为容器；
    # 暂时让前端容器也接受 /api 反代到 host.docker.internal:8080）
    location /api/ {
        proxy_pass http://host.docker.internal:8080;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_buffering off;     # SSE 必须关闭缓冲
        proxy_read_timeout 600s;
    }
}
```

- [ ] **Step 3：compose 加 frontend 服务**

在 `docker-compose.yml` `services:` 下追加：

```yaml
  frontend:
    build: ./web
    container_name: sqlai-web
    ports:
      - "80:80"
    extra_hosts:
      - "host.docker.internal:host-gateway"
```

- [ ] **Step 4：build 镜像（Windows + 国内网络可能慢；预算 5-10 分钟）**

```
docker compose build frontend 2>&1 | Select-Object -Last 15
```

- [ ] **Step 5：commit**

```
git add web/Dockerfile web/nginx.conf docker-compose.yml
git commit -m "chore(devenv): containerize web frontend with nginx + reverse proxy to api"
```

---

## 验收清单

- [ ] `cd web && npm run build` ✅ 输出 `dist/`，无 TS 错误
- [ ] `cd web && npm run dev` 起来后，浏览器打开 `http://localhost:5173/chat`，能看到数据源下拉、问答输入框
- [ ] 后端跑（`cargo run -p sqlai-api`）+ 前端 dev → 真实问答跑通：输入"看一下 default.orders 按天的订单金额趋势" → 看到 SQL/表/折线图/摘要
- [ ] 进入 `/admin/terms` 创建一条业务词表 → 列表显示
- [ ] `/admin/few-shots` 投票按钮工作
- [ ] `docker compose up -d frontend` 后 `http://localhost/` 可用
- [ ] `git log` 至少 5 条本子计划 commit

v1.0 全部完成。
