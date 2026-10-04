// テスト専用: 旧画面と React 版に同じ操作を流して、そのたびの #panel1 を記録する道具 (parity.card / parity.nt の共用)。
import { expect } from 'vitest';
import { mountNew, mountOld, outline, type Screen } from './dual';
import type { SalesKpiData } from '../types';

export type Log = [string, string[]][];

export const panel = (s: Screen): string[] => outline(s.el('panel1'));
export const cardsOf = (s: Screen): string[] => outline(s.el('cards1'));

/** パネルの表を 1 行ずつ降りて、各段の #panel1 を記録する。戻る → 次の行。 */
export function walk(s: Screen, log: Log, tag: string, lim: number, depth = 0): void {
  log.push([tag, panel(s)]);
  if (depth > 3) throw new Error('表の段が深すぎる');
  // 旧画面は 1 回押すたびに画面全体を描き直すので、全員を降りると時間がかかる。段ごとに先頭 lim 行と最後の 1 行だけ降りる
  const total = s.qa('#panel1 table.cdrill tbody tr').length;
  const idx = [...new Set([...Array.from({ length: Math.min(lim, total) }, (_, i) => i), total - 1])].filter((i) => i >= 0);
  const n = idx.length;
  for (let k = 0; k < n; k++) {
    const tr = s.qa('#panel1 table.cdrill tbody tr')[idx[k] ?? 0];
    const name = tr?.getAttribute('data-name') ?? '';
    const btn = tr?.querySelector('button');
    if (!btn) throw new Error('行のボタンが無い');
    s.click(btn);
    walk(s, log, `${tag} > ${name}`, lim, depth + 1);
    s.clickButton('#panel1', /に戻る/);
  }
}

export const segBtn = (s: Screen, k: string): HTMLElement => {
  const b = s.qa('#panel1 [data-seg]').find((x) => x.getAttribute('data-seg') === k);
  if (!b) throw new Error('区分チップが無い: ' + k);
  return b;
};
export const nth = (s: Screen, sel: string, i: number): HTMLElement => {
  const b = s.qa(sel)[i];
  if (!b) throw new Error(sel + ' の ' + String(i) + ' 番目が無い');
  return b;
};

/** 区分チップ・BPO を 1 つずつ入れて walk する。 */
export function walkFilters(s: Screen, log: Log, tag: string): void {
  walk(s, log, tag, 6);
  const segs = s.qa('#panel1 [data-seg]').map((b) => b.getAttribute('data-seg') ?? '');
  for (const k of segs.filter((x) => x)) {
    s.click(segBtn(s, k));
    walk(s, log, `${tag} [区分 ${k}]`, 1);
    s.click(segBtn(s, k)); // 外す
  }
  const bpo = s.q('#panel1-bpo');
  if (bpo) {
    s.click(bpo);
    walk(s, log, `${tag} [BPO だけ]`, 1);
    s.click(s.el('panel1-bpo'));
  }
}

/** 7 枚のカードを順に開き、閉じる。 */
export function allCards(s: Screen, log: Log, tag: string): void {
  for (let i = 0; i < 7; i++) {
    s.clickCard(i);
    log.push([`${tag} card${String(i)} 開いた直後 cards1`, cardsOf(s)]);
    walkFilters(s, log, `${tag} card${String(i)}`);
    s.clickCard(i); // 閉じる
    log.push([`${tag} card${String(i)} 閉じた直後`, [...panel(s), ...cardsOf(s)]]);
  }
}

export async function run(mount: typeof mountOld, data: SalesKpiData, setup: (s: Screen, log: Log) => void, hidden: string[] = []): Promise<Log> {
  const s = await mount(data, hidden);
  const log: Log = [];
  try {
    setup(s, log);
  } finally {
    s.teardown();
  }
  return log;
}

export async function same(data: SalesKpiData, setup: (s: Screen, log: Log) => void, hidden: string[] = []): Promise<Log> {
  const o = await run(mountOld, data, setup, hidden);
  const n = await run(mountNew, data, setup, hidden);
  expect(n.length).toBe(o.length);
  for (let i = 0; i < o.length; i++) expect(n[i], o[i]?.[0]).toEqual(o[i]);
  return o;
}

