import { useEffect, useState } from 'react'
import { deleteFewShot, listFewShots, voteFewShot } from '../../api/client'
import type { FewShot } from '../../api/types'
import {
  Cell,
  ConfirmDelete,
  DataGrid,
  ErrorBanner,
  GhostButton,
  PageHeader,
} from './ui'

export function FewShotTab() {
  const [list, setList] = useState<FewShot[]>([])
  const [err, setErr] = useState<string | null>(null)

  async function refresh() {
    try {
      setList(await listFewShots())
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }
  useEffect(() => {
    refresh()
  }, [])

  async function vote(id: string, delta: number) {
    try {
      await voteFewShot(id, delta)
      await refresh()
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }
  async function rm(id: string) {
    try {
      await deleteFewShot(id)
      await refresh()
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }

  return (
    <div className="space-y-5">
      <PageHeader
        title="Few-shot"
        caption="Vote-curated examples for retrieval"
        count={list.length}
      />

      <ErrorBanner message={err} />

      <div className="rounded-md border border-rule bg-paper px-5 py-3 text-[12px] text-soft">
        <span className="text-[10px] uppercase tracking-[0.2em] text-mute">
          说明 ·{' '}
        </span>
        Few-shot 示例通过{' '}
        <code className="font-mono">POST /api/admin/few-shots</code> 入库（一般在采纳一次对话时插入）；这里展示已有例子，支持 👍/👎 投票与删除。投票分 ≥ 0 的会进入检索注入到 LLM prompt。
      </div>

      <DataGrid
        columns={[
          { key: 'q', label: 'question' },
          { key: 'sql', label: 'sql' },
          { key: 'vote', label: 'vote', align: 'right' },
          { key: 'op', label: '', align: 'right' },
        ]}
        empty="暂无 few-shot；可在采纳一次对话后通过 API 入库"
        rows={list.map((f) => (
          <tr
            key={f.id}
            className="border-b border-rule/60 last:border-0 hover:bg-deep/40"
          >
            <Cell className="max-w-[280px] text-ink">{f.question}</Cell>
            <Cell mono mute className="max-w-[420px]">
              <span className="block truncate" title={f.sql_text}>
                {f.sql_text}
              </span>
            </Cell>
            <Cell align="right" className="font-mono">
              <VoteBadge n={f.vote} />
            </Cell>
            <Cell align="right">
              <div className="inline-flex items-center gap-1">
                <GhostButton onClick={() => vote(f.id, 1)} title="赞同">
                  👍
                </GhostButton>
                <GhostButton onClick={() => vote(f.id, -1)} title="反对">
                  👎
                </GhostButton>
                <ConfirmDelete onConfirm={() => rm(f.id)} />
              </div>
            </Cell>
          </tr>
        ))}
      />
    </div>
  )
}

function VoteBadge({ n }: { n: number }) {
  const tone =
    n > 0
      ? 'bg-sage/10 text-sage border-sage/30'
      : n < 0
        ? 'bg-vermillion/10 text-vermillion border-vermillion/30'
        : 'bg-deep text-mute border-rule'
  return (
    <span
      className={
        'inline-flex min-w-[2.2em] justify-center rounded border px-2 py-0.5 text-[11px] tabular ' +
        tone
      }
    >
      {n > 0 ? `+${n}` : n}
    </span>
  )
}
