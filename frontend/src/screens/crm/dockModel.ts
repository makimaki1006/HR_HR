/**
 * 架電画面のパネルの配置 (3 列。列ごとにパネルをタブで切り替える)。
 *
 * - 純粋な関数だけ (HubSpot も画面も触らない)。配置を変えても通信は起きない
 * - 「案件の概要」は、置いた列の上端に固定で出す (タブにはしない)
 * - 配置はこのブラウザに残す (localStorage。版付きのキー)。読めない・壊れているときは既定の配置に戻す
 */

export type PanelId = 'queue' | 'properties' | 'overview' | 'activity' | 'result' | 'links';
export const PANEL_IDS: readonly PanelId[] = ['queue', 'properties', 'overview', 'activity', 'result', 'links'];

export const PANEL_LABELS: Record<PanelId, string> = {
  queue: '架電一覧', properties: 'プロパティ', overview: '案件の概要', activity: '活動ログ', result: '架電結果の入力', links: '求人検索・リンク先',
};

/** 列の上端に固定で出すパネル (タブにしない) */
export const PINNED_PANEL: PanelId = 'overview';

export type ColumnIndex = 0 | 1 | 2;
export const COLUMN_INDEXES: readonly ColumnIndex[] = [0, 1, 2];
export const COLUMN_LABELS: Record<ColumnIndex, string> = { 0: '左', 1: '中央', 2: '右' };

export interface DockColumn {
  /** 並び順 (タブの順)。固定のパネルも含む */
  panels: PanelId[];
  /** 前に出しているタブ。固定のパネルにはならない。タブが無ければ null */
  active: PanelId | null;
}

export interface DockLayout {
  columns: [DockColumn, DockColumn, DockColumn];
  /** 列の幅の比 (空の列は細い帯になり、比は使わない) */
  widths: [number, number, number];
}

/**
 * 既定の配置 (v2, 2026-10-08): 左 = 架電一覧 + プロパティ、中央 = 案件の概要 (上端) + 活動ログ / 架電結果の入力、
 * 右 = 求人検索・リンク先 (Google の検索結果を広く・高く出す)。
 * 幅の比は 1440px で 左 ≈ 300px / 中央 ≈ 490px / 右 ≈ 640px (45%)。1366 / 1280px でも同じ比 (左 ≈ 285 / 265px)
 */
export const DEFAULT_LAYOUT: DockLayout = {
  columns: [
    { panels: ['queue', 'properties'], active: 'queue' },
    { panels: ['overview', 'activity', 'result'], active: 'activity' },
    { panels: ['links'], active: 'links' },
  ],
  widths: [0.21, 0.34, 0.45],
};

/** 列の最小幅 (並んでいる列の幅の合計に対する比)。px の最小は CSS で別に効かせる */
export const MIN_WIDTH_RATIO = 0.12;
/** 区切りを矢印キーで動かす幅 (比) */
export const KEY_RESIZE_STEP = 0.02;

export type DockAction =
  /** パネルを列へ移す (`index` が無ければ末尾)。移したパネルを前に出す */
  | { type: 'move'; panel: PanelId; to: ColumnIndex; index?: number }
  /** 同じ列の中で前 (-1) / 後ろ (+1) へ */
  | { type: 'reorder'; panel: PanelId; delta: -1 | 1 }
  /** パネルのある列で、そのタブを前に出す */
  | { type: 'activate'; panel: PanelId }
  /** 左右に並んだ 2 列の幅を変える。`delta` は並んでいる列の幅の合計に対する比 (+ で左の列が広がる) */
  | { type: 'resize'; left: ColumnIndex; right: ColumnIndex; delta: number; minRatio?: number }
  | { type: 'reset' };

export function columnOf(layout: DockLayout, panel: PanelId): ColumnIndex {
  const i = layout.columns.findIndex(c => c.panels.includes(panel));
  return (i === -1 ? 0 : i) as ColumnIndex;
}

/** タブとして並ぶパネル (固定のパネルを除く) */
export function tabPanels(col: DockColumn): PanelId[] {
  return col.panels.filter(p => p !== PINNED_PANEL);
}

/** パネルの無い列は細い帯で出す (幅の比は使わない) */
export const isEmptyColumn = (col: DockColumn) => col.panels.length === 0;

/** 前に出すタブを決め直す (今のタブが列に無ければ先頭のタブ) */
function fixActive(col: DockColumn, prefer?: PanelId | null): DockColumn {
  const tabs = tabPanels(col);
  const want = prefer !== undefined && prefer !== null && tabs.includes(prefer) ? prefer : col.active;
  return { panels: col.panels, active: want !== null && tabs.includes(want) ? want : (tabs[0] ?? null) };
}

function withColumn(layout: DockLayout, i: ColumnIndex, col: DockColumn): DockLayout {
  const columns = [...layout.columns] as DockLayout['columns'];
  columns[i] = col;
  return { ...layout, columns };
}

export function dockReducer(state: DockLayout, action: DockAction): DockLayout {
  switch (action.type) {
    case 'reset': return DEFAULT_LAYOUT;
    case 'activate': {
      if (action.panel === PINNED_PANEL) return state;
      const i = columnOf(state, action.panel);
      const col = state.columns[i];
      if (col.active === action.panel || !col.panels.includes(action.panel)) return state;
      return withColumn(state, i, { ...col, active: action.panel });
    }
    case 'reorder': {
      const i = columnOf(state, action.panel);
      const panels = [...state.columns[i].panels];
      const from = panels.indexOf(action.panel);
      const to = from + action.delta;
      if (from === -1 || to < 0 || to >= panels.length) return state;
      panels.splice(from, 1);
      panels.splice(to, 0, action.panel);
      return withColumn(state, i, { ...state.columns[i], panels });
    }
    case 'move': {
      const from = columnOf(state, action.panel);
      if (!state.columns[from].panels.includes(action.panel)) return state;
      // 元の列から外す
      const source = state.columns[from].panels.filter(p => p !== action.panel);
      let next = withColumn(state, from, fixActive({ panels: source, active: state.columns[from].active === action.panel ? null : state.columns[from].active }));
      const target = next.columns[action.to].panels.filter(p => p !== action.panel);
      const at = action.index === undefined ? target.length : Math.max(0, Math.min(target.length, action.index));
      target.splice(at, 0, action.panel);
      next = withColumn(next, action.to, fixActive({ panels: target, active: next.columns[action.to].active }, action.panel));
      return next;
    }
    case 'resize': {
      const { left, right } = action;
      if (left === right) return state;
      const visible = COLUMN_INDEXES.filter(i => !isEmptyColumn(state.columns[i]));
      if (!visible.includes(left) || !visible.includes(right)) return state;
      const total = visible.reduce<number>((sum, i) => sum + state.widths[i], 0);
      const min = (action.minRatio ?? MIN_WIDTH_RATIO) * total;
      const pair = state.widths[left] + state.widths[right];
      if (pair < min * 2) return state;
      const nextLeft = Math.min(pair - min, Math.max(min, state.widths[left] + action.delta * total));
      if (Math.abs(nextLeft - state.widths[left]) < 1e-9) return state;
      const widths = [...state.widths] as DockLayout['widths'];
      widths[left] = nextLeft;
      widths[right] = pair - nextLeft;
      return { ...state, widths };
    }
  }
}

/** 区切りの左の列が、2 列の合計のうち何 % か (読み上げ・aria-valuenow 用) */
export function leftPercent(layout: DockLayout, left: ColumnIndex, right: ColumnIndex): number {
  const pair = layout.widths[left] + layout.widths[right];
  return pair > 0 ? Math.round((layout.widths[left] / pair) * 100) : 50;
}

// ---------------------------------------------------------------------------
// このブラウザに残す
// ---------------------------------------------------------------------------

export const DOCK_STORAGE_KEY = 'hrhr.crm.dockLayout.v2';
/** 以前 (v1) の置き場所。残っていたら新しい既定の配置に切り替え、1 回だけ知らせてから消す */
export const LEGACY_DOCK_STORAGE_KEY = 'hrhr.crm.dockLayout.v1';
const VERSION = 2;

const isPanelId = (v: unknown): v is PanelId => typeof v === 'string' && (PANEL_IDS as readonly string[]).includes(v);

/**
 * 残した配置を読む。形が違う・知らないパネル・同じパネルが 2 回・幅が不正なら既定の配置。
 * 後から増えたパネル (残した配置に無いもの) は、既定の配置の列の末尾に足す
 */
export function parseLayout(raw: string | null): DockLayout {
  if (raw === null) return DEFAULT_LAYOUT;
  let v: unknown;
  try { v = JSON.parse(raw); } catch { return DEFAULT_LAYOUT; }
  if (typeof v !== 'object' || v === null) return DEFAULT_LAYOUT;
  const o = v as { v?: unknown; columns?: unknown; widths?: unknown };
  if (o.v !== VERSION || !Array.isArray(o.columns) || o.columns.length !== 3 || !Array.isArray(o.widths) || o.widths.length !== 3) return DEFAULT_LAYOUT;
  const widths = o.widths.map(w => (typeof w === 'number' && Number.isFinite(w) && w > 0 && w < 100 ? w : NaN));
  if (widths.some(Number.isNaN)) return DEFAULT_LAYOUT;
  const seen = new Set<PanelId>();
  const columns: DockColumn[] = [];
  for (const c of o.columns as unknown[]) {
    if (typeof c !== 'object' || c === null) return DEFAULT_LAYOUT;
    const { panels, active } = c as { panels?: unknown; active?: unknown };
    if (!Array.isArray(panels)) return DEFAULT_LAYOUT;
    const list: PanelId[] = [];
    for (const p of panels) {
      if (!isPanelId(p) || seen.has(p)) return DEFAULT_LAYOUT;
      seen.add(p);
      list.push(p);
    }
    columns.push(fixActive({ panels: list, active: isPanelId(active) ? active : null }));
  }
  let layout: DockLayout = { columns: columns as DockLayout['columns'], widths: widths as DockLayout['widths'] };
  for (const p of PANEL_IDS) {
    if (seen.has(p)) continue;
    const i = columnOf(DEFAULT_LAYOUT, p);
    const col = layout.columns[i];
    layout = withColumn(layout, i, fixActive({ panels: [...col.panels, p], active: col.active }));
  }
  return layout;
}

export function serializeLayout(layout: DockLayout): string {
  return JSON.stringify({ v: VERSION, columns: layout.columns, widths: layout.widths });
}

export function localStorageOrNull(): Storage | null {
  try { return window.localStorage; } catch { return null; }
}

export function loadLayout(storage: Storage | null): DockLayout {
  return loadLayoutWithNotice(storage).layout;
}

/**
 * 配置を読む (読むだけ。書き換えない)。以前の版 (v1) の配置だけが残っていたら、新しい既定の配置 (求人検索・リンク先を右の列) にして
 * `migrated: true` を返す (画面で 1 回だけ知らせる)。以前の置き場所は、新しい配置を残した後に [`clearLegacyLayout`] で消す
 */
export function loadLayoutWithNotice(storage: Storage | null): { layout: DockLayout; migrated: boolean } {
  if (storage === null) return { layout: DEFAULT_LAYOUT, migrated: false };
  try {
    const cur = storage.getItem(DOCK_STORAGE_KEY);
    if (cur !== null) return { layout: parseLayout(cur), migrated: false };
    return { layout: DEFAULT_LAYOUT, migrated: storage.getItem(LEGACY_DOCK_STORAGE_KEY) !== null };
  } catch { return { layout: DEFAULT_LAYOUT, migrated: false }; }
}

/** 以前 (v1) の配置を消す (新しい配置を残せた後に呼ぶ。次に開いたときは知らせない) */
export function clearLegacyLayout(storage: Storage | null): void {
  try { storage?.removeItem(LEGACY_DOCK_STORAGE_KEY); } catch { /* 消せなくても、新しい配置が残っていれば次は知らせない */ }
}

/** 残せたら true (残せない環境では、開いている間だけ配置が効く) */
export function saveLayout(storage: Storage | null, layout: DockLayout): boolean {
  if (storage === null) return false;
  try { storage.setItem(DOCK_STORAGE_KEY, serializeLayout(layout)); return true; } catch { return false; }
}
