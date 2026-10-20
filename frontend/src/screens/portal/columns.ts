// 列の設定から、一覧と詳細の見せ方を決める。顧客の画面と、社内の「列の設定」のプレビューが同じ決まりを使う。
import type { ColumnConfig, ColumnGroup, ColumnKind, SheetConfig } from './types';

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

type LayoutSource = Pick<SheetConfig, 'columns' | 'titleColumn' | 'statusColumn' | 'sortColumn'>;

export interface CardLayout {
  title: ColumnConfig | undefined;
  status: ColumnConfig | undefined;
  sort: ColumnConfig | undefined;
  /** 副題に並べる列（日付、そのほか 1 列）。 */
  subtitle: ColumnConfig[];
  /** カードの最後の行に出す、日付と時刻の列（面接日など）。 */
  extra: ColumnConfig | undefined;
}

export function cardLayout(src: LayoutSource): CardLayout {
  const shown = src.columns.filter((c) => c.visible);
  const find = (h: string | undefined) => (h ? shown.find((c) => c.header === h) : undefined);
  const title = find(src.titleColumn) ?? shown[0];
  const status = find(src.statusColumn);
  const sort = find(src.sortColumn);
  const used = new Set([title, status].filter((c): c is ColumnConfig => c !== undefined));
  const listed = shown.filter((c) => c.inList && !used.has(c));
  const extra = listed.find((c) => kindOf(c) === 'datetime' && c !== sort);
  const other = listed.find((c) => kindOf(c) === 'text' && c !== sort);
  const subtitle = [sort, other].filter((c): c is ColumnConfig => c !== undefined).slice(0, CARD_SUBTITLE_COUNT);
  return { title, status, sort, subtitle, extra };
}

/** 検索に使う列: 名前の列、電話番号の列、一覧に出していない 1 行の文字の列（ふりがななど）。 */
export function searchColumns(src: LayoutSource): ColumnConfig[] {
  const title = cardLayout(src).title;
  return src.columns.filter((c) => c.visible && (c === title || kindOf(c) === 'phone' || (!c.inList && kindOf(c) === 'text' && !c.multiline)));
}

/** 並べ替え用の日付の列があれば新しい順、無ければシートの行が新しい順。 */
export function sortByDate<T extends { rowNumber: number; values: Record<string, string> }>(rows: T[], sortHeader: string | undefined): T[] {
  return [...rows].sort((a, b) => {
    if (sortHeader) {
      const da = a.values[sortHeader] ?? '';
      const db = b.values[sortHeader] ?? '';
      if (da !== db) return da < db ? 1 : -1;
    }
    return b.rowNumber - a.rowNumber;
  });
}
