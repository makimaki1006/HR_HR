// 見る画面の決まり（画面の部品から切り離した純粋な関数）。
// サーバーは見せる項目だけを、columns と同じ並びの cells で返す（PortalSheetRows）。
import type { PortalApplicant } from '../../../../generated/PortalApplicant';
import type { PortalColumn } from '../../../../generated/PortalColumn';

export type Group = 'summary' | 'customer' | 'staff' | 'other';

/** 詳細のまとまりの見出し。summary は詳細の上に置き、見出しは付けない。 */
export const GROUP_TITLE: Record<Exclude<Group, 'summary'>, string> = {
  customer: '貴社で記入',
  staff: '弊社からの連絡',
  other: 'そのほか',
};
export const DETAIL_GROUPS: Exclude<Group, 'summary'>[] = ['customer', 'staff', 'other'];

export function groupOf(c: PortalColumn): Group {
  return c.group === 'summary' || c.group === 'customer' || c.group === 'staff' ? c.group : 'other';
}

/** 一覧の見せ方。名前（太字）・状況（札）・その下の行に並べる項目。番号は columns の位置。 */
export interface ListLayout {
  /** 一覧に出す項目（PC の表の列）。 */
  list: number[];
  title: number | null;
  status: number | null;
  /** スマホのカードの 2 行目に出す項目（名前・状況を除いた先頭の 2 つ）。 */
  sub: number[];
}

export function listLayout(columns: PortalColumn[]): ListLayout {
  const idx = columns.map((_, i) => i);
  const inList = idx.filter((i) => columns[i]?.in_list === true);
  const list = inList.length > 0 ? inList : idx.slice(0, 1);
  const title = list.find((i) => columns[i]?.kind === 'text') ?? list[0] ?? null;
  const status = list.find((i) => i !== title && columns[i]?.kind === 'choice') ?? null;
  const sub = list.filter((i) => i !== title && i !== status).slice(0, 2);
  return { list, title, status, sub };
}

/** 状況の札の色。 */
export function statusTone(s: string): 'warn' | 'info' | 'ok' | 'neutral' {
  const v = s.trim();
  if (v === '未対応' || v === '未連絡') return 'warn';
  if (['内定', '採用', '入社日調整中', '入社'].includes(v)) return 'ok';
  if (['不採用', '辞退', '書類選考落ち', '面接辞退', '内定辞退', '連絡不通'].includes(v)) return 'neutral';
  return 'info';
}

const norm = (s: string): string => s.normalize('NFKC').toLowerCase().replace(/\s+/g, '');

/** 探す: 見せる項目のどれかに入っていれば出す。電話番号はハイフンを除いても合う。 */
export function matches(a: PortalApplicant, query: string): boolean {
  const q = norm(query);
  if (q === '') return true;
  const digits = /^[0-9-]+$/.test(q) ? q.replace(/-/g, '') : '';
  return a.cells.some((c) => {
    const v = norm(c);
    return v.includes(q) || (digits.length >= 4 && v.replace(/[^0-9]/g, '').includes(digits));
  });
}

/**
 * サーバーで見られなくなったか (404、または共有が外れた 503 sheet_unavailable)。
 * このときは画面に残っている内容を捨てる。混雑 (503 sheet_busy) や通信の失敗は一時的なので、内容を残して帯で知らせる
 */
export function isGone(r: { status: number; error: string }): boolean {
  return r.status === 404 || (r.status === 503 && r.error === 'sheet_unavailable');
}

/** 新しく入った行（シートの下の行）を上にする。 */
export function newestFirst(list: PortalApplicant[]): PortalApplicant[] {
  return [...list].sort((a, b) => b.row - a.row);
}

/** 電話番号のリンク（数字と + だけ）。数字が無ければ null。 */
export function telHref(v: string): string | null {
  const d = v.normalize('NFKC').replace(/[^0-9+]/g, '');
  return d.length >= 6 ? `tel:${d}` : null;
}

/** メールのリンク。形が合わなければ null。 */
export function mailHref(v: string): string | null {
  const t = v.trim();
  return /^[^\s@<>"]+@[^\s@<>"]+\.[^\s@<>"]+$/.test(t) ? `mailto:${t}` : null;
}

/** URL の ?sheet=&row= を読む。 */
export function readUrl(search: string): { sheet: string | null; row: number | null } {
  const p = new URLSearchParams(search);
  const row = Number(p.get('row'));
  return { sheet: p.get('sheet'), row: Number.isInteger(row) && row > 0 ? row : null };
}

export function urlFor(path: string, sheet: string | null, row: number | null): string {
  const p = new URLSearchParams();
  if (sheet) p.set('sheet', sheet);
  if (row !== null) p.set('row', String(row));
  const q = p.toString();
  return q === '' ? path : `${path}?${q}`;
}
