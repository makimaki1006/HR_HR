/**
 * 求人の「出来事」: 掲載開始・本文/画像/給与の変更・課金の開始/停止/増減・掲載終了。
 * それぞれの前後4週の応募と、「ほかに応募を動かしそうなこと」の確認を作る。
 *
 * 増えた・減ったの判定語は付けない（判定の基準が合意されていないため）。件数と差だけを出す。
 * 4週 = 28日。前の4週 = 出来事の日の前の28日、後の4週 = 出来事の日から28日（応募を取得した日まで）。
 *
 * 「ほかに応募を動かしそうなこと」の見方（画面の右の欄にも同じ文を出す）:
 * - 課金: 前後4週（出来事の日の前後28日）に、別の課金の変更（開始・停止・20%以上の増減）があれば「要注意」、無ければ「小さい」。
 * - 市場: 職種を指定している求人だけ。前後4週の月の求人数が5%以上動いていれば「要注意」、未満なら「小さい」。
 * - ほかの変更: 前後4週に別の出来事（本文・画像・給与の変更、掲載の開始・終了）があれば「要注意」、無ければ「小さい」。
 */
import type { JobCopyRecord } from './data';
import { addDays, changeKinds, countApplications, daysBetween, formatMonthJa, marketChange, publishedVersions, versionChanges } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import type { MarketRow } from './marketChartModel';

const DATE = /^\d{4}-\d{2}-\d{2}$/;
export const WINDOW_DAYS = 28;
/** 課金の変更を出来事にする最小の変化（4週あたりの額が前の期間と比べて動いた割合）。 */
export const BILLING_EVENT_MIN_PCT = 20;
/** 市場の求人数がこの割合以上動いていたら「要注意」。 */
export const MARKET_CAUTION_PCT = 5;

export type EventKind = 'start' | 'content' | 'billing' | 'end';

export interface ListingEvent {
  id: string;
  /** 古い順の番号（1 から）。グラフの番号と一覧の番号が同じ。 */
  n: number;
  date: string;
  kind: EventKind;
  title: string;
  /** 変えた中身の1行の要約。 */
  summary: string;
  /** 本文・画像・給与の変更のとき、前後の版（全文を比べる画面に渡す）。 */
  versionId: string | null;
  previousVersionId: string | null;
  /** 課金の変更のとき、4週あたりの額（前 → 後）。開始は前が null、停止は後が null。 */
  billingFrom: number | null;
  billingTo: number | null;
}

export interface EventCounts {
  /** 前の4週の応募。比べる前が無い（掲載開始・応募の日別データ無し）とき null。 */
  before: number | null;
  /** 後の4週の応募。応募の日別データが無い・出来事が取得日より後のとき null。 */
  after: number | null;
  /** 前の4週のうち、掲載が始まってからの日数（28 未満なら前は短い期間の数）。 */
  beforeDays: number;
  /** 後の4週のうち、データのある日数（28 未満なら取得日で切れている）。 */
  afterDays: number;
}

const yen = (value: number) => `¥${Math.round(value).toLocaleString('ja-JP')}`;

/** 課金の期間を4週あたりの額にする。額が不明な期間は含めない。 */
function billingSegments(entries: readonly BillingEntry[]): { start: string; end: string; per4: number }[] {
  return entries
    .filter(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= entry.end && entry.amountYen !== null)
    .sort((a, b) => a.start.localeCompare(b.start))
    .map(entry => ({ start: entry.start, end: entry.end, per4: Math.round((entry.amountYen ?? 0) * WINDOW_DAYS / (daysBetween(entry.start, entry.end) + 1)) }));
}

/** 同じ額で続く期間をひとつにまとめた課金の帯（グラフの下の帯）。 */
export function billingBands(entries: readonly BillingEntry[]): { start: string; end: string; per4: number }[] {
  const bands: { start: string; end: string; per4: number }[] = [];
  for (const segment of billingSegments(entries)) {
    const last = bands.at(-1);
    if (last?.per4 === segment.per4 && segment.start <= addDays(last.end, 1)) last.end = segment.end > last.end ? segment.end : last.end;
    else bands.push({ ...segment });
  }
  return bands;
}

interface RawEvent { date: string; kind: EventKind; title: string; summary: string; versionId: string | null; previousVersionId: string | null; billingFrom: number | null; billingTo: number | null }

function billingEvents(entries: readonly BillingEntry[]): RawEvent[] {
  const events: RawEvent[] = [];
  const base = { kind: 'billing' as const, versionId: null, previousVersionId: null };
  const bands = billingBands(entries);
  bands.forEach((band, index) => {
    const prev = bands[index - 1];
    if (!prev) {
      events.push({ ...base, date: band.start, title: `課金を開始（${yen(band.per4)}／4週）`, summary: '求人票は変えていません。', billingFrom: null, billingTo: band.per4 });
      return;
    }
    if (band.start > addDays(prev.end, 1)) {
      const stop = addDays(prev.end, 1);
      events.push({ ...base, date: stop, title: `課金を停止（${yen(prev.per4)}／4週 → なし）`, summary: '求人票は変えていません。', billingFrom: prev.per4, billingTo: null });
      events.push({ ...base, date: band.start, title: `課金を再開（${yen(band.per4)}／4週）`, summary: '求人票は変えていません。', billingFrom: null, billingTo: band.per4 });
      return;
    }
    const pct = prev.per4 === 0 ? (band.per4 === 0 ? 0 : 100) : Math.abs(band.per4 - prev.per4) / prev.per4 * 100;
    if (pct >= BILLING_EVENT_MIN_PCT) {
      events.push({ ...base, date: band.start, title: `課金を${band.per4 > prev.per4 ? '増額' : '減額'}（${yen(prev.per4)} → ${yen(band.per4)}／4週）`, summary: '求人票は変えていません。', billingFrom: prev.per4, billingTo: band.per4 });
    }
  });
  return events;
}

/**
 * 掲載を始めた日のうち、いちばん早いもの。HubSpot の本文の履歴は連携を始めた日からしか無いので、
 * 公開開始日時・Indeed/AirWork の週ごとの実績（表示か課金のある最初の週）も見る。
 * 取り込んだ媒体CSVの最初の検出日は、保存していないので使わない。
 */
export function earliestStart(job: Pick<JobCopyRecord, 'listingStartedOn'>, billing: readonly BillingEntry[]): { date: string; basis: string } | null {
  const candidates: { date: string; basis: string }[] = [];
  if (job.listingStartedOn && DATE.test(job.listingStartedOn)) candidates.push({ date: job.listingStartedOn, basis: 'HubSpot の公開開始日時' });
  const live = billing.filter(entry => entry.source === 'hrhacker' && DATE.test(entry.start) && ((entry.impressions ?? 0) > 0 || (entry.amountYen ?? 0) > 0)).sort((a, b) => a.start.localeCompare(b.start))[0];
  if (live) candidates.push({ date: live.start, basis: `${live.media === 'AirWork' ? 'AirWork' : 'Indeed'} の週ごとの実績の最初の週` });
  return candidates.sort((a, b) => a.date.localeCompare(b.date))[0] ?? null;
}

const KIND_ORDER: Record<EventKind, number> = { start: 0, content: 1, billing: 2, end: 3 };

/** 出来事を古い順に番号を付けて返す。billing は実際の課金（仮の金額は渡さない）。 */
export function buildListingEvents(job: JobCopyRecord, billing: readonly BillingEntry[]): ListingEvent[] {
  const raw: RawEvent[] = [];
  const changes = versionChanges(job);
  const versions = publishedVersions(job);
  const realStart = earliestStart(job, billing);
  const firstRecorded = changes[0]?.date ?? null;
  // 本当の掲載開始が、HubSpot に本文を記録しはじめた日より前にあるとき、開始はそちらにする。
  const startsEarlier = realStart !== null && (firstRecorded === null || realStart.date < firstRecorded);
  if (startsEarlier) raw.push({ date: realStart.date, kind: 'start', title: '掲載を開始', summary: `${realStart.basis}から分かる、掲載を始めた日です。`, versionId: null, previousVersionId: null, billingFrom: null, billingTo: null });
  for (const change of changes) {
    const previous = changes[change.index - 1];
    if (change.index === 0) {
      raw.push(startsEarlier
        ? { date: change.date, kind: 'content', title: 'HubSpot に本文を記録しはじめた', summary: '最初の本文と画像を記録しました。HubSpot の履歴は連携を始めた日からしか残っていないため、掲載を始めた日ではありません。', versionId: change.versionId, previousVersionId: null, billingFrom: null, billingTo: null }
        : { date: change.date, kind: 'start', title: '掲載を開始', summary: '最初の本文と画像を記録しました。', versionId: change.versionId, previousVersionId: null, billingFrom: null, billingTo: null });
      continue;
    }
    const kinds = changeKinds(change);
    if (!kinds.length) continue;
    const salaryText = change.salaryChanged && previous?.salary && change.salary ? `（${previous.salary.raw} → ${change.salary.raw}）` : '';
    const title = kinds.length === 1 && kinds[0] === '給与' ? `給与を変更${salaryText}` : `${kinds.join('と')}を変更`;
    const parts: string[] = [];
    if (change.salaryChanged) parts.push('給与の行を変更');
    if (change.otherBodyChanged) parts.push(`本文を変更（追加${String(change.bodyAdded)}行・削除${String(change.bodyRemoved)}行）`);
    if (kinds.includes('画像')) parts.push('画像を変更');
    raw.push({ date: change.date, kind: 'content', title, summary: `${parts.join('。')}。`, versionId: change.versionId, previousVersionId: previous?.versionId ?? null, billingFrom: null, billingTo: null });
  }
  raw.push(...billingEvents(billing));
  const publication = job.mediaPublication;
  if (publication && publication !== 'unavailable' && publication.status === 'private' && publication.privateRecordedOn && DATE.test(publication.privateRecordedOn)) {
    raw.push({ date: publication.privateRecordedOn, kind: 'end', title: '掲載を終了', summary: '媒体で非公開になったことを記録した日です。実際に止まったのはこの日かそれ以前です。', versionId: versions.at(-1)?.id ?? null, previousVersionId: null, billingFrom: null, billingTo: null });
  }
  raw.sort((a, b) => a.date.localeCompare(b.date) || KIND_ORDER[a.kind] - KIND_ORDER[b.kind]);
  // 掲載開始より前の課金は、そのままでは矛盾して見えるので、前の記録であることを書く。
  const startDate = raw.find(event => event.kind === 'start')?.date ?? null;
  const marked = raw.map(event => event.kind === 'billing' && startDate !== null && event.date < startDate ? { ...event, summary: `掲載を開始した日より前の課金の記録です。求人票は変えていません。` } : event);
  return marked.map((event, index) => ({ ...event, id: `ev-${String(index + 1)}`, n: index + 1 }));
}

/** 出来事の日の前後4週の応募。asOf は応募を取得した日（null なら数えない）。listingStart は掲載を始めた日。 */
export function eventCounts(job: JobCopyRecord, event: Pick<ListingEvent, 'date' | 'kind'>, asOf: string | null, listingStart: string | null): EventCounts {
  const byDate = job.overallApplications?.byDate;
  const none = { before: null, after: null, beforeDays: 0, afterDays: 0 };
  if (!byDate || !asOf) return none;
  const beforeStart = addDays(event.date, -WINDOW_DAYS);
  const beforeDays = event.kind === 'start' || !listingStart ? 0 : Math.max(0, Math.min(WINDOW_DAYS, daysBetween(listingStart, event.date)));
  const afterEnd = addDays(asOf, 1);
  const afterDays = Math.max(0, Math.min(WINDOW_DAYS, daysBetween(event.date, afterEnd)));
  return {
    before: event.kind === 'start' || beforeDays === 0 ? null : countApplications(byDate, beforeStart, event.date),
    after: afterDays === 0 ? null : countApplications(byDate, event.date, addDays(event.date, afterDays)),
    beforeDays, afterDays,
  };
}

export type CheckMark = 'small' | 'caution' | 'unknown';
export interface EventCheck { key: 'billing' | 'market' | 'other'; label: string; mark: CheckMark; markLabel: string; text: string }

/** 市場の状態。rows が null のときは取得できていない（職種の指定の有無は state で分ける）。 */
export interface MarketInput { state: 'unset' | 'none' | 'set'; rows?: readonly MarketRow[] | null | undefined }

/** 2026-08-05 → 8/05 */
export const md = (date: string) => `${String(Number(date.slice(5, 7)))}/${date.slice(8, 10)}`;
function distanceText(event: Pick<ListingEvent, 'date'>, other: Pick<ListingEvent, 'date'>): string {
  const diff = daysBetween(event.date, other.date);
  return diff === 0 ? '同じ日' : diff < 0 ? `${String(-diff)}日前（${md(other.date)}）` : `${String(diff)}日後（${md(other.date)}）`;
}
const plainTitle = (event: ListingEvent) => event.title.replace(/（.*$/u, '');

function marketUnknownText(result: Exclude<ReturnType<typeof marketChange>, { ok: true }>): string {
  if (result.noDataFrom) return `市場のデータは${formatMonthJa(addMonths(result.noDataFrom, -1))}までで、この期間にまだ届いていないため、比べていません。`;
  if (result.reason === 'same_month') return '前後4週が同じ月に入るため、月ごとの市場の数字では比べられません。';
  return 'この期間の市場のデータがないため、比べていません。';
}
function addMonths(month: string, delta: number): string {
  const total = Number(month.slice(0, 4)) * 12 + Number(month.slice(5, 7)) - 1 + delta;
  return `${String(Math.floor(total / 12))}-${String(total % 12 + 1).padStart(2, '0')}`;
}

export function eventChecks(event: ListingEvent, events: readonly ListingEvent[], billing: readonly BillingEntry[], market: MarketInput, asOf: string | null): EventCheck[] {
  const near = (other: ListingEvent) => other.id !== event.id && Math.abs(daysBetween(event.date, other.date)) <= WINDOW_DAYS;
  const checks: EventCheck[] = [];
  if (event.kind !== 'billing') {
    const nearBilling = events.filter(other => other.kind === 'billing' && near(other));
    if (nearBilling.length) {
      checks.push({ key: 'billing', label: '課金', mark: 'caution', markLabel: '要注意', text: `${nearBilling.map(other => `「${plainTitle(other)}」が${distanceText(event, other)}に`).join('、')}あります。` });
    } else {
      const covered = billing.some(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= addDays(event.date, WINDOW_DAYS) && entry.end >= addDays(event.date, -WINDOW_DAYS));
      checks.push(covered
        ? { key: 'billing', label: '課金', mark: 'small', markLabel: '小さい', text: '前後4週に課金の大きな変更はありません。' }
        : { key: 'billing', label: '課金', mark: 'unknown', markLabel: '未確認', text: 'この期間の課金のデータがないため、比べていません。' });
    }
  }
  if (market.state === 'unset') checks.push({ key: 'market', label: '市場', mark: 'unknown', markLabel: '未指定', text: '職種が未指定のため比べていません。' });
  else if (market.state === 'none') checks.push({ key: 'market', label: '市場', mark: 'unknown', markLabel: '未指定', text: '市場と比べないと指定されています。' });
  else if (!market.rows) checks.push({ key: 'market', label: '市場', mark: 'unknown', markLabel: '未確認', text: '市場の数字を取得できていないため、比べていません。' });
  else {
    const last = asOf && addDays(event.date, WINDOW_DAYS) > asOf ? asOf : addDays(event.date, WINDOW_DAYS);
    const result = marketChange(market.rows, addDays(event.date, -WINDOW_DAYS), last);
    if (result.ok) {
      const pct = result.value.changePct;
      const sign = pct > 0 ? '+' : pct < 0 ? '−' : '±';
      const text = `競合の求人数 ${sign}${Math.abs(pct).toFixed(0)}%（${String(result.value.fromJobs)}件 → ${String(result.value.toJobs)}件）。`;
      checks.push({ key: 'market', label: '市場', mark: Math.abs(pct) >= MARKET_CAUTION_PCT ? 'caution' : 'small', markLabel: Math.abs(pct) >= MARKET_CAUTION_PCT ? '要注意' : '小さい', text });
    } else {
      checks.push({ key: 'market', label: '市場', mark: 'unknown', markLabel: '未確認', text: marketUnknownText(result) });
    }
  }
  const others = events.filter(other => other.kind !== 'billing' && near(other));
  if (others.length) {
    const shown = others.slice(0, 2).map(other => `「${plainTitle(other)}」が${distanceText(event, other)}に`);
    checks.push({ key: 'other', label: 'ほかの変更', mark: 'caution', markLabel: '要注意', text: `${shown.join('、')}あります${others.length > 2 ? `（ほか${String(others.length - 2)}件）` : ''}。` });
  } else checks.push({ key: 'other', label: 'ほかの変更', mark: 'small', markLabel: '小さい', text: '前後4週にほかの変更はありません。' });
  return checks;
}

/** 出来事を押したときの前後の窓（グラフの塗り）。before は掲載開始のときなし。 */
export function eventWindows(event: Pick<ListingEvent, 'date' | 'kind'>, asOf: string | null): { before: { start: string; end: string } | null; after: { start: string; end: string } } {
  const afterEnd = asOf && addDays(event.date, WINDOW_DAYS) > asOf ? asOf : addDays(event.date, WINDOW_DAYS);
  return { before: event.kind === 'start' ? null : { start: addDays(event.date, -WINDOW_DAYS), end: event.date }, after: { start: event.date, end: afterEnd < event.date ? event.date : afterEnd } };
}

/** 一覧の結果欄の文字。判定語は付けない。 */
export function resultText(event: Pick<ListingEvent, 'kind'>, counts: EventCounts, unavailable = '応募の件数が分かりません'): string {
  if (counts.after === null) return unavailable;
  if (event.kind === 'start' || counts.before === null) return `最初の4週 ${String(counts.after)} 件`;
  return `${String(counts.before)} → ${String(counts.after)} 件`;
}
