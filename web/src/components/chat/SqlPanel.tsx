import { lazy, Suspense, useState } from 'react'

const Editor = lazy(() => import('@monaco-editor/react'))

interface Props {
  sql: string
  label: string
}

export function SqlPanel({ sql, label }: Props) {
  const [open, setOpen] = useState(false)
  return (
    <div className="rounded border border-rule bg-paper">
      <button
        className="flex w-full items-center justify-between px-4 py-2 text-left text-[12px] tracking-wide text-soft hover:bg-deep/60"
        onClick={() => setOpen(!open)}
        type="button"
      >
        <span className="flex items-baseline gap-2">
          <span className="text-[10px] uppercase tracking-[0.18em] text-mute">sql</span>
          <span className="font-mono">{label}</span>
        </span>
        <span className="text-mute">{open ? '–' : '+'}</span>
      </button>
      {open && (
        <Suspense fallback={<div className="p-4 text-[12px] text-mute">loading editor…</div>}>
          <div className="border-t border-rule">
            <Editor
              height="220px"
              defaultLanguage="sql"
              value={sql}
              theme="vs"
              options={{
                readOnly: true,
                minimap: { enabled: false },
                fontSize: 13,
                fontFamily: '"IBM Plex Mono", "Courier New", monospace',
                wordWrap: 'on',
                lineNumbers: 'off',
                folding: false,
                scrollBeyondLastLine: false,
                renderLineHighlight: 'none',
                padding: { top: 12, bottom: 12 },
              }}
            />
          </div>
        </Suspense>
      )}
    </div>
  )
}
