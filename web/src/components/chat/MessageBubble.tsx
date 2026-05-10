import type { ReactNode } from 'react'
import { cx } from '../../lib/classnames'

interface Props {
  role: 'user' | 'assistant'
  children: ReactNode
}

export function MessageBubble({ role, children }: Props) {
  if (role === 'user') {
    return (
      <div className="flex justify-end">
        <div className="max-w-[78%] rounded-md bg-ink px-4 py-2.5 text-[14px] leading-relaxed text-canvas shadow-[0_4px_12px_-6px_rgba(31,26,20,0.30)]">
          {children}
        </div>
      </div>
    )
  }
  return (
    <div className={cx('flex gap-3')}>
      <div
        aria-hidden
        className="display mt-1 hidden h-7 w-7 shrink-0 items-center justify-center rounded-full border border-rule bg-paper text-[13px] text-vermillion sm:flex"
      >
        Σ
      </div>
      <div className="min-w-0 flex-1 space-y-3">{children}</div>
    </div>
  )
}
