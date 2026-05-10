/**
 * 回答完后给业务用户的"推荐询问"——侧重"换个角度看同一张表"，
 * 而不是在原 skill 里钻得更深。
 *
 * 设计原则：3 条都跨到不同 skill，让用户视野从一条线扩展到一张面。
 */

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

export function recommendations(skill: string, args: Args): string[] {
  const db = String(args.db ?? 'default')
  const table = String(args.table ?? '')
  const fq = table ? `${db}.${table}` : db

  switch (skill) {
    case 'metric_overview':
      return [
        `${fq} 销售额 / 销量 Top 5`,
        `${fq} 按渠道 / 品类的占比`,
        `${fq} 与上一周期对比`,
      ]

    case 'topn':
      return [
        `${fq} 整体随时间的趋势`,
        `${fq} 各项的占比构成`,
        `${fq} 上一周期的 Top 对比`,
      ]

    case 'compare_period':
      return [
        `${fq} 整体趋势`,
        `${fq} Top 5 维度`,
        `${fq} 各维度占比构成`,
      ]

    case 'share_breakdown':
      return [
        `${fq} 各项的时间趋势`,
        `${fq} Top N 排行`,
        `${fq} 与上一周期占比对比`,
      ]

    case 'trend_segment':
      return [
        `${fq} 整体走势`,
        `${fq} 各维度占比`,
        `${fq} 维度差异归因`,
      ]

    case 'drill_down':
      return [
        `差异最大维度的时间趋势`,
        `${fq} 各维度占比构成`,
        `${fq} Top N 项`,
      ]

    case 'correlation_matrix':
      return [
        `${fq} 各列分别的趋势`,
        `${fq} 这些列的分布对比`,
        `${fq} 用这些列做聚类（K-means）`,
      ]

    case 'distribution_shift':
      return [
        `${fq} 整体趋势`,
        `${fq} Top 5 项`,
        `${fq} 时段平均值对比`,
      ]

    case 'forecast_simple':
      return [
        `${fq} 历史同期对比`,
        `${fq} 按维度的趋势`,
        `${fq} 主要构成的占比`,
      ]

    case 'cluster_kmeans':
      return [
        `${fq} 各簇的指标分布`,
        `${fq} 用这些特征做分类预测`,
        `${fq} 特征之间的相关性`,
      ]

    case 'classify_logreg':
      return [
        `${fq} 特征值分布`,
        `${fq} 特征之间的相关性`,
        `${fq} 用这些特征做 K-means 聚类`,
      ]

    default:
      return [
        `${fq} 整体趋势`,
        `${fq} Top 5`,
        `${fq} 占比构成`,
      ]
  }
}
