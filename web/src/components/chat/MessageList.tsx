import type { ReactNode } from 'react'

interface Props { children: ReactNode }

export function MessageList({ children }: Props) {
  return <div className="space-y-4">{children}</div>
}
