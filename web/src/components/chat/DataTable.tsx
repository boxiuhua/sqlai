interface Props {
  columns: string[]
  rows: any[]
}

export function DataTable({ columns, rows }: Props) {
  if (columns.length === 0 || rows.length === 0) {
    return (
      <div className="px-5 py-4 text-[13px] text-mute">
        无结果行
      </div>
    )
  }
  return (
    <div className="overflow-auto">
      <table className="min-w-full text-[13px]">
        <thead>
          <tr className="border-b border-rule">
            {columns.map((c) => (
              <th
                key={c}
                className="px-5 py-2.5 text-left font-medium text-[10px] uppercase tracking-[0.16em] text-mute"
              >
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.slice(0, 200).map((r, i) => (
            <tr
              key={i}
              className="group border-b border-rule/60 last:border-0 hover:bg-deep/40"
            >
              {columns.map((c) => (
                <td key={c} className="tabular px-5 py-2 align-baseline text-soft">
                  {fmt(r?.[c])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length > 200 && (
        <div className="border-t border-rule bg-deep/40 px-5 py-1.5 text-[11px] tracking-wide text-mute">
          仅展示前 200 / {rows.length} 行
        </div>
      )}
    </div>
  )
}

function fmt(v: any): string {
  if (v == null) return '—'
  if (typeof v === 'object') return JSON.stringify(v)
  if (typeof v === 'number') {
    return v.toLocaleString(undefined, { maximumFractionDigits: 4 })
  }
  return String(v)
}
