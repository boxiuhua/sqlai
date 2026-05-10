import { useState, type ReactNode } from 'react'
import { cx } from '../../lib/classnames'

/* ----- 页面头：标题 + 副标题 + 计数 + 右侧操作槽 ----- */

export function PageHeader({
  title,
  caption,
  count,
  action,
}: {
  title: string
  caption?: string
  count?: number
  action?: ReactNode
}) {
  return (
    <div className="flex items-end justify-between border-b border-rule pb-3">
      <div className="flex items-baseline gap-3">
        <h2 className="display text-[26px] leading-none text-ink">{title}</h2>
        {typeof count === 'number' && (
          <span className="font-mono text-[11px] tabular text-mute">
            {count} 条
          </span>
        )}
        {caption && (
          <span className="hidden text-[10px] uppercase tracking-[0.2em] text-mute md:block">
            · {caption}
          </span>
        )}
      </div>
      {action}
    </div>
  )
}

/* ----- 表单分节 ----- */

export function FormCard({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="rounded-md border border-rule bg-paper shadow-[0_1px_0_rgba(31,26,20,0.04)]">
      {title && (
        <div className="border-b border-rule px-5 py-2.5 text-[10px] uppercase tracking-[0.22em] text-mute">
          {title}
        </div>
      )}
      <div className="p-5">{children}</div>
    </section>
  )
}

/* ----- 单个字段：上方小字 label + 输入 ----- */

export function Field({
  label,
  hint,
  children,
}: {
  label: string
  hint?: string
  children: ReactNode
}) {
  return (
    <label className="block">
      <div className="mb-1 flex items-baseline gap-2">
        <span className="text-[10px] uppercase tracking-[0.18em] text-mute">{label}</span>
        {hint && <span className="text-[11px] text-mute">· {hint}</span>}
      </div>
      {children}
    </label>
  )
}

/* ----- 文本输入（与 ChatInput 同语言） ----- */

export function TextInput(props: React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      className={cx(
        'w-full rounded border border-rule bg-canvas px-3 py-2 font-mono text-[13px] text-ink',
        'placeholder:text-mute placeholder:font-sans',
        'focus:border-vermillion focus:outline-none focus:shadow-[0_0_0_3px_rgba(184,52,27,0.10)]',
        'transition-shadow',
        props.className,
      )}
    />
  )
}

export function TextArea(props: React.TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      {...props}
      className={cx(
        'w-full rounded border border-rule bg-canvas px-3 py-2 text-[13px] text-ink',
        'placeholder:text-mute',
        'focus:border-vermillion focus:outline-none focus:shadow-[0_0_0_3px_rgba(184,52,27,0.10)]',
        'transition-shadow',
        props.className,
      )}
    />
  )
}

/* ----- 主按钮 ----- */

export function PrimaryButton(props: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        'inline-flex items-center gap-1.5 rounded bg-ink px-4 py-2 text-[12px] font-medium uppercase tracking-[0.16em] text-canvas',
        'transition-colors hover:bg-vermillion',
        'disabled:cursor-not-allowed disabled:opacity-40',
        props.className,
      )}
    />
  )
}

export function GhostButton(props: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        'inline-flex items-center gap-1 rounded border border-rule bg-paper px-2.5 py-1 text-[11px] tracking-wide text-soft',
        'transition-colors hover:border-strong hover:text-ink',
        'disabled:cursor-not-allowed disabled:opacity-40',
        props.className,
      )}
    />
  )
}

/* ----- 危险按钮：两段确认（第一次点变成"确认删除？"，再点才执行） ----- */

export function ConfirmDelete({
  onConfirm,
  label = '删除',
  confirmLabel = '确认删除？',
}: {
  onConfirm: () => void | Promise<void>
  label?: string
  confirmLabel?: string
}) {
  const [armed, setArmed] = useState(false)
  return (
    <button
      type="button"
      onClick={() => {
        if (!armed) {
          setArmed(true)
          window.setTimeout(() => setArmed(false), 2500)
        } else {
          onConfirm()
          setArmed(false)
        }
      }}
      className={cx(
        'rounded border px-2.5 py-1 text-[11px] tracking-wide transition-colors',
        armed
          ? 'border-vermillion bg-vermillion/10 text-vermillion'
          : 'border-rule text-soft hover:border-vermillion/60 hover:text-vermillion',
      )}
    >
      {armed ? confirmLabel : label}
    </button>
  )
}

/* ----- 错误提示行 ----- */

export function ErrorBanner({ message }: { message?: string | null }) {
  if (!message) return null
  return (
    <div className="rounded-md border border-vermillion/40 bg-vermillion/5 px-3 py-2 text-[12px] text-vermillion">
      <span className="text-[10px] uppercase tracking-[0.2em]">error · </span>
      {message}
    </div>
  )
}

/* ----- 表格容器（统一边框 + 头部小字） ----- */

export function DataGrid({
  columns,
  rows,
  empty,
}: {
  columns: { key: string; label: string; align?: 'left' | 'right'; className?: string }[]
  rows: ReactNode[]
  empty?: string
}) {
  if (rows.length === 0) {
    return (
      <div className="rounded-md border border-rule bg-paper px-5 py-8 text-center text-[13px] text-mute">
        {empty ?? '暂无数据'}
      </div>
    )
  }
  return (
    <div className="overflow-hidden rounded-md border border-rule bg-paper shadow-[0_1px_0_rgba(31,26,20,0.04)]">
      <table className="min-w-full text-[13px]">
        <thead>
          <tr className="border-b border-rule bg-deep/40">
            {columns.map((c) => (
              <th
                key={c.key}
                className={cx(
                  'px-5 py-2.5 text-[10px] uppercase tracking-[0.16em] text-mute',
                  c.align === 'right' ? 'text-right' : 'text-left',
                  c.className,
                )}
              >
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{rows}</tbody>
      </table>
    </div>
  )
}

export function Cell({
  children,
  align,
  mono,
  mute,
  className,
}: {
  children: ReactNode
  align?: 'left' | 'right'
  mono?: boolean
  mute?: boolean
  className?: string
}) {
  return (
    <td
      className={cx(
        'px-5 py-2.5 align-baseline',
        align === 'right' ? 'text-right tabular' : '',
        mono ? 'font-mono text-[12px]' : '',
        mute ? 'text-mute' : 'text-soft',
        className,
      )}
    >
      {children}
    </td>
  )
}
