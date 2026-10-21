// 列の設定から、一覧と詳細の見せ方を決める。顧客の画面と、社内の「列の設定」のプレビューが同じ決まりを使う。
import { NOW } from './fixtures';
import type { ApplicantRow, ColumnConfig, ColumnGroup, ColumnKind, SheetConfig } from './types';

/** カードの副題に出す値の数（日付の列を含む）。 */
export const CARD_SUBTITLE_COUNT = 2;

export const labelOf = (c: ColumnConfig): string => (c.label.trim() !== '' ? c.label.trim() : c.header);
export const kindOf = (c: ColumnConfig): ColumnKind => c.kind ?? (c.options ? 'choice' : 'text');
export const groupOf = (c: ColumnConfig): ColumnGroup => c.group ?? (c.writable ? 'customer' : 'other');

export const GROUP_LABEL: Record<ColumnGroup, string> = {
  summary: '見出し（詳細の上）',
  customer: '貴社で記入',
  staff: '弊社からの連絡',
  other: 'そのほか',
};
export const GROUP_ORDER: ColumnGroup[] = ['summary', 'customer', 'staff', 'other'];

export const KIND_LABEL: Record<ColumnKind, string> = {
  text: '文字',
  date: '日付',
  datetime: '日付と時刻',
  phone: '電話番号',
  email: 'メール',
  choice: '選択肢',
};
export const KIND_ORDER: ColumnKind[] = ['text', 'date', 'datetime', 'phone', 'email', 'choice'];

type LayoutSource = Pick<SheetConfig, 'columns' | 'titleColumn' | 'statusColumn' | 'sortColumn' | 'interviewColumn' | 'interviewTimeColumn'>;

export interface CardLayout {
  title: ColumnConfig | undefined;
  status: ColumnConfig | undefined;
  sort: ColumnConfig | undefined;
  /** 副題に並べる列（日付、そのほか 1 列）。 */
  subtitle: ColumnConfig[];
  /** カードの最後の行に出す、面接の日時の列（面接予定日など）。 */
  extra: ColumnConfig | undefined;
  /** 面接の時刻が別の列にあるとき、その列（extra の値のあとに添える）。 */
  extraTime: ColumnConfig | undefined;
}

/** カードの最後の行に出す文字。日付と、別の列にある時刻をつなぐ。 */
export function extraText(layout: CardLayout, values: Record<string, string>): string {
  const d = layout.extra ? (values[layout.extra.header] ?? '') : '';
  const t = layout.extraTime ? (values[layout.extraTime.header] ?? '') : '';
  return [isBlankValue(d) ? '' : d, isBlankValue(t) ? '' : t].filter((x) => x !== '').join(' ');
}

export function cardLayout(src: LayoutSource): CardLayout {
  const shown = src.columns.filter((c) => c.visible);
  const find = (h: string | undefined) => (h ? shown.find((c) => c.header === h) : undefined);
  const title = find(src.titleColumn) ?? shown[0];
  const status = find(src.statusColumn);
  const sort = find(src.sortColumn);
  const used = new Set([title, status].filter((c): c is ColumnConfig => c !== undefined));
  const listed = shown.filter((c) => c.inList && !used.has(c));
  const interview = find(src.interviewColumn);
  const extra = interview ?? listed.find((c) => kindOf(c) === 'datetime' && c !== sort);
  const extraTime = interview ? find(src.interviewTimeColumn) : undefined;
  const rest = listed.filter((c) => c !== sort && c !== extra && c !== extraTime);
  const other = rest.find((c) => kindOf(c) === 'choice') ?? rest.find((c) => kindOf(c) === 'text');
  const subtitle = [sort, other].filter((c): c is ColumnConfig => c !== undefined).slice(0, CARD_SUBTITLE_COUNT);
  return { title, status, sort, subtitle, extra, extraTime };
}

/** 検索に使う列: 名前の列、電話番号の列、一覧に出していない 1 行の文字の列（ふりがななど）。 */
export function searchColumns(src: LayoutSource): ColumnConfig[] {
  const title = cardLayout(src).title;
  return src.columns.filter((c) => c.visible && (c === title || kindOf(c) === 'phone' || (!c.inList && kindOf(c) === 'text' && !c.multiline)));
}

// ---- 日付の読み方 ----
// シートの日付は書き方がそろっていない（「2023年1月20日」「2022/11/9」「2月24日」「記載なし」）。
// 画面には元の文字を出し、並べ替えと集計のときだけここで読む。

export const dayNum = (y: number, m: number, d: number): number => Math.floor(Date.UTC(y, m - 1, d) / 86400000);

const NOW_YEAR = Number(NOW.slice(0, 4));
/** 試作の「今日」の通し番号。 */
export const TODAY_NUM = dayNum(NOW_YEAR, Number(NOW.slice(5, 7)), Number(NOW.slice(8, 10)));

/** 「記載なし」「なし」「－」などは空と同じ扱い。 */
export function isBlankValue(v: string | undefined): boolean {
  const t = (v ?? '').trim();
  return t === '' || /^(記載なし|なし|無し|未記載|未定|不明|[-－ー—]+)$/.test(t);
}

const validDay = (y: number, m: number, d: number): number | null => {
  if (m < 1 || m > 12 || d < 1 || d > 31) return null;
  const n = dayNum(y, m, d);
  const back = new Date(n * 86400000);
  return back.getUTCMonth() + 1 === m ? n : null;
};

/**
 * 日付の文字を通し番号にする。読めなければ null。
 * 年のない日付（「2月24日」）は、ref（その行の応募日の通し番号）から年を補う。応募日より前になるなら翌年。ref が無ければ今年。
 */
export function parseLooseDay(v: string | undefined, ref?: number | null): number | null {
  if (isBlankValue(v)) return null;
  const t = (v ?? '').trim();
  const full = /^(\d{4})\s*[年/.-]\s*(\d{1,2})\s*[月/.-]\s*(\d{1,2})/.exec(t);
  if (full) return validDay(Number(full[1]), Number(full[2]), Number(full[3]));
  const short = /^(\d{1,2})\s*[月/]\s*(\d{1,2})(?!\d)/.exec(t);
  if (!short) return null;
  const m = Number(short[1]);
  const d = Number(short[2]);
  if (ref === undefined || ref === null) return validDay(NOW_YEAR, m, d);
  const refYear = new Date(ref * 86400000).getUTCFullYear();
  const a = validDay(refYear, m, d);
  if (a === null) return null;
  return a < ref - 31 ? validDay(refYear + 1, m, d) : a;
}

/** 時刻の文字（「11:00」「11時」「11時30分」）を 0 時からの分にする。読めなければ null。 */
export function parseTimeOfDay(v: string | undefined): number | null {
  if (isBlankValue(v)) return null;
  const m = /(\d{1,2})\s*(?::|時)\s*(\d{1,2})?/.exec(v ?? '');
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2] ?? '0');
  return h > 23 || min > 59 ? null : h * 60 + min;
}

/** 応募者の行だけにする（見出しと記入例の行を除く）。 */
export function applicantRows(sheet: Pick<SheetConfig, 'headerRow' | 'exampleRows'>, rows: ApplicantRow[]): ApplicantRow[] {
  const first = sheet.headerRow + (sheet.exampleRows ?? 0);
  return rows.filter((r) => r.rowNumber > first);
}

/** 並べ替え用の日付の列があれば新しい順（書き方が混ざっていても日付で比べる。読めない行は最後）、無ければシートの行が新しい順。 */
export function sortByDate<T extends { rowNumber: number; values: Record<string, string> }>(rows: T[], sortHeader: string | undefined): T[] {
  return [...rows].sort((a, b) => {
    if (sortHeader) {
      const da = parseLooseDay(a.values[sortHeader]);
      const db = parseLooseDay(b.values[sortHeader]);
      if (da !== db) {
        if (da === null) return 1;
        if (db === null) return -1;
        return db - da;
      }
    }
    return b.rowNumber - a.rowNumber;
  });
}
