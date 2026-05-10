import { useEffect, useState } from 'react'
import { createDatasource, listDatasources } from '../../api/client'
import type { Datasource } from '../../api/types'
import {
  Cell,
  DataGrid,
  ErrorBanner,
  Field,
  FormCard,
  PageHeader,
  PrimaryButton,
  TextInput,
} from './ui'

export function DatasourceTab() {
  const [list, setList] = useState<Datasource[]>([])
  const [form, setForm] = useState({
    name: '',
    kind: 'clickhouse',
    host: '127.0.0.1',
    port: 8123,
    db: 'default',
    user_name: 'admin',
    secret_ref: 'env:CLICKHOUSE_PASSWORD',
  })
  const [err, setErr] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function refresh() {
    try {
      setList(await listDatasources())
    } catch (e: any) {
      setErr(String(e?.message ?? e))
    }
  }
  useEffect(() => {
    refresh()
  }, [])

  async function submit() {
    if (!form.name.trim()) {
      setErr('name 不能为空')
      return
    }
    setBusy(true)
    try {
      await createDatasource(form)
      setErr(null)
      await refresh()
    } catch (e: any) {
      setErr(e?.response?.data?.error?.message ?? String(e?.message ?? e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="space-y-5">
      <PageHeader title="数据源" caption="ClickHouse Connections" count={list.length} />

      <FormCard title="新增 / 更新（按 name 上覆盖）">
        <div className="space-y-3">
          <ErrorBanner message={err} />
          <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
            <Field label="name" hint="唯一标识">
              <TextInput
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                placeholder="ch_local"
              />
            </Field>
            <Field label="kind">
              <TextInput
                value={form.kind}
                onChange={(e) => setForm({ ...form, kind: e.target.value })}
                placeholder="clickhouse"
              />
            </Field>
            <Field label="db">
              <TextInput
                value={form.db}
                onChange={(e) => setForm({ ...form, db: e.target.value })}
              />
            </Field>
            <Field label="host">
              <TextInput
                value={form.host}
                onChange={(e) => setForm({ ...form, host: e.target.value })}
                placeholder="host.docker.internal"
              />
            </Field>
            <Field label="port">
              <TextInput
                value={String(form.port)}
                onChange={(e) =>
                  setForm({ ...form, port: Number(e.target.value) || 0 })
                }
                inputMode="numeric"
              />
            </Field>
            <Field label="user_name">
              <TextInput
                value={form.user_name}
                onChange={(e) =>
                  setForm({ ...form, user_name: e.target.value })
                }
              />
            </Field>
            <div className="md:col-span-3">
              <Field label="secret_ref" hint="env:VAR_NAME 形式（避免明文密码）">
                <TextInput
                  value={form.secret_ref}
                  onChange={(e) =>
                    setForm({ ...form, secret_ref: e.target.value })
                  }
                />
              </Field>
            </div>
          </div>
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
          { key: 'kind', label: 'kind' },
          { key: 'host', label: 'host : port' },
          { key: 'db', label: 'db' },
          { key: 'user', label: 'user' },
          { key: 'time', label: 'updated', align: 'right' },
        ]}
        empty="还没有数据源；上面填一条"
        rows={list.map((d) => (
          <tr
            key={d.id}
            className="border-b border-rule/60 last:border-0 hover:bg-deep/40"
          >
            <Cell mono className="text-ink font-medium">
              {d.name}
            </Cell>
            <Cell mute>{d.kind}</Cell>
            <Cell mono>
              {d.host}
              <span className="text-mute">:{d.port}</span>
            </Cell>
            <Cell mono>{d.db}</Cell>
            <Cell mono>{d.user_name}</Cell>
            <Cell align="right" mute>
              {fmtTime(d.updated_at)}
            </Cell>
          </tr>
        ))}
      />
    </div>
  )
}

function fmtTime(iso: string): string {
  try {
    const d = new Date(iso)
    return (
      d.getFullYear() +
      '-' +
      String(d.getMonth() + 1).padStart(2, '0') +
      '-' +
      String(d.getDate()).padStart(2, '0') +
      ' ' +
      String(d.getHours()).padStart(2, '0') +
      ':' +
      String(d.getMinutes()).padStart(2, '0')
    )
  } catch {
    return iso
  }
}
