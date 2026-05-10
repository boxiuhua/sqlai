import type { ChartEvent, RowsEvent, SkillCallEvent } from '../../api/types'
import { csvExportUrl } from '../../api/client'
import { recommendations } from '../../lib/recommendations'
import { miningRecommendations } from '../../lib/miningRecommendations'
import { SqlPanel } from './SqlPanel'
import { DataTable } from './DataTable'
import { ChartView } from './ChartView'

interface Props {
  messageId?: string
  skillCall?: SkillCallEvent | null
  rows?: RowsEvent | null
  chart?: ChartEvent | null
  summary?: string | null
  done?: boolean
  onFollowup?: (q: string) => void
  followupDisabled?: boolean
}

export function AssistantPanel({
  messageId,
  skillCall,
  rows,
  chart,
  summary,
  done,
  onFollowup,
  followupDisabled,
}: Props) {
  const showFollowups = done && skillCall && onFollowup
  const suggestions = showFollowups
    ? recommendations(skillCall.skill, skillCall.args ?? {})
    : []
  const mining = showFollowups
    ? miningRecommendations(skillCall.skill, skillCall.args ?? {})
    : []

  return (
    <div className="space-y-5">
      {summary && (
        <figure className="rise relative overflow-hidden rounded-md border border-rule bg-paper px-6 py-5 shadow-[0_1px_0_rgba(31,26,20,0.04)]">
          <div
            aria-hidden
            className="display pointer-events-none absolute -left-1 -top-3 select-none text-[64px] leading-none text-vermillion/15"
          >
            “
          </div>
          <blockquote className="display pl-7 text-[17px] leading-relaxed text-ink">
            {summary}
          </blockquote>
          <figcaption className="mt-2 pl-7 text-[10px] uppercase tracking-[0.2em] text-mute">
            BI summary · auto-generated
          </figcaption>
        </figure>
      )}

      {chart && rows && (
        <ChartView spec={{ kind: chart.kind, x: chart.x, y: chart.y }} rows={rows.rows} />
      )}

      {rows && (
        <section className="rise rounded-md border border-rule bg-paper shadow-[0_1px_0_rgba(31,26,20,0.04)]">
          <header className="flex items-baseline justify-between border-b border-rule px-5 py-3">
            <div className="flex items-baseline gap-3">
              <span className="display text-[15px] text-ink">{rows.label}</span>
              <span className="text-[11px] tabular text-mute">
                {rows.rows.length} 行 · {rows.columns.length} 列
                {rows.truncated && <span className="ml-1 text-vermillion">· 已截断</span>}
              </span>
            </div>
            {messageId && (
              <a
                href={csvExportUrl(messageId)}
                className="group inline-flex items-center gap-1.5 text-[11px] tracking-wide text-soft hover:text-vermillion"
                download
              >
                <span aria-hidden className="transition-transform group-hover:translate-y-px">↓</span>
                导出 CSV
              </a>
            )}
          </header>
          <DataTable columns={rows.columns} rows={rows.rows} />
        </section>
      )}

      {skillCall && (
        <details className="rise group rounded-md border border-rule bg-deep/60 px-4 py-3 open:bg-deep">
          <summary className="flex cursor-pointer list-none items-center justify-between text-[12px] text-soft">
            <span className="flex items-baseline gap-2">
              <span className="text-[10px] uppercase tracking-[0.18em] text-mute">skill</span>
              <code className="font-mono text-[12px] text-vermillion">{skillCall.skill}</code>
              {skillCall.plan?.explanation && (
                <span className="text-soft">· {skillCall.plan.explanation}</span>
              )}
            </span>
            <span className="text-mute transition-transform group-open:rotate-90">›</span>
          </summary>
          <div className="mt-3 space-y-2">
            {skillCall.plan?.steps?.map((s, i) => (
              <SqlPanel key={i} sql={s.sql} label={s.label} />
            ))}
          </div>
        </details>
      )}

      {showFollowups && suggestions.length > 0 && (
        <div className="rise space-y-2">
          <div className="flex items-baseline gap-2 text-[10px] uppercase tracking-[0.2em] text-mute">
            <span aria-hidden className="block h-px w-6 bg-strong" />
            <span>推荐询问 · you may also ask</span>
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
            {suggestions.map((q, i) => (
              <button
                key={`${q}-${i}`}
                type="button"
                disabled={followupDisabled}
                onClick={() => onFollowup!(q)}
                className="group flex items-start gap-2 rounded-md border border-rule bg-paper px-3.5 py-2.5 text-left text-[12.5px] leading-snug text-soft transition-all hover:-translate-y-px hover:border-vermillion/60 hover:text-ink hover:shadow-[0_8px_18px_-12px_rgba(184,52,27,0.30)] disabled:cursor-not-allowed disabled:opacity-50"
              >
                <span aria-hidden className="display text-vermillion/70 transition-transform group-hover:translate-x-0.5">
                  ›
                </span>
                <span className="flex-1">{q}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {showFollowups && mining.length > 0 && (
        <div className="rise space-y-2">
          <div className="flex items-baseline gap-2 text-[10px] uppercase tracking-[0.2em] text-mute">
            <span aria-hidden className="block h-px w-6 bg-cobalt/60" />
            <span>数据挖掘 · mining</span>
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
            {mining.map((m, i) => (
              <button
                key={`${m.kind}-${i}`}
                type="button"
                disabled={followupDisabled}
                onClick={() => onFollowup!(m.question)}
                className="group flex flex-col gap-1.5 rounded-md border border-rule bg-paper px-3.5 py-2.5 text-left transition-all hover:-translate-y-px hover:border-cobalt/60 hover:shadow-[0_8px_18px_-12px_rgba(44,79,124,0.30)] disabled:cursor-not-allowed disabled:opacity-50"
              >
                <span className="inline-flex w-fit items-center gap-1 rounded border border-cobalt/30 bg-cobalt/5 px-1.5 py-0.5 text-[10px] uppercase tracking-[0.16em] text-cobalt">
                  <span aria-hidden>◇</span>
                  {m.label}
                </span>
                <span className="text-[12.5px] leading-snug text-soft group-hover:text-ink">
                  {m.question}
                </span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
