// 営業KPI: 旧画面 (templates/tabs/sales_kpi.html の JS) がクライアント側で計算していた
// 値を、DOM に依存しない純関数として写したもの。式は旧 JS と同じにする
// (calc.legacy.test.ts で旧 JS の関数コピーと同じ入力→同じ出力を確認する)。
// 見た目 (JSX) は SalesKpiView.tsx。ここには文字列の組み立ても含めるが、HTML は作らない。
import type {
  CallPeriodKey,
  Counts,
  DealRow,
  KadenPeriod,
  KetteiNoOwner,
  KetteiRow,
  ListStock,
  Person,
  SalesKpiData,
  Snapshot,
  StockList,
} from './types';

// ---------------------------------------------------------------- 定数

export const ALL_TEAMS = 'すべて';
/** 名簿にチームが無い人に付くチーム名 (Rust `TEAM_NONE` と同じ)。 */
export const TEAM_NONE = 'チーム未設定';
export const WD = ['日', '月', '火', '水', '木', '金', '土'] as const;

/** 直す場所 (スプレッドシート)。旧画面の定数をそのまま。 */
export const SHEETS = {
  roster: {
    url: 'https://docs.google.com/spreadsheets/d/1KLQW1L9SVCqz9_E0qxHbSsoOU4jLh3_2KtC4BK8dCtA/edit',
    sheet: '②メンバー配置_入力',
  },
  exclude: {
    url: 'https://docs.google.com/spreadsheets/d/1jrUspALKO6CNhSzSWJJoJ3VLuSnqfj_eTu3hocO-o2k/edit',
    sheet: 'KPI営業_集計除外',
  },
  listGroups: {
    url: 'https://docs.google.com/spreadsheets/d/1jrUspALKO6CNhSzSWJJoJ3VLuSnqfj_eTu3hocO-o2k/edit',
    sheet: 'KPI営業_リスト区分',
  },
} as const;
export type SheetKey = keyof typeof SHEETS;

/** 架電の期間。この順で chip が並ぶ (ユーザー指示 2026-09-10)。初期は今週。 */
export const CALL_PERIODS: readonly { key: CallPeriodKey; label: string }[] = [
  { key: 'this_week', label: '今週' },
  { key: 'prev_week', label: '先週' },
  { key: 'today', label: '今日' },
  { key: 'yesterday', label: '昨日' },
  { key: 'this_month', label: '今月' },
];
export const DEFAULT_CALL_PERIOD: CallPeriodKey = 'this_week';

export type SnapMode = 'week' | 'month';
export const SNAP = {
  week: {
    chip: 'その週の商談',
    head: 'その週に予定された商談',
    note: 'その週（月〜日）に予定されていた商談だけを数えたもの。毎週おなじ長さの窓なので、',
    noteBold: '週どうしを比べられます',
    noteTail: '。',
  },
  month: {
    chip: '当月の累積',
    head: '当月に予定された商談（月初からの累積）',
    note: 'その月に予定されている商談を月初から積み上げたもの。',
    noteBold: '月が変わるとゼロから数え直しになります。',
    noteTail: '月初の週が前の週より小さいのは、減ったのではなく別の月を見ているためです。',
  },
} as const;

export const KADEN_CLASSES = ['未架電', '未接触', '接触済み'] as const;
export const LS_KINDS = ['アクティブ', '保管'] as const;

// ---------------------------------------------------------------- 書式

export const fmt = (n: number | null | undefined): string =>
  n == null ? '—' : n.toLocaleString('ja-JP');

export const pct = (n: number | null | undefined): string =>
  n == null ? '—' : n.toFixed(1) + '%';

/** `yyyy-MM-dd` (先頭 10 文字) を [y, m, d] に。読めなければ null。 */
export function ymdParts(s: string): [number, number, number] | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s);
  if (!m) return null;
  return [Number(m[1]), Number(m[2]), Number(m[3])];
}

/** `M/D`。旧 JS は Date 経由 (ブラウザの TZ に依存) だったが、JST では同じ結果。 */
export function md(s: string): string {
  const p = ymdParts(s);
  if (!p) return '—';
  return `${String(p[1])}/${String(p[2])}`;
}

/** 曜日 1 文字。 */
export function wd(s: string): string {
  const p = ymdParts(s);
  if (!p) return '—';
  return WD[new Date(Date.UTC(p[0], p[1] - 1, p[2])).getUTCDay()] ?? '—';
}

/** `today − s` の日数 (四捨五入)。どちらかが読めなければ NaN (旧 JS と同じ)。 */
export function ago(today: string, s: string): number {
  const a = ymdParts(today);
  const b = ymdParts(s);
  if (!a || !b) return NaN;
  return Math.round(
    (Date.UTC(a[0], a[1] - 1, a[2]) - Date.UTC(b[0], b[1] - 1, b[2])) / 86400000,
  );
}

/** `yyyy-MM-dd` の start..end (両端を含む) を 1 日ずつ。 */
export function daySpan(start: string, end: string): string[] {
  const a = ymdParts(start);
  const b = ymdParts(end);
  if (!a || !b) return [];
  const out: string[] = [];
  let t = Date.UTC(a[0], a[1] - 1, a[2]);
  const tEnd = Date.UTC(b[0], b[1] - 1, b[2]);
  while (t <= tEnd) {
    const d = new Date(t);
    out.push(
      `${String(d.getUTCFullYear())}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`,
    );
    t += 86400000;
  }
  return out;
}

/** 先頭 10 文字 (旧 JS `TODAY`)。 */
export const todayOf = (d: Pick<SalesKpiData, 'generated_at'>): string =>
  (d.generated_at || '').slice(0, 10);

// ---------------------------------------------------------------- 絞り込み

export interface Scope {
  team: string;
  /** ownerId。空なら個人指定なし。 */
  person: string;
  hidden: ReadonlySet<string>;
}

export const isSalesTeam = (t: string | undefined): boolean => !!t && t !== TEAM_NONE;

/** ownerId → チーム。 */
export function teamOfMap(people: readonly Person[]): Record<string, string> {
  const o: Record<string, string> = {};
  for (const p of people) o[p.id] = p.team;
  return o;
}

/** hidden に入っていない & ok(id) の人の Counts をキーごとに足す。 */
export function sumIf(
  byPerson: Readonly<Record<string, Counts>> | undefined,
  hidden: ReadonlySet<string>,
  ok: (id: string) => boolean,
): Counts {
  const o: Counts = {};
  for (const [id, v] of Object.entries(byPerson ?? {})) {
    if (hidden.has(id) || !ok(id)) continue;
    for (const [k, n] of Object.entries(v)) o[k] = (o[k] ?? 0) + n;
  }
  return o;
}

/** いま選んでいる範囲 (チップ・プルダウン・チェック) で足す。 */
export function sumScope(
  byPerson: Readonly<Record<string, Counts>> | undefined,
  scope: Scope,
  teamOf: Readonly<Record<string, string>>,
): Counts {
  return sumIf(byPerson, scope.hidden, (id) =>
    scope.person ? id === scope.person : scope.team === ALL_TEAMS || teamOf[id] === scope.team,
  );
}

/** チーム (全社なら全チーム) で足す。個人の選択は見ない (平均の分子)。 */
export function sumTeam(
  byPerson: Readonly<Record<string, Counts>> | undefined,
  hidden: ReadonlySet<string>,
  teamOf: Readonly<Record<string, string>>,
  teamName: string | null,
): Counts {
  return sumIf(byPerson, hidden, (id) => !teamName || teamOf[id] === teamName);
}

export const teamLabel = (p: Person | undefined): string =>
  !p ? '' : p.team + (p.team === TEAM_NONE && p.hsTeam ? '・' + p.hsTeam : '');

/** 個人プルダウンに出る人。 */
export function visiblePeople(people: readonly Person[], scope: Scope): Person[] {
  return people.filter(
    (p) => !scope.hidden.has(p.id) && (scope.team === ALL_TEAMS || p.team === scope.team),
  );
}

export const headOf = (people: readonly Person[], hidden: ReadonlySet<string>, t: string): number =>
  people.filter((p) => p.team === t && !hidden.has(p.id)).length;

export interface AvgBase {
  label: string;
  team: string | null;
  n: number;
}

/** 誰と比べるか。個人を選んでいるときは「その人のチーム」。 */
export function avgBase(people: readonly Person[], scope: Scope): AvgBase | null {
  if (scope.person) {
    const p = people.find((x) => x.id === scope.person);
    if (!p) return null;
    return { label: p.team + ' の平均', team: p.team, n: headOf(people, scope.hidden, p.team) };
  }
  if (scope.team !== ALL_TEAMS) {
    return { label: '1人あたり', team: scope.team, n: headOf(people, scope.hidden, scope.team) };
  }
  return {
    label: '1人あたり',
    team: null,
    n: people.filter((p) => !scope.hidden.has(p.id)).length,
  };
}

export interface AvgLine {
  label: string;
  /** `(total/n).toFixed(1) + unit` */
  value: string;
  n: number;
}

/** 平均の 1 行。件数のカードにだけ付ける。 */
export function avgLine(ab: AvgBase | null, total: number, unit = '件'): AvgLine | null {
  if (!ab?.n) return null;
  return { label: ab.label, value: (total / ab.n).toFixed(1) + unit, n: ab.n };
}

/** 取引の一覧を範囲で絞る (旧 `pick`)。 */
export function pickRows<T extends { owner: string; team: string }>(
  rows: readonly T[],
  scope: Scope,
): T[] {
  return rows.filter(
    (r) =>
      !scope.hidden.has(r.owner) &&
      (scope.person ? r.owner === scope.person : scope.team === ALL_TEAMS || r.team === scope.team),
  );
}

export const hiddenCount = (people: readonly Person[], hidden: ReadonlySet<string>): number =>
  people.filter((p) => hidden.has(p.id)).length;

export const personName = (people: readonly Person[], id: string): string | undefined =>
  people.find((p) => p.id === id)?.name;

// ---------------------------------------------------------------- 前週比・増減

export interface Wow {
  /** 0 のとき「先週と同じ」 */
  same: boolean;
  good: boolean;
  arrow: '▲' | '▼';
  /** |d| を書式化したもの */
  abs: string;
  prev: string;
  unit: string;
}

/** 旧 `wowEl`。prev が null なら無し。 */
export function wow(now: number, prev: number | null, unit = '', invert = false): Wow | null {
  if (prev == null) return null;
  const d = now - prev;
  if (!d) return { same: true, good: false, arrow: '▲', abs: '', prev: fmt(prev), unit };
  const good = invert ? d < 0 : d > 0;
  return {
    same: false,
    good,
    arrow: d > 0 ? '▲' : '▼',
    abs: Math.abs(d).toLocaleString('ja-JP'),
    prev: fmt(prev),
    unit,
  };
}

export const growText = (n: number | null | undefined): string =>
  n == null ? '—' : n === 0 ? '±0' : (n > 0 ? '+' : '') + fmt(n);

export type Tone = 'ok' | 'alert' | 'faint';
export const growTone = (n: number | null | undefined): Tone =>
  n == null || n === 0 ? 'faint' : n > 0 ? 'ok' : 'alert';

/** `＋n` / `−n` (全角)。 */
export const signed = (n: number): string => (n > 0 ? '＋' : '−') + fmt(Math.abs(n));

// ---------------------------------------------------------------- 今月の成績

export interface BpoInner {
  v: number;
  /** `内 BPO v件（P%）` の P。母数 0 なら null */
  ratio: string | null;
}

/** 旧 `oi(k)`。BPO 経由の内訳。0 なら無し。 */
export function bpoInner(a: Counts, k: 'apo' | 'pool' | '実施' | 'cyomi'): BpoInner | null {
  const v = a['bpo_' + k] ?? 0;
  const t = a[k] ?? 0;
  if (!v) return null;
  return { v, ratio: t ? ((v / t) * 100).toFixed(0) : null };
}

export const denOf = (c: Counts): number =>
  (c['実施'] ?? 0) + (c['未実施'] ?? 0) + (c['未処理'] ?? 0) + (c['要判定'] ?? 0);

export interface CardSpec {
  key: string;
  lab: string;
  val: number | null;
  unit?: string;
  isPct?: boolean;
  hint?: string;
  bpo?: BpoInner | null;
  /** 任意の補足 (sub2)。 */
  sub2?: string;
  avg?: AvgLine | null;
  wow?: Wow | null;
  tone?: 'alert' | 'warn' | 'ok';
}

export interface MonthView {
  a: Counts;
  den: number;
  rate: number | null;
  ab: AvgBase | null;
  tt: Counts;
  cards: CardSpec[];
}

export function monthView(d: SalesKpiData, scope: Scope, teamOf: Record<string, string>): MonthView {
  const a = sumScope(d.by_person, scope, teamOf);
  const den = denOf(a);
  const rate = den ? ((a['実施'] ?? 0) / den) * 100 : null;
  const ab = avgBase(d.people, scope);
  const tt = sumTeam(d.by_person, scope.hidden, teamOf, ab?.team ?? null);
  const tden = denOf(tt);
  const anqDen = a.anq_den ?? 0;
  const anqNum = a.anq_num ?? 0;
  const cards: CardSpec[] = [
    {
      key: 'apo',
      lab: '① 取ったアポ',
      val: a.apo ?? 0,
      unit: '件',
      hint: '今月アポ日が確定した数',
      bpo: bpoInner(a, 'apo'),
      avg: avgLine(ab, tt.apo ?? 0),
    },
    {
      key: 'pool',
      lab: '③ 商談の予定',
      val: a.pool ?? 0,
      unit: '件',
      hint: '今月に商談日が入っている数',
      bpo: bpoInner(a, 'pool'),
      avg: avgLine(ab, tt.pool ?? 0),
    },
    {
      key: 'den',
      lab: '④ 日が過ぎた分',
      val: den,
      unit: '件',
      hint: 'このうち下の率を計算',
      avg: avgLine(ab, tden),
    },
    {
      key: 'done',
      lab: '② やった商談',
      val: a['実施'] ?? 0,
      unit: '件',
      hint: 'ステージが先へ進んだ数',
      bpo: bpoInner(a, '実施'),
      avg: avgLine(ab, tt['実施'] ?? 0),
    },
    {
      key: 'rate',
      lab: '⑥ 商談化率',
      val: rate,
      isPct: true,
      hint: fmt(a['実施'] ?? 0) + ' ÷ ' + fmt(den) + ' 件',
    },
    {
      key: 'anq',
      lab: '⑤ アンケート回収率',
      val: anqDen ? (anqNum / anqDen) * 100 : null,
      isPct: true,
      hint: fmt(anqNum) + ' ÷ ' + fmt(anqDen) + ' 件（④ 日が過ぎた分と同じ母数）',
    },
    {
      key: 'cyomi',
      lab: '⑨ 持っているCヨミ',
      val: a.cyomi ?? 0,
      unit: '件',
      hint: '決定者と合意できた案件',
      bpo: bpoInner(a, 'cyomi'),
      avg: avgLine(ab, tt.cyomi ?? 0),
    },
  ];
  return { a, den, rate, ab, tt, cards };
}

/** scope 文 (旧 `$('scope')`)。 */
export function scopeText(d: SalesKpiData, scope: Scope, a: Counts): string {
  const hid = hiddenCount(d.people, scope.hidden);
  const hidNote = hid ? '　' + String(hid) + '名をチェックで外しています。' : '';
  const body = scope.person
    ? (personName(d.people, scope.person) ?? '') +
      ' の数字だけを表示しています。' +
      (Object.keys(a).length
        ? ''
        : 'この担当者には今月の商談がありません（架電リストの数字だけ出ます）。')
    : scope.team === ALL_TEAMS
      ? '全チームの合計を表示しています。チーム名か、右のプルダウンで絞り込めます。'
      : scope.team + ' の数字だけを表示しています。';
  return body + hidNote;
}

/** 集計除外の内訳 (旧 scope2)。件数 0 なら null。 */
export function excludedParts(excluded: Counts): { n: number; parts: string[] } | null {
  const n = excluded['件数'] ?? 0;
  if (!n) return null;
  return {
    n,
    parts: Object.keys(excluded)
      .filter((k) => k !== '件数')
      .map((k) => k + ' ' + fmt(excluded[k]) + '件'),
  };
}

// ---------------------------------------------------------------- いま手を打てること

export type OpenKey = 'stale' | 'anq' | 'cyomi' | 'week' | 'next';

export interface ActionView {
  stale: DealRow[];
  anq: DealRow[];
  cys: DealRow[];
  wk: DealRow[];
  nx: DealRow[];
  cards: (CardSpec & { key: OpenKey })[];
}

export function actionView(d: SalesKpiData, scope: Scope): ActionView {
  const stale = pickRows(d.stale, scope);
  const anq = pickRows(d.anq_missing, scope);
  const cys = pickRows(d.cyomi_stale, scope);
  const wk = pickRows(d.week_deals, scope);
  const nx = pickRows(d.next_week_deals, scope);
  const cards: (CardSpec & { key: OpenKey })[] = [
    {
      key: 'stale',
      lab: '⑦ ステージが止まっている',
      val: stale.length,
      unit: '件',
      tone: stale.length ? 'alert' : 'ok',
      hint: stale.length ? '商談日が過ぎたのに動いていない' : 'ありません',
    },
    {
      key: 'anq',
      lab: '⑤ アンケート未回収',
      val: anq.length,
      unit: '件',
      tone: anq.length ? 'warn' : 'ok',
      hint: '今週・来週これからの商談のうち',
    },
    {
      key: 'cyomi',
      lab: '⑨ Cヨミで止まっている',
      val: cys.length,
      unit: '件',
      tone: cys.length ? 'warn' : 'ok',
      hint: '30日以上ステージが動いていない',
    },
    {
      key: 'week',
      lab: '③ 今週の商談',
      val: wk.length,
      unit: '件',
      hint: 'うち ' + String(wk.filter((r) => r.past).length) + '件 は日が過ぎました',
    },
    {
      key: 'next',
      lab: '③ 来週の商談',
      val: nx.length,
      unit: '件',
      hint: md(d.next_week.start) + '〜' + md(d.next_week.end) + ' の予定',
    },
  ];
  return { stale, anq, cys, wk, nx, cards };
}

export interface PanelConf {
  title: string;
  rows: DealRow[];
  opt: { stale?: boolean; days?: boolean };
  desc: string;
  byDay: boolean;
  span: { start: string; end: string } | null;
}

export function panelConf(d: SalesKpiData, av: ActionView, key: OpenKey): PanelConf {
  switch (key) {
    case 'stale':
      return {
        title: 'ステージが止まっている取引',
        rows: av.stale,
        opt: { stale: true },
        desc: '商談の日が過ぎたのに、ステージが「アポ日確定」のままです。商談したなら次のステージへ、しなかったならキャンセルに動かしてください。',
        byDay: false,
        span: null,
      };
    case 'anq':
      return {
        title: 'アンケートが未回収の商談',
        rows: av.anq,
        opt: {},
        desc: '今週・来週これから商談するもののうち、事前アンケートがまだ返ってきていないものです。商談前に催促してください。',
        byDay: false,
        span: null,
      };
    case 'cyomi':
      return {
        title: 'Cヨミで30日以上動いていない取引',
        rows: av.cys,
        opt: { days: true },
        desc: '決定者と価値合意ができてから30日以上、ステージが動いていません。次の一手を決めてください。',
        byDay: false,
        span: null,
      };
    case 'week':
      return {
        title: '今週の商談',
        rows: av.wk,
        opt: {},
        desc: 'まずどの日に何件あるかを出しています。日を押すと、その日の商談が出ます。終わった日はうすく表示しています。',
        byDay: true,
        span: d.week,
      };
    case 'next':
      return {
        title: '来週の商談',
        rows: av.nx,
        opt: {},
        desc: '来週の予定です。まずどの日に何件あるかを出しています。日を押すと、その日の商談が出ます。',
        byDay: true,
        span: d.next_week,
      };
  }
}

export interface WeekDay {
  date: string;
  rows: DealRow[];
  past: boolean;
  today: boolean;
}

/** 週の日別 (0 件の日も出す)。`past` は行があれば先頭行の past、無ければ日付 < today。 */
export function weekDays(rows: readonly DealRow[], span: { start: string; end: string }, today: string): WeekDay[] {
  const byDay = groupByDate(rows);
  return daySpan(span.start, span.end).map((dt) => {
    const r = byDay[dt] ?? [];
    const first = r[0];
    return { date: dt, rows: r, past: first ? !!first.past : dt < today, today: dt === today };
  });
}

export function groupByDate(rows: readonly DealRow[]): Record<string, DealRow[]> {
  const byDay: Record<string, DealRow[]> = {};
  for (const r of rows) (byDay[r.date] ??= []).push(r);
  return byDay;
}

// ---------------------------------------------------------------- 架電 (Zoom)

export interface CallRow {
  id: string;
  name: string;
  team: string;
  calls: number;
  conn: number;
  lng: number;
  prev: number | null;
}

export interface DailyBar {
  date: string;
  calls: number;
  connected: number;
  long: number;
}

export interface CallsView {
  per: KadenPeriod;
  hasPrev: boolean;
  cur: Counts;
  prv: Counts | null;
  kt: Counts;
  calls: number;
  conn: number;
  lng: number;
  /** 期間に入っている日 */
  dl: string[];
  upto: string;
  partial: boolean;
  /** HH:MM。無ければ '' */
  asof: string;
  /** 'none' = この期間の行が無い、'partial' = 最終日が途中 */
  fresh: 'none' | 'partial' | null;
  cards: CardSpec[];
  /** 日別グラフ (calls > 100 の日だけ) */
  bars: DailyBar[];
  mx: number;
  rows: CallRow[];
  unmatchedTop: string[];
  totalCalls: number;
}

export function callsView(
  d: SalesKpiData,
  scope: Scope,
  teamOf: Record<string, string>,
  ab: AvgBase | null,
  callPeriod: CallPeriodKey,
): CallsView | null {
  const c = d.calls;
  // 旧 JS: `if(C_&&C_.periods)`。古いサーバでは無いことがあった。
  if (!(c as Partial<typeof c> | undefined)?.periods) return null;
  const per: KadenPeriod = c.periods[callPeriod];
  const prevP = callPeriod === 'this_week' ? c.periods.prev_week_same : null;
  const cur = sumScope(per.by_person, scope, teamOf);
  const prv = prevP ? sumScope(prevP.by_person, scope, teamOf) : null;
  const kt = sumTeam(per.by_person, scope.hidden, teamOf, ab?.team ?? null);
  const calls = cur.calls ?? 0;
  const conn = cur.connected ?? 0;
  const lng = cur.long ?? 0;
  const dl = per.days;
  const upto = c.last_day || c.generated_at || '';
  const partial = c.last_day_partial;
  const asof = (c.fetched_at || '').slice(11, 16);
  const last = dl[dl.length - 1];
  const fresh: CallsView['fresh'] =
    dl.length === 0 ? 'none' : partial && last === upto ? 'partial' : null;
  const first = dl[0] ?? '';
  const partialNote =
    partial && dl.length && last === upto
      ? '（' + md(upto) + ' は' + (asof ? asof + ' 時点' : '集計中') + '）'
      : '';
  const cards: CardSpec[] = [
    {
      key: 'conn',
      lab: '架電数',
      val: dl.length ? conn : null,
      unit: '件',
      hint: dl.length
        ? first === last
          ? md(first)
          : md(first) + '〜' + md(last ?? '')
        : 'まだ集計されていません',
      sub2: 'Zoomでつながった通話の数' + partialNote,
      avg: dl.length ? avgLine(ab, kt.connected ?? 0) : null,
      wow: prv && dl.length ? wow(conn, prv.connected ?? 0, '件') : null,
    },
    {
      key: 'calls',
      lab: '発信した回数',
      val: calls,
      unit: '件',
      hint: 'かけ直しや切ったものを含む全発信',
      avg: avgLine(ab, kt.calls ?? 0),
    },
    {
      key: 'ratio',
      lab: 'つながった率',
      val: calls ? (conn / calls) * 100 : null,
      isPct: true,
      hint: fmt(conn) + ' ÷ ' + fmt(calls) + ' 件',
    },
    {
      key: 'long',
      lab: '5分超の通話',
      val: lng,
      unit: '件',
      hint: '深い会話。アポにつながりやすい',
      avg: avgLine(ab, kt.long ?? 0),
    },
    // 平均は付けない (日数で割った平均をさらに人数で割ると意味が変わる)。
    {
      key: 'perday',
      lab: '1日あたりの架電数',
      val: dl.length ? Math.round(conn / dl.length) : null,
      unit: '件',
      hint: dl.length ? String(dl.length) + '日で割った平均' : '—',
    },
  ];
  const bars: DailyBar[] = c.daily.filter((x) => x.calls > 100);
  const mx = bars.length ? Math.max(...bars.map((x) => x.calls)) : 0;

  const people = c.people;
  const rows: CallRow[] = Object.entries(per.by_person)
    .filter(([oid]) => {
      const pp = people.find((x) => x.id === oid);
      if (scope.hidden.has(oid)) return false;
      if (scope.person) return oid === scope.person;
      if (scope.team === ALL_TEAMS) return true;
      return !!pp && pp.team === scope.team;
    })
    .map(([oid, v]) => {
      const pp = people.find((x) => x.id === oid);
      const pv = prevP ? prevP.by_person[oid] : undefined;
      return {
        id: oid,
        name: pp?.name ?? 'owner_' + oid,
        team: teamLabel(pp) || '—',
        calls: v.calls ?? 0,
        conn: v.connected ?? 0,
        lng: v.long ?? 0,
        prev: prevP ? (pv?.connected ?? 0) : null,
      };
    })
    .sort((x, y) => y.conn - x.conn)
    .slice(0, 40);
  const um = c.unmatched_by_dept;
  const unmatchedTop = Object.keys(um)
    .slice(0, 3)
    .map((k) => k + ' ' + fmt(um[k]) + '件');
  return {
    per,
    hasPrev: !!prevP,
    cur,
    prv,
    kt,
    calls,
    conn,
    lng,
    dl,
    upto,
    partial,
    asof,
    fresh,
    cards,
    bars,
    mx,
    rows,
    unmatchedTop,
    totalCalls: per.total.calls ?? 0,
  };
}

// ---------------------------------------------------------------- 架電リストの残り

export interface KadenScope {
  c: Counts;
  base: number;
  who: string;
  whole: boolean;
}

export interface KadenListView {
  /** null = 担当者ごとの内訳がまだ無い */
  scope: KadenScope | null;
  salesCls: Counts;
  unCls: Counts;
  noOwn: number;
  nTeams: number;
  cards: CardSpec[];
  /** 未架電/未接触/接触済み の幅 (%) */
  widths: Record<(typeof KADEN_CLASSES)[number], number>;
  fillParts: string[];
  unassignedRows: SalesKpiData['kaden']['unassigned']['people'];
  showUnassigned: boolean;
}

export function kadenListView(d: SalesKpiData, scope: Scope, teamOf: Record<string, string>): KadenListView {
  const k = d.kaden;
  const nTeams = Object.keys(k.by_team).filter((t) => t !== TEAM_NONE).length;
  const salesCls = sumIf(k.by_person, scope.hidden, (id) => isSalesTeam(teamOf[id]));
  const unCls = sumIf(k.by_person, scope.hidden, (id) => !isSalesTeam(teamOf[id]));
  const noOwner = k.no_owner;
  const noOwn = noOwner.base ?? 0;
  for (const key of ['未架電', '未接触', '接触済み', 'base']) {
    unCls[key] = (unCls[key] ?? 0) + (noOwner[key] ?? 0);
  }
  const whole = !scope.person && scope.team === ALL_TEAMS;
  let ks: KadenScope | null;
  if (!k.has_by_owner) {
    ks = whole ? { c: k.cls, base: k.base, who: '会社全体', whole: true } : null;
  } else if (whole) {
    ks = { c: salesCls, base: salesCls.base ?? 0, who: '営業' + String(nTeams) + 'チームの合計', whole: true };
  } else {
    const c = sumScope(k.by_person, scope, teamOf);
    const who = scope.person
      ? (personName(d.people, scope.person) ?? 'この担当者') + ' が持っている分'
      : scope.team + ' が持っている分';
    ks = { c, base: c.base ?? 0, who, whole: false };
  }
  const g = (key: string): number => ks?.c[key] ?? 0;
  const sb = ks?.base ?? 0;
  const touched = g('未接触') + g('接触済み');
  const cards: CardSpec[] = sb
    ? [
        { key: 'mikaden', lab: 'まだかけていない', val: g('未架電'), unit: '件', hint: pct((g('未架電') / sb) * 100) + ' を占めます' },
        { key: 'misesshoku', lab: 'つながらず', val: g('未接触'), unit: '件', hint: '受付ブロック・不在・不通' },
        { key: 'sesshoku', lab: '話せた', val: g('接触済み'), unit: '件', hint: '担当者と接触できた' },
        {
          key: 'touched',
          lab: '手をつけた割合',
          val: (touched / sb) * 100,
          isPct: true,
          hint: fmt(touched) + ' ÷ ' + fmt(sb) + ' 件',
          sub2: '母数 ' + fmt(sb) + '件',
        },
      ]
    : [];
  const widths = {
    未架電: sb ? (g('未架電') / sb) * 100 : 0,
    未接触: sb ? (g('未接触') / sb) * 100 : 0,
    接触済み: sb ? (g('接触済み') / sb) * 100 : 0,
  };
  const fillParts = Object.keys(k.fill).map(
    (key) => key + ' ' + pct(((k.fill[key] ?? 0) / k.total) * 100),
  );
  const unassignedRows = k.unassigned.people
    .filter((p) => p.base > 0 && !scope.hidden.has(p.id))
    .slice(0, 8);
  return {
    scope: ks,
    salesCls,
    unCls,
    noOwn,
    nTeams,
    cards,
    widths,
    fillParts,
    unassignedRows,
    showUnassigned: !!ks && ks.whole && k.has_by_owner && !!(unCls.base ?? 0),
  };
}

// ---------------------------------------------------------------- 週次スナップショット

export interface SnapCells {
  kind: 'data';
  pool: string;
  poolPartial: boolean;
  done: string;
  rate: string;
}

export interface SnapRow {
  week: string;
  weekStart: string;
  cells: SnapCells | { kind: 'missing' };
  zoom: { text: string | null; partialNote: string | null };
  apo: string;
  stale: string;
  base: string | null;
  baseDiff: number | null;
}

export interface SnapView {
  rows: SnapRow[];
  missing: number;
  few: boolean;
}

export function snapView(snapshots: readonly Snapshot[], mode: SnapMode): SnapView {
  const shown = snapshots.slice(-8);
  let missing = 0;
  const rows = shown.map((s, ix) => {
    const tt = s.totals;
    const prev = ix ? shown[ix - 1] : undefined;
    const pb = prev ? prev.kaden_base : null;
    const bd = pb && s.kaden_base ? s.kaden_base - pb : null;
    const src = mode === 'week' ? s.week_totals : tt;
    let cells: SnapRow['cells'];
    if (src) {
      const dn = denOf(src);
      cells = {
        kind: 'data',
        pool: fmt(src.pool),
        poolPartial: mode === 'week' && s.week_partial,
        done: fmt(src['実施']),
        rate: dn ? pct((src['実施'] / dn) * 100) : '—',
      };
    } else {
      missing++;
      cells = { kind: 'missing' };
    }
    return {
      week: s.week,
      weekStart: s.week_start,
      cells,
      zoom: {
        text: s.zoom_called == null ? null : fmt(s.zoom_called),
        partialNote: s.zoom_partial ? String(s.zoom_days) + '日目まで（集計中）' : null,
      },
      apo: fmt(tt.apo),
      stale: fmt(s.stale),
      base: s.kaden_base ? fmt(s.kaden_base) : null,
      baseDiff: bd,
    };
  });
  return { rows, missing, few: snapshots.length < 2 };
}

// ---------------------------------------------------------------- 決定者・決裁者

export interface KetteiView {
  rows: KetteiRow[];
  no: KetteiNoOwner | null;
  asof: string;
  cols: string[];
  sum: Record<string, number>;
  grew: number;
  grewKnown: number;
  shown: number;
  miss: number;
  who: string;
}

/** 列名 (`kettei.cols` の要素) で 1 升を引く。生成型は列が固定なので、動的な列名は文字列で引く。 */
export const ketteiCell = (o: KetteiRow | KetteiNoOwner, col: string): number | null => {
  const v = (o as Record<string, unknown>)[col];
  return typeof v === 'number' ? v : null;
};

export function hasKettei(d: Pick<SalesKpiData, 'kettei'>): boolean {
  const ke = d.kettei;
  return ke.rows.length > 0 || !!ke.no_owner;
}

export function ketteiView(d: SalesKpiData, scope: Scope): KetteiView {
  const ke = d.kettei;
  const cols = ke.cols;
  const rows = pickRows(ke.rows, scope);
  const no = scope.team === ALL_TEAMS && !scope.person ? ke.no_owner : null;
  const asof = ke.date ? md(ke.date) + '（' + wd(ke.date) + '）' : '—';
  const sum: Record<string, number> = {};
  for (const c of [...cols, '合計']) sum[c] = 0;
  let grew = 0;
  let grewKnown = 0;
  const addRow = (o: KetteiRow | KetteiNoOwner): void => {
    for (const c of cols) sum[c] = (sum[c] ?? 0) + (ketteiCell(o, c) ?? 0);
    sum['合計'] = (sum['合計'] ?? 0) + (o['合計'] || 0);
    if (o['増加'] != null) {
      grew += o['増加'];
      grewKnown++;
    }
  };
  for (const r of rows) addRow(r);
  if (no) addRow(no);
  const who = scope.person
    ? (personName(d.people, scope.person) ?? 'この担当者')
    : scope.team === ALL_TEAMS
      ? '全社'
      : scope.team;
  const shown = rows.length + (no ? 1 : 0);
  return { rows, no, asof, cols, sum, grew, grewKnown, shown, miss: shown - grewKnown, who };
}

// ---------------------------------------------------------------- リストの在庫

export interface StockRow {
  label: string;
  note?: string;
  sub?: boolean;
  key?: string;
  n: Counts;
  named: Counts;
}

export const lsGet = (c: Counts | undefined, b: string): number => c?.[b] ?? 0;

export const lsAdd = (a: Counts, b: Counts | undefined): Counts => {
  const o: Counts = { ...a };
  for (const [k, v] of Object.entries(b ?? {})) o[k] = (o[k] ?? 0) + v;
  return o;
};

/** 割合。母数 0 なら `—`。 */
export const lsPct = (n: number, d: number): string => (d ? pct((n / d) * 100) : '—');

/** 内訳の名前をリストをまたいで初出順に。 */
export function lsNames(lists: readonly StockList[], kind: string): string[] {
  const names: string[] = [];
  for (const l of lists) {
    for (const g of l.groups) {
      if (g.kind === kind && !names.includes(g.name)) names.push(g.name);
    }
  }
  return names;
}

/** 1 つのリストの行: 内訳 → 区分の計 → その他。 */
export function lsRows(l: StockList, lists: readonly StockList[]): StockRow[] {
  const rows: StockRow[] = [];
  for (const kind of LS_KINDS) {
    const names = lsNames(lists, kind);
    if (!names.length) continue;
    let n: Counts = {};
    let named: Counts = {};
    for (const name of names) {
      const g = l.groups.find((x) => x.kind === kind && x.name === name);
      rows.push({ label: name, note: kind, n: g?.counts ?? {}, named: g?.named ?? {} });
      n = lsAdd(n, g?.counts);
      named = lsAdd(named, g?.named);
    }
    rows.push({ label: kind + 'の計', sub: true, key: kind, n, named });
  }
  rows.push({
    label: 'その他',
    note: '区分シートに書かれていない人・担当者なし',
    sub: true,
    key: 'その他',
    n: l.other,
    named: l.other_named,
  });
  return rows;
}

export interface StockOverview {
  lists: StockList[];
  bands: string[];
  allBand: string;
  named: boolean;
  hasGroups: boolean;
  /** 列見出し: リスト名… + 計 */
  cols: string[];
  rowsBy: StockRow[][];
  /** リストごとの全体 (企業人数で絞らない) + 合計 */
  whole: number[];
  wholeNamed: number[];
  /** 行ごと: [各リストの件数…, 合計] */
  rows: { row: StockRow; ns: number[]; ms: number[] }[];
  trendParts: string[] | null;
}

export function stockOverview(ls: ListStock): StockOverview {
  const lists = ls.lists;
  const allBand = ls.all_band || ALL_TEAMS;
  const rowsBy = lists.map((l) => lsRows(l, lists));
  const whole = lists.map((l) => lsGet(l.total, allBand));
  const wholeNamed = lists.map((l) => lsGet(l.total_named, allBand));
  whole.push(whole.reduce((a, b) => a + b, 0));
  wholeNamed.push(wholeNamed.reduce((a, b) => a + b, 0));
  const first = rowsBy[0] ?? [];
  const rows = first.map((row, i) => {
    const ns = rowsBy.map((rs) => lsGet(rs[i]?.n, allBand));
    const ms = rowsBy.map((rs) => lsGet(rs[i]?.named, allBand));
    ns.push(ns.reduce((a, b) => a + b, 0));
    ms.push(ms.reduce((a, b) => a + b, 0));
    return { row, ns, ms };
  });
  const trd = ls.trend;
  let trendParts: string[] | null = null;
  if (trd) {
    trendParts = lists
      .map((l, i) => {
        const p = trd.lists[l.name];
        if (!p) return null;
        const now: Record<string, number> = { 全体: lsGet(l.total, allBand) };
        for (const r of (rowsBy[i] ?? []).filter((r) => r.sub && r.key && (LS_KINDS as readonly string[]).includes(r.key))) {
          if (r.key) now[r.key] = lsGet(r.n, allBand);
        }
        const p2 = p as unknown as Record<string, number>;
        return (
          l.name +
          ' ' +
          ['全体', ...LS_KINDS].map((k) => k + ' ' + growText((now[k] ?? 0) - (p2[k] ?? 0))).join(' ／ ')
        );
      })
      .filter((x): x is string => x !== null);
  }
  return {
    lists,
    bands: ls.bands,
    allBand,
    named: ls.has_named,
    hasGroups: lists.some((l) => l.groups.length > 0),
    cols: [...lists.map((l) => l.name), '計'],
    rowsBy,
    whole,
    wholeNamed,
    rows,
    trendParts,
  };
}

// ---------------------------------------------------------------- タブ

export type TabKey = 'kpi' | 'kettei' | 'stock';

export function tabsOf(d: Pick<SalesKpiData, 'kettei' | 'list_stock'>): { key: TabKey; label: string }[] {
  const tabs: { key: TabKey; label: string }[] = [{ key: 'kpi', label: '営業KPI' }];
  if (hasKettei(d)) tabs.push({ key: 'kettei', label: '決定者・決裁者' });
  if (d.list_stock.lists.length) tabs.push({ key: 'stock', label: 'リストの在庫' });
  return tabs;
}

// ---------------------------------------------------------------- 見出し

export function rangeText(d: SalesKpiData): string {
  return (
    '2026年9月　／　今週 ' +
    md(d.week.start) +
    '（' +
    wd(d.week.start) +
    '）〜' +
    md(d.week.end) +
    '（' +
    wd(d.week.end) +
    '）　※' +
    d.generated_at +
    ' 時点'
  );
}

/** フッターの突合の文 (this_week があるときだけ)。 */
export function matchedText(d: SalesKpiData): string {
  const tw = (d.calls as Partial<SalesKpiData['calls']> | undefined)?.periods?.this_week;
  if (!tw) return '';
  return (
    '（今週の発信 ' +
    fmt(tw.total.calls) +
    '件のうち ' +
    fmt(tw.matched) +
    '件が突合できました。残りは medica事業部など他部署の発信です）'
  );
}
