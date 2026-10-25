// 応募者画面の社内の管理 (/app/portal-admin) の、画面に依らない決まり。テストで確かめる。
import type { PortalAdminColumn } from '../../../generated/PortalAdminColumn';

// ---- 画面の中の行き先 (?view=) ----

export type Route =
  | { view: 'customers' }
  | { view: 'customer'; id: string }
  | { view: 'sheet-add'; customer: string }
  | { view: 'sheet'; id: string }
  | { view: 'accounts' }
  | { view: 'activity' };

export function parseRoute(search: string): Route {
  const q = new URLSearchParams(search);
  const id = q.get('id') ?? '';
  switch (q.get('view')) {
    case 'customer':
      return id === '' ? { view: 'customers' } : { view: 'customer', id };
    case 'sheet-add': {
      const customer = q.get('customer') ?? '';
      return customer === '' ? { view: 'customers' } : { view: 'sheet-add', customer };
    }
    case 'sheet':
      return id === '' ? { view: 'customers' } : { view: 'sheet', id };
    case 'accounts':
      return { view: 'accounts' };
    case 'activity':
      return { view: 'activity' };
    default:
      return { view: 'customers' };
  }
}

export function routeHref(r: Route): string {
  const q = new URLSearchParams();
  q.set('view', r.view);
  if (r.view === 'customer' || r.view === 'sheet') q.set('id', r.id);
  if (r.view === 'sheet-add') q.set('customer', r.customer);
  return `?${q.toString()}`;
}

// ---- 表示の名前 ----

export type Stage = 'sheet_only' | 'both' | 'portal_only';

export const STAGE_LABEL: Record<Stage, string> = {
  sheet_only: 'シートだけ',
  both: '両方',
  portal_only: '画面だけ',
};

export const STAGE_MEANING: Record<Stage, string> = {
  sheet_only: '顧客は今までどおりシートを使う。画面にはログインできない。',
  both: '顧客はシートと画面のどちらにも書ける。',
  portal_only: '顧客は画面だけを使う。シートの共有は外す。',
};

export function stageOf(s: string): Stage {
  return s === 'both' || s === 'portal_only' ? s : 'sheet_only';
}

export const ACCOUNT_STATUS_LABEL: Record<string, string> = {
  invited: '招待中',
  active: '利用中',
  disabled: '停止中',
};

export const KIND_OPTIONS: { value: string; label: string }[] = [
  { value: 'text', label: '文字' },
  { value: 'choice', label: '選択肢' },
  { value: 'date', label: '日付' },
  { value: 'datetime', label: '日付と時刻' },
  { value: 'phone', label: '電話番号' },
  { value: 'email', label: 'メールアドレス' },
];

export const GROUP_OPTIONS: { value: string; label: string }[] = [
  { value: 'summary', label: '基本情報' },
  { value: 'customer', label: '貴社で記入' },
  { value: 'staff', label: '弊社で記入' },
  { value: 'other', label: 'そのほか' },
];

export const LOG_KIND_LABEL: Record<string, string> = {
  customer_add: '顧客の追加',
  customer_change: '顧客の変更',
  sheet_add: 'シートの登録',
  sheet_change: 'シートの変更',
  sheet_preview: 'シートを試しに読んだ',
  column_change: '項目の設定',
  account_invite: '招待',
  account_disable: 'アカウントの停止',
  account_enable: 'アカウントの再開',
  account_extra_email: '追加の送り先',
  login: 'ログイン',
  login_failed: 'ログインの失敗',
  login_locked: 'ロック中のログイン',
  logout: 'ログアウト',
};

export const SOURCE_LABEL: Record<string, string> = {
  admin: '社内の管理画面',
  portal: '顧客の画面',
  system: 'アプリ',
};

// ---- 項目の設定 ----

/** 見せない項目は書けず、一覧にも出さない。書ける項目は見せる */
export function setColumn(cols: PortalAdminColumn[], i: number, patch: Partial<PortalAdminColumn>): PortalAdminColumn[] {
  return cols.map((c, j) => {
    if (j !== i) return c;
    const next = { ...c, ...patch };
    if (patch.visible === false) {
      next.writable = false;
      next.in_list = false;
    }
    if (patch.writable === true || patch.in_list === true) next.visible = true;
    return next;
  });
}

/** 並びを 1 つ上 (-1)・下 (+1) へ */
export function moveColumn(cols: PortalAdminColumn[], i: number, delta: -1 | 1): PortalAdminColumn[] {
  const j = i + delta;
  if (j < 0 || j >= cols.length) return cols;
  const next = cols.slice();
  const a = next[i];
  const b = next[j];
  if (a === undefined || b === undefined) return cols;
  next[i] = b;
  next[j] = a;
  return next;
}

/** シートを読んで分かった見出しのうち、まだ項目に無いものを、見せない項目として末尾に足す */
export function withNewHeaders(cols: PortalAdminColumn[], headers: string[]): PortalAdminColumn[] {
  const have = new Set(cols.map((c) => c.header));
  const add: PortalAdminColumn[] = [];
  for (const raw of headers) {
    const h = raw.trim();
    if (h === '' || have.has(h)) continue;
    have.add(h);
    add.push({ header: h, label: '', visible: false, writable: false, in_list: false, multiline: false, kind: 'text', group: 'other' });
  }
  return add.length === 0 ? cols : [...cols, ...add];
}

/** シートの見出しに無くなった項目 (見出しを変えた・列を消した) */
export function missingHeaders(cols: PortalAdminColumn[], headers: string[]): string[] {
  const set = new Set(headers.map((h) => h.trim()));
  return cols.filter((c) => !set.has(c.header)).map((c) => c.header);
}

/** URL の #gid= / ?gid= のタブの番号。無ければ null */
export function gidFromUrl(url: string): number | null {
  const m = /[#?&]gid=(\d+)/.exec(url);
  return m?.[1] !== undefined ? Number(m[1]) : null;
}

export const SHEET_URL_PATTERN = /^https:\/\/docs\.google\.com\/spreadsheets\/d\/[\w-]{20,}/;
