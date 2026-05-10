import ReactECharts from 'echarts-for-react'
import { buildChartOption } from '../../lib/chartTheme'

interface Props {
  spec: { kind: 'bar' | 'line' | 'pie' | 'none'; x?: string; y?: string }
  rows: any[]
}

export function ChartView({ spec, rows }: Props) {
  const option = buildChartOption(spec, rows)
  if (!option) return null
  const label =
    spec.kind === 'bar'
      ? 'bar chart'
      : spec.kind === 'line'
        ? 'line chart'
        : spec.kind === 'pie'
          ? 'doughnut'
          : ''
  return (
    <div className="rise rounded-md border border-rule bg-paper shadow-[0_1px_0_rgba(31,26,20,0.04),0_12px_28px_-18px_rgba(31,26,20,0.18)]">
      <div className="flex items-baseline justify-between border-b border-rule px-5 py-3">
        <span className="display text-[15px] tracking-wide text-ink">可视化</span>
        <span className="text-[10px] uppercase tracking-[0.18em] text-mute">{label}</span>
      </div>
      <div className="px-3 py-4">
        <ReactECharts option={option} style={{ height: 320 }} opts={{ renderer: 'canvas' }} />
      </div>
    </div>
  )
}
