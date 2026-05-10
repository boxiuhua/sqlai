import { NavLink, Outlet } from 'react-router-dom'

const tabs = [
  { to: 'datasources', label: '数据源', en: 'Datasources' },
  { to: 'terms',       label: '业务词表', en: 'Glossary' },
  { to: 'metrics',     label: '指标', en: 'Metrics' },
  { to: 'few-shots',   label: 'Few-shot', en: 'Examples' },
]

export default function Admin() {
  return (
    <div className="flex h-full bg-canvas">
      <aside className="w-56 border-r border-rule bg-paper/60 px-4 py-6">
        <div className="mb-4 px-3 text-[10px] uppercase tracking-[0.22em] text-mute">
          运营 · Admin
        </div>
        <nav className="space-y-0.5">
          {tabs.map((t) => (
            <NavLink
              key={t.to}
              to={t.to}
              className={({ isActive }) =>
                'group flex items-baseline justify-between rounded px-3 py-2 text-[13px] transition-colors ' +
                (isActive
                  ? 'bg-ink text-canvas'
                  : 'text-soft hover:bg-deep/60 hover:text-ink')
              }
            >
              {({ isActive }) => (
                <>
                  <span className="font-medium">{t.label}</span>
                  <span
                    className={
                      'text-[10px] uppercase tracking-[0.18em] ' +
                      (isActive ? 'text-canvas/60' : 'text-mute')
                    }
                  >
                    {t.en}
                  </span>
                </>
              )}
            </NavLink>
          ))}
        </nav>
      </aside>
      <section className="relative flex-1 overflow-auto">
        <div className="mx-auto max-w-[1100px] space-y-6 px-8 py-8">
          <Outlet />
        </div>
      </section>
    </div>
  )
}
