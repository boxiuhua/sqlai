import { useEffect, useState } from 'react'
import { deleteMetric, listMetrics, upsertMetric } from '../../api/client'
import type { MetricDef } from '../../api/types'
import {
  Cell,
  ConfirmDelete,
  DataGrid,
  ErrorBanner,
  Field,
  FormCard,
  PageHeader,
  PrimaryButton,
  TextInput,
} from './ui'

export function MetricTab() {
  const [list, setList] = useState<MetricDef[]>([])
  const [form, setForm] = useState({
    name: '',
    dimension_keys: '',
    measure_sql: '',
    owner: '',
  })
  const [err, setErr] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function refresh() {
    try {
      setList(await listMetrics())
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }
  useEffect(() => {
    refresh()
  }, [])

  async function submit() {
    if (!form.name.trim() || !form.measure_sql.trim()) {
      setErr('name 与 measure_sql 不能为空')
      return
    }
    setBusy(true)
    try {
      await upsertMetric({
        name: form.name,
        dimension_keys: form.dimension_keys
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        measure_sql: form.measure_sql,
        owner: form.owner || undefined,
      })
      setErr(null)
      setForm({ name: '', dimension_keys: '', measure_sql: '', owner: '' })
      await refresh()
    } catch (e: any) {
      setErr(e?.response?.data?.error?.message ?? String(e?.message ?? e))
    } finally {
      setBusy(false)
    }
  }

  async function rm(n: string) {
    try {
      await deleteMetric(n)
      await refresh()
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }

  return (
    <div className="space-y-5">
      <PageHeader
        title="指标定义"
        caption="Metric registry · Owner-managed"
        count={list.length}
      />

      <FormCard title="新增 / 更新（按 name 上覆盖）">
        <div className="space-y-3">
          <ErrorBanner message={err} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <Field label="name" hint="唯一名">
              <TextInput
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                placeholder="daily_gmv"
              />
            </Field>
            <Field label="owner" hint="可选">
              <TextInput
                value={form.owner}
                onChange={(e) => setForm({ ...form, owner: e.target.value })}
                placeholder="data-team"
              />
            </Field>
          </div>
          <Field label="dimension_keys" hint="逗号分隔">
            <TextInput
              value={form.dimension_keys}
              onChange={(e) =>
                setForm({ ...form, dimension_keys: e.target.value })
              }
              placeholder="date, channel"
            />
          </Field>
          <Field label="measure_sql">
            <TextInput
              value={form.measure_sql}
              onChange={(e) =>
                setForm({ ...form, measure_sql: e.target.value })
              }
              placeholder="sum(amount)"
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
          { key: 'name', label: 'name' },
          { key: 'dims', label: 'dimensions' },
          { key: 'sql', label: 'measure_sql' },
          { key: 'owner', label: 'owner' },
          { key: 'op', label: '', align: 'right' },
        ]}
        empty="暂无指标；上面填一条"
        rows={list.map((m) => (
          <tr
            key={m.id}
            className="border-b border-rule/60 last:border-0 hover:bg-deep/40"
          >
            <Cell className="text-ink font-medium">{m.name}</Cell>
            <Cell mute>{m.dimension_keys.join(', ') || '—'}</Cell>
            <Cell mono>{m.measure_sql}</Cell>
            <Cell mute>{m.owner ?? '—'}</Cell>
            <Cell align="right">
              <ConfirmDelete onConfirm={() => rm(m.name)} />
            </Cell>
          </tr>
        ))}
      />
    </div>
  )
}
