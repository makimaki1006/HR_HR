/**
 * 「競合の推移」：競合調査の月ごとの記録（HubSpot の求人の competitor_monthly）を、月ごとの行にする。
 * 競合の件数・競合の給与の中央値（下限）・その月に記録した自社の給与・その月の自社の応募を並べるだけで、
 * 高い・低い・良い・悪いの判定は付けない（並んだ月に、ほかの事情も動いているため）。
 */
import type { CompetitorMonth } from '../../generated/CompetitorMonth';
import { competitorUrl } from '../competitor/listingLink';
import type { JobCopyRecord } from './data';
import { jstDate, todayJst } from './timelineModel';

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
  /** その月の自社の応募。応募日の分かる応募が無いとき null。 */
  applications: number | null;
  /** 応募を数えた日がその月の途中（月がまだ終わっていない）なら true。 */
  partial: boolean;
  /** 給与の中央値（下限）の元になった求人の件数。古い記録では null。 */
  salaryN: number | null;
  /** 競合の給与が、CSV 内の給与を月給に換算した値（PC の CSV など）。 */
  converted: boolean;
  /** 前の月の記録と違う条件の名前（職種名・地域・上位の件数・給与の単位・媒体・雇用形態）。最初の月は空。 */
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

/**
 * 調べた地域。都道府県を入力しなかった月は「全国」（記録の都道府県は、求人の中でいちばん多いもの）。
 * 入力したかが分からない古い記録は、記録の地域をそのまま出す。
 */
export function regionText(entry: CompetitorMonth): string {
  const { conditions } = entry;
  if (conditions.region_specified === false) return '全国（地域を指定していない）';
  return `${conditions.prefecture ?? ''}${conditions.municipality ?? ''}` || '全国';
}
/** 入力した職種名。古い記録・入力しなかった月は「指定なし」。 */
const jobNameText = (entry: CompetitorMonth) => entry.conditions.job_name ?? '指定なし';
/** 条件の 1 行（出来事と報告で使う）。 */
export function conditionsText(entry: CompetitorMonth): string {
  const { conditions } = entry;
  return [
    `職種名：${jobNameText(entry)}`,
    `地域：${regionText(entry)}`,
    ...(conditions.employment_type ? [`雇用形態：${conditions.employment_type}`] : []),
    `上位 ${String(conditions.top_n)}件`,
    conditions.source,
    ...(conditions.salary_converted ? ['給与は CSV の給与を月給に換算した値'] : []),
  ].join(' ／ ');
}

function changedConditions(previous: CompetitorMonth, current: CompetitorMonth): string[] {
  const changed: string[] = [];
  if (jobNameText(previous) !== jobNameText(current)) changed.push('職種名');
  if (regionText(previous) !== regionText(current)) changed.push('地域');
  if (previous.conditions.top_n !== current.conditions.top_n) changed.push('上位の件数');
  if (previous.salary.unit !== current.salary.unit || Boolean(previous.conditions.salary_converted) !== Boolean(current.conditions.salary_converted)) changed.push('給与の単位');
  if (previous.conditions.source !== current.conditions.source) changed.push('媒体');
  if ((previous.conditions.employment_type ?? '') !== (current.conditions.employment_type ?? '')) changed.push('雇用形態');
  return changed;
}

/**
 * 月ごとの行（古い月から）。応募は、応募日がその月に入る応募の数（重複を除いた、求人の画面と同じ数え方。
 * 応募日が無い応募は作成日で数えたものも入る）。応募を数えた日がその月の途中なら partial。
 */
export function competitorRows(job: Pick<JobCopyRecord, 'competitorMonthly' | 'overallApplications'>, now?: Date): CompetitorRow[] {
  const entries = [...(job.competitorMonthly ?? [])].sort((a, b) => a.month.localeCompare(b.month));
  const byDate = job.overallApplications?.byDate;
  const countedOn = jstDate(job.overallApplications?.fetchedAt) ?? todayJst(now);
  return entries.map((entry, index) => {
    const previous = entries[index - 1];
    return {
      month: entry.month, date: entry.run_at.slice(0, 10), total: entry.count.total,
      median: entry.salary.all.min_median, own: entry.own_salary?.min ?? null, unit: entry.salary.unit,
      applications: byDate ? Object.entries(byDate).reduce((sum, [date, count]) => date.startsWith(`${entry.month}-`) ? sum + count : sum, 0) : null,
      partial: countedOn.slice(0, 7) === entry.month,
      salaryN: entry.count.salary_median_n,
      converted: entry.conditions.salary_converted === true,
      changed: previous ? changedConditions(previous, entry) : [],
      entry,
    };
  });
}

/** 表の下の、応募の数え方の注記。 */
export function applicationsNote(job: Pick<JobCopyRecord, 'overallApplications'>, rows: readonly CompetitorRow[]): string {
  const apps = job.overallApplications;
  const parts = ['自社の応募は、応募日がその月に入る応募の数です'];
  if (apps?.createdDateCount) parts.push(`応募日が無い応募 ${apps.createdDateCount.toLocaleString('ja-JP')}件は、作成日の月で数えています`);
  if (apps && apps.missingDate > 0) parts.push(`応募日の分からない応募 ${apps.missingDate.toLocaleString('ja-JP')}件は含みません`);
  if (rows.some(row => row.applications === null)) parts.push('応募日の分かる応募が無いため「—」です');
  if (rows.some(row => row.partial && row.applications !== null)) parts.push('「途中」の月は、月の途中までの数です');
  return `${parts.join('。')}。`;
}

/** 「この求人で競合調査をする」の行き先。HubSpot の求人だけ（番号が数字）。それ以外は null。 */
export function competitorEntryUrl(job: Pick<JobCopyRecord, 'hubspotId' | 'title' | 'listingPrefecture' | 'listingMunicipality' | 'listingSummary'>): string | null {
  if (!job.hubspotId || !/^\d+$/.test(job.hubspotId)) return null;
  return competitorUrl({
    id: job.hubspotId, name: job.title, prefecture: job.listingPrefecture, municipality: job.listingMunicipality,
    jobName: job.listingSummary?.occupation ?? job.listingSummary?.job_kind ?? null,
  });
}
