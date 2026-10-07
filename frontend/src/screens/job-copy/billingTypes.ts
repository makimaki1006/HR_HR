import type { HrhPerformanceCollection } from './hrhPerformanceModel';

/**
 * 課金の 1 期間。課金CSVの取り込み (billingImport.ts) と HRハッカー実績 (hrh_performance) の
 * 両方をこの形にそろえ、タイムラインの課金レーンが読む。
 *
 * - 日付は JST の暦日 `YYYY-MM-DD`。期間終了日を含む (両端を含む)。
 * - 金額が空欄・未取得のときは `null`。0 円とは扱わない。
 * - 同じ求人で期間が重なる行は合算しない。重なりは `overlapsSourceRows` に残す。
 */
export interface BillingPeriod {
  /** 結びついた求人 (JobCopyRecord.id)。 */
  jobId: string;
  /** 媒体名。求人一覧と同じ表記 ('Airワーク' | 'HRハッカー')。 */
  media: BillingMedia;
  /** 媒体求人ID (JobCopyRecord.mediaJobId と完全一致したもの)。 */
  mediaJobId: string;
  periodStart: string;
  periodEnd: string;
  /** 円。空欄なら null (0 円とは区別する)。 */
  amountYen: number | null;
  taxBasis: BillingTaxBasis;
  planName: string | null;
  impressions: number | null;
  clicks: number | null;
  /** 媒体の管理画面に出る応募数。HubSpot の応募件数とは別物。 */
  mediaApplications: number | null;
  source: BillingSource;
  /** 課金CSVの行番号 (見出し行が 1)。HRハッカー実績では null。 */
  sourceRow: number | null;
  /** 同じ求人で期間が重なる、ほかの課金CSV行の行番号。 */
  overlapsSourceRows: number[];
}

export type BillingMedia = 'Airワーク' | 'HRハッカー';
export type BillingTaxBasis = '税込' | '税抜' | '不明';
/** csv: 画面で読み込んだ課金CSV (再読み込みで消える)。hrh_performance: HRハッカー実績 (取得済みデータ)。 */
export type BillingSource = 'csv' | 'hrh_performance';

/** 求人ごとの課金期間を開始日順で返す。 */
export function billingPeriodsForJob(periods: readonly BillingPeriod[], jobId: string): BillingPeriod[] {
  return periods.filter(period => period.jobId === jobId)
    .sort((a, b) => a.periodStart.localeCompare(b.periodStart) || a.periodEnd.localeCompare(b.periodEnd));
}

/** HRハッカー実績を BillingPeriod にそろえる。cost_yen が null なら amountYen も null。 */
export function billingPeriodsFromHrhPerformance(jobId: string, mediaJobId: string, performance: HrhPerformanceCollection | undefined): BillingPeriod[] {
  if (!performance) return [];
  return performance.rows.map(row => ({
    jobId, media: 'HRハッカー', mediaJobId,
    periodStart: row.period_start, periodEnd: row.period_end,
    amountYen: row.cost_yen, taxBasis: '不明', planName: null,
    impressions: row.impressions, clicks: row.clicks, mediaApplications: row.applications,
    source: 'hrh_performance', sourceRow: null, overlapsSourceRows: [],
  }));
}
