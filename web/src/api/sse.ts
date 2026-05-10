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
