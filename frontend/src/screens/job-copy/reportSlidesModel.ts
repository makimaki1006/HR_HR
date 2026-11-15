/**
 * 顧客報告のスライド（暫定 2026-11-15）。集計は既存のものを使い回し、ここでは
 * 「どのスライドを出すか」「見出しの1文」「図に渡す数」だけを決める。画面に依存しない。
 *
 * 決まり:
 * - 見出しの1文は、集計から決まった型で作る。数字は集計から写すだけ。判定の言葉・割合の差は出さない。
 * - 分からない数字は紙に書かない。スライド（または図の一部）を外し、外した理由を社内向けに返す。
 * - 日付は JST の暦日 YYYY-MM-DD。期間は両端を含む。
 */
import { addDays, countApplications, daysBetween, formatYen } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import { billingInRange, weekStart } from './dealSummaryModel';
import type { EventSource, Overlap } from './dealSummaryModel';
import type { ApplicationRow } from './data';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { buildDistribution } from './applicantCompositionModel';
import type { ApplicantDistribution } from './applicantCompositionModel';
import { AREA_OTHER, AREA_UNKNOWN, roundJointDemographics } from './applicantArea';
import { classifyApplicationReasons, REASON_CATEGORIES, tally } from './reasonCategories';
import type { ReasonCategory } from './reasonCategories';

export const WINDOW_DAYS = 28;
/** 応募者の傾向・応募理由を読む求人の数の上限（案件の単位）。これを超えると一部だけの集計になるので外す。 */
export const APPLICANT_LISTING_LIMIT = 20;
/** 週の棒がこれより多いときは月ごとにまとめる。 */
const MAX_WEEK_BARS = 26;
/** 求人ごとの棒の数。残りは「ほか N件の求人」にまとめる。 */
const MAX_LISTING_BARS = 8;
/** 変えたことと前後のカードの数（新しいものから）。 */
const MAX_CHANGE_CARDS = 4;
export const MAX_NEXT_ACTIONS = 3;
/** 表紙の差出人の会社名（設定する1か所）。空欄なら担当者名だけを出す。 */
export const REPORT_SENDER = '';

export type ReportScope = 'deal' | 'listing';
export type SlideId = 'cover' | 'summary' | 'trend' | 'listings' | 'changes' | 'applicants' | 'reasons' | 'next';
export const SLIDE_ORDER: readonly SlideId[] = ['cover', 'summary', 'trend', 'listings', 'changes', 'applicants', 'reasons', 'next'];
export const SLIDE_NAMES: Record<SlideId, string> = {
  cover: '表紙', summary: 'まとめ', trend: '応募の推移', listings: '求人ごとの応募', changes: '変えたことと前後の応募',
  applicants: '応募者の傾向', reasons: '応募理由', next: '次に行うこと',
};

export interface ReportListing {
  id: string;
  title: string;
  media: 'hrh' | 'airwork' | 'reclogi';
  ended: boolean;
  /** 掲載を終えた日。分からないときは null。 */
  endedOn: string | null;
  /** 応募日別の件数。読めなかったときは null（0 とは区別する）。 */
  byDate: Record<string, number> | null;
  billing: readonly BillingEntry[];
  /** 課金データを集めていない求人（AirWork で実績が無いもの）。 */
  billingUnavailable: boolean;
  /** 本文・画像の変更と掲載の開始・終了。読めなかったときは null。 */
  events: EventSource | null;
}

export interface ReportApplicants {
  /** 応募者の属性（応募日・性別・年齢・地域）。全部の求人の分を読めたときだけ。読めない・一部だけなら null。 */
  rows: readonly ApplicationRow[] | null;
  /** 応募理由。全部の求人の分を読めたときだけ。 */
  reasons: readonly ApplicantReasonCollection[] | null;
  /** rows・reasons が null の理由（社内向け）。 */
  missingReason?: string | undefined;
}

export interface ReportInput {
  scope: ReportScope;
  id: string;
  /** 案件名・求人名。 */
  name: string;
  company: string | null;
  listings: readonly ReportListing[];
  /** 複数の求人に結び付いた応募の余分な写し（日ごと）。案件の合計から引く。 */
  overlap: Overlap;
  /** 応募データがそろっている最後の日。 */
  asOf: string;
  /** 取引の面接・採用（契約からの合計）。求人の単位では null。読めなかったときも null。 */
  outcomes: { interviews: number | null; hires: number | null } | null;
  /** 取引の面接・採用を読めなかった（案件の単位だけ）。 */
  outcomesFailed?: boolean | undefined;
  applicants: ReportApplicants;
  /** 一部の求人だけを集計しているときの社内向けの注記。 */
  notes?: readonly string[] | undefined;
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
}

export interface Bar { label: string; count: number }
export interface TrendBar extends Bar { marks: string[] }
export interface ChangeCard { date: string; listingId: string; title: string; what: string; before: number; after: number }
export interface Kpi { label: string; value: string; sub: string | null }

interface SlideBase { name: string; kicker: string; headline: string; note: string | null }
export type Slide = SlideBase & (
  | { id: 'cover'; addressee: string; title: string; subtitle: string; footer: string }
  | { id: 'summary'; kpis: Kpi[]; points: string[] }
  | { id: 'trend'; unit: 'week' | 'month'; bars: TrendBar[] }
  | { id: 'listings'; bars: Bar[] }
  | { id: 'changes'; cards: ChangeCard[] }
  | { id: 'applicants'; total: number; groups: { title: string; rows: Bar[] }[] }
  | { id: 'reasons'; n: number; bars: Bar[] }
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

/** 期間の終わりを応募データの最後の日までにそろえる。始まりが終わりより後なら終わりにそろえる。 */
export function clampPeriod(from: string, to: string, asOf: string): { from: string; to: string; clamped: boolean } {
  const end = to > asOf ? asOf : to;
  return { from: from > end ? end : from, to: end, clamped: end !== to };
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
/** 求人の掲載を始めた日（分からなければ最初の応募の日）。 */
function listingStart(listing: ReportListing): string | null {
  return listing.events?.startOn ?? firstPositive(listing.byDate);
}
/** 前の期間を数えられるか: どれかの求人が前の期間の初めより前から分かっている。 */
export function previousKnown(listings: readonly ReportListing[], previousFrom: string): boolean {
  const starts = listings.filter(listing => listing.byDate).map(listingStart).filter((date): date is string => date !== null);
  return starts.some(date => date <= previousFrom);
}

// ---- 見出しの1文 ----

export function summaryHeadline(current: number, previous: number | null): string {
  if (previous === null) return `この期間の応募は ${String(current)}件です`;
  if (current > previous) return `応募は前の期間の ${String(previous)}件から ${String(current)}件に増えました`;
  if (current < previous) return `応募は前の期間の ${String(previous)}件から ${String(current)}件に減りました`;
  return `応募は前の期間と同じ ${String(current)}件でした`;
}

export function trendHeadline(bars: readonly Bar[], unit: 'week' | 'month', starts: readonly string[]): string {
  const total = bars.reduce((sum, bar) => sum + bar.count, 0);
  if (total === 0) return 'この期間に応募はありませんでした';
  const max = Math.max(...bars.map(bar => bar.count));
  const tops = bars.map((bar, index) => ({ bar, start: starts[index] ?? '' })).filter(item => item.bar.count === max);
  const top = tops[0];
  if (tops.length > 1 || !top) return `期間中の応募は ${String(total)}件で、${unit === 'week' ? '週' : '月'}ごとでは最も多いときで ${String(max)}件です`;
  const when = unit === 'week' ? `${dayJa(top.start)}の週` : `${num(top.start.slice(5, 7))}月`;
  return `応募が最も多かったのは${when}で、${String(max)}件です`;
}

export function listingsHeadline(bars: readonly Bar[]): string {
  const total = bars.reduce((sum, bar) => sum + bar.count, 0);
  if (total === 0) return 'この期間は、どの求人にも応募がありませんでした';
  const max = Math.max(...bars.map(bar => bar.count));
  const tops = bars.filter(bar => bar.count === max);
  if (tops.length === 1 && tops[0]) return `応募が最も多かったのは「${tops[0].label}」で、${String(max)}件です`;
  return `応募が最も多かった求人は ${String(tops.length)}件あり、それぞれ ${String(max)}件です`;
}

const changeNoun: Record<string, string> = { 本文を見直し: '本文の見直し', 画像を見直し: '画像の見直し', 本文と画像を見直し: '本文と画像の見直し' };
export function changesHeadline(cards: readonly ChangeCard[]): string {
  let best: ChangeCard | null = null;
  for (const card of cards) if (!best || card.after - card.before > best.after - best.before || (card.after - card.before === best.after - best.before && card.date > best.date)) best = card;
  if (best && best.after > best.before) return `${dayJa(best.date)}の「${best.title}」の${changeNoun[best.what] ?? '見直し'}の前後4週で、応募は ${String(best.before)}件から ${String(best.after)}件になりました`;
  return '掲載内容を見直した前後4週の応募件数です';
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
export function reasonsHeadline(bars: readonly Bar[]): string {
  const named = bars.filter(bar => bar.label !== REASON_LABELS.その他 && bar.count > 0);
  const max = Math.max(0, ...named.map(bar => bar.count));
  const tops = named.filter(bar => bar.count === max);
  if (max === 0 || tops.length > 3) return 'うかがえた応募理由の内訳です';
  return `うかがえた応募理由は${tops.map(bar => `「${bar.label}」`).join('と')}が最も多くなっています`;
}

// ---- スライドの中身 ----

function trendBars(input: ReportInput, from: string, to: string, changeMarks: readonly { date: string; what: string }[]): { unit: 'week' | 'month'; bars: TrendBar[]; starts: string[] } {
  const end = addDays(to, 1);
  const weeks: string[] = [];
  for (let week = weekStart(from); week <= to; week = addDays(week, 7)) weeks.push(week);
  const unit = weeks.length > MAX_WEEK_BARS ? 'month' : 'week';
  const spans: { start: string; end: string; label: string }[] = [];
  if (unit === 'week') for (const week of weeks) { const start = week < from ? from : week; spans.push({ start, end: addDays(week, 7) < end ? addDays(week, 7) : end, label: shortDay(start) }); }
  else {
    for (let month = `${from.slice(0, 7)}-01`; month <= to; ) {
      const next = month.slice(5, 7) === '12' ? `${String(Number(month.slice(0, 4)) + 1)}-01-01` : `${month.slice(0, 5)}${String(Number(month.slice(5, 7)) + 1).padStart(2, '0')}-01`;
      const start = month < from ? from : month;
      spans.push({ start, end: next < end ? next : end, label: `${num(month.slice(5, 7))}月` });
      month = next;
    }
  }
  const bars = spans.map(span => ({
    label: span.label,
    count: applicationsBetween(input.listings, span.start, span.end, input.overlap) ?? 0,
    marks: [...new Set(changeMarks.filter(mark => mark.date >= span.start && mark.date < span.end).map(mark => mark.what))],
  }));
  return { unit, bars, starts: spans.map(span => span.start) };
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

/** 見直しの前後4週（その求人の応募）。前後どちらかがそろっていなければ null。 */
export function changeCard(change: { listing: ReportListing; date: string; what: string }, asOf: string): ChangeCard | null {
  const { listing, date, what } = change;
  if (!listing.byDate) return null;
  const beforeStart = addDays(date, -WINDOW_DAYS);
  const afterEnd = addDays(date, WINDOW_DAYS);
  const start = listingStart(listing);
  if (afterEnd > addDays(asOf, 1) || start === null || start > beforeStart) return null;
  return { date, listingId: listing.id, title: listing.title, what, before: countApplications(listing.byDate, beforeStart, date), after: countApplications(listing.byDate, date, afterEnd) };
}

function distributionRows(distribution: ApplicantDistribution | null, sort: boolean): Bar[] {
  const rows = (distribution?.categories ?? []).filter(row => row.count > 0).map(row => ({ label: row.category, count: row.count }));
  if (!sort) return rows;
  const rank = (label: string) => label === AREA_OTHER || label === 'その他' ? 1 : label === AREA_UNKNOWN || label === '不明' ? 2 : 0;
  return rows.map((row, index) => ({ row, index })).sort((a, b) => rank(a.row.label) - rank(b.row.label) || b.row.count - a.row.count || a.index - b.index).map(item => item.row);
}

/** 応募者の傾向。地域は求人の画面と同じ規則（3件未満の地域は「その他」、組み合わせで特定できる地域も出さない）。 */
export function applicantGroups(rows: readonly ApplicationRow[]): { title: string; rows: Bar[] }[] {
  const attributes = rows.map(row => ({ gender: row.gender, age: row.age ? Number(row.age) : null, prefecture: row.prefecture, municipality: row.municipality }));
  const gender = buildDistribution(attributes, 'gender');
  const age = buildDistribution(attributes, 'age');
  const protectedCells = roundJointDemographics({ total: attributes.length, cells: attributes.map(row => ({
    gender: row.gender ?? '不明', age: buildDistribution([row], 'age')?.categories[0]?.category ?? '不明',
    prefecture: row.prefecture ?? '不明', municipality: row.municipality ?? '不明', count: 1,
  })) });
  const counts = new Map<string, number>();
  for (const cell of protectedCells.cells) counts.set(cell.prefecture, (counts.get(cell.prefecture) ?? 0) + cell.count);
  const prefecture: ApplicantDistribution = { total: attributes.length, categories: [...counts].map(([category, count]) => ({ category, count, percentage: null })) };
  return [
    { title: '性別', rows: distributionRows(gender, true) },
    { title: '年代', rows: distributionRows(age, false) },
    { title: 'お住まい', rows: distributionRows(prefecture, true) },
  ];
}

/** 応募理由の件数（複数回答）。全部の求人の理由をまとめ、同じ応募は1回だけ数える。期間内の応募日のものだけ。 */
export function reasonBars(collections: readonly ApplicantReasonCollection[], from: string, to: string): { n: number; bars: Bar[] } | null {
  if (!collections.length || collections.some(collection => !collection.available)) return null;
  const first = collections[0];
  if (!first) return null;
  // 複数の求人に結び付いた応募も、まとめた中では1回だけ数える（応募のキーでまとまる）ので外さない。
  const base: ApplicantReasonCollection = { ...first };
  delete base.multiListingApplicants;
  const merged: ApplicantReasonCollection = {
    ...base,
    items: collections.flatMap(collection => collection.items),
    selections: collections.every(collection => collection.selections !== null) ? collections.flatMap(collection => collection.selections ?? []) : null,
  };
  const classified = classifyApplicationReasons(merged);
  if (!classified) return null;
  const inPeriod = classified.applications.filter(application => application.applicationDate !== null && application.applicationDate >= from && application.applicationDate <= to);
  const result = tally(inPeriod);
  const bars = REASON_CATEGORIES.map((category, index) => ({ label: REASON_LABELS[category], count: result.counts.find(row => row.category === category)?.total ?? 0, index }))
    .filter(bar => bar.count > 0)
    .sort((a, b) => (a.label === REASON_LABELS.その他 ? 1 : 0) - (b.label === REASON_LABELS.その他 ? 1 : 0) || b.count - a.count || a.index - b.index)
    .map(({ label, count }) => ({ label, count }));
  return { n: result.n, bars };
}

export function buildReport(input: ReportInput, options: ReportOptions): ReportResult {
  const { from, to, clamped } = clampPeriod(options.from, options.to, input.asOf);
  const end = addDays(to, 1);
  const length = daysBetween(from, end);
  const previousFrom = addDays(from, -length);
  const previousTo = addDays(from, -1);
  const excluded: Excluded[] = [];
  const slides: Slide[] = [];
  const exclude = (id: SlideId, reason: string) => { excluded.push({ name: SLIDE_NAMES[id], reason }); };
  for (const note of input.notes ?? []) excluded.push({ name: '集計の範囲', reason: note });
  if (clamped) excluded.push({ name: '期間', reason: `応募データは ${shortDay(input.asOf)} までのため、期間の終わりを ${shortDay(to)} にしています` });

  const current = applicationsBetween(input.listings, from, end, input.overlap);
  const unknownListings = input.listings.filter(listing => !listing.byDate);
  if (current !== null && unknownListings.length) excluded.push({ name: '応募の件数', reason: `応募日を読み込めなかった求人 ${String(unknownListings.length)}件（${unknownListings.slice(0, 3).map(listing => listing.title).join('・')}${unknownListings.length > 3 ? ' ほか' : ''}）は件数に入っていません` });
  const previous = current !== null && previousKnown(input.listings, previousFrom) ? applicationsBetween(input.listings, previousFrom, from, input.overlap) : null;
  if (current !== null && previous === null) excluded.push({ name: 'まとめ（前の期間）', reason: '前の期間の初めより前の応募データが無いため、前の期間と比べていません' });

  const changes = current === null ? [] : changesInPeriod(input.listings, from, to);
  const cards = changes.map(change => changeCard(change, input.asOf));
  const completeCards = cards.filter((card): card is ChangeCard => card !== null);
  const changesSlide: Slide | null = completeCards.length ? (() => {
    const shown = completeCards.slice(-MAX_CHANGE_CARDS);
    return { id: 'changes' as const, name: SLIDE_NAMES.changes, kicker: SLIDE_NAMES.changes, headline: changesHeadline(shown), note: '前後の4週の応募件数。応募は掲載の時期やほかの求人の動きでも変わるため、変更だけが理由とは限りません。', cards: shown };
  })() : null;

  const rowsInPeriod = input.applicants.rows?.filter(row => row.date !== null && row.date >= from && row.date <= to) ?? null;
  const reasons = input.applicants.reasons ? reasonBars(input.applicants.reasons, from, to) : null;

  const nextItems = options.nextActions.slice(0, MAX_NEXT_ACTIONS).map(item => ({ text: item.text.trim(), sub: item.sub.trim() })).filter(item => item.text);

  // 表紙
  const footer = [options.sender.trim(), options.owner.trim() ? `担当：${options.owner.trim()}` : '', fullDayJa(options.createdOn)].filter(Boolean).join(' ／ ');
  const addressee = options.addressee.trim();
  const subject = options.subject.trim() || input.name;
  slides.push({ id: 'cover', name: SLIDE_NAMES.cover, kicker: '', headline: '採用活動のご報告', note: null, addressee: addressee ? `${addressee} 御中` : '', title: '採用活動のご報告', subtitle: `${subject} ／ ${periodText(from, to)}`, footer });

  if (current === null) {
    for (const id of ['summary', 'trend', 'listings', 'changes'] as const) exclude(id, '応募日別の件数を読み込めなかったため');
  } else {
    // まとめ
    const kpis: Kpi[] = [{ label: '応募', value: `${String(current)}件`, sub: previous === null ? null : `前の期間 ${String(previous)}件` }];
    if (input.outcomes?.interviews != null) kpis.push({ label: '面接', value: `${String(input.outcomes.interviews)}件`, sub: 'ご契約からの合計' });
    if (input.outcomes?.hires != null) kpis.push({ label: '採用', value: `${String(input.outcomes.hires)}名`, sub: 'ご契約からの合計' });
    if (input.scope === 'deal' && (input.outcomesFailed || input.outcomes?.interviews == null || input.outcomes.hires == null)) excluded.push({ name: 'まとめ（面接・採用）', reason: input.outcomesFailed ? '取引の面接・採用の値を読み込めなかったため外しています' : '取引に面接・採用の値が入っていないため外しています' });
    const live = input.listings.filter(listing => !listing.ended).length;
    kpis.push({ label: '掲載中の求人', value: `${String(live)}件`, sub: input.listings.length > 1 ? `求人 ${String(input.listings.length)}件のうち` : null });
    const billed = input.listings.every(listing => !listing.billingUnavailable) ? input.listings.map(listing => billingInRange(listing.billing, from, end)) : null;
    const costs = billed?.length && billed.every((value): value is number => value !== null) ? billed : null;
    const cost = costs ? costs.reduce((sum, value) => sum + value, 0) : null;
    if (cost !== null && cost > 0) kpis.push({ label: '費用', value: formatYen(cost), sub: current > 0 ? `応募1件あたり ${formatYen(Math.round(cost / current))}` : null });
    else excluded.push({ name: 'まとめ（費用）', reason: cost === 0 ? 'この期間の課金が 0円のため外しています' : 'この期間の課金の実績が無い、または一部の求人で分からないため外しています' });
    const points = [changesSlide?.headline ?? null, reasons && reasons.n > 0 ? reasonsHeadline(reasons.bars) : null, nextItems[0] ? `次は、${nextItems[0].text}` : null].filter((point): point is string => point !== null).slice(0, 3);
    slides.push({ id: 'summary', name: SLIDE_NAMES.summary, kicker: SLIDE_NAMES.summary, headline: summaryHeadline(current, previous), note: null, kpis, points });

    // 応募の推移
    const trend = trendBars(input, from, to, changes.map(change => ({ date: change.date, what: change.what })));
    slides.push({ id: 'trend', name: SLIDE_NAMES.trend, kicker: SLIDE_NAMES.trend, headline: trendHeadline(trend.bars, trend.unit, trend.starts), note: `${trend.unit === 'week' ? '週' : '月'}ごとの応募件数（当社で記録した応募日）。応募は掲載の時期やほかの求人の動きでも変わります。`, unit: trend.unit, bars: trend.bars });

    // 求人ごとの応募
    const known = input.listings.filter(listing => listing.byDate);
    if (input.scope === 'listing' || known.length < 2) exclude('listings', input.scope === 'listing' ? '求人1件の報告のため' : '応募日が分かる求人が1件だけのため');
    else {
      const all = known.map(listing => ({ label: `${listing.title}${listing.ended ? listing.endedOn ? `（${shortDay(listing.endedOn)} 終了）` : '（掲載終了）' : ''}`, count: countApplications(listing.byDate ?? undefined, from, end) }))
        .sort((a, b) => b.count - a.count);
      const head = all.slice(0, MAX_LISTING_BARS);
      const rest = all.slice(MAX_LISTING_BARS);
      const bars = rest.length ? [...head, { label: `ほか ${String(rest.length)}件の求人`, count: rest.reduce((sum, bar) => sum + bar.count, 0) }] : head;
      slides.push({ id: 'listings', name: SLIDE_NAMES.listings, kicker: SLIDE_NAMES.listings, headline: listingsHeadline(head), note: '求人ごとの期間中の応募件数。同じ方が複数の求人に応募した場合は、それぞれの求人で数えています。', bars });
    }

    // 変えたことと前後の応募
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
    const prefix = current !== null && total === current ? `期間中の応募 ${String(total)}件の内訳（件）。` : `期間中の応募のうち、応募日が分かる ${String(total)}件の内訳（件）。`;
    slides.push({ id: 'applicants', name: SLIDE_NAMES.applicants, kicker: SLIDE_NAMES.applicants, headline: applicantsHeadline(groups, total), note: `${prefix}3件未満の地域は「その他」にまとめています。`, total, groups });
  }

  // 応募理由
  if (!input.applicants.reasons) exclude('reasons', input.applicants.missingReason ?? '応募理由を読み込めなかったため');
  else if (!reasons) exclude('reasons', '応募理由を読み込めなかったため');
  else if (reasons.n === 0) exclude('reasons', '期間中に応募理由をうかがえた応募が無いため');
  else slides.push({ id: 'reasons', name: SLIDE_NAMES.reasons, kicker: SLIDE_NAMES.reasons, headline: reasonsHeadline(reasons.bars), note: `応募理由をうかがえた ${String(reasons.n)}件の内訳（複数回答）。お電話でうかがった内容と応募時のご記入から分けています。`, n: reasons.n, bars: reasons.bars });

  // 次に行うこと
  if (!nextItems.length) exclude('next', '「次に行うこと」が未記入のため');
  else slides.push({ id: 'next', name: SLIDE_NAMES.next, kicker: SLIDE_NAMES.next, headline: options.nextHeadline.trim() || (nextItems[0]?.text ?? SLIDE_NAMES.next), note: null, items: nextItems });

  slides.sort((a, b) => SLIDE_ORDER.indexOf(a.id) - SLIDE_ORDER.indexOf(b.id));
  return { slides, excluded, period: { from, to, previousFrom, previousTo, clamped } };
}

/** 既定の期間: 応募データの最後の日までの12週。 */
export function defaultPeriod(asOf: string): { from: string; to: string } {
  return { from: addDays(asOf, -(12 * 7 - 1)), to: asOf };
}

// ---- 下書き（期間・表紙・入れるスライド・次に行うこと）。画面の中だけに持つ ----

export interface ReportDraft {
  from: string;
  to: string;
  addressee: string;
  subject: string;
  owner: string;
  off: SlideId[];
  next: NextAction[];
  nextHeadline: string;
}
