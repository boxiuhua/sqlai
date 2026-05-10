import { Outlet, NavLink } from 'react-router-dom'

export default function App() {
  return (
    <div className="relative z-10 flex h-full flex-col bg-canvas">
      <header className="border-b border-rule bg-paper/80 backdrop-blur">
        <div className="mx-auto flex max-w-[1200px] items-end justify-between px-6 py-3">
          <div className="flex items-baseline gap-3">
            <span className="display text-[22px] font-semibold leading-none text-ink">
              sqlai
            </span>
            <span className="hidden h-4 w-px bg-strong sm:block" />
            <span className="hidden text-[10px] uppercase tracking-[0.28em] text-mute sm:block">
              智能问数 · BI Companion
            </span>
          </div>
          <nav className="flex items-baseline gap-1">
            {[
              { to: '/chat', label: '问答', en: 'Ask' },
              { to: '/admin', label: '运营', en: 'Admin' },
            ].map((t) => (
              <NavLink
                key={t.to}
                to={t.to}
                className={({ isActive }) =>
                  'group relative px-3 py-1 text-[13px] transition-colors ' +
                  (isActive ? 'text-vermillion' : 'text-soft hover:text-ink')
                }
              >
                {({ isActive }) => (
                  <>
                    <span className="font-medium">{t.label}</span>
                    <span className="ml-1 text-[10px] uppercase tracking-[0.2em] text-mute">
                      {t.en}
                    </span>
                    {isActive && (
                      <span
                        aria-hidden
                        className="absolute -bottom-3 left-3 right-3 h-px bg-vermillion"
                      />
                    )}
                  </>
                )}
              </NavLink>
            ))}
          </nav>
        </div>
      </header>
      <main className="relative flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  )
}
