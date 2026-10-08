import { describe, expect, it } from 'vitest';
import {
  DEFAULT_LAYOUT, DOCK_STORAGE_KEY, LEGACY_DOCK_STORAGE_KEY, clearLegacyLayout, columnOf, dockReducer, leftPercent, loadLayout, loadLayoutWithNotice,
  parseLayout, saveLayout, serializeLayout, tabPanels,
} from './dockModel';
import type { DockLayout } from './dockModel';

const panelsOf = (l: DockLayout) => l.columns.map(c => c.panels);
const activeOf = (l: DockLayout) => l.columns.map(c => c.active);

/** 読み書きを数える Storage の代わり */
function memoryStorage(initial: Record<string, string> = {}, opts: { throwOnSet?: boolean; throwOnGet?: boolean } = {}): Storage & { data: Record<string, string> } {
  const data = { ...initial };
  return {
    data,
    get length() { return Object.keys(data).length; },
    clear: () => { for (const k of Object.keys(data)) Reflect.deleteProperty(data, k); },
    getItem: (k: string) => { if (opts.throwOnGet) throw new Error('blocked'); return k in data ? data[k] ?? null : null; },
    key: (i: number) => Object.keys(data)[i] ?? null,
    removeItem: (k: string) => { Reflect.deleteProperty(data, k); },
    setItem: (k: string, v: string) => { if (opts.throwOnSet) throw new Error('quota'); data[k] = v; },
  };
}

describe('dockReducer', () => {
  it('the default (v2): left = 架電一覧 + プロパティ (架電一覧 in front), center = 案件の概要 (fixed) + 活動ログ / 架電結果の入力, right = 求人検索・リンク先 (wide)', () => {
    expect(panelsOf(DEFAULT_LAYOUT)).toEqual([['queue', 'properties'], ['overview', 'activity', 'result'], ['links']]);
    expect(activeOf(DEFAULT_LAYOUT)).toEqual(['queue', 'activity', 'links']);
    expect(tabPanels(DEFAULT_LAYOUT.columns[1])).toEqual(['activity', 'result']);
    // 1440px (区切り 2 本 = 12px) で 左 ≈ 300px、右 ≈ 45%
    const total = DEFAULT_LAYOUT.widths[0] + DEFAULT_LAYOUT.widths[1] + DEFAULT_LAYOUT.widths[2];
    expect(Math.round((DEFAULT_LAYOUT.widths[0] / total) * (1440 - 12))).toBe(300);
    expect(DEFAULT_LAYOUT.widths[2] / total).toBeCloseTo(0.45);
  });

  it('move: 活動ログ to the left column goes to the end and comes to the front; the center falls back to its first tab', () => {
    const l = dockReducer(DEFAULT_LAYOUT, { type: 'move', panel: 'activity', to: 0 });
    expect(panelsOf(l)).toEqual([['queue', 'properties', 'activity'], ['overview', 'result'], ['links']]);
    expect(activeOf(l)).toEqual(['activity', 'result', 'links']);
    expect(columnOf(l, 'activity')).toBe(0);
  });

  it('move to an index inserts before it; moving the last panel out leaves the column empty (active null)', () => {
    let l = dockReducer(DEFAULT_LAYOUT, { type: 'move', panel: 'links', to: 1 });
    expect(panelsOf(l)[1]).toEqual(['overview', 'activity', 'result', 'links']);
    expect(panelsOf(l)[2]).toEqual([]);
    l = dockReducer(l, { type: 'move', panel: 'links', to: 2 });
    expect(panelsOf(l)[2]).toEqual(['links']);
    expect(activeOf(l)[2]).toBe('links');
    l = dockReducer(l, { type: 'move', panel: 'result', to: 2, index: 0 });
    expect(panelsOf(l)[2]).toEqual(['result', 'links']);
    expect(activeOf(l)[2]).toBe('result');
    l = dockReducer(l, { type: 'move', panel: 'result', to: 1 });
    l = dockReducer(l, { type: 'move', panel: 'links', to: 1 });
    expect(panelsOf(l)[2]).toEqual([]);
    expect(activeOf(l)[2]).toBeNull();
  });

  it('the fixed panel (案件の概要) moves but never becomes the active tab; a column with only it has no active tab', () => {
    let l = dockReducer(DEFAULT_LAYOUT, { type: 'move', panel: 'links', to: 1 });
    l = dockReducer(l, { type: 'move', panel: 'overview', to: 2 });
    expect(panelsOf(l)[2]).toEqual(['overview']);
    expect(activeOf(l)).toEqual(['queue', 'links', null]);
    l = dockReducer(l, { type: 'activate', panel: 'overview' });
    expect(activeOf(l)[2]).toBeNull();
  });

  it('reorder moves a tab one step within its column and stops at the ends', () => {
    let l = dockReducer(DEFAULT_LAYOUT, { type: 'reorder', panel: 'result', delta: -1 });
    expect(panelsOf(l)[1]).toEqual(['overview', 'result', 'activity']);
    l = dockReducer(l, { type: 'reorder', panel: 'queue', delta: -1 });
    expect(l).toBe(l);
    expect(panelsOf(l)[0]).toEqual(['queue', 'properties']);
    const same = dockReducer(l, { type: 'reorder', panel: 'properties', delta: 1 });
    expect(same).toBe(l);
  });

  it('activate brings a tab to the front in its own column only', () => {
    const l = dockReducer(DEFAULT_LAYOUT, { type: 'activate', panel: 'properties' });
    expect(activeOf(l)).toEqual(['properties', 'activity', 'links']);
    expect(dockReducer(l, { type: 'activate', panel: 'properties' })).toBe(l);
  });

  it('resize moves width between two neighbours and keeps both at least the minimum share', () => {
    // 3 列とも並んでいる (幅の合計 1.0)。左と中央の間で 0.1 だけ動かす
    const l = dockReducer(DEFAULT_LAYOUT, { type: 'resize', left: 0, right: 1, delta: 0.1 });
    expect(l.widths[0]).toBeCloseTo(0.31);
    expect(l.widths[1]).toBeCloseTo(0.24);
    expect(l.widths[2]).toBeCloseTo(0.45);
    const tooFar = dockReducer(DEFAULT_LAYOUT, { type: 'resize', left: 0, right: 1, delta: -1 });
    expect(tooFar.widths[0]).toBeCloseTo(0.12);
    const other = dockReducer(DEFAULT_LAYOUT, { type: 'resize', left: 1, right: 2, delta: 1, minRatio: 0.2 });
    expect(other.widths[2]).toBeCloseTo(0.2);
    expect(leftPercent(other, 1, 2)).toBe(75);
    // 空の列とは区切りが無いので何もしない
    const twoCols = dockReducer(DEFAULT_LAYOUT, { type: 'move', panel: 'links', to: 1 });
    expect(dockReducer(twoCols, { type: 'resize', left: 1, right: 2, delta: 0.1 })).toBe(twoCols);
  });

  it('reset goes back to the default layout and widths', () => {
    let l = dockReducer(DEFAULT_LAYOUT, { type: 'move', panel: 'activity', to: 2 });
    l = dockReducer(l, { type: 'resize', left: 0, right: 1, delta: 0.05 });
    expect(dockReducer(l, { type: 'reset' })).toEqual(DEFAULT_LAYOUT);
  });
});

describe('persisting the layout (localStorage)', () => {
  it('round-trips through the storage under a versioned key', () => {
    let l = dockReducer(DEFAULT_LAYOUT, { type: 'move', panel: 'activity', to: 0 });
    l = dockReducer(l, { type: 'resize', left: 0, right: 1, delta: 0.08 });
    const st = memoryStorage();
    expect(saveLayout(st, l)).toBe(true);
    expect(Object.keys(st.data)).toEqual([DOCK_STORAGE_KEY]);
    expect(DOCK_STORAGE_KEY).toMatch(/\.v2$/);
    expect(loadLayout(st)).toEqual(l);
  });

  it('broken / unknown / duplicated / old-version data falls back to the default layout', () => {
    const good = JSON.parse(serializeLayout(DEFAULT_LAYOUT)) as { v: number; columns: { panels: string[]; active: string | null }[]; widths: number[] };
    const cases: (string | null)[] = [
      null, '', '{', 'null', '[]', '"x"',
      JSON.stringify({ ...good, v: 0 }),
      JSON.stringify({ ...good, columns: good.columns.slice(0, 2) }),
      JSON.stringify({ ...good, widths: [0.3, -1, 0.3] }),
      JSON.stringify({ ...good, widths: [0.3, 'a', 0.3] }),
      JSON.stringify({ ...good, columns: [{ panels: ['queue', 'nope'], active: 'queue' }, good.columns[1], good.columns[2]] }),
      JSON.stringify({ ...good, columns: [{ panels: ['queue', 'activity'], active: 'queue' }, good.columns[1], good.columns[2]] }),
      JSON.stringify({ ...good, columns: [{ panels: 'queue', active: 'queue' }, good.columns[1], good.columns[2]] }),
    ];
    for (const raw of cases) expect(parseLayout(raw), String(raw)).toEqual(DEFAULT_LAYOUT);
  });

  it('a panel missing from the stored layout is added back to its default column; a bad active tab falls back to the first tab', () => {
    const raw = JSON.stringify({ v: 2, columns: [{ panels: ['queue'], active: 'links' }, { panels: ['overview', 'activity', 'result', 'links'], active: 'overview' }, { panels: [], active: null }], widths: [1, 2, 1] });
    const l = parseLayout(raw);
    expect(panelsOf(l)).toEqual([['queue', 'properties'], ['overview', 'activity', 'result', 'links'], []]);
    expect(activeOf(l)).toEqual(['queue', 'activity', null]);
    expect(l.widths).toEqual([1, 2, 1]);
  });

  it('a layout saved by the previous version (v1) is replaced by the new default once, with a notice; the old key is removed after saving', () => {
    const v1 = JSON.stringify({ v: 1, columns: [{ panels: ['queue', 'properties'], active: 'queue' }, { panels: ['overview', 'activity', 'result', 'links'], active: 'result' }, { panels: [], active: null }], widths: [0.26, 0.74, 0.3] });
    const st = memoryStorage({ [LEGACY_DOCK_STORAGE_KEY]: v1 });
    expect(LEGACY_DOCK_STORAGE_KEY).toBe('hrhr.crm.dockLayout.v1');
    // 読むだけでは書き換えない (同じ結果を何度でも返す)
    expect(loadLayoutWithNotice(st)).toEqual({ layout: DEFAULT_LAYOUT, migrated: true });
    expect(loadLayoutWithNotice(st)).toEqual({ layout: DEFAULT_LAYOUT, migrated: true });
    expect(panelsOf(loadLayout(st))[2]).toEqual(['links']);
    // 画面が新しい配置を残してから以前の配置を消す → 次に開いたときは案内しない
    expect(saveLayout(st, DEFAULT_LAYOUT)).toBe(true);
    clearLegacyLayout(st);
    expect(Object.keys(st.data)).toEqual([DOCK_STORAGE_KEY]);
    expect(loadLayoutWithNotice(st)).toEqual({ layout: DEFAULT_LAYOUT, migrated: false });
    // v1 が無い (初めて開いた) ときは案内しない。v2 の中に v1 の形が入っていても既定
    expect(loadLayoutWithNotice(memoryStorage())).toEqual({ layout: DEFAULT_LAYOUT, migrated: false });
    expect(parseLayout(v1)).toEqual(DEFAULT_LAYOUT);
  });

  it('storage that throws (private mode, blocked) never breaks the screen', () => {
    expect(loadLayout(memoryStorage({}, { throwOnGet: true }))).toEqual(DEFAULT_LAYOUT);
    expect(loadLayoutWithNotice(memoryStorage({}, { throwOnGet: true }))).toEqual({ layout: DEFAULT_LAYOUT, migrated: false });
    expect(saveLayout(memoryStorage({}, { throwOnSet: true }), DEFAULT_LAYOUT)).toBe(false);
    expect(loadLayout(null)).toEqual(DEFAULT_LAYOUT);
    expect(saveLayout(null, DEFAULT_LAYOUT)).toBe(false);
  });
});
