import { useEffect, useState } from 'react'
import { deleteTerm, listTerms, upsertTerm } from '../../api/client'
import type { BusinessTerm } from '../../api/types'
import {
  Cell,
  ConfirmDelete,
  DataGrid,
  ErrorBanner,
  Field,
  FormCard,
  PageHeader,
  PrimaryButton,
  TextArea,
  TextInput,
} from './ui'

export function TermTab() {
  const [list, setList] = useState<BusinessTerm[]>([])
  const [form, setForm] = useState({
    term: '',
    aliases: '',
    definition: '',
    formula: '',
  })
  const [err, setErr] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function refresh() {
    try {
      setList(await listTerms())
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }
  useEffect(() => {
    refresh()
  }, [])

  async function submit() {
    if (!form.term.trim() || !form.definition.trim()) {
      setErr('term 与 definition 不能为空')
      return
    }
    setBusy(true)
    try {
      await upsertTerm({
        term: form.term,
        aliases: form.aliases
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        definition: form.definition,
        formula: form.formula || undefined,
      })
      setErr(null)
      setForm({ term: '', aliases: '', definition: '', formula: '' })
      await refresh()
    } catch (e: any) {
      setErr(e?.response?.data?.error?.message ?? String(e?.message ?? e))
    } finally {
      setBusy(false)
    }
  }

  async function rm(t: string) {
    try {
      await deleteTerm(t)
      await refresh()
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }

  return (
    <div className="space-y-5">
      <PageHeader
        title="业务词表"
        caption="Glossary · Embedded for retrieval"
        count={list.length}
      />

      <FormCard title="新增 / 更新（按 term 名上覆盖；服务端会自动 embed）">
        <div className="space-y-3">
          <ErrorBanner message={err} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <Field label="term" hint="唯一名">
              <TextInput
                value={form.term}
                onChange={(e) => setForm({ ...form, term: e.target.value })}
                placeholder="GMV"
              />
            </Field>
            <Field label="aliases" hint="逗号分隔">
              <TextInput
                value={form.aliases}
                onChange={(e) => setForm({ ...form, aliases: e.target.value })}
                placeholder="成交额, 总成交"
              />
            </Field>
          </div>
          <Field label="definition">
            <TextArea
              rows={2}
              value={form.definition}
              onChange={(e) =>
                setForm({ ...form, definition: e.target.value })
              }
              placeholder="已支付订单金额合计"
            />
          </Field>
          <Field label="formula" hint="可选；SQL 片段">
            <TextInput
              value={form.formula}
              onChange={(e) => setForm({ ...form, formula: e.target.value })}
              placeholder="SUM(amount) WHERE status='paid'"
            />
          </Field>
          <div className="pt-1">
            <PrimaryButton onClick={submit} disabled={busy}>
              {busy ? '保存中…' : '保存'}
            </PrimaryButton>
          </div>
        </div>
      </FormCard>

      <DataGrid
        columns={[
          { key: 'term', label: 'term' },
          { key: 'aliases', label: 'aliases' },
          { key: 'def', label: 'definition' },
          { key: 'formula', label: 'formula' },
          { key: 'op', label: '', align: 'right' },
        ]}
        empty="暂无业务词表条目；上面填一条"
        rows={list.map((t) => (
          <tr
            key={t.id}
            className="border-b border-rule/60 last:border-0 hover:bg-deep/40"
          >
            <Cell className="text-ink font-medium">{t.term}</Cell>
            <Cell mute>{t.aliases.join(', ') || '—'}</Cell>
            <Cell>{t.definition}</Cell>
            <Cell mono mute>
              {t.formula ?? '—'}
            </Cell>
            <Cell align="right">
              <ConfirmDelete onConfirm={() => rm(t.term)} />
            </Cell>
          </tr>
        ))}
      />
    </div>
  )
}
