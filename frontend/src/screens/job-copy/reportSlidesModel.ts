/**
 * 顧客報告のスライド。並びは定期ミーティングの資料に合わせる（2026-11-20 利用者決定）:
 * 表紙 → 目次 → 掲載求人 → 採用の導線 → 採用手法別の進捗 → 数字の表 → 総評と前回との比較 → 求人ごとの現状と課題仮説
 * → 応募者への対応方針 → 求人ごとの次の施策 → 次回までの目標と行動計画 → アンケートと次回の日程。
 * 集計は既存のものを使い回し、ここでは「どのスライドを出すか」「図・表に渡す数」だけを決める。画面に依存しない。
 *
 * 決まり:
 * - 数字は集計から写すだけ。文章（総評・課題仮説・対応方針・施策・行動計画）はコンサルタントが書いたものをそのまま出す。
 * - 数字は「前回の MTG の期間」と並べる（前回 → 今回）。判定の言葉・割合の差は出さない。媒体の費用は出さない。
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
import { AREA_OTHER, AREA_UNKNOWN, mergeSmallAreas } from './applicantArea';
import { REASON_CATEGORIES, selectedCategory } from './reasonCategories';
import type { ReasonCategory } from './reasonCategories';
import type { HrhPerformanceRow } from './hrhPerformanceModel';

export const WINDOW_DAYS = 28;
/** 応募者の傾向・応募理由を読む求人の数の上限（案件の単位）。超えるときは読まずにスライドを外す。 */
export const APPLICANT_LISTING_LIMIT = 20;
/** 週の棒がこれより多いときは月ごとにまとめる。 */
const MAX_WEEK_BARS = 26;
/** お住まいの棒の数（名前の出る都道府県）。残りは「その他」に足す。 */
const MAX_AREA_BARS = 6;
/** 変えたことと前後のカードの数（新しいものから）。 */
const MAX_CHANGE_CARDS = 4;
/** 見出しに入れる求人名の長さ。超えたら省いて「…」。 */
export const HEADLINE_TITLE_CHARS = 20;
/** 求人の名前の長さ。 */
const BAR_LABEL_CHARS = 28;
/** 表紙の差出人の会社名（設定する1か所）。空欄なら担当者名だけを出す。 */
export const REPORT_SENDER = '';
/** 電話でうかがった応募理由の分類（HubSpot の応募の項目）。 */
export const HEARING_CATEGORY_SOURCE = 'ouboriyuukategori_hiaringu';

export type ReportScope = 'deal' | 'listing';
/** 決まった名前のスライド。求人ごとのスライドは `listing:<求人ID>`（現状と課題仮説）・`measure:<求人ID>`（次の施策）。数字の表の続きは `numbers:<枚目>`。 */
export type FixedSlideId = 'cover' | 'toc' | 'listings' | 'route' | 'goals' | 'numbers' | 'review' | 'trend' | 'changes' | 'applicants' | 'reasons' | 'around' | 'policy' | 'plan' | 'closing';
export type SlideId = FixedSlideId | `numbers:${string}` | `listing:${string}` | `measure:${string}`;
export const SLIDE_NAMES: Record<FixedSlideId, string> = {
  cover: '表紙', toc: '目次', listings: '掲載求人', route: '採用の導線', goals: '採用手法別の進捗', numbers: '数字の表', review: '総評と前回との比較',
  trend: '応募の推移', changes: '変えたことと前後の応募', applicants: '応募者の傾向', reasons: '応募理由', around: '周りの求人の様子',
  policy: '応募者への対応方針', plan: '次回までの目標と行動計画', closing: 'アンケートと次回の日程',
};
/** 定期ミーティングの資料の並びに無い、補足のスライド。画面では最初は入れない（左で選べば入る）。 */
export const SUPPLEMENT_SLIDES: readonly FixedSlideId[] = ['trend', 'changes', 'applicants', 'reasons', 'around'];
/** 目次の章。 */
export const SECTIONS = { listings: '掲載求人のご確認', results: '成果報告', proposal: 'ご提案', survey: 'アンケートのお願い' } as const;
/** 表紙の題の既定。 */
export const DEFAULT_TITLE = '運用戦略';
/** スライドの下の帯に出す名前。 */
export const DECK_NAME = '定期ミーティング資料';
/** 採用の導線（決まった説明）。 */
export const ROUTE_NODES: readonly string[] = ['求人広告', '直接応募', 'ハローワーク', '直接のお問い合わせ', 'ホームページ'];
export const ROUTE_CAPTION = 'さまざまなルートに導線を確保し、見ていただける機会を増やします';
const MEDIA_LABELS = { hrh: 'HRハッカー', airwork: 'AirWork', reclogi: 'リクロジ' } as const;
/** 数字の表の行の数（AirWork 以外／AirWork）。残りは「ほか N件の求人」にまとめる。 */
const MAX_TABLE_ROWS = 8;
const MAX_AIRWORK_ROWS = 4;
/**
 * 数字の表の1枚に入る行の数（列の見出しを除く。媒体の見出し・合計も1行と数える）。超える分は次の1枚に送る。
 * 印刷の大きさ（10in × 5.625in）で、列の見出しと10行がちょうど入る。1行の余裕を残して9行にする。
 */
const NUMBER_LINES_PER_SLIDE = 9;
/** ご契約からの合計（面接・採用）の段が使う行の数。 */
const TOTALS_LINES = 4;
/** 求人名が折り返さずに入る文字数（表示回数などの列があるとき／応募数だけのとき）。 */
const NUMBER_LABEL_CHARS = { perf: 35, plain: 55 } as const;
/** 求人ごとのスライド（現状と課題仮説）を作る求人の数。 */
export const MAX_DETAIL_LISTINGS = 6;
/** 掲載求人のスライドに並べる求人名の数。 */
const MAX_LISTING_NAMES = 10;
/** 職種ごとの目標・対応方針の場合分けの欄の数。 */
export const MAX_GOALS = 4;
export const MAX_POLICIES = 3;
/** MTG の記録が無いときの期間（丸い週の数）。 */
export const DEFAULT_WEEKS = 4;

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
  /** 媒体の週ごとの実績（表示回数・クリック数）。HRハッカーは Indeed、AirWork は AirWork。記録が無いときは null。費用は紙に出さない。 */
  perf?: readonly HrhPerformanceRow[] | null | undefined;
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
  /** この案件の MTG の記録の日（古い順、JST の暦日）。読めなかった・求人の単位は null。 */
  mtgDates?: readonly string[] | null | undefined;
}

/** 求人ごとに書く文章。 */
export interface ListingText { catch: string; hypothesis: string; appeal: string; measure: string }
/** コンサルタントが書く文章（数字は入れない。数字は集計から写す）。 */
export interface ReportTexts {
  goals: { name: string; goal: string }[];
  mediaFree: string; mediaPaid: string; mediaOther: string;
  overall: string; outlook: string;
  listings: Record<string, ListingText>;
  policyTitle: string;
  policies: { when: string; how: string }[];
  commonGoal: string; planOurs: string; planClient: string; planGoal: string;
  survey: string; nextMeeting: string; memo: string;
}
export const emptyListingText = (): ListingText => ({ catch: '', hypothesis: '', appeal: '', measure: '' });
export function emptyTexts(): ReportTexts {
  return {
    goals: Array.from({ length: MAX_GOALS }, () => ({ name: '', goal: '' })), mediaFree: '', mediaPaid: '', mediaOther: '',
    overall: '', outlook: '', listings: {}, policyTitle: '', policies: Array.from({ length: MAX_POLICIES }, () => ({ when: '', how: '' })),
    commonGoal: '', planOurs: '', planClient: '', planGoal: '', survey: '', nextMeeting: '', memo: '',
  };
}

export interface ReportOptions {
  /** 今回の期間（前回の MTG から、応募データの最後の日まで）。 */
  from: string;
  to: string;
  /** 前回の期間（前回の MTG のときに報告した期間）。省くと、今回と同じ長さの直前の期間。 */
  previousFrom?: string | undefined;
  previousTo?: string | undefined;
  addressee: string;
  /** 表紙の題。空欄なら「運用戦略」。 */
  subject: string;
  owner: string;
  sender: string;
  /** 定期 MTG の日（表紙に出す）。 */
  createdOn: string;
  texts?: ReportTexts | undefined;
  /** 入れないスライド。目次にも使わない。 */
  off?: readonly string[] | undefined;
  /** 面接・採用（案件に手入力した累計）を出すか。既定は出す（2026-11-16 利用者決定）。媒体の費用は報告に書かない。 */
  include?: { outcomes?: boolean } | undefined;
}

export interface Bar { label: string; count: number }
export interface TrendBar extends Bar { marks: string[] }
export interface ChangeCard { date: string; listingId: string; title: string; what: string; before: number; after: number }
export interface Kpi { label: string; value: string; sub: string | null }
/** 数字の表の1行。分からない数は null（紙では「—」）。 */
export interface NumberRow { label: string; impressions: number | null; clicks: number | null; ctr: string | null; applications: number | null }
export interface NumberGroup { name: string | null; period: string | null; rows: NumberRow[]; total: NumberRow | null }
export interface TextBox { title: string; lines: string[] }

interface NumbersSlide { kind: 'numbers'; hasPerf: boolean; groups: NumberGroup[]; /** ご契約からの合計（面接・採用）。期間の数字とは別の段に出す。 */ totals: { heading: string; kpis: Kpi[] } | null }
/** blank: 文章が未記入のスライド。画面には書く場所として出すが、PDF・目次には入れない。 */
interface SlideBase { name: string; kicker: string; headline: string; note: string | null; blank?: boolean | undefined }
export type Slide = SlideBase & (
  | { id: 'cover'; kind: 'cover'; addressee: string; title: string; subtitle: string; footer: string }
  | { id: 'toc'; kind: 'toc'; items: string[] }
  | { id: 'listings'; kind: 'listings'; names: string[]; total: number }
  | { id: 'route'; kind: 'route'; nodes: string[]; message: string }
  | { id: 'goals'; kind: 'goals'; goals: { name: string; goal: string }[]; media: TextBox[] }
  | ({ id: 'numbers' } & NumbersSlide)
  /** 数字の表の続き（行が多くて1枚に入らない分）。 */
  | ({ id: `numbers:${string}` } & NumbersSlide)
  | { id: 'review'; kind: 'review'; points: string[]; columns: string[]; compare: { label: string; cells: string[] }[]; outlook: string[] }
  | { id: `listing:${string}`; kind: 'listing'; facts: Kpi[]; hypothesis: string[] }
  | { id: 'trend'; kind: 'trend'; unit: 'week' | 'month'; bars: TrendBar[] }
  | { id: 'changes'; kind: 'changes'; cards: ChangeCard[] }
  | { id: 'applicants'; kind: 'applicants'; total: number; groups: { title: string; rows: Bar[] }[] }
  | { id: 'reasons'; kind: 'reasons'; n: number; bars: Bar[] }
  | { id: 'around'; kind: 'around'; kpis: Kpi[]; bars: Bar[] }
  | { id: 'policy'; kind: 'policy'; cases: { when: string[]; how: string[] }[] }
  | { id: `measure:${string}`; kind: 'measure'; boxes: TextBox[] }
  | { id: 'plan'; kind: 'plan'; goal: string; columns: TextBox[] }
  | { id: 'closing'; kind: 'closing'; boxes: TextBox[] }
);
export interface Excluded { name: string; reason: string }
export interface ReportResult {
  slides: Slide[];
  excluded: Excluded[];
  period: { from: string; to: string; previousFrom: string; previousTo: string; clamped: boolean };
  /** 求人ごとのスライドを作った求人（文章を書く欄に使う）。 */
  detailListings: { id: string; title: string }[];
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

// ---- 見出しの1文（補足のスライド） ----

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

const smallAreasMerged = (distribution: ApplicantDistribution | null) => distribution && mergeSmallAreas(distribution);
/** 応募者の傾向。性別・年代・都道府県のそれぞれ単独の内訳（顧客に渡すので、3件未満の都道府県は「その他」）。 */
export function applicantGroups(rows: readonly ApplicationRow[]): { title: string; rows: Bar[] }[] {
  const attributes = rows.map(row => ({ gender: normalizeGender(row.gender), age: row.age ? Number(row.age) : null, prefecture: row.prefecture, municipality: row.municipality }));
  return [
    { title: '性別', rows: distributionRows(buildDistribution(attributes, 'gender'), true) },
    { title: '年代', rows: distributionRows(buildDistribution(attributes, 'age'), false) },
    { title: 'お住まい', rows: capWithOther(distributionRows(smallAreasMerged(buildDistribution(attributes, 'prefecture')), true), MAX_AREA_BARS) },
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

/** 求人の名前。同じ求人名があるときは媒体と勤務地を添える。 */
function listingLabels(listings: readonly ReportListing[]): Map<string, string> {
  const counts = new Map<string, number>();
  for (const listing of listings) counts.set(listing.title, (counts.get(listing.title) ?? 0) + 1);
  return new Map(listings.map(listing => {
    const extra = (counts.get(listing.title) ?? 0) > 1 ? `［${[MEDIA_LABELS[listing.media], listing.place ?? ''].filter(Boolean).join('・')}］` : '';
    const ended = listing.ended ? listing.endedOn ? `（${shortDay(listing.endedOn)} 終了）` : '（掲載終了）' : '';
    return [listing.id, `${clip(listing.title, BAR_LABEL_CHARS)}${extra}${ended}`];
  }));
}

// ---- 期間（前回の MTG から決める） ----

export interface ReportPeriods { from: string; to: string; previousFrom: string; previousTo: string }
/**
 * 前回の MTG の日から期間を決める。今回 = 前回の MTG の日〜応募データの最後の日。
 * 前回 = その前の MTG の日〜前回の MTG の前日（その前の MTG が無ければ、今回と同じ長さの直前の期間）。
 */
export function periodsFromMtg(mtg: string, earlier: string | null, asOf: string): ReportPeriods {
  const from = mtg > asOf ? asOf : mtg;
  const length = daysBetween(from, addDays(asOf, 1));
  return { from, to: asOf, previousFrom: earlier !== null && earlier < from ? earlier : addDays(from, -length), previousTo: addDays(from, -1) };
}
/** MTG の記録のうち、期間の基準にできる日（今日より前で、応募データの最後の日まで）。古い順。 */
export function usableMtgDates(mtgDates: readonly string[] | null | undefined, asOf: string, today: string): string[] {
  return [...new Set(mtgDates ?? [])].filter(date => /^\d{4}-\d{2}-\d{2}$/.test(date) && date < today && date <= asOf).sort();
}
/** 既定の期間。MTG の記録があれば前回の MTG から、無ければ直近の丸い4週と、その前の4週。mtg は基準にした MTG の日。 */
export function initialPeriods(mtgDates: readonly string[] | null | undefined, asOf: string, today: string): ReportPeriods & { mtg: string | null } {
  const past = usableMtgDates(mtgDates, asOf, today);
  const mtg = past.at(-1) ?? null;
  if (mtg !== null) return { ...periodsFromMtg(mtg, past.at(-2) ?? null, asOf), mtg };
  const period = alignedPeriod(asOf, DEFAULT_WEEKS);
  return { ...period, previousFrom: addDays(period.from, -7 * DEFAULT_WEEKS), previousTo: addDays(period.from, -1), mtg: null };
}

// ---- 媒体の数字（表示回数・クリック数） ----

export interface PerfSum { impressions: number; clicks: number; from: string; to: string }
/**
 * 期間にすっぽり入る週の記録を足す。期間をまたぐ週は入れない（どちらの期間の数か分けられないため）。
 * 入る週が無い・数が空の週がある・週が途中で欠けているときは null（一部だけの数を出さない）。from〜to は実際に足した週の範囲。
 */
export function perfBetween(rows: readonly HrhPerformanceRow[] | null | undefined, from: string, to: string): PerfSum | null {
  const inside = [...(rows ?? [])].filter(row => row.period_start >= from && row.period_end <= to).sort((a, b) => a.period_start.localeCompare(b.period_start));
  const first = inside[0];
  const last = inside.at(-1);
  if (!first || !last) return null;
  let impressions = 0;
  let clicks = 0;
  for (const [index, row] of inside.entries()) {
    const before = inside[index - 1];
    if (row.impressions === null || row.clicks === null || (before && addDays(before.period_end, 1) !== row.period_start)) return null;
    impressions += row.impressions;
    clicks += row.clicks;
  }
  return { impressions, clicks, from: first.period_start, to: last.period_end };
}
const count = (value: number) => value.toLocaleString('ja-JP');
/** クリック率（クリック数 ÷ 表示回数）。小数1桁。 */
export function ctrText(perf: Pick<PerfSum, 'impressions' | 'clicks'> | null): string | null {
  return perf && perf.impressions > 0 ? `${(perf.clicks / perf.impressions * 100).toFixed(1)}%` : null;
}
const slashDay = (date: string) => date.replaceAll('-', '/');
const rangeText = (from: string, to: string) => `${slashDay(from)}〜${slashDay(to)}`;
/** 文章の欄を行に分ける（空の行は捨てる）。 */
export const lines = (text: string | undefined) => (text ?? '').split('\n').map(line => line.trim()).filter(Boolean);

interface Counted { listing: ReportListing; label: string; cur: PerfSum | null; prev: PerfSum | null; apps: number | null; prevApps: number | null }
const sumPerf = (list: readonly Counted[], key: 'cur' | 'prev'): { impressions: number; clicks: number } | null =>
  list.length && list.every(item => item[key]) ? { impressions: list.reduce((sum, item) => sum + (item[key]?.impressions ?? 0), 0), clicks: list.reduce((sum, item) => sum + (item[key]?.clicks ?? 0), 0) } : null;
const sumApps = (list: readonly Counted[]): number | null => list.every(item => item.apps !== null) ? list.reduce((sum, item) => sum + (item.apps ?? 0), 0) : null;
function numberRow(label: string, perf: { impressions: number; clicks: number } | null, applications: number | null): NumberRow {
  return { label, impressions: perf?.impressions ?? null, clicks: perf?.clicks ?? null, ctr: ctrText(perf), applications };
}
/** 表の1つのまとまり（AirWork 以外／AirWork）。行が多いときは残りを1行にまとめる。 */
function numberGroup(name: string | null, list: readonly Counted[], limit: number, withTotal: boolean): NumberGroup | null {
  if (!list.length) return null;
  const head = list.slice(0, limit);
  const rest = list.slice(limit);
  const rows = head.map(item => numberRow(item.label, item.cur, item.apps));
  if (rest.length) rows.push(numberRow(`ほか ${String(rest.length)}件の求人`, sumPerf(rest, 'cur'), sumApps(rest)));
  const ranges = list.map(item => item.cur).filter((perf): perf is PerfSum => perf !== null);
  const from = ranges.map(perf => perf.from).sort()[0];
  const to = ranges.map(perf => perf.to).sort().at(-1);
  return { name, period: from && to ? rangeText(from, to) : null, rows, total: withTotal && list.length > 1 ? numberRow('合計', sumPerf(list, 'cur'), sumApps(list)) : null };
}
/** 数字の表を、1枚に入る行の数で分ける。媒体の途中で分けたときは、次の1枚の見出しに「（続き）」を付ける。長い求人名は折り返す分も数える。 */
function paginateNumbers(groups: readonly NumberGroup[], hasPerf: boolean, withTotals: boolean): NumberGroup[][] {
  const width = hasPerf ? NUMBER_LABEL_CHARS.perf : NUMBER_LABEL_CHARS.plain;
  const cost = (row: NumberRow) => Math.max(1, Math.ceil(Array.from(row.label).length / width));
  const pages: NumberGroup[][] = [[]];
  let used = 0;
  const newPage = () => { pages.push([]); used = 0; };
  for (const group of groups) {
    const items = [...group.rows.map(row => ({ row, total: false })), ...(group.total ? [{ row: group.total, total: true }] : [])];
    const head = group.name ? 1 : 0;
    let start = 0;
    while (start < items.length) {
      // 見出しと1行も入らないなら次の1枚へ（媒体の見出しだけが下に残らないように）。
      const first = items[start];
      if (used > 0 && first && used + head + cost(first.row) > NUMBER_LINES_PER_SLIDE) newPage();
      let end = start;
      let lines = used + head;
      for (let item = items[end]; item && (end === start || lines + cost(item.row) <= NUMBER_LINES_PER_SLIDE); item = items[end]) { lines += cost(item.row); end += 1; }
      const chunk = items.slice(start, end);
      pages[pages.length - 1]?.push({ name: group.name && start > 0 ? `${group.name}（続き）` : group.name, period: group.period, rows: chunk.filter(item => !item.total).map(item => item.row), total: chunk.find(item => item.total)?.row ?? null });
      used = lines;
      start = end;
      if (start < items.length) newPage();
    }
  }
  if (withTotals && used > 0 && used + TOTALS_LINES > NUMBER_LINES_PER_SLIDE) newPage();
  return pages;
}
const arrow = (before: string | null, after: string | null) => before === null && after === null ? '—' : `${before ?? '—'} → ${after ?? '—'}`;
const countOrNull = (value: number | null | undefined, unit = '') => value == null ? null : `${count(value)}${unit}`;

export function buildReport(input: ReportInput, options: ReportOptions): ReportResult {
  const { from, to, clamped } = clampPeriod(options.from, options.to, input.asOf);
  const end = addDays(to, 1);
  const length = daysBetween(from, end);
  const given = options.previousFrom && options.previousTo && options.previousFrom <= options.previousTo && options.previousTo < from;
  const previousFrom = given && options.previousFrom ? options.previousFrom : addDays(from, -length);
  const previousTo = given && options.previousTo ? options.previousTo : addDays(from, -1);
  const previousEnd = addDays(previousTo, 1);
  const off = new Set(options.off ?? []);
  const texts = options.texts ?? emptyTexts();
  const excluded: Excluded[] = [];
  const slides: Slide[] = [];
  const exclude = (id: FixedSlideId, reason: string) => { excluded.push({ name: SLIDE_NAMES[id], reason }); };
  if (clamped) excluded.push({ name: '期間', reason: `応募データは ${shortDay(input.asOf)} までのため、期間の終わりを ${shortDay(to)} にしています` });

  const unknownListings = input.listings.filter(listing => !listing.byDate);
  const partial = [...(input.partial ?? [])];
  if (unknownListings.length) partial.push(`応募日を読み込めなかった求人が ${String(unknownListings.length)}件あります（${unknownListings.slice(0, 3).map(listing => clip(listing.title, HEADLINE_TITLE_CHARS)).join('・')}${unknownListings.length > 3 ? ' ほか' : ''}）`);
  const current = applicationsBetween(input.listings, from, end, input.overlap);
  const previous = current !== null && previousKnown(input.listings, previousFrom) ? applicationsBetween(input.listings, previousFrom, previousEnd, input.overlap) : null;

  const changes = current === null ? [] : changesInPeriod(input.listings, from, to);
  const completeCards = changes.map(change => changeCard(change, input.asOf)).filter((card): card is ChangeCard => card !== null);
  const multi = new Set(input.applicants.multiKeys ?? []);
  const rowsInPeriod = input.applicants.rows?.filter(row => row.date !== null && row.date >= from && row.date <= to) ?? null;
  const reasons = input.applicants.reasons ? reasonBars(input.applicants.reasons, from, to, multi) : null;

  // 求人ごとの数字（今回・前回）。掲載を終えていて、今回の期間に応募も媒体の数字も無い求人は表に入れない。
  const labels = listingLabels(input.listings);
  const counted: Counted[] = input.listings.map(listing => ({
    listing, label: labels.get(listing.id) ?? listing.title,
    cur: perfBetween(listing.perf, from, to), prev: perfBetween(listing.perf, previousFrom, previousTo),
    apps: listing.byDate ? countApplications(listing.byDate, from, end) : null,
    // 前回の応募数は、前回の期間の初めにはもう掲載していた（または応募があった）求人だけ。それより後に始めた求人の 0 件とは区別する。
    prevApps: listing.byDate && ((listing.events?.startOn != null && listing.events.startOn <= previousFrom) || previousKnown([listing], previousFrom)) ? countApplications(listing.byDate, previousFrom, previousEnd) : null,
  })).filter(item => !item.listing.ended || (item.apps ?? 0) > 0 || item.cur !== null)
    .map((item, index) => ({ item, index })).sort((a, b) => (b.item.apps ?? -1) - (a.item.apps ?? -1) || (b.item.cur?.clicks ?? -1) - (a.item.cur?.clicks ?? -1) || a.index - b.index).map(({ item }) => item);
  const main = counted.filter(item => item.listing.media !== 'airwork');
  const airwork = counted.filter(item => item.listing.media === 'airwork');
  const hasPerf = counted.some(item => item.cur !== null);
  const hasNumbers = counted.length > 0 && (hasPerf || counted.some(item => item.apps !== null));
  const detail = hasNumbers ? counted.slice(0, MAX_DETAIL_LISTINGS) : [];

  // 表紙
  const footer = [options.sender.trim(), options.owner.trim() ? `担当：${options.owner.trim()}` : ''].filter(Boolean).join(' ／ ');
  const addressee = options.addressee.trim();
  const title = options.subject.trim() || DEFAULT_TITLE;
  slides.push({ id: 'cover', kind: 'cover', name: SLIDE_NAMES.cover, kicker: '', headline: title, note: null, addressee: addressee ? `${addressee} 御中` : '', title, subtitle: `定期MTG ${slashDay(options.createdOn)}`, footer });

  // 掲載求人
  const live = input.listings.filter(listing => !listing.ended);
  if (input.partial?.length) exclude('listings', `求人の一部しか読んでいないため（${input.partial.join('。')}）`);
  else if (!live.length) exclude('listings', '掲載中の求人が無いため');
  else {
    const byTitle = new Map<string, number>();
    for (const listing of live) byTitle.set(listing.title, (byTitle.get(listing.title) ?? 0) + 1);
    const names = [...byTitle].map(([name, n]) => n > 1 ? `${clip(name, BAR_LABEL_CHARS)}（${String(n)}求人）` : clip(name, BAR_LABEL_CHARS));
    const shown = names.slice(0, MAX_LISTING_NAMES);
    if (names.length > shown.length) shown.push(`ほか ${String(names.length - shown.length)}種類`);
    slides.push({ id: 'listings', kind: 'listings', name: SLIDE_NAMES.listings, kicker: SECTIONS.listings, headline: '掲載中の求人', note: null, names: shown, total: live.length });
  }

  // 採用の導線（決まった説明）
  slides.push({ id: 'route', kind: 'route', name: SLIDE_NAMES.route, kicker: SECTIONS.listings, headline: '採用の導線', note: null, nodes: [...ROUTE_NODES], message: ROUTE_CAPTION });

  // 採用手法別の進捗（職種ごとの目標と、媒体の枠）
  const goals = texts.goals.map(goal => ({ name: goal.name.trim(), goal: goal.goal.trim() })).filter(goal => goal.name || goal.goal).slice(0, MAX_GOALS);
  const media = [{ title: '無料の媒体', lines: lines(texts.mediaFree) }, { title: '有料の媒体', lines: lines(texts.mediaPaid) }, { title: 'その他', lines: lines(texts.mediaOther) }];
  const goalsEmpty = !goals.length && media.every(box => !box.lines.length);
  if (goalsEmpty) exclude('goals', '職種ごとの目標・媒体の枠が未記入のため');
  slides.push({ id: 'goals', kind: 'goals', name: SLIDE_NAMES.goals, kicker: SECTIONS.listings, headline: '採用手法別の進捗', note: null, goals, media, blank: goalsEmpty });

  // 数字の表
  const periodNote = `期間：${rangeText(from, to)}`;
  if (!hasNumbers) {
    exclude('numbers', counted.length ? '応募日別の件数も、媒体の週ごとの記録も読み込めなかったため' : '期間中に掲載していた求人が無いため');
    exclude('review', '数字の表を作れないため');
  } else {
    excluded.push({ name: '数字の表（応募開始数）', reason: '応募開始数は案件管理に記録が無いため、表に入れていません' });
    if (!hasPerf) excluded.push({ name: '数字の表（表示回数・クリック数）', reason: 'この期間にすっぽり入る週の記録がまだ取れていないため、応募数だけの表にしています' });
    else {
      const missing = counted.filter(item => item.cur === null);
      if (missing.length) excluded.push({ name: '数字の表（表示回数・クリック数）', reason: `週の記録がまだ取れていない求人は「—」にしています（${missing.slice(0, 3).map(item => clip(item.listing.title, HEADLINE_TITLE_CHARS)).join('・')}${missing.length > 3 ? ' ほか' : ''}）` });
      if (new Set(counted.flatMap(item => item.cur ? [`${item.cur.from}/${item.cur.to}`] : [])).size > 1) excluded.push({ name: '数字の表（表示回数・クリック数）', reason: '求人によって、数えた週の範囲が違います。表の下の期間は、いちばん広い範囲です' });
    }
    if (partial.length) excluded.push({ name: '数字の表', reason: `求人の一部しか数えられません（${partial.join('。')}）` });
    const groups = [numberGroup(airwork.length ? 'Indeed ほか' : null, main, MAX_TABLE_ROWS, true), numberGroup('AirWork', airwork, MAX_AIRWORK_ROWS, false)].filter((group): group is NumberGroup => group !== null);
    // 面接・採用は期間の数ではなく、案件を読んだ日までの累計。期間の数字とは段を分け、読んだ日を見出しに書く。
    const totalKpis: Kpi[] = [];
    if (input.scope === 'deal' && options.include?.outcomes !== false) {
      const { interviews = null, hires = null } = input.outcomes ?? {};
      if (interviews !== null) totalKpis.push({ label: '面接', value: `${String(interviews)}件`, sub: null });
      if (hires !== null) totalKpis.push({ label: '採用', value: `${String(hires)}名`, sub: null });
      if (input.outcomesFailed || !input.outcomes) excluded.push({ name: '数字の表（面接・採用）', reason: '面接・採用は読み込めなかったため外しています' });
      else if (interviews === null && hires === null) excluded.push({ name: '数字の表（面接・採用）', reason: '面接・採用は案件に記録が無いため外しています' });
      else if (interviews === null) excluded.push({ name: '数字の表（面接）', reason: '面接は案件に記録が無いため外しています' });
      else if (hires === null) excluded.push({ name: '数字の表（採用）', reason: '採用は案件に記録が無いため外しています' });
    }
    const totals = totalKpis.length && input.outcomes ? { heading: `ご契約からの合計（${shortDay(input.outcomes.readOn)} 時点）`, kpis: totalKpis } : null;
    const perfPeriods = groups.filter(group => group.period && group.period !== rangeText(from, to)).map(group => `${group.name ?? '表示回数・クリック数'}：${group.period ?? ''}`);
    // 行が多いときは次の1枚に分ける（枠からはみ出して下の帯と重ならないように）。合計の段は最後の1枚に入れる。
    const pages = paginateNumbers(groups, hasPerf, totals !== null);
    pages.forEach((pageGroups, i) => {
      const last = i === pages.length - 1;
      const more = pages.length > 1 ? `（${String(i + 1)}/${String(pages.length)}）` : '';
      slides.push({
        id: i === 0 ? 'numbers' : `numbers:${String(i + 1)}`, kind: 'numbers', name: `${SLIDE_NAMES.numbers}${more}`, kicker: SECTIONS.results, headline: `期間中の数字${more}`,
        note: `${[periodNote, ...perfPeriods].join('\u3000')}。応募数は当社で記録した応募日で数えています。`, hasPerf, groups: pageGroups, totals: last ? totals : null,
      });
    });

    // 総評と前回との比較
    const points = lines(texts.overall);
    const outlook = lines(texts.outlook);
    const columns = hasPerf ? ['応募数', '表示回数', 'クリック数', 'クリック率'] : ['応募数'];
    const cells = (apps: [number | null, number | null], prev: { impressions: number; clicks: number } | null, cur: { impressions: number; clicks: number } | null) => [
      arrow(countOrNull(apps[0]), countOrNull(apps[1])),
      ...(hasPerf ? [arrow(countOrNull(prev?.impressions), countOrNull(cur?.impressions)), arrow(countOrNull(prev?.clicks), countOrNull(cur?.clicks)), arrow(ctrText(prev), ctrText(cur))] : []),
    ];
    const compare = detail.map(item => ({ label: item.label, cells: cells([item.prevApps, item.apps], item.prev, item.cur) }));
    if (counted.length > 1 && current !== null && !partial.length) compare.unshift({ label: '全体', cells: cells([previous, current], sumPerf(counted, 'prev'), sumPerf(counted, 'cur')) });
    if (previous === null && current !== null) excluded.push({ name: '前回との比較（応募数）', reason: '最初の応募が前回の期間の初めより後のため、全体の応募数は前回と比べていません' });
    if (!points.length) excluded.push({ name: '総評', reason: '総評が未記入のため、前回との比較だけを出しています' });
    slides.push({
      id: 'review', kind: 'review', name: SLIDE_NAMES.review, kicker: SECTIONS.results, headline: '総評と前回との比較',
      note: `前回：${rangeText(previousFrom, previousTo)}（${String(daysBetween(previousFrom, previousEnd))}日間）\u3000今回：${rangeText(from, to)}（${String(length)}日間）。「前回 → 今回」の順です。`, points, columns, compare, outlook,
    });

    // 求人ごとの現状と課題仮説
    for (const item of detail) {
      const text = texts.listings[item.listing.id] ?? emptyListingText();
      const facts: Kpi[] = [];
      if (item.cur) {
        facts.push({ label: '表示回数', value: `${count(item.cur.impressions)}回`, sub: item.prev ? `前回 ${count(item.prev.impressions)}回` : null });
        const ctr = ctrText(item.cur);
        if (ctr) facts.push({ label: 'クリック率', value: ctr, sub: ctrText(item.prev) ? `前回 ${ctrText(item.prev) ?? ''}` : null });
        facts.push({ label: 'クリック数', value: `${count(item.cur.clicks)}回`, sub: item.prev ? `前回 ${count(item.prev.clicks)}回` : null });
      }
      if (item.apps !== null) facts.push({ label: '応募数', value: `${count(item.apps)}件`, sub: item.prevApps === null ? null : `前回 ${count(item.prevApps)}件` });
      const name = item.label;
      slides.push({
        id: `listing:${item.listing.id}`, kind: 'listing', name: `現状と課題仮説：${name}`, kicker: SECTIONS.results,
        headline: text.catch.trim() ? `${name}｜${text.catch.trim()}` : name, note: periodNote, facts, hypothesis: lines(text.hypothesis),
      });
    }
    if (counted.length > detail.length) excluded.push({ name: '求人ごとのスライド', reason: `求人ごとのスライドは応募の多い ${String(MAX_DETAIL_LISTINGS)}件までです（ほか ${String(counted.length - detail.length)}件は数字の表だけ）` });
  }

  // ---- 補足（定期ミーティングの資料の並びには無い。画面で選んだときだけ入れる） ----
  if (current === null) {
    for (const id of ['trend', 'changes'] as const) exclude(id, '応募日別の件数を読み込めなかったため');
  } else {
    if (partial.length) exclude('trend', `求人の一部しか数えられないため（${partial.join('。')}）`);
    else {
      const trend = trendBars(input, from, to, changes.map(change => ({ date: change.date, what: change.what })));
      slides.push({ id: 'trend', kind: 'trend', name: SLIDE_NAMES.trend, kicker: SLIDE_NAMES.trend, headline: trendHeadline(trend.bars, trend.unit, trend.starts, trend.full), note: `${trend.unit === 'week' ? '週' : '月'}ごとの応募件数（当社で記録した応募日）。応募は掲載の時期やほかの求人の動きでも変わります。`, unit: trend.unit, bars: trend.bars });
    }
    // 変えたことと前後の応募（求人ごとの数なので、ほかの求人が数えられなくても出せる）
    if (completeCards.length) {
      const shown = completeCards.slice(-MAX_CHANGE_CARDS);
      slides.push({ id: 'changes', kind: 'changes', name: SLIDE_NAMES.changes, kicker: SLIDE_NAMES.changes, headline: changesHeadline(shown), note: '前後の4週の応募件数。応募は掲載の時期やほかの求人の動きでも変わるため、変更だけが理由とは限りません。', cards: shown });
      if (completeCards.length < changes.length) excluded.push({ name: SLIDE_NAMES.changes, reason: `前後4週の応募データがそろっていない見直し ${String(changes.length - completeCards.length)}件は外しています` });
    } else exclude('changes', changes.length ? '期間中の見直しは、どれも前後4週の応募データがそろっていないため' : '期間中に本文・画像の見直しが無いため');
  }

  // 応募者の傾向
  if (input.applicants.excludedMulti) excluded.push({ name: '応募者の傾向・応募理由', reason: `複数の求人に結び付いた応募 ${String(input.applicants.excludedMulti)}件（求人ごとに数えた数）は、同じ方を重ねて数えないよう外しています` });
  if (rowsInPeriod === null) exclude('applicants', input.applicants.missingReason ?? '応募者の属性を読み込めなかったため');
  else if (!rowsInPeriod.length) exclude('applicants', '期間中に応募日の分かる応募が無いため');
  else {
    const groups = applicantGroups(rowsInPeriod);
    const total = rowsInPeriod.length;
    const prefix = current !== null && !partial.length && total === current ? `期間中の応募 ${String(total)}件の内訳（件）。` : `期間中の応募のうち ${String(total)}件の内訳（件）。`;
    slides.push({ id: 'applicants', kind: 'applicants', name: SLIDE_NAMES.applicants, kicker: SLIDE_NAMES.applicants, headline: applicantsHeadline(groups, total), note: `${prefix}3件未満の都道府県は「その他」にまとめています。`, total, groups });
  }

  // 応募理由
  if (!reasons) exclude('reasons', input.applicants.missingReason ?? '応募理由を読み込めなかったため');
  else if (!reasons.ok) exclude('reasons', reasons.reason);
  else if (reasons.n === 0) exclude('reasons', '期間中に、お電話で応募理由をうかがって分類した応募が無いため');
  else slides.push({ id: 'reasons', kind: 'reasons', name: SLIDE_NAMES.reasons, kicker: SLIDE_NAMES.reasons, headline: reasonsHeadline(reasons.bars), note: `お電話で応募理由をうかがえた ${String(reasons.n)}件の内訳。1件で複数の理由をうかがった場合は、それぞれに数えています。`, n: reasons.n, bars: reasons.bars });

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
        id: 'around', kind: 'around', name: SLIDE_NAMES.around, kicker: before ? `${SLIDE_NAMES.around}（${num(record.month.slice(5, 7))}月の調査・期間より前）` : SLIDE_NAMES.around,
        headline: median === null ? `${when}に調べた周りの求人は ${String(record.count.total)}件でした` : `${when}に調べた周りの求人は ${String(record.count.total)}件、給与の下限の中央値は ${salaryText(median, unit)}でした`,
        note: `${fullDayJa(day)}に Indeed で調べた周りの求人（${place}）${before ? '。報告の期間より前の調査です' : ''}。下の棒は、求人票によく出てくる言葉と、その言葉がある求人の件数です。`,
        kpis, bars: record.keywords_top5.map(([word, n]) => ({ label: word, count: n })),
      });
    }
  }

  // ---- ご提案 ----
  // 応募者への対応方針（場合分け）
  const cases = texts.policies.map(item => ({ when: lines(item.when), how: lines(item.how) })).filter(item => item.when.length || item.how.length).slice(0, MAX_POLICIES);
  if (!cases.length) exclude('policy', '対応方針が未記入のため');
  slides.push({ id: 'policy', kind: 'policy', name: SLIDE_NAMES.policy, kicker: SECTIONS.proposal, headline: texts.policyTitle.trim() ? `応募者への対応方針｜${texts.policyTitle.trim()}` : '応募者への対応方針', note: null, cases, blank: !cases.length });

  // 求人ごとの次の施策
  let measures = 0;
  for (const item of detail) {
    const text = texts.listings[item.listing.id];
    const boxes = [{ title: '訴求のポイント', lines: lines(text?.appeal) }, { title: '施策の内容', lines: lines(text?.measure) }].filter(box => box.lines.length);
    if (boxes.length) measures += 1;
    const name = item.label;
    slides.push({ id: `measure:${item.listing.id}`, kind: 'measure', name: `次の施策：${name}`, kicker: SECTIONS.proposal, headline: `${name}｜次の施策`, note: null, boxes, blank: !boxes.length });
  }
  if (detail.length && !measures) excluded.push({ name: '求人ごとの次の施策', reason: '訴求のポイント・施策の内容が未記入のため' });

  // 次回までの共通目標と三者の行動計画
  const plan = [{ title: '当社の行動計画', lines: lines(texts.planOurs) }, { title: 'ご担当者さまの行動計画', lines: lines(texts.planClient) }, { title: '運用上のゴール', lines: lines(texts.planGoal) }];
  const planEmpty = !texts.commonGoal.trim() && plan.every(box => !box.lines.length);
  if (planEmpty) exclude('plan', '共通目標・行動計画が未記入のため');
  slides.push({ id: 'plan', kind: 'plan', name: SLIDE_NAMES.plan, kicker: SECTIONS.proposal, headline: '次回の定期MTGまでの共通目標', note: null, goal: texts.commonGoal.trim(), columns: plan, blank: planEmpty });

  // アンケート・次回の日程・メモ
  slides.push({
    id: 'closing', kind: 'closing', name: SLIDE_NAMES.closing, kicker: SECTIONS.survey, headline: 'アンケートと次回のお打ち合わせ', note: null,
    boxes: [
      { title: 'お客さまアンケート', lines: lines(texts.survey).length ? lines(texts.survey) : ['アンケートへのご協力をお願いいたします'] },
      { title: '次回のお打ち合わせ日', lines: lines(texts.nextMeeting) },
      { title: 'その他のメモ', lines: lines(texts.memo) },
    ].filter(box => box.lines.length),
  });

  // 目次（入れるスライドのある章だけ）
  const on = (test: (slide: Slide) => boolean) => slides.some(slide => !off.has(slide.id) && !slide.blank && test(slide));
  const items = [
    on(slide => ['listings', 'route', 'goals'].includes(slide.kind)) ? SECTIONS.listings : null,
    on(slide => ['numbers', 'review', 'listing', 'trend', 'changes', 'applicants', 'reasons', 'around'].includes(slide.kind)) ? SECTIONS.results : null,
    on(slide => ['policy', 'measure', 'plan'].includes(slide.kind)) ? SECTIONS.proposal : null,
    on(slide => slide.kind === 'closing') ? SECTIONS.survey : null,
  ].filter((item): item is NonNullable<typeof item> => item !== null);
  slides.splice(1, 0, { id: 'toc', kind: 'toc', name: SLIDE_NAMES.toc, kicker: '', headline: '目次', note: null, items });

  return { slides, excluded, period: { from, to, previousFrom, previousTo, clamped }, detailListings: detail.map(item => ({ id: item.listing.id, title: item.listing.title })) };
}

// ---- 下書き（期間・表紙・入れるスライド・書いた文章）。画面の中だけに持つ ----

export interface ReportDraft {
  from: string;
  to: string;
  previousFrom: string;
  previousTo: string;
  /** 期間の基準にした MTG の日。自分で期間を決めたときは null。 */
  mtg: string | null;
  addressee: string;
  subject: string;
  owner: string;
  /** 定期 MTG の日（表紙）。 */
  meetingOn: string;
  off: string[];
  /** 面接・採用（ご契約からの合計）を出すか。未指定は出す。 */
  outcomes?: boolean | undefined;
  texts: ReportTexts;
}
