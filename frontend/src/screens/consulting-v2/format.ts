// 表示の整え方。旧画面 (templates/tabs/cs_dashboard.html) の num / man / yen / DASH と同じ書き方にそろえる。

export const DASH = '—';

export function num(v: number | null | undefined, digits = 0): string {
  if (v === null || v === undefined || Number.isNaN(v)) return '';
  return v.toLocaleString('ja-JP', { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** 金額。万円単位に丸める。空は DASH (0 と書かない) */
export function yen(v: number | null | undefined): string {
  if (v === null || v === undefined) return DASH;
  return `${num(Math.round(v / 10000))}万`;
}

export function pct1(v: number | null | undefined, digits = 1): string {
  if (v === null || v === undefined) return DASH;
  return `${v.toFixed(digits)}%`;
}

export function nz(v: number | null | undefined): string {
  return v === null || v === undefined ? DASH : num(v);
}

/** 秒を「3分12秒」に。値が無ければ DASH */
export function durText(sec: number | null | undefined): string {
  if (sec === null || sec === undefined) return DASH;
  const s = Math.round(sec);
  if (s < 60) return `${String(s)}秒`;
  const rest = s % 60;
  return `${String(Math.floor(s / 60))}分${rest > 0 ? `${String(rest)}秒` : ''}`;
}

function parseDay(s: string | null | undefined): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s ?? '');
  if (!m) return null;
  return Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
}

/** "YYYY-MM-DD" どうしの日数 (b − a)。読めなければ null */
export function dayDiff(a: string | null | undefined, b: string | null | undefined): number | null {
  const x = parseDay(a);
  const y = parseDay(b);
  if (x === null || y === null) return null;
  return Math.round((y - x) / 86_400_000);
}

/** today から n か月前の日。日はその月の末日で止める。読めなければ空文字 (絞らない) */
export function monthsBefore(today: string | null | undefined, n: number): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(today ?? '');
  if (!m) return '';
  const y = Number(m[1]);
  const mo = Number(m[2]) - 1 - n;
  const last = new Date(Date.UTC(y, mo + 1, 0)).getUTCDate();
  return new Date(Date.UTC(y, mo, Math.min(Number(m[3]), last))).toISOString().slice(0, 10);
}

/** 注意の名札の短い書き方 (旧画面の FLAG_SHORT と同じ)。表に無いものはそのまま出す */
export const FLAG_SHORT: Record<string, string> = {
  '満足度が4以下': '満足度4以下',
  '満了まで60日以内': '満了60日以内',
  '接触の記録が無い': '接触記録なし',
  '接触が30日以上空いている': '接触30日以上空き',
  '担当が退職者のまま': '担当が退職者',
  '満了90日前でMTGが30日以上空いている': '満了90日前MTG30日以上空いている',
  '採用目標の半分に届いていない': '採用目標の半分未満',
  'MTGが60日以上空いている': 'MTG60日以上空いている',
  'MTGが30〜59日空いている': 'MTG30〜59日空いている',
};

export type FlagTone = 'danger' | 'warn' | 'result';

/** 名札の分類 (旧画面の flagGroup と同じ規則。順番を入れ替えると分類が変わる) */
export function flagTone(label: string): { tone: FlagTone; word: string } {
  if (label.includes('MTG')) return { tone: 'danger', word: '関係が危ない' };
  if (label.includes('満足度') || label.includes('退職')) return { tone: 'danger', word: '関係が危ない' };
  if (label.includes('満了') || label.includes('接触')) return { tone: 'warn', word: '時間が迫っている' };
  return { tone: 'result', word: '成果が出ていない' };
}

export function shortFlag(label: string): string {
  if (label.startsWith('採用単価が同じ進捗')) return '採用単価1.5倍以上';
  return FLAG_SHORT[label] ?? label;
}

/** 案件管理で同じ案件を開く相対 URL。取引 ID は数字だけ (HubSpot の取引 ID)。そうでなければ一覧 */
export function caseManagementHref(dealId: string | null | undefined): { href: string; linked: boolean } {
  const id = (dealId ?? '').trim();
  if (/^\d+$/.test(id)) return { href: `/app/job-copy?deal=${encodeURIComponent(id)}`, linked: true };
  return { href: '/app/job-copy', linked: false };
}

/** 空文字・null・undefined のときは代わりの文字を返す */
export function orText(v: string | null | undefined, fallback: string): string {
  return v === null || v === undefined || v === '' ? fallback : v;
}
