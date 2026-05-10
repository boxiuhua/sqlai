import { useEffect, useState } from 'react'
import { ask } from '../api/sse'
import { createSession, listDatasources, listMessages } from '../api/client'
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
  assistantMessageId?: string
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
          case 'done':
            cur.done = true
            // SSE 流末尾后端会持久化 assistant 消息；拉一次找最后一条 assistant 取 id
            // 用于把 CSV 导出链接接到 AssistantPanel。
            listMessages(sessionId)
              .then((msgs) => {
                const asst = [...msgs].reverse().find((m) => m.role === 'assistant')
                if (!asst) return
                setTurns((curr2) => {
                  const next2 = [...curr2]
                  if (next2[idx]) {
                    next2[idx] = { ...next2[idx], assistantMessageId: asst.id }
                  }
                  return next2
                })
              })
              .catch(() => {})
            break
        }
        next[idx] = cur
        return next
      })
    }, () => setPending(false))
  }

  const stage = (t: Turn): string => {
    if (t.done || t.error) return ''
    if (!t.intent) return '判定意图'
    if (!t.skillCall) return '选择 skill'
    if (!t.rows) return '执行 SQL'
    if (!t.summary) return '生成摘要'
    return ''
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-rule bg-paper/70 backdrop-blur">
        <div className="mx-auto flex max-w-[1200px] items-baseline justify-between px-6 py-2.5">
          <div className="flex items-baseline gap-3 text-[12px]">
            <span className="text-[10px] uppercase tracking-[0.2em] text-mute">datasource</span>
            <select
              className="rounded border border-rule bg-paper px-2 py-1 font-mono text-[12px] text-ink focus:border-vermillion focus:outline-none"
              value={datasourceId ?? ''}
              onChange={(e) => setDatasourceId(e.target.value || null)}
            >
              {datasources.map((d) => (
                <option key={d.id} value={d.id}>{d.name}</option>
              ))}
            </select>
          </div>
          {sessionId && (
            <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-mute">
              session · {sessionId.slice(0, 8)}
            </span>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-auto">
        <div className="mx-auto max-w-[1100px] space-y-8 px-6 py-8">
          {turns.length === 0 && (
            <EmptyState onPick={send} disabled={pending || !sessionId} />
          )}
          <MessageList>
            {turns.map((t, i) => (
              <article key={i} className="space-y-4">
                <MessageBubble role="user">{t.user}</MessageBubble>
                <MessageBubble role="assistant">
                  {stage(t) && <Stage label={stage(t)} />}
                  {t.intent?.kind === 'clarify' && (
                    <div className="rounded-md border border-ochre/40 bg-ochre/5 px-4 py-3 text-[13px] text-cocoa">
                      <span className="text-[10px] uppercase tracking-[0.2em] text-ochre">需要澄清 · </span>
                      {t.intent.prompt}
                    </div>
                  )}
                  {t.intent?.kind === 'reject' && (
                    <div className="rounded-md border border-vermillion/40 bg-vermillion/5 px-4 py-3 text-[13px] text-vermillion">
                      <span className="text-[10px] uppercase tracking-[0.2em]">已拒绝 · </span>
                      {t.intent.reason}
                    </div>
                  )}
                  <AssistantPanel
                    messageId={t.assistantMessageId}
                    skillCall={t.skillCall}
                    rows={t.rows}
                    chart={t.chart}
                    summary={t.summary}
                    done={t.done}
                    onFollowup={send}
                    followupDisabled={pending || !sessionId}
                  />
                  {t.error && (
                    <div className="rounded-md border border-vermillion/40 bg-vermillion/5 px-4 py-3 text-[13px] text-vermillion">
                      <span className="text-[10px] uppercase tracking-[0.2em]">error · </span>
                      {t.error}
                    </div>
                  )}
                </MessageBubble>
              </article>
            ))}
          </MessageList>
        </div>
      </div>
      <ChatInput onSubmit={send} disabled={pending || !sessionId} />
    </div>
  )
}

function Stage({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 text-[11px] text-mute">
      <span className="relative inline-flex h-1.5 w-1.5">
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-vermillion/60" />
        <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-vermillion" />
      </span>
      <span className="uppercase tracking-[0.18em]">{label}</span>
    </div>
  )
}

function EmptyState({
  onPick,
  disabled,
}: {
  onPick: (q: string) => void
  disabled?: boolean
}) {
  const examples = [
    '看一下 default.orders 按天的订单金额趋势',
    'default.orders 销售额最高的前 5 个商品',
    '对比 1 月与 2 月 default.orders 总金额',
    '未来 7 天 default.orders 销售额预估',
  ]
  return (
    <div className="rise mx-auto max-w-[680px] py-10 text-center">
      <div aria-hidden className="display text-[64px] leading-none text-vermillion/20">
        Σ
      </div>
      <h1 className="display mt-4 text-[28px] leading-tight text-ink">
        用一句话问数据。
      </h1>
      <p className="mt-2 text-[13px] text-soft">
        sqlai 会自动检索 schema · 选择分析 skill · 校验并执行 SQL · 推回表格 / 图表 / 摘要
      </p>
      <p className="mt-4 text-[10px] uppercase tracking-[0.2em] text-mute">
        点击下方任一示例直接发送
      </p>
      <div className="mt-3 grid grid-cols-1 gap-2 text-left sm:grid-cols-2">
        {examples.map((q) => (
          <button
            key={q}
            type="button"
            disabled={disabled}
            onClick={() => onPick(q)}
            className="group flex items-start gap-2 rounded-md border border-rule bg-paper px-4 py-3 text-left text-[13px] text-soft transition-all hover:-translate-y-px hover:border-vermillion/60 hover:text-ink hover:shadow-[0_8px_20px_-12px_rgba(184,52,27,0.35)] disabled:cursor-not-allowed disabled:opacity-50"
          >
            <span aria-hidden className="display text-vermillion/70 transition-transform group-hover:translate-x-0.5">
              ›
            </span>
            <span className="flex-1">{q}</span>
          </button>
        ))}
      </div>
    </div>
  )
}
