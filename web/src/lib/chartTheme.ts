/**
 * 图表主题：editorial financial paper。
 * 不用 ECharts 默认配色；衬线刻度 + 朱红/钴蓝/赭石 5 色，柔和阴影 + 虚线网格。
 */

const PALETTE = ['#B8341B', '#2C4F7C', '#B8893E', '#5C7252', '#6B4226', '#8E867B']

const SANS = '"IBM Plex Sans", "PingFang SC", "Microsoft YaHei", system-ui, sans-serif'
const SERIF = '"Fraunces Variable", "Songti SC", "Noto Serif SC", serif'

const TOOLTIP = {
  backgroundColor: 'rgba(255,255,255,0.97)',
  borderColor: '#E5DDCB',
  borderWidth: 1,
  padding: [10, 14],
  textStyle: { color: '#1F1A14', fontSize: 12, fontFamily: SANS },
  extraCssText: 'box-shadow: 0 12px 32px -12px rgba(31,26,20,0.20);',
}

function baseAxes(xs: string[]) {
  return {
    grid: { left: 56, right: 28, top: 40, bottom: 40, containLabel: false },
    xAxis: {
      type: 'category' as const,
      data: xs,
      axisLine: { lineStyle: { color: '#C8BDA5' } },
      axisTick: { show: false },
      axisLabel: {
        color: '#4B4640',
        fontSize: 11,
        fontFamily: SANS,
        rotate: xs.length > 12 ? 30 : 0,
        margin: 10,
      },
    },
    yAxis: {
      type: 'value' as const,
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: '#E5DDCB', type: 'dashed' as const } },
      axisLabel: {
        color: '#8E867B',
        fontSize: 11,
        fontFamily: SERIF,
      },
    },
  }
}

interface Spec {
  kind: 'bar' | 'line' | 'pie' | 'none'
  x?: string
  y?: string
}

export function buildChartOption(spec: Spec, rows: any[]): any | null {
  if (spec.kind === 'none' || !spec.x || !spec.y || rows.length === 0) return null
  const x = spec.x
  const y = spec.y
  const xs = rows.map((r) => String(r[x] ?? ''))
  const ys = rows.map((r) => Number(r[y] ?? 0))

  if (spec.kind === 'bar') {
    return {
      color: PALETTE,
      textStyle: { fontFamily: SANS },
      tooltip: { trigger: 'axis', axisPointer: { type: 'line', lineStyle: { color: '#C8BDA5', type: 'dashed' } }, ...TOOLTIP },
      ...baseAxes(xs),
      series: [
        {
          type: 'bar',
          data: ys,
          barWidth: '54%',
          itemStyle: {
            color: {
              type: 'linear',
              x: 0, y: 0, x2: 0, y2: 1,
              colorStops: [
                { offset: 0, color: '#C9462A' },
                { offset: 1, color: '#8B2412' },
              ],
            },
            borderRadius: [4, 4, 0, 0],
            shadowColor: 'rgba(184,52,27,0.18)',
            shadowBlur: 12,
            shadowOffsetY: 4,
          },
          emphasis: {
            itemStyle: {
              color: {
                type: 'linear',
                x: 0, y: 0, x2: 0, y2: 1,
                colorStops: [
                  { offset: 0, color: '#D86B57' },
                  { offset: 1, color: '#B8341B' },
                ],
              },
            },
          },
          animationDuration: 700,
          animationEasing: 'cubicOut',
        },
      ],
    }
  }

  if (spec.kind === 'line') {
    return {
      color: PALETTE,
      textStyle: { fontFamily: SANS },
      tooltip: { trigger: 'axis', axisPointer: { type: 'line', lineStyle: { color: '#C8BDA5', type: 'dashed' } }, ...TOOLTIP },
      ...baseAxes(xs),
      series: [
        {
          type: 'line',
          data: ys,
          smooth: true,
          symbol: 'circle',
          symbolSize: 7,
          showSymbol: false,
          lineStyle: { color: PALETTE[0], width: 2.5, cap: 'round', join: 'round' },
          itemStyle: {
            color: PALETTE[0],
            borderColor: '#FFFFFF',
            borderWidth: 2,
          },
          areaStyle: {
            color: {
              type: 'linear',
              x: 0, y: 0, x2: 0, y2: 1,
              colorStops: [
                { offset: 0, color: 'rgba(184,52,27,0.20)' },
                { offset: 0.6, color: 'rgba(184,52,27,0.06)' },
                { offset: 1, color: 'rgba(184,52,27,0.00)' },
              ],
            },
          },
          emphasis: { focus: 'series', scale: 1.08 },
          animationDuration: 900,
          animationEasing: 'cubicOut',
        },
      ],
    }
  }

  // pie => doughnut with center number
  const total = ys.reduce((a, b) => a + b, 0)
  return {
    color: PALETTE,
    textStyle: { fontFamily: SANS },
    tooltip: { trigger: 'item', ...TOOLTIP },
    legend: {
      bottom: 0,
      icon: 'circle',
      itemWidth: 8,
      itemHeight: 8,
      itemGap: 14,
      textStyle: { color: '#4B4640', fontSize: 11, fontFamily: SANS },
    },
    series: [
      {
        type: 'pie',
        radius: ['58%', '78%'],
        center: ['50%', '46%'],
        avoidLabelOverlap: true,
        label: {
          show: true,
          position: 'center',
          formatter: () => `{title|总和}\n{value|${formatNumber(total)}}`,
          rich: {
            title: {
              color: '#8E867B',
              fontSize: 11,
              fontFamily: SANS,
              letterSpacing: 1,
              padding: [0, 0, 6, 0],
            },
            value: {
              color: '#1F1A14',
              fontSize: 28,
              fontFamily: SERIF,
              fontWeight: 600,
            },
          },
        },
        labelLine: { show: false },
        itemStyle: {
          borderColor: '#FFFFFF',
          borderWidth: 3,
        },
        emphasis: {
          scale: true,
          scaleSize: 8,
          itemStyle: { shadowBlur: 16, shadowColor: 'rgba(31,26,20,0.20)' },
        },
        data: rows.map((r) => ({
          name: String(r[x]),
          value: Number(r[y]),
        })),
      },
    ],
  }
}

function formatNumber(n: number): string {
  if (!isFinite(n)) return String(n)
  const abs = Math.abs(n)
  if (abs >= 1e9) return (n / 1e9).toFixed(2) + 'B'
  if (abs >= 1e6) return (n / 1e6).toFixed(2) + 'M'
  if (abs >= 1e4) return (n / 1e4).toFixed(2) + '万'
  return n.toLocaleString(undefined, { maximumFractionDigits: 2 })
}
