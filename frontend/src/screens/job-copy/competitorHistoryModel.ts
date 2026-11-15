/**
 * 「競合の推移」：競合調査の月ごとの記録（HubSpot の求人の competitor_monthly）を、月ごとの行にする。
 * 競合の件数・競合の給与の中央値（下限）・その月に記録した自社の給与・その月の自社の応募を並べるだけで、
 * 高い・低い・良い・悪いの判定は付けない（並んだ月に、ほかの事情も動いているため）。
 */
import type { CompetitorMonth } from '../../generated/CompetitorMonth';
import { competitorUrl } from '../competitor/listingLink';
import type { JobCopyRecord } from './data';

export interface CompetitorRow {
  month: string;
  /** 記録した日（YYYY-MM-DD）。 */
  date: string;
  total: number;
  /** 競合の給与の中央値（下限）。読めなかった月は null。 */
  median: number | null;
  /** その月に記録した自社の給与（下限）。読めなかった月は null。 */
  own: number | null;
  /** "万円" か "円/時"。 */
  unit: string;
  /** その月の自社の応募。応募の日別データが無いとき null。 */
  applications: number | null;
  /** 前の月の記録と違う条件の名前（検索の言葉・地域・上位の件数・給与の単位）。最初の月は空。 */
  changed: string[];
  entry: CompetitorMonth;
}

/** 2026-11 → 2026年11月 */
export const monthJa = (month: string) => `${month.slice(0, 4)}年${String(Number(month.slice(5, 7)))}月`;

/** 23.5 → 「23.5万円」、1250 → 「1,250円」。値が無ければ「—」。 */
export function salaryText(value: number | null | undefined, unit: string): string {
  if (value === null || value === undefined) return '—';
  return unit === '万円' ? `${value.toLocaleString('ja-JP', { maximumFractionDigits: 2 })}万円` : `${Math.round(value).toLocaleString('ja-JP')}円`;
}

const region = (entry: CompetitorMonth) => `${entry.conditions.prefecture ?? ''}${entry.conditions.municipality ?? ''}`;
/** 条件の 1 行（出来事と報告で使う）。 */
export function conditionsText(entry: CompetitorMonth): string {
  return [`検索の言葉「${entry.conditions.title}」`, `地域：${region(entry) || '全国'}`, `上位 ${String(entry.conditions.top_n)}件`, entry.conditions.source].join(' ／ ');
}

function changedConditions(previous: CompetitorMonth, current: CompetitorMonth): string[] {
  const changed: string[] = [];
  if (previous.conditions.title !== current.conditions.title) changed.push('検索の言葉');
  if (region(previous) !== region(current)) changed.push('地域');
  if (previous.conditions.top_n !== current.conditions.top_n) changed.push('上位の件数');
  if (previous.salary.unit !== current.salary.unit) changed.push('給与の単位');
  return changed;
}

/** 月ごとの行（古い月から）。応募は、応募日がその月に入る応募の数（重複を除いた、求人の画面と同じ数え方）。 */
export function competitorRows(job: Pick<JobCopyRecord, 'competitorMonthly' | 'overallApplications'>): CompetitorRow[] {
  const entries = [...(job.competitorMonthly ?? [])].sort((a, b) => a.month.localeCompare(b.month));
  const byDate = job.overallApplications?.byDate;
  return entries.map((entry, index) => {
    const previous = entries[index - 1];
    return {
      month: entry.month, date: entry.run_at.slice(0, 10), total: entry.count.total,
      median: entry.salary.all.min_median, own: entry.own_salary?.min ?? null, unit: entry.salary.unit,
      applications: byDate ? Object.entries(byDate).reduce((sum, [date, count]) => date.startsWith(`${entry.month}-`) ? sum + count : sum, 0) : null,
      changed: previous ? changedConditions(previous, entry) : [],
      entry,
    };
  });
}

/** 「この求人で競合調査をする」の行き先。HubSpot の求人だけ（番号が数字）。それ以外は null。 */
export function competitorEntryUrl(job: Pick<JobCopyRecord, 'hubspotId' | 'title' | 'listingPrefecture' | 'listingMunicipality' | 'listingSummary'>): string | null {
  if (!job.hubspotId || !/^\d+$/.test(job.hubspotId)) return null;
  return competitorUrl({
    id: job.hubspotId, name: job.title, prefecture: job.listingPrefecture, municipality: job.listingMunicipality,
    jobName: job.listingSummary?.occupation ?? job.listingSummary?.job_kind ?? null,
  });
}
