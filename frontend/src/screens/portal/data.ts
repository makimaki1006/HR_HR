// 試作 v2 のデータの持ち方（サーバーの代わり）。顧客の画面と社内の管理画面が同じものを読み書きする。
// シートが正本という前提を再現する: 値はマス（行 × 項目）ごとに保存し、保存の直前にシートの今の値と比べる。
// どの操作も記録（activity）に残す。
import { useSyncExternalStore } from 'react';
import { CURRENT_ACCOUNT_ID, NOW, STAFF_NAME, accounts, activity as initialActivity, customers as initialCustomers, initialRows, sheets as initialSheets } from './fixtures';
import { applicantRows, kindOf, labelOf } from './columns';
import type { ActivityEntry, ApplicantRow, CellSaveResult, ColumnConfig, Customer, FunnelStage, ListPrefs, PortalAccount, SheetConfig, Viewer } from './types';

export interface DataState {
  viewer: Viewer;
  customers: Customer[];
  sheets: SheetConfig[];
  rows: Record<string, ApplicantRow[]>;
  /** シートから最後に読んだ時刻（ISO）。 */
  readAt: Record<string, string>;
  /** 新しい順。 */
  activity: ActivityEntry[];
  accounts: PortalAccount[];
  /** 表示中の顧客とシート。顧客の担当者は自分の顧客に固定、社内の人は切り替えられる。 */
  currentCustomerId: string;
  currentSheetId: string;
  /** 試作の操作: 次の保存の結果。 */
  nextSave: 'ok' | 'conflict' | 'failed';
  /** 一覧の見せ方（列の表示・並べ替え・絞り込み）。キーは「人|シート」。 */
  listPrefs: Record<string, ListPrefs>;
  /** 本物のシート（テスト）につないでいるか。URL に ?live=1 があるときだけ使う。 */
  live: LiveState;
}

/** 本物のシート（テスト）とのつながり。off は試作のサンプルだけで動く。 */
export interface LiveState {
  status: 'off' | 'loading' | 'ok' | 'offline';
  /** つながらないときの説明。 */
  message?: string;
}

/** 本物のシートにつなぐ試作のシート（顧客 c1 の s1）。 */
export const LIVE_SHEET_ID = 's1';
const LIVE_BRIDGE = 'http://127.0.0.1:5180';
const LIVE_SID = '1m5DRb117MXqCQw4GhJC21spHE6N3lqE0dtqP7zxwLeg';
const LIVE_GID = 510300204;
const LIVE_POLL_MS = 30_000;
const liveRequested = (): boolean => {
  try { return new URLSearchParams(window.location.search).get('live') === '1'; } catch { return false; }
};

const firstSheetOf = (sheets: SheetConfig[], customerId: string): string =>
  (sheets.find((s) => s.customerId === customerId && s.connection === 'ok') ?? sheets.find((s) => s.customerId === customerId))?.id ?? '';

const me = accounts.find((a) => a.id === CURRENT_ACCOUNT_ID);

let state: DataState = {
  viewer: { kind: 'customer', accountId: CURRENT_ACCOUNT_ID },
  customers: initialCustomers,
  // 本物のシート（テスト）のときは、シートの切り替えで本物と試作が分かるように名前に添える。
  sheets: liveRequested()
    ? initialSheets.map((sh) => (sh.id === LIVE_SHEET_ID ? { ...sh, tabName: sh.tabName + '（本物のシート）' } : sh.customerId === 'c1' ? { ...sh, tabName: sh.tabName + '（試作）' } : sh))
    : initialSheets,
  // 見出しの下の記入例の行は応募者ではないので、最初から入れない。
  rows: Object.fromEntries(initialSheets.map((sh) => [sh.id, applicantRows(sh, initialRows[sh.id] ?? [])])),
  readAt: Object.fromEntries(Object.keys(initialRows).map((k) => [k, NOW])),
  activity: [...initialActivity].sort((a, b) => b.at.localeCompare(a.at)),
  accounts,
  currentCustomerId: me?.customerId ?? 'c1',
  currentSheetId: firstSheetOf(initialSheets, me?.customerId ?? 'c1'),
  nextSave: 'ok',
  listPrefs: {},
  live: { status: liveRequested() ? 'loading' : 'off' },
};

/** 一覧の見せ方を覚えるときの鍵。顧客の担当者は本人、社内の人は名前ごと。 */
function prefsKey(sheetId: string): string {
  const who = state.viewer.kind === 'staff' ? 'staff:' + state.viewer.name : state.viewer.accountId;
  return who + '|' + sheetId;
}

/** 覚えている一覧の見せ方。無ければ null。 */
export function getListPrefs(s: DataState, sheetId: string): ListPrefs | null {
  const key = prefsKey(sheetId);
  const mem = s.listPrefs[key];
  if (mem) return mem;
  try {
    const raw = localStorage.getItem('pt.listprefs.' + key);
    if (raw) return JSON.parse(raw) as ListPrefs;
  } catch { /* 読めなければ既定 */ }
  return null;
}

const listeners = new Set<() => void>();
function set(next: Partial<DataState>): void {
  state = { ...state, ...next };
  listeners.forEach((l) => { l(); });
}
function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => { listeners.delete(l); };
}

export function useData(): DataState {
  return useSyncExternalStore(subscribe, () => state);
}

export function getData(): DataState {
  return state;
}

// ---- 読み取りの補助 ----

export function viewerName(s: DataState = state): string {
  if (s.viewer.kind === 'staff') return s.viewer.name;
  const id = s.viewer.accountId;
  const a = s.accounts.find((x) => x.id === id);
  if (!a) return '';
  return a.displayName !== '' ? a.displayName : a.email;
}

export function viewerEmail(s: DataState = state): string {
  if (s.viewer.kind === 'staff') return '';
  const id = s.viewer.accountId;
  return s.accounts.find((x) => x.id === id)?.email ?? '';
}

export function sheetsOf(s: DataState, customerId: string): SheetConfig[] {
  return s.sheets.filter((x) => x.customerId === customerId);
}

export function currentSheet(s: DataState): SheetConfig | undefined {
  return s.sheets.find((x) => x.id === s.currentSheetId);
}

export function currentCustomer(s: DataState): Customer | undefined {
  return s.customers.find((x) => x.id === s.currentCustomerId);
}

/** 1 マスの変更の記録（新しい順）。 */
export function cellHistory(s: DataState, sheetId: string, rowNumber: number, column?: string): ActivityEntry[] {
  return s.activity.filter((e) => e.sheetId === sheetId && e.rowNumber === rowNumber && (column === undefined || e.column === column));
}

/** 選考状況の値 → 段階。設定に無い値は undefined。 */
export function stageOf(sheet: SheetConfig, statusValue: string): FunnelStage | undefined {
  return sheet.stageMap?.[statusValue];
}

// ---- 書き込み ----

let seq = 100;
/** 試作の時計。サンプルの「今」（NOW）から、開いてからの経過分だけ進める。 */
const LOADED_AT = Date.now();
const nowIso = (): string => new Date(Date.parse(NOW) + (Date.now() - LOADED_AT)).toISOString();

/** 項目の表示名（記録の文に使う）。 */
function colName(sheet: SheetConfig | undefined, column: string): string {
  const c = sheet?.columns.find((x) => x.header === column);
  return c ? labelOf(c) : column;
}

/** 「シートで先に変更されていた」を再現する値。必ず next と違う、その列に入りうる値にする。 */
function conflictValue(col: ColumnConfig | undefined, current: string, next: string): string {
  const opts = (col?.options ?? []).filter((o) => o !== '' && o !== next && o !== current);
  if (opts.length > 0) return opts[0] ?? '';
  const k = col ? kindOf(col) : 'text';
  if (k === 'date' || k === 'datetime') {
    const m = /^(\d{4})\/(\d{1,2})\/(\d{1,2})(.*)$/.exec(next !== '' ? next : current);
    if (m) {
      const dt = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]) + 1));
      const pad = (n: number): string => String(n).padStart(2, '0');
      const v = `${String(dt.getUTCFullYear())}/${pad(dt.getUTCMonth() + 1)}/${pad(dt.getUTCDate())}${m[4] ?? ''}`;
      if (v !== next) return v;
    }
    return '2026/10/27' + (k === 'datetime' ? ' 10:00' : '');
  }
  const appended = (current !== '' ? current + ' ' : '') + '弊社で追記。';
  return appended === next ? appended + '（再）' : appended;
}
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function rowName(sheet: SheetConfig | undefined, row: ApplicantRow | undefined): string {
  const title = sheet?.titleColumn;
  return title ? (row?.values[title] ?? '') : '';
}

export function log(entry: Omit<ActivityEntry, 'id' | 'at' | 'accountEmail' | 'customerId' | 'actorName' | 'source'> & Partial<Pick<ActivityEntry, 'customerId' | 'actorName' | 'source' | 'accountEmail'>>): void {
  seq += 1;
  const e: ActivityEntry = {
    id: 'x' + String(seq),
    at: nowIso(),
    accountEmail: viewerEmail(),
    customerId: state.currentCustomerId,
    actorName: viewerName(),
    source: state.viewer.kind === 'staff' ? 'admin' : 'portal',
    ...entry,
  };
  set({ activity: [e, ...state.activity] });
}

function replaceRow(sheetId: string, row: ApplicantRow): Record<string, ApplicantRow[]> {
  return { ...state.rows, [sheetId]: (state.rows[sheetId] ?? []).map((r) => (r.rowNumber === row.rowNumber ? row : r)) };
}

// ---- 本物のシート（テスト）: ローカルの橋渡しサーバー（scripts/portal_dev/sheet_bridge.py）を通して読み書きする ----

interface LiveSheet {
  headers: Record<string, number>;
  rows: { rowNumber: number; values: Record<string, string> }[];
  options: Record<string, string[]>;
  readAt: string;
  modifiedTime: string;
}

/** 本物のシートの行が画面に入っているか。入っていれば保存もシートへ送る。 */
let liveLoaded = false;
let liveModified = '';
let liveTimer: ReturnType<typeof setInterval> | null = null;

async function liveFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const r = await fetch(LIVE_BRIDGE + path, init?.method === 'POST' ? { ...init, headers: { 'Content-Type': 'application/json' } } : init);
  const body = (await r.json()) as T & { error?: string };
  if (!r.ok) throw new Error(body.error ?? 'つながりません（' + String(r.status) + '）');
  return body;
}

function liveOffline(e: unknown): void {
  const msg = e instanceof Error ? e.message : '';
  set({ live: { status: 'offline', message: liveLoaded ? '本物のシートにつながりません。最後に読んだ内容を出しています。' : '本物のシートにつながりません。試作のサンプルを出しています。' + (msg !== '' && !msg.startsWith('Failed') ? '（' + msg + '）' : '') } });
}

/** シートから読んだ内容を画面に入れる。2 回目からは、前回と違うマスを「シートで変更」として記録し、行に印を付ける。 */
function applyLiveSheet(d: LiveSheet): void {
  const sheet = state.sheets.find((x) => x.id === LIVE_SHEET_ID);
  if (!sheet) return;
  const headers = sheet.columns.map((c) => c.header);
  const before = new Map((state.rows[LIVE_SHEET_ID] ?? []).map((r) => [r.rowNumber, r]));
  const first = !liveLoaded;
  const rows: ApplicantRow[] = d.rows.map((src) => {
    const values: Record<string, string> = Object.fromEntries(headers.map((h) => [h, src.values[h] ?? '']));
    const old = before.get(src.rowNumber);
    if (first) return { rowNumber: src.rowNumber, values, changedSinceLastView: null, changedColumns: [] };
    if (!old) {
      const name = sheet.titleColumn ? (values[sheet.titleColumn] ?? '') : '';
      log({ kind: 'sheet_change', detail: `${name}さんがシートに追加`, actorName: 'シート', source: 'sheet', accountEmail: '', sheetId: LIVE_SHEET_ID, rowNumber: src.rowNumber });
      return { rowNumber: src.rowNumber, values, changedSinceLastView: 'new', changedColumns: [] };
    }
    const changed = headers.filter((h) => (old.values[h] ?? '') !== values[h]);
    for (const h of changed) {
      log({ kind: 'sheet_change', detail: `${rowName(sheet, old)}さんの「${colName(sheet, h)}」がシートで変更`, actorName: 'シート', source: 'sheet', accountEmail: '', sheetId: LIVE_SHEET_ID, rowNumber: src.rowNumber, column: h, before: old.values[h] ?? '', after: values[h] ?? '' });
    }
    if (changed.length === 0) return { ...old, values };
    return { ...old, values, changedSinceLastView: old.changedSinceLastView ?? 'updated', changedColumns: [...new Set([...(old.changedColumns ?? []), ...changed])] };
  });
  const columns = sheet.columns.map((c) => {
    const opts = d.options[c.header];
    return opts && opts.length > 0 ? { ...c, options: opts, kind: 'choice' as const } : c;
  });
  // 試作のサンプルの記録（架空の人の変更）は、本物の行と混ざらないように初回に外す。
  const activity = first ? state.activity.filter((e) => e.sheetId !== LIVE_SHEET_ID || e.id.startsWith('x')) : state.activity;
  liveLoaded = true;
  liveModified = d.modifiedTime;
  set({
    activity,
    rows: { ...state.rows, [LIVE_SHEET_ID]: rows },
    sheets: state.sheets.map((x) => (x.id === LIVE_SHEET_ID ? { ...x, columns } : x)),
    readAt: { ...state.readAt, [LIVE_SHEET_ID]: nowIso() },
    live: { status: 'ok' },
  });
}

async function liveReload(): Promise<void> {
  try {
    const d = await liveFetch<LiveSheet>(`/sheet?sid=${LIVE_SID}&gid=${String(LIVE_GID)}`);
    applyLiveSheet(d);
  } catch (e) { liveOffline(e); }
}

async function livePoll(): Promise<void> {
  try {
    const v = await liveFetch<{ modifiedTime: string }>(`/version?sid=${LIVE_SID}`);
    if (!liveLoaded || v.modifiedTime !== liveModified) {
      await liveReload();
      return;
    }
    set({ readAt: { ...state.readAt, [LIVE_SHEET_ID]: nowIso() }, live: { status: 'ok' } });
  } catch (e) { liveOffline(e); }
}

/** ?live=1 のとき、本物のシートを読み、画面を開いている間は 30 秒おきに変わったかを確かめる。何度呼んでも 1 回だけ始める。 */
export function startLive(): void {
  if (state.live.status === 'off' || liveTimer !== null) return;
  void liveReload();
  liveTimer = setInterval(() => { if (document.visibilityState === 'visible') void livePoll(); }, LIVE_POLL_MS);
}

/** つながらないときに、手で読み直す。 */
export function retryLive(): void {
  if (state.live.status === 'off') return;
  set({ live: { status: 'loading' } });
  void liveReload();
}

/** 本物のシートへ 1 マスを保存する。結果の形と記録の残し方は試作の saveCell と同じ。 */
async function liveSaveCell(sheet: SheetConfig, row: ApplicantRow, column: string, base: string, next: string, replacement: boolean): Promise<CellSaveResult> {
  let res: { kind: 'saved'; value: string } | { kind: 'conflict'; latest: string };
  try {
    res = await liveFetch('/cell', { method: 'POST', body: JSON.stringify({ sid: LIVE_SID, gid: LIVE_GID, rowNumber: row.rowNumber, header: column, base, next }) });
  } catch (e) {
    const msg = e instanceof Error && !e.message.startsWith('Failed') ? e.message : '本物のシートにつながりません。';
    return { kind: 'failed', message: 'シートに保存できませんでした。' + msg };
  }
  const at = nowIso();
  const cur = (state.rows[sheet.id] ?? []).find((r) => r.rowNumber === row.rowNumber) ?? row;
  if (res.kind === 'conflict') {
    const latest = res.latest;
    if ((cur.values[column] ?? '') !== latest) {
      set({ rows: replaceRow(sheet.id, { ...cur, changedSinceLastView: 'updated', changedColumns: [...new Set([...(cur.changedColumns ?? []), column])], values: { ...cur.values, [column]: latest } }) });
      log({ kind: 'sheet_change', detail: `${rowName(sheet, cur)}さんの「${colName(sheet, column)}」がシートで変更`, actorName: 'シート', source: 'sheet', accountEmail: '', sheetId: sheet.id, rowNumber: row.rowNumber, column, before: cur.values[column] ?? '', after: latest });
    }
    log({ kind: 'save_conflict', detail: `${rowName(sheet, cur)}さんの「${colName(sheet, column)}」は、シートで先に変更されていたため保存しなかった`, sheetId: sheet.id, rowNumber: row.rowNumber, column, before: base, after: next });
    return { kind: 'conflict', latest, by: 'シート', at };
  }
  const current = cur.values[column] ?? '';
  set({ rows: replaceRow(sheet.id, { ...cur, changedSinceLastView: null, changedColumns: [], values: { ...cur.values, [column]: res.value } }), readAt: { ...state.readAt, [sheet.id]: at } });
  if (replacement) {
    log({ kind: 'option_replace', detail: `${rowName(sheet, cur)}さんの「${colName(sheet, column)}」を「${current || '（空）'}」から「${next || '（空）'}」に置き換え（項目の設定）`, sheetId: sheet.id, rowNumber: row.rowNumber, column, before: current, after: next });
  } else {
    log({ kind: 'save', detail: `${rowName(sheet, cur)}さんの「${colName(sheet, column)}」を「${next || '（空）'}」に変更`, sheetId: sheet.id, rowNumber: row.rowNumber, column, before: current, after: next });
  }
  return { kind: 'saved', value: res.value };
}

export const actions = {
  setViewer(viewer: Viewer): void {
    if (viewer.kind === 'customer') {
      const a = state.accounts.find((x) => x.id === viewer.accountId);
      const cid = a?.customerId ?? state.currentCustomerId;
      set({ viewer, currentCustomerId: cid, currentSheetId: firstSheetOf(state.sheets, cid) });
    } else {
      set({ viewer });
    }
  },
  selectCustomer(customerId: string): void {
    set({ currentCustomerId: customerId, currentSheetId: firstSheetOf(state.sheets, customerId) });
  },
  selectSheet(sheetId: string): void {
    const sh = state.sheets.find((x) => x.id === sheetId);
    set({ currentSheetId: sheetId, currentCustomerId: sh?.customerId ?? state.currentCustomerId });
  },

  /**
   * 1 マスを保存する。base は編集を始めたときの値。保存の直前にシートの今の値と比べ、
   * 違えば書かずに conflict（誰がいつ変えたか付き）を返す。
   */
  async saveCell(sheetId: string, rowNumber: number, column: string, base: string, next: string, opts: { replacement?: boolean } = {}): Promise<CellSaveResult> {
    if (sheetId === LIVE_SHEET_ID && liveLoaded) {
      const liveSheet = state.sheets.find((x) => x.id === sheetId);
      const liveRow = (state.rows[sheetId] ?? []).find((r) => r.rowNumber === rowNumber);
      if (!liveSheet || !liveRow) return { kind: 'failed', message: 'この応募者がシートに見つかりません。' };
      return liveSaveCell(liveSheet, liveRow, column, base, next, opts.replacement === true);
    }
    await sleep(450);
    const sheet = state.sheets.find((x) => x.id === sheetId);
    const row = (state.rows[sheetId] ?? []).find((r) => r.rowNumber === rowNumber);
    if (!sheet || !row) return { kind: 'failed', message: 'この応募者がシートに見つかりません。' };
    const mode = state.nextSave;
    if (mode === 'failed') {
      set({ nextSave: 'ok' });
      return { kind: 'failed', message: 'シートに保存できませんでした。' };
    }
    if (mode === 'conflict') {
      // シートで社内の人が先に書き換えた、を再現する。
      const latest = conflictValue(sheet.columns.find((c) => c.header === column), row.values[column] ?? '', next);
      const at = nowIso();
      set({ nextSave: 'ok', rows: replaceRow(sheetId, { ...row, changedSinceLastView: 'updated', changedColumns: [...new Set([...(row.changedColumns ?? []), column])], values: { ...row.values, [column]: latest } }), readAt: { ...state.readAt, [sheetId]: at } });
      log({ kind: 'sheet_change', detail: `${rowName(sheet, row)}さんの「${colName(sheet, column)}」がシートで変更`, actorName: STAFF_NAME, source: 'sheet', accountEmail: '', sheetId, rowNumber, column, before: row.values[column] ?? '', after: latest });
      log({ kind: 'save_conflict', detail: `${rowName(sheet, row)}さんの「${colName(sheet, column)}」は、シートで先に変更されていたため保存しなかった`, sheetId, rowNumber, column, before: base, after: next });
      return { kind: 'conflict', latest, by: STAFF_NAME, at };
    }
    const current = row.values[column] ?? '';
    if (current !== base) {
      const last = cellHistory(state, sheetId, rowNumber, column)[0];
      log({ kind: 'save_conflict', detail: `${rowName(sheet, row)}さんの「${colName(sheet, column)}」は、シートで先に変更されていたため保存しなかった`, sheetId, rowNumber, column, before: base, after: next });
      return { kind: 'conflict', latest: current, by: last?.actorName ?? 'シート', at: last?.at ?? (state.readAt[sheetId] ?? NOW) };
    }
    const saved: ApplicantRow = { ...row, changedSinceLastView: null, changedColumns: [], values: { ...row.values, [column]: next } };
    set({ rows: replaceRow(sheetId, saved), readAt: { ...state.readAt, [sheetId]: nowIso() } });
    if (opts.replacement === true) {
      log({ kind: 'option_replace', detail: `${rowName(sheet, row)}さんの「${colName(sheet, column)}」を「${current || '（空）'}」から「${next || '（空）'}」に置き換え（項目の設定）`, sheetId, rowNumber, column, before: current, after: next });
    } else {
      log({ kind: 'save', detail: `${rowName(sheet, row)}さんの「${colName(sheet, column)}」を「${next || '（空）'}」に変更`, sheetId, rowNumber, column, before: current, after: next });
    }
    return { kind: 'saved', value: next };
  },

  markSeen(sheetId: string, rowNumber: number): void {
    const row = (state.rows[sheetId] ?? []).find((r) => r.rowNumber === rowNumber);
    if (row?.changedSinceLastView == null) return;
    set({ rows: replaceRow(sheetId, { ...row, changedSinceLastView: null, changedColumns: [] }) });
  },

  /** そのマスを選んだら、シートで変わった印を消す。 */
  markCellSeen(sheetId: string, rowNumber: number, column: string): void {
    const row = (state.rows[sheetId] ?? []).find((r) => r.rowNumber === rowNumber);
    if (!row?.changedColumns?.includes(column)) return;
    set({ rows: replaceRow(sheetId, { ...row, changedColumns: row.changedColumns.filter((c) => c !== column) }) });
  },

  /** 一覧の列の表示・並べ替え・絞り込みを、人とシートごとに覚える。 */
  setListPrefs(sheetId: string, prefs: ListPrefs): void {
    const key = prefsKey(sheetId);
    set({ listPrefs: { ...state.listPrefs, [key]: prefs } });
    try { localStorage.setItem('pt.listprefs.' + key, JSON.stringify(prefs)); } catch { /* 保存できなくても動く */ }
  },

  /** 項目（列）の設定をまとめて置き換える。detail は記録に残す説明。 */
  updateColumns(sheetId: string, columns: ColumnConfig[], detail: string, opts: { reorder?: boolean } = {}): void {
    const before = state.sheets.find((x) => x.id === sheetId);
    set({ sheets: state.sheets.map((x) => (x.id === sheetId ? { ...x, columns } : x)) });
    // 列名の並びを before / after に入れるのは並び替えのときだけ。それ以外は説明の文をそのまま記録に出す。
    if (opts.reorder === true) {
      log({ kind: 'column_change', detail, sheetId, before: (before?.columns ?? []).map((c) => (c.label !== '' ? c.label : c.header)).join('、'), after: columns.map((c) => (c.label !== '' ? c.label : c.header)).join('、') });
    } else {
      log({ kind: 'column_change', detail, sheetId });
    }
  },

  /** プルダウンの選択肢を変える（シートの入力規則も書き換える想定）。 */
  setOptions(sheetId: string, column: string, options: string[]): void {
    const sheet = state.sheets.find((x) => x.id === sheetId);
    const old = sheet?.columns.find((c) => c.header === column)?.options ?? [];
    set({ sheets: state.sheets.map((x) => (x.id === sheetId ? { ...x, columns: x.columns.map((c) => (c.header === column ? { ...c, options, kind: 'choice' as const } : c)) } : x)) });
    log({ kind: 'column_change', detail: `「${colName(sheet, column)}」の選択肢を変更`, sheetId, column, before: old.filter(Boolean).join('、'), after: options.filter(Boolean).join('、') });
  },

  /** 選考状況の選択肢ごとの段階（数字の集計用）を変える。 */
  setStageMap(sheetId: string, stageMap: Record<string, FunnelStage>): void {
    set({ sheets: state.sheets.map((x) => (x.id === sheetId ? { ...x, stageMap } : x)) });
    log({ kind: 'column_change', detail: '選考状況の段階（数字の集計）を変更', sheetId });
  },

  updateSheet(sheetId: string, patch: Partial<SheetConfig>, detail?: string): void {
    set({ sheets: state.sheets.map((x) => (x.id === sheetId ? { ...x, ...patch } : x)) });
    if (detail) log({ kind: 'column_change', detail, sheetId });
  },

  updateCustomers(customers: Customer[]): void {
    set({ customers });
  },
  updateAccounts(list: PortalAccount[]): void {
    set({ accounts: list });
  },

  logExport(sheetId: string, count: number): void {
    log({ kind: 'export', detail: `応募者 ${String(count)} 件を書き出し`, sheetId });
  },

  // ---- 試作の操作 ----
  setNextSave(v: DataState['nextSave']): void {
    set({ nextSave: v });
  },
  simulateSheetEdit(sheetId: string): void {
    const sheet = state.sheets.find((x) => x.id === sheetId);
    const list = state.rows[sheetId] ?? [];
    const row = list[2];
    if (!sheet || !row) return;
    const status = sheet.statusColumn;
    const statusOpts = sheet.columns.find((c) => c.header === status)?.options ?? [];
    const nextStatus = statusOpts[3] ?? '';
    const values = { ...row.values };
    if (status) values[status] = nextStatus;
    set({ rows: replaceRow(sheetId, { ...row, changedSinceLastView: 'updated', changedColumns: status ? [status] : [], values }), readAt: { ...state.readAt, [sheetId]: nowIso() } });
    if (status) log({ kind: 'sheet_change', detail: `${rowName(sheet, row)}さんの「${colName(sheet, status)}」がシートで変更`, actorName: STAFF_NAME, source: 'sheet', accountEmail: '', sheetId, rowNumber: row.rowNumber, column: status, before: row.values[status] ?? '', after: nextStatus });
  },
  simulateNewApplicant(sheetId: string): void {
    const sheet = state.sheets.find((x) => x.id === sheetId);
    const list = state.rows[sheetId] ?? [];
    if (!sheet) return;
    const max = list.reduce((m, r) => Math.max(m, r.rowNumber), 1);
    const values: Record<string, string> = Object.fromEntries(sheet.columns.map((c) => [c.header, '']));
    if (sheet.sortColumn) values[sheet.sortColumn] = '2026/10/20';
    if (sheet.titleColumn) values[sheet.titleColumn] = '新井 ひなた';
    const phoneCol = sheet.columns.find((c) => kindOf(c) === 'phone');
    if (phoneCol) values[phoneCol.header] = '090-0000-9999';
    if (sheet.statusColumn) values[sheet.statusColumn] = sheet.columns.find((c) => c.header === sheet.statusColumn)?.options?.[0] ?? '';
    const row: ApplicantRow = { rowNumber: max + 1, changedSinceLastView: 'new', values };
    set({ rows: { ...state.rows, [sheetId]: [row, ...list] }, readAt: { ...state.readAt, [sheetId]: nowIso() } });
    log({ kind: 'sheet_change', detail: '新井 ひなたさんがシートに追加', actorName: STAFF_NAME, source: 'sheet', accountEmail: '', sheetId, rowNumber: row.rowNumber });
  },
};
