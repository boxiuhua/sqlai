/**
 * 数据挖掘推荐：在"BI 探索"基础上额外提供 ML / 统计型推荐。
 *
 * 推荐策略：根据刚命中的 skill + 参数，推断哪些挖掘类 skill 在这张表 / 维度上
 * 是有意义的下一步，并给出可直接发问的中文问句。
 */

export type MiningKind =
  | 'forecast'
  | 'cluster'
  | 'classify'
  | 'correlation'
  | 'distribution'
  | 'anomaly'
  | 'drilldown'

export interface MiningChip {
  kind: MiningKind
  label: string
  question: string
}

interface Args {
  db?: string
  table?: string
  dimension?: string
  date_column?: string
  measure_sql?: string
  feature_columns?: string[]
  label_column?: string
  granularity?: string
  [k: string]: unknown
}

const KIND_LABEL: Record<MiningKind, string> = {
  forecast: '预测',
  cluster: '聚类',
  classify: '分类',
  correlation: '相关性',
  distribution: '分布对比',
  anomaly: '异常检测',
  drilldown: '归因拆解',
}

export function miningRecommendations(skill: string, args: Args): MiningChip[] {
  const db = String(args.db ?? 'default')
  const table = String(args.table ?? '')
  const fq = table ? `${db}.${table}` : db
  const dim = typeof args.dimension === 'string' ? args.dimension : '主要维度'
  const date = typeof args.date_column === 'string' ? args.date_column : 'date_column'
  const gran = typeof args.granularity === 'string' ? args.granularity : 'day'
  const horizonText = gran === 'day' ? '7 天' : gran === 'week' ? '4 周' : '3 个月'
  const periodText = gran === 'day' ? '本周与上周' : gran === 'week' ? '本月与上月' : '今年与去年'

  const all: MiningChip[] = []

  // forecast：所有时间序列类 skill 都可以接外推
  if (['metric_overview', 'trend_segment', 'compare_period', 'forecast_simple'].includes(skill)) {
    all.push({
      kind: 'forecast',
      label: KIND_LABEL.forecast,
      question: `${fq} 按${gran}外推未来${horizonText}的趋势`,
    })
  }

  // distribution shift：时间窗口对比
  if (['metric_overview', 'compare_period', 'topn', 'forecast_simple'].includes(skill)) {
    all.push({
      kind: 'distribution',
      label: KIND_LABEL.distribution,
      question: `${fq} ${periodText}的金额分布对比`,
    })
  }

  // correlation：多数值列时
  if (['metric_overview', 'topn', 'share_breakdown', 'trend_segment', 'compare_period'].includes(skill)) {
    all.push({
      kind: 'correlation',
      label: KIND_LABEL.correlation,
      question: `${fq} 各数值列之间的相关性矩阵`,
    })
  }

  // cluster：有维度可分群
  if (['topn', 'share_breakdown', 'metric_overview', 'compare_period'].includes(skill)) {
    all.push({
      kind: 'cluster',
      label: KIND_LABEL.cluster,
      question: `对 ${fq} 的数值特征做 K-means 分 3 群`,
    })
  }

  // classify：聚类或归因后做监督预测
  if (['cluster_kmeans', 'drill_down', 'correlation_matrix', 'distribution_shift'].includes(skill)) {
    all.push({
      kind: 'classify',
      label: KIND_LABEL.classify,
      question: `${fq} 用现有数值特征做逻辑回归分类`,
    })
  }

  // 归因：看完趋势 / 对比 / 占比，往往想问"为什么"
  if (['compare_period', 'metric_overview', 'share_breakdown'].includes(skill)) {
    all.push({
      kind: 'drilldown',
      label: KIND_LABEL.drilldown,
      question: `${fq} 按 ${dim} 拆解差异最大的维度组合`,
    })
  }

  // 异常：用 distribution_shift 在某个数值列上做 IQR / 3σ 风格的异常
  if (['metric_overview', 'topn', 'forecast_simple'].includes(skill)) {
    all.push({
      kind: 'anomaly',
      label: KIND_LABEL.anomaly,
      question: `${fq} 找出近期金额异常（超过 P95）的记录`,
    })
  }

  // 兜底：任何 skill 都至少给 forecast / correlation / cluster
  if (all.length === 0) {
    all.push(
      { kind: 'forecast', label: KIND_LABEL.forecast, question: `${fq} 未来一周走势预估` },
      { kind: 'correlation', label: KIND_LABEL.correlation, question: `${fq} 数值列相关性` },
      { kind: 'cluster', label: KIND_LABEL.cluster, question: `${fq} 做 K-means 分群` },
    )
  }

  // 去重 + 限 3 条
  const seen = new Set<MiningKind>()
  const out: MiningChip[] = []
  for (const c of all) {
    if (seen.has(c.kind)) continue
    seen.add(c.kind)
    out.push(c)
    if (out.length === 3) break
  }
  // 同时确保 date 不会被 lint 警告为 unused
  void date
  return out
}
