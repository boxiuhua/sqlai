import { useState } from 'react'

interface Props {
  onSubmit: (q: string) => void
  disabled?: boolean
}

export function ChatInput({ onSubmit, disabled }: Props) {
  const [v, setV] = useState('')
  return (
    <form
      className="border-t border-rule bg-paper px-4 py-3"
      onSubmit={(e) => {
        e.preventDefault()
        const q = v.trim()
        if (!q) return
        onSubmit(q)
        setV('')
      }}
    >
      <div className="flex items-stretch gap-2 rounded-md border border-strong bg-canvas focus-within:border-vermillion focus-within:shadow-[0_0_0_4px_rgba(184,52,27,0.12)] transition-shadow">
        <span aria-hidden className="display flex items-center pl-4 text-[15px] text-mute">
          ?
        </span>
        <input
          className="flex-1 bg-transparent py-3 pr-2 text-[14px] text-ink placeholder:text-mute focus:outline-none"
          placeholder="问点什么…  例如：看一下 default.orders 按天的订单金额趋势"
          value={v}
          onChange={(e) => setV(e.target.value)}
          disabled={disabled}
        />
        <button
          type="submit"
          className="m-1 rounded bg-ink px-5 text-[12px] font-medium uppercase tracking-[0.18em] text-canvas transition-colors hover:bg-vermillion disabled:cursor-not-allowed disabled:opacity-40"
          disabled={disabled}
        >
          发送
        </button>
      </div>
    </form>
  )
}
