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
  | { event: 'metrics_recommend' } // 后端目前永远返回空数组；payload 形状未冻结，等真实 metric 推荐做完再补类型
