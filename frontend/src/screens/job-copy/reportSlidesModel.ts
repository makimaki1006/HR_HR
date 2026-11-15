/**
 * 顧客報告のスライド（暫定 2026-11-15）。集計は既存のものを使い回し、ここでは
 * 「どのスライドを出すか」「見出しの1文」「図に渡す数」だけを決める。画面に依存しない。
 *
 * 決まり:
 * - 見出しの1文は、集計から決まった型で作る。数字は集計から写すだけ。判定の言葉・割合の差は出さない。
 * - 分からない数字・一部だけの数字は紙に書かない。スライド（または図の一部）を外し、外した理由を社内向けに返す。
 * - 日付は JST の暦日 YYYY-MM-DD。期間は両端を含む。
 */
import type { CompetitorMonth } from '../../generated/CompetitorMonth';
import { regionText, salaryText } from './competitorHistoryModel';
import { addDays, countApplications, daysBetween } from './timelineModel';
import { weekStart } from './dealSummaryModel';
import type { EventSource, Overlap } from './dealSummaryModel';
import type { ApplicationRow } from './data';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { buildDistribution } from './applicantCompositionModel';
import type { ApplicantDistribution } from './applicantCompositionModel';
import { AREA_OTHER, AREA_UNKNOWN } from './applicantArea';
import { REASON_CATEGORIES, selectedCategory } from './reasonCategories';
import type { ReasonCategory } from './reasonCategories';

export const WINDOW_DAYS = 28;
/** 応募者の傾向・応募理由を読む求人の数の上限（案件の単位）。超えるときは読まずにスライドを外す。 */
export const APPLICANT_LISTING_LIMIT = 20;
/** 週の棒がこれより多いときは月ごとにまとめる。 */
const MAX_WEEK_BARS = 26;
/** 求人ごとの棒の数。残りは「ほか N件の求人」にまとめる。 */
const MAX_LISTING_BARS = 8;
/** お住まいの棒の数（名前の出る都道府県）。残りは「その他」に足す。 */
const MAX_AREA_BARS = 6;
/** 変えたことと前後のカードの数（新しいものから）。 */
const MAX_CHANGE_CARDS = 4;
/** 見出しに入れる求人名の長さ。超えたら省いて「…」。 */
export const HEADLINE_TITLE_CHARS = 20;
/** 棒の名前の長さ。 */
const BAR_LABEL_CHARS = 28;
export const MAX_NEXT_ACTIONS = 3;
/** 表紙の差出人の会社名（設定する1か所）。空欄なら担当者名だけを出す。 */
export const REPORT_SENDER = '';
/** 電話でうかがった応募理由の分類（HubSpot の応募の項目）。 */
export const HEARING_CATEGORY_SOURCE = 'ouboriyuukategori_hiaringu';

export type ReportScope = 'deal' | 'listing';
export type SlideId = 'cover' | 'summary' | 'trend' | 'listings' | 'changes' | 'applicants' | 'reasons' | 'around' | 'next';
export const SLIDE_ORDER: readonly SlideId[] = ['cover', 'summary', 'trend', 'listings', 'changes', 'applicants', 'reasons', 'around', 'next'];
export const SLIDE_NAMES: Record<SlideId, string> = {
  cover: '表紙', summary: 'まとめ', trend: '応募の推移', listings: '求人ごとの応募', changes: '変えたことと前後の応募',
  applicants: '応募者の傾向', reasons: '応募理由', around: '周りの求人の様子', next: '次に行うこと',
};
const MEDIA_LABELS = { hrh: 'HRハッカー', airwork: 'AirWork', reclogi: 'リクロジ' } as const;

export interface ReportListing {
  id: string;
  title: string;
  media: 'hrh' | 'airwork' | 'reclogi';
  /** 勤務地（同じ求人名の区別に使う）。 */
  place?: string | null | undefined;
  ended: boolean;
  /** 掲載を終えた日。分からないときは null。 */
  endedOn: string | null;
  /** 応募日別の件数。読めなかったときは null（0 とは区別する）。 */
  byDate: Record<string, number> | null;
  /** 本文・画像の変更と掲載の開始・終了。読めなかったときは null。 */
  events: EventSource | null;
}

export interface ReportApplicants {
  /** 応募者の属性。全部の求人の分を読めたときだけ。複数の求人に結び付いた応募は外してある。読めない・一部だけなら null。 */
  rows: readonly ApplicationRow[] | null;
  /** 応募理由。全部の求人の分を読めたときだけ。 */
  reasons: readonly ApplicantReasonCollection[] | null;
  /** 複数の求人に結び付いた応募のキー（求人ごとに別のキー）。応募者の傾向・応募理由の両方から外す。 */
  multiKeys?: readonly string[] | undefined;
  /** 複数の求人に結び付いているため外した応募の数（求人ごとに数えた数）。 */
  excludedMulti?: number | undefined;
  /** rows・reasons が null の理由（社内向け）。 */
  missingReason?: string | undefined;
}

export interface ReportInput {
  scope: ReportScope;
  id: string;
  /** 案件名・求人名。読めなかったときは null（紙には出さない）。 */
  name: string | null;
  company: string | null;
  listings: readonly ReportListing[];
  /** 複数の求人に結び付いた応募の余分な写し（日ごと）。案件の合計から引く。 */
  overlap: Overlap;
  /** 応募データがそろっている最後の日。 */
  asOf: string;
  /**
   * 案件の面接・採用（案件に手入力した、ご契約からの累計。期間の数ではない）。readOn は案件を読んだ日（JST）。
   * 求人の単位・読めなかったときは null。既定で紙に出す（期間の数字とは段を分け、読んだ日を添える）。
   */
  outcomes: { interviews: number | null; hires: number | null; readOn: string } | null;
  /** 案件の面接・採用を読み込めなかった（未入力とは区別して、外した理由に書く）。 */
  outcomesFailed?: boolean | undefined;
  applicants: ReportApplicants;
  /** 競合調査の月ごとの記録（求人の報告だけ。案件の報告では読まない）。 */
  competitor?: readonly CompetitorMonth[] | undefined;
  /** 求人の一部しか数えていない理由（求人が多くて切った等）。あれば件数を使うスライドを外す。 */
  partial?: readonly string[] | undefined;
}

export interface NextAction { text: string; sub: string }
export interface ReportOptions {
  from: string;
  to: string;
  addressee: string;
  subject: string;
  owner: string;
  sender: string;
  createdOn: string;
  nextActions: readonly NextAction[];
  nextHeadline: string;
  /** 入れないスライド。まとめの箇条書きにも使わない。 */
  off?: readonly SlideId[] | undefined;
  /** 面接・採用（案件に手入力した累計）を出すか。既定は出す（2026-11-16 利用者決定）。媒体の費用は報告に書かない。 */
  include?: { outcomes?: boolean } | undefined;
}

export interface Bar { label: string; count: number }
export interface TrendBar extends Bar { marks: string[] }
export interface ChangeCard { date: string; listingId: string; title: string; what: string; before: number; after: number }
export interface Kpi { label: string; value: string; sub: string | null }

interface SlideBase { name: string; kicker: string; headline: string; note: string | null }
export type Slide = SlideBase & (
  | { id: 'cover'; addressee: string; title: string; subtitle: string; footer: string }
  | { id: 'summary'; kpis: Kpi[]; /** ご契約からの合計（面接・採用）。期間の数字とは別の段に出す。 */ totals: { heading: string; kpis: Kpi[] } | null; points: string[] }
  | { id: 'trend'; unit: 'week' | 'month'; bars: TrendBar[] }
  | { id: 'listings'; bars: Bar[] }
  | { id: 'changes'; cards: ChangeCard[] }
  | { id: 'applicants'; total: number; groups: { title: string; rows: Bar[] }[] }
  | { id: 'reasons'; n: number; bars: Bar[] }
  | { id: 'around'; kpis: Kpi[]; bars: Bar[] }
  | { id: 'next'; items: NextAction[] }
);
export interface Excluded { name: string; reason: string }
export interface ReportResult {
  slides: Slide[];
  excluded: Excluded[];
  period: { from: string; to: string; previousFrom: string; previousTo: string; clamped: boolean };
}

const num = (value: string) => String(Number(value));
/** 9/15 */
export const shortDay = (date: string) => `${num(date.slice(5, 7))}/${num(date.slice(8, 10))}`;
/** 9月15日 */
export const dayJa = (date: string) => `${num(date.slice(5, 7))}月${num(date.slice(8, 10))}日`;
/** 2026年9月15日 */
export const fullDayJa = (date: string) => `${date.slice(0, 4)}年${dayJa(date)}`;
/** 2026年9月1日〜11月15日（年が違えば両方に年）。 */
export function periodText(from: string, to: string): string {
  return from.slice(0, 4) === to.slice(0, 4) ? `${fullDayJa(from)}〜${dayJa(to)}` : `${fullDayJa(from)}〜${fullDayJa(to)}`;
}
/** 長い名前を省く（文字単位）。 */
export function clip(text: string, max: number): string {
  const chars = Array.from(text);
  return chars.length > max ? `${chars.slice(0, max).join('')}…` : text;
}
/** 性別の表記ゆれをまとめる（男/男性、女/女性）。 */
export function normalizeGender(value: string | null): string | null {
  const text = value?.normalize('NFKC').trim() ?? '';
  if (!text) return null;
  if (text === '男' || text === '男性') return '男性';
  if (text === '女' || text === '女性') return '女性';
  return text;
}

/** 期間の終わりを応募データの最後の日までにそろえる。始まりが終わりより後なら終わりにそろえる。 */
export function clampPeriod(from: string, to: string, asOf: string): { from: string; to: string; clamped: boolean } {
  const end = to > asOf ? asOf : to;
  return { from: from > end ? end : from, to: end, clamped: end !== to };
}

/** 月曜始まり〜日曜終わりの丸い週で、応募データの最後の日までに終わる直近 weeks 週。 */
export function alignedPeriod(asOf: string, weeks: number): { from: string; to: string } {
  const monday = weekStart(asOf);
  const to = addDays(monday, 6) === asOf ? asOf : addDays(monday, -1);
  return { from: addDays(to, -(7 * weeks - 1)), to };
}
/** 既定の期間: 丸い12週。 */
export function defaultPeriod(asOf: string): { from: string; to: string } {
  return alignedPeriod(asOf, 12);
}

/** 求人の応募の合計（応募日が分かる求人だけ。重なる応募は1回）。どの求人も分からなければ null。 */
export function applicationsBetween(listings: readonly Pick<ReportListing, 'byDate'>[], from: string, toExclusive: string, overlap: Overlap = {}): number | null {
  const known = listings.filter(listing => listing.byDate);
  if (!known.length) return null;
  return known.reduce((sum, listing) => sum + countApplications(listing.byDate ?? undefined, from, toExclusive), 0) - countApplications(overlap, from, toExclusive);
}

function firstPositive(byDate: Record<string, number> | null): string | null {
  const dates = Object.entries(byDate ?? {}).filter(([, count]) => count > 0).map(([date]) => date).sort();
  return dates[0] ?? null;
}
/** 前の期間を数えられるか: 案件のまとめと同じく、最初の応募の日が前の期間の初め以前。 */
export function previousKnown(listings: readonly Pick<ReportListing, 'byDate'>[], previousFrom: string): boolean {
  const first = listings.map(listing => firstPositive(listing.byDate)).filter((date): date is string => date !== null).sort()[0];
  return first !== undefined && first <= previousFrom;
}

// ---- 見出しの1文 ----

export function summaryHeadline(current: number, previous: number | null): string {
  if (previous === null) return `この期間の応募は ${String(current)}件です`;
  if (current > previous) return `応募は前の期間の ${String(previous)}件から ${String(current)}件に増えました`;
  if (current < previous) return `応募は前の期間の ${String(previous)}件から ${String(current)}件に減りました`;
  return `応募は前の期間と同じ ${String(current)}件でした`;
}

/** full: 期間の両端の端数の週・月は見出しの対象にしない。 */
export function trendHeadline(bars: readonly Bar[], unit: 'week' | 'month', starts: readonly string[], full: readonly boolean[] = bars.map(() => true)): string {
  const total = bars.reduce((sum, bar) => sum + bar.count, 0);
  if (total === 0) return 'この期間に応募はありませんでした';
  const candidates = bars.map((bar, index) => ({ bar, start: starts[index] ?? '', full: full[index] ?? true })).filter(item => item.full);
  const max = Math.max(0, ...candidates.map(item => item.bar.count));
  const tops = candidates.filter(item => item.bar.count === max);
  const top = tops[0];
  const word = unit === 'week' ? '週' : '月';
  if (max === 0 || !top) return `期間中の応募は ${String(total)}件です`;
  if (tops.length > 1) return `期間中の応募は ${String(total)}件で、${word}ごとでは最も多いときで ${String(max)}件です`;
  const when = unit === 'week' ? `${dayJa(top.start)}の週` : `${num(top.start.slice(5, 7))}月`;
  return `応募が最も多かったのは${when}で、${String(max)}件です`;
}

export function listingsHeadline(bars: readonly Bar[]): string {
  const total = bars.reduce((sum, bar) => sum + bar.count, 0);
  if (total === 0) return 'この期間は、どの求人にも応募がありませんでした';
  const max = Math.max(...bars.map(bar => bar.count));
  const tops = bars.filter(bar => bar.count === max);
  if (tops.length === 1 && tops[0]) return `応募が最も多かったのは「${clip(tops[0].label, HEADLINE_TITLE_CHARS)}」で、${String(max)}件です`;
  return `応募が最も多かった求人は ${String(tops.length)}件あり、それぞれ ${String(max)}件です`;
}

const changeNoun: Record<string, string> = { 本文を見直し: '本文の見直し', 画像を見直し: '画像の見直し', 本文と画像を見直し: '本文と画像の見直し' };
export const CHANGES_PLAIN_HEADLINE = '掲載内容を見直した前後4週の応募件数です';
export function changesHeadline(cards: readonly ChangeCard[]): string {
  let best: ChangeCard | null = null;
  for (const card of cards) if (!best || card.after - card.before > best.after - best.before || (card.after - card.before === best.after - best.before && card.date > best.date)) best = card;
  if (best && best.after > best.before) return `${dayJa(best.date)}の「${clip(best.title, HEADLINE_TITLE_CHARS)}」の${changeNoun[best.what] ?? '見直し'}の前後4週で、応募は ${String(best.before)}件から ${String(best.after)}件になりました`;
  return CHANGES_PLAIN_HEADLINE;
}

export function applicantsHeadline(groups: readonly { title: string; rows: readonly Bar[] }[], total: number): string {
  const uniqueTop = (title: string, skip: readonly string[]) => {
    const rows = groups.find(group => group.title === title)?.rows.filter(row => !skip.includes(row.label) && row.count > 0) ?? [];
    const max = Math.max(0, ...rows.map(row => row.count));
    const tops = rows.filter(row => row.count === max);
    return tops.length === 1 ? tops[0] ?? null : null;
  };
  const gender = uniqueTop('性別', ['不明', 'その他']);
  const area = uniqueTop('お住まい', [AREA_OTHER, AREA_UNKNOWN]);
  if (gender && area) return `応募者は${gender.label}が ${String(gender.count)}件で最も多く、お住まいは${area.label}が ${String(area.count)}件で最も多くなっています`;
  if (gender) return `応募者は${gender.label}が ${String(gender.count)}件で最も多くなっています`;
  if (area) return `応募者のお住まいは${area.label}が ${String(area.count)}件で最も多くなっています`;
  return `応募者 ${String(total)}件の内訳です`;
}

export const REASON_LABELS: Record<ReasonCategory, string> = { 給与: '給与・手当', 勤務地: '勤務地・通勤', 職種興味: '仕事内容への関心', 会社規模: '会社の規模', その他: 'その他' };
export const REASONS_PLAIN_HEADLINE = 'お電話でうかがえた応募理由の内訳です';
export function reasonsHeadline(bars: readonly Bar[]): string {
  const named = bars.filter(bar => bar.label !== REASON_LABELS.その他 && bar.count > 0);
  const max = Math.max(0, ...named.map(bar => bar.count));
  const tops = named.filter(bar => bar.count === max);
  if (max === 0 || tops.length > 3) return REASONS_PLAIN_HEADLINE;
  return `お電話でうかがえた応募理由は${tops.map(bar => `「${bar.label}」`).join('と')}が最も多くなっています`;
}

// ---- スライドの中身 ----

function nextMonth(month: string): string {
  return month.slice(5, 7) === '12' ? `${String(Number(month.slice(0, 4)) + 1)}-01-01` : `${month.slice(0, 5)}${String(Number(month.slice(5, 7)) + 1).padStart(2, '0')}-01`;
}
function trendBars(input: ReportInput, from: string, to: string, changeMarks: readonly { date: string; what: string }[]): { unit: 'week' | 'month'; bars: TrendBar[]; starts: string[]; full: boolean[] } {
  const end = addDays(to, 1);
  const weeks: string[] = [];
  for (let week = weekStart(from); week <= to; week = addDays(week, 7)) weeks.push(week);
  const unit = weeks.length > MAX_WEEK_BARS ? 'month' : 'week';
  const spans: { start: string; end: string; label: string; full: boolean }[] = [];
  if (unit === 'week') for (const week of weeks) {
    const start = week < from ? from : week;
    const stop = addDays(week, 7) < end ? addDays(week, 7) : end;
    spans.push({ start, end: stop, label: shortDay(start), full: start === week && stop === addDays(week, 7) });
  }
  else {
    for (let month = `${from.slice(0, 7)}-01`; month <= to; month = nextMonth(month)) {
      const start = month < from ? from : month;
      const stop = nextMonth(month) < end ? nextMonth(month) : end;
      spans.push({ start, end: stop, label: `${num(month.slice(5, 7))}月`, full: start === month && stop === nextMonth(month) });
    }
  }
  const bars = spans.map(span => ({
    label: span.label,
    count: applicationsBetween(input.listings, span.start, span.end, input.overlap) ?? 0,
    marks: [...new Set(changeMarks.filter(mark => mark.date >= span.start && mark.date < span.end).map(mark => mark.what))],
  }));
  return { unit, bars, starts: spans.map(span => span.start), full: spans.map(span => span.full) };
}

/** 期間内の本文・画像の見直し（求人ごと、日ごとに1件）。 */
export function changesInPeriod(listings: readonly ReportListing[], from: string, to: string): { listing: ReportListing; date: string; what: string }[] {
  const out: { listing: ReportListing; date: string; what: string }[] = [];
  for (const listing of listings) {
    const events = listing.events;
    if (!events) continue;
    const bodies = new Set(events.bodyChanges ?? (events.bodyChangedAt ? [events.bodyChangedAt] : []));
    const images = new Set(events.imageChanges ?? []);
    for (const date of [...new Set([...bodies, ...images])].sort()) {
      if (date < from || date > to) continue;
      out.push({ listing, date, what: bodies.has(date) && images.has(date) ? '本文と画像を見直し' : bodies.has(date) ? '本文を見直し' : '画像を見直し' });
    }
  }
  return out.sort((a, b) => a.date.localeCompare(b.date));
}

/** 見直しの前後4週（その求人の応募）。後の4週がそろわない・その求人の最初の応募が前の4週の初めより後なら null。 */
export function changeCard(change: { listing: ReportListing; date: string; what: string }, asOf: string): ChangeCard | null {
  const { listing, date, what } = change;
  if (!listing.byDate) return null;
  const beforeStart = addDays(date, -WINDOW_DAYS);
  const afterEnd = addDays(date, WINDOW_DAYS);
  const first = firstPositive(listing.byDate);
  if (afterEnd > addDays(asOf, 1) || first === null || first > beforeStart) return null;
  return { date, listingId: listing.id, title: listing.title, what, before: countApplications(listing.byDate, beforeStart, date), after: countApplications(listing.byDate, date, afterEnd) };
}

function distributionRows(distribution: ApplicantDistribution | null, sort: boolean): Bar[] {
  const rows = (distribution?.categories ?? []).filter(row => row.count > 0).map(row => ({ label: row.category, count: row.count }));
  if (!sort) return rows;
  const rank = (label: string) => label === AREA_OTHER || label === 'その他' ? 1 : label === AREA_UNKNOWN || label === '不明' ? 2 : 0;
  return rows.map((row, index) => ({ row, index })).sort((a, b) => rank(a.row.label) - rank(b.row.label) || b.row.count - a.row.count || a.index - b.index).map(item => item.row);
}
/** 名前の出る棒を limit 本までにし、残りを「その他」に足す。 */
function capWithOther(rows: Bar[], limit: number): Bar[] {
  const named = rows.filter(row => row.label !== AREA_OTHER && row.label !== AREA_UNKNOWN);
  if (named.length <= limit) return rows;
  const keep = new Set(named.slice(0, limit));
  const moved = named.filter(row => !keep.has(row)).reduce((sum, row) => sum + row.count, 0);
  const other = rows.find(row => row.label === AREA_OTHER);
  const unknown = rows.filter(row => row.label === AREA_UNKNOWN);
  return [...named.slice(0, limit), { label: AREA_OTHER, count: (other?.count ?? 0) + moved }, ...unknown];
}

/** 応募者の傾向。性別・年代・都道府県のそれぞれ単独の内訳（3件未満の都道府県は「その他」）。 */
export function applicantGroups(rows: readonly ApplicationRow[]): { title: string; rows: Bar[] }[] {
  const attributes = rows.map(row => ({ gender: normalizeGender(row.gender), age: row.age ? Number(row.age) : null, prefecture: row.prefecture, municipality: row.municipality }));
  return [
    { title: '性別', rows: distributionRows(buildDistribution(attributes, 'gender'), true) },
    { title: '年代', rows: distributionRows(buildDistribution(attributes, 'age'), false) },
    { title: 'お住まい', rows: capWithOther(distributionRows(buildDistribution(attributes, 'prefecture'), true), MAX_AREA_BARS) },
  ];
}

export type ReasonResult = { ok: true; n: number; bars: Bar[] } | { ok: false; reason: string };
/**
 * 応募理由（お電話でうかがって選んだ分類だけ。キーワードの推定は使わない）。複数回答。
 * 期間内の応募日のもの。複数の求人に結び付いた応募は外す（求人の画面と同じ決まり）。
 */
export function reasonBars(collections: readonly ApplicantReasonCollection[], from: string, to: string, exclude: ReadonlySet<string> = new Set()): ReasonResult {
  if (!collections.length || collections.some(collection => !collection.available || collection.selections === null)) return { ok: false, reason: '応募理由を読み込めなかったため' };
  if (collections.some(collection => collection.truncated)) return { ok: false, reason: '応募理由の記録が多く、一部しか読み込めていないため' };
  const byApplicant = new Map<string, Set<ReasonCategory>>();
  for (const collection of collections) for (const selection of collection.selections ?? []) {
    if (selection.sourceProperty !== HEARING_CATEGORY_SOURCE || exclude.has(selection.applicant)) continue;
    if (selection.applicationDate === null || selection.applicationDate < from || selection.applicationDate > to) continue;
    const category = selectedCategory(selection);
    if (!category || category === 'unset') continue;
    const set = byApplicant.get(selection.applicant) ?? new Set<ReasonCategory>();
    set.add(category);
    byApplicant.set(selection.applicant, set);
  }
  const bars = REASON_CATEGORIES.map((category, index) => ({ label: REASON_LABELS[category], count: [...byApplicant.values()].filter(set => set.has(category)).length, index }))
    .filter(bar => bar.count > 0)
    .sort((a, b) => (a.label === REASON_LABELS.その他 ? 1 : 0) - (b.label === REASON_LABELS.その他 ? 1 : 0) || b.count - a.count || a.index - b.index)
    .map(({ label, count }) => ({ label, count }));
  return { ok: true, n: byApplicant.size, bars };
}

/** 求人ごとの棒の名前。同じ求人名があるときは媒体と勤務地を添える。 */
function listingLabels(listings: readonly ReportListing[]): Map<string, string> {
  const counts = new Map<string, number>();
  for (const listing of listings) counts.set(listing.title, (counts.get(listing.title) ?? 0) + 1);
  return new Map(listings.map(listing => {
    const extra = (counts.get(listing.title) ?? 0) > 1 ? `［${[MEDIA_LABELS[listing.media], listing.place ?? ''].filter(Boolean).join('・')}］` : '';
    const ended = listing.ended ? listing.endedOn ? `（${shortDay(listing.endedOn)} 終了）` : '（掲載終了）' : '';
    return [listing.id, `${clip(listing.title, BAR_LABEL_CHARS)}${extra}${ended}`];
  }));
}

export function buildReport(input: ReportInput, options: ReportOptions): ReportResult {
  const { from, to, clamped } = clampPeriod(options.from, options.to, input.asOf);
  const end = addDays(to, 1);
  const length = daysBetween(from, end);
  const previousFrom = addDays(from, -length);
  const previousTo = addDays(from, -1);
  const off = new Set(options.off ?? []);
  const excluded: Excluded[] = [];
  const slides: Slide[] = [];
  const exclude = (id: SlideId, reason: string) => { excluded.push({ name: SLIDE_NAMES[id], reason }); };
  if (clamped) excluded.push({ name: '期間', reason: `応募データは ${shortDay(input.asOf)} までのため、期間の終わりを ${shortDay(to)} にしています` });

  const unknownListings = input.listings.filter(listing => !listing.byDate);
  const partial = [...(input.partial ?? [])];
  if (unknownListings.length) partial.push(`応募日を読み込めなかった求人が ${String(unknownListings.length)}件あります（${unknownListings.slice(0, 3).map(listing => clip(listing.title, HEADLINE_TITLE_CHARS)).join('・')}${unknownListings.length > 3 ? ' ほか' : ''}）`);
  const current = applicationsBetween(input.listings, from, end, input.overlap);
  const previous = current !== null && previousKnown(input.listings, previousFrom) ? applicationsBetween(input.listings, previousFrom, from, input.overlap) : null;

  const changes = current === null ? [] : changesInPeriod(input.listings, from, to);
  const completeCards = changes.map(change => changeCard(change, input.asOf)).filter((card): card is ChangeCard => card !== null);
  const changesSlide = completeCards.length ? (() => {
    const shown = completeCards.slice(-MAX_CHANGE_CARDS);
    return { id: 'changes' as const, name: SLIDE_NAMES.changes, kicker: SLIDE_NAMES.changes, headline: changesHeadline(shown), note: '前後の4週の応募件数。応募は掲載の時期やほかの求人の動きでも変わるため、変更だけが理由とは限りません。', cards: shown };
  })() : null;

  const multi = new Set(input.applicants.multiKeys ?? []);
  const rowsInPeriod = input.applicants.rows?.filter(row => row.date !== null && row.date >= from && row.date <= to) ?? null;
  const reasons = input.applicants.reasons ? reasonBars(input.applicants.reasons, from, to, multi) : null;
  if (input.applicants.excludedMulti) excluded.push({ name: '応募者の傾向・応募理由', reason: `複数の求人に結び付いた応募 ${String(input.applicants.excludedMulti)}件（求人ごとに数えた数）は、同じ方を重ねて数えないよう外しています` });

  const nextItems = options.nextActions.slice(0, MAX_NEXT_ACTIONS).map(item => ({ text: item.text.trim(), sub: item.sub.trim() })).filter(item => item.text);

  // 表紙
  const footer = [options.sender.trim(), options.owner.trim() ? `担当：${options.owner.trim()}` : '', fullDayJa(options.createdOn)].filter(Boolean).join(' ／ ');
  const addressee = options.addressee.trim();
  const subject = options.subject.trim();
  slides.push({ id: 'cover', name: SLIDE_NAMES.cover, kicker: '', headline: '採用活動のご報告', note: null, addressee: addressee ? `${addressee} 御中` : '', title: '採用活動のご報告', subtitle: subject ? `${subject} ／ ${periodText(from, to)}` : periodText(from, to), footer });

  if (current === null) {
    for (const id of ['summary', 'trend', 'listings', 'changes'] as const) exclude(id, '応募日別の件数を読み込めなかったため');
  } else if (partial.length) {
    for (const id of ['summary', 'trend', 'listings'] as const) exclude(id, `求人の一部しか数えられないため（${partial.join('。')}）`);
  } else {
    // まとめ
    if (previous === null) excluded.push({ name: 'まとめ（前の期間）', reason: '最初の応募が前の期間の初めより後のため、前の期間と比べていません' });
    const kpis: Kpi[] = [{ label: '応募', value: `${String(current)}件`, sub: previous === null ? null : `前の期間 ${String(previous)}件` }];
    // 面接・採用は期間の数ではなく、案件を読んだ日までの累計。期間の数字とは段を分け、読んだ日を見出しに書く。
    const totalKpis: Kpi[] = [];
    if (input.scope === 'deal' && options.include?.outcomes !== false) {
      const { interviews = null, hires = null } = input.outcomes ?? {};
      if (interviews !== null) totalKpis.push({ label: '面接', value: `${String(interviews)}件`, sub: null });
      if (hires !== null) totalKpis.push({ label: '採用', value: `${String(hires)}名`, sub: null });
      if (input.outcomesFailed || !input.outcomes) excluded.push({ name: 'まとめ（面接・採用）', reason: '面接・採用は読み込めなかったため外しています' });
      else if (interviews === null && hires === null) excluded.push({ name: 'まとめ（面接・採用）', reason: '面接・採用は案件に記録が無いため外しています' });
      else if (interviews === null) excluded.push({ name: 'まとめ（面接）', reason: '面接は案件に記録が無いため外しています' });
      else if (hires === null) excluded.push({ name: 'まとめ（採用）', reason: '採用は案件に記録が無いため外しています' });
    }
    const totals = totalKpis.length && input.outcomes ? { heading: `ご契約からの合計（${shortDay(input.outcomes.readOn)} 時点）`, kpis: totalKpis } : null;
    const withApplications = input.listings.filter(listing => countApplications(listing.byDate ?? undefined, from, end) > 0).length;
    if (input.listings.length > 1) kpis.push({ label: '応募のあった求人', value: `${String(withApplications)}件`, sub: `求人 ${String(input.listings.length)}件のうち` });
    const live = input.listings.filter(listing => !listing.ended).length;
    kpis.push({ label: '掲載中の求人', value: `${String(live)}件`, sub: null });
    const points = [
      changesSlide && !off.has('changes') && changesSlide.headline !== CHANGES_PLAIN_HEADLINE ? changesSlide.headline : null,
      reasons?.ok && reasons.n > 0 && !off.has('reasons') && reasonsHeadline(reasons.bars) !== REASONS_PLAIN_HEADLINE ? reasonsHeadline(reasons.bars) : null,
      nextItems[0] && !off.has('next') ? `次は、${nextItems[0].text}` : null,
    ].filter((point): point is string => point !== null);
    slides.push({ id: 'summary', name: SLIDE_NAMES.summary, kicker: SLIDE_NAMES.summary, headline: summaryHeadline(current, previous), note: null, kpis, totals, points });

    // 応募の推移
    const trend = trendBars(input, from, to, changes.map(change => ({ date: change.date, what: change.what })));
    slides.push({ id: 'trend', name: SLIDE_NAMES.trend, kicker: SLIDE_NAMES.trend, headline: trendHeadline(trend.bars, trend.unit, trend.starts, trend.full), note: `${trend.unit === 'week' ? '週' : '月'}ごとの応募件数（当社で記録した応募日）。応募は掲載の時期やほかの求人の動きでも変わります。`, unit: trend.unit, bars: trend.bars });

    // 求人ごとの応募
    if (input.scope === 'listing' || input.listings.length < 2) exclude('listings', input.scope === 'listing' ? '求人1件の報告のため' : '求人が1件だけのため');
    else {
      const labels = listingLabels(input.listings);
      const all = input.listings.map(listing => ({ label: labels.get(listing.id) ?? listing.title, count: countApplications(listing.byDate ?? undefined, from, end) })).sort((a, b) => b.count - a.count);
      const head = all.slice(0, MAX_LISTING_BARS);
      const rest = all.slice(MAX_LISTING_BARS);
      const bars = rest.length ? [...head, { label: `ほか ${String(rest.length)}件の求人`, count: rest.reduce((sum, bar) => sum + bar.count, 0) }] : head;
      slides.push({ id: 'listings', name: SLIDE_NAMES.listings, kicker: SLIDE_NAMES.listings, headline: listingsHeadline(head), note: '求人ごとの期間中の応募件数。同じ方が複数の求人に応募した場合は、それぞれの求人で数えています。', bars });
    }
  }
  if (current !== null) {
    // 変えたことと前後の応募（求人ごとの数なので、ほかの求人が数えられなくても出せる）
    if (changesSlide) slides.push(changesSlide);
    else exclude('changes', changes.length ? '期間中の見直しは、どれも前後4週の応募データがそろっていないため' : '期間中に本文・画像の見直しが無いため');
    if (changesSlide && completeCards.length < changes.length) excluded.push({ name: '変えたことと前後の応募', reason: `前後4週の応募データがそろっていない見直し ${String(changes.length - completeCards.length)}件は外しています` });
  }

  // 応募者の傾向
  if (rowsInPeriod === null) exclude('applicants', input.applicants.missingReason ?? '応募者の属性を読み込めなかったため');
  else if (!rowsInPeriod.length) exclude('applicants', '期間中に応募日の分かる応募が無いため');
  else {
    const groups = applicantGroups(rowsInPeriod);
    const total = rowsInPeriod.length;
    const prefix = current !== null && !partial.length && total === current ? `期間中の応募 ${String(total)}件の内訳（件）。` : `期間中の応募のうち ${String(total)}件の内訳（件）。`;
    slides.push({ id: 'applicants', name: SLIDE_NAMES.applicants, kicker: SLIDE_NAMES.applicants, headline: applicantsHeadline(groups, total), note: `${prefix}3件未満の都道府県は「その他」にまとめています。`, total, groups });
  }

  // 応募理由
  if (!reasons) exclude('reasons', input.applicants.missingReason ?? '応募理由を読み込めなかったため');
  else if (!reasons.ok) exclude('reasons', reasons.reason);
  else if (reasons.n === 0) exclude('reasons', '期間中に、お電話で応募理由をうかがって分類した応募が無いため');
  else slides.push({ id: 'reasons', name: SLIDE_NAMES.reasons, kicker: SLIDE_NAMES.reasons, headline: reasonsHeadline(reasons.bars), note: `お電話で応募理由をうかがえた ${String(reasons.n)}件の内訳。1件で複数の理由をうかがった場合は、それぞれに数えています。`, n: reasons.n, bars: reasons.bars });

  // 周りの求人の様子（競合調査の記録。期間の終わりまでに記録した、いちばん新しい月を 1 枚に写す。判定は付けない）
  if (input.scope === 'deal') exclude('around', '競合調査は求人ごとの記録のため、案件の報告には入れていません（求人の報告で出せます）');
  else {
    const records = [...(input.competitor ?? [])].sort((a, b) => a.run_at.localeCompare(b.run_at));
    const record = records.filter(item => item.run_at.slice(0, 10) <= to).at(-1);
    if (!record) exclude('around', records.length ? '期間の終わりまでに記録した競合調査がありません' : '競合調査の記録がありません');
    else {
      const unit = record.salary.unit;
      const day = record.run_at.slice(0, 10);
      const median = record.salary.all.min_median;
      const kpis: Kpi[] = [{ label: '周りの求人', value: `${String(record.count.total)}件`, sub: null }];
      if (median !== null) kpis.push({ label: '周りの求人の給与（下限の中央値）', value: salaryText(median, unit), sub: null });
      // 周りの給与が月給に換算した値のときは、貴社の給与と並べない（記録の差と同じ決まり）。
      if (record.own_salary?.min != null && record.conditions.salary_converted !== true) kpis.push({ label: '貴社の求人の給与（下限）', value: salaryText(record.own_salary.min, unit), sub: `${shortDay(day)} 時点` });
      const place = regionText(record);
      // 期間の始めより前に調べた記録しか無いときは、それと分かるように書く。
      const before = day < from;
      const when = before ? `${num(record.month.slice(5, 7))}月の調査（期間より前）、${dayJa(day)}` : dayJa(day);
      slides.push({
        id: 'around', name: SLIDE_NAMES.around, kicker: before ? `${SLIDE_NAMES.around}（${num(record.month.slice(5, 7))}月の調査・期間より前）` : SLIDE_NAMES.around,
        headline: median === null ? `${when}に調べた周りの求人は ${String(record.count.total)}件でした` : `${when}に調べた周りの求人は ${String(record.count.total)}件、給与の下限の中央値は ${salaryText(median, unit)}でした`,
        note: `${fullDayJa(day)}に Indeed で調べた周りの求人（${place}）${before ? '。報告の期間より前の調査です' : ''}。下の棒は、求人票によく出てくる言葉と、その言葉がある求人の件数です。`,
        kpis, bars: record.keywords_top5.map(([word, count]) => ({ label: word, count })),
      });
    }
  }

  // 次に行うこと
  if (!nextItems.length) exclude('next', '「次に行うこと」が未記入のため');
  else slides.push({ id: 'next', name: SLIDE_NAMES.next, kicker: SLIDE_NAMES.next, headline: options.nextHeadline.trim() || (nextItems[0]?.text ?? SLIDE_NAMES.next), note: null, items: nextItems });

  slides.sort((a, b) => SLIDE_ORDER.indexOf(a.id) - SLIDE_ORDER.indexOf(b.id));
  return { slides, excluded, period: { from, to, previousFrom, previousTo, clamped } };
}

// ---- 下書き（期間・表紙・入れるスライド・次に行うこと）。画面の中だけに持つ ----

export interface ReportDraft {
  from: string;
  to: string;
  addressee: string;
  subject: string;
  owner: string;
  off: SlideId[];
  /** 面接・採用（ご契約からの合計）を出すか。未指定は出す。 */
  outcomes?: boolean | undefined;
  next: NextAction[];
  nextHeadline: string;
}
