/** 日付の計算 (表示に依存しない部分)。値は常に YYYY-MM-DD */

export function pad(n: number): string { return String(n).padStart(2, '0'); }
export function toIso(y: number, m: number, d: number): string { return `${String(y)}-${pad(m)}-${pad(d)}`; }

export function fromDate(dt: Date): string { return toIso(dt.getFullYear(), dt.getMonth() + 1, dt.getDate()); }

export function todayIso(): string { return fromDate(new Date()); }

/** YYYY-MM-DD / YYYY/MM/DD / YYYY.M.D (全角数字も可) を受け付ける。実在しない日付は null */
export function parseDate(text: string): string | null {
  const half = text.trim().replace(/[０-９]/g, c => String(c.charCodeAt(0) - 0xff10));
  const m = /^(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})$/.exec(half);
  if (m === null) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const dt = new Date(y, mo - 1, d);
  if (dt.getFullYear() !== y || dt.getMonth() !== mo - 1 || dt.getDate() !== d) return null;
  return toIso(y, mo, d);
}

export function splitIso(iso: string): { y: number; m: number; d: number } {
  const [y = 0, m = 1, d = 1] = iso.split('-').map(Number);
  return { y, m, d };
}

/** 表示用 (YYYY/MM/DD)。日付として読めない文字列はそのまま */
export function displayDate(iso: string): string { return parseDate(iso) === null ? iso : iso.replaceAll('-', '/'); }

export function addDays(iso: string, n: number): string {
  const { y, m, d } = splitIso(iso);
  return fromDate(new Date(y, m - 1, d + n));
}

export function addMonths(iso: string, n: number): string {
  const { y, m, d } = splitIso(iso);
  const last = new Date(y, m - 1 + n + 1, 0).getDate();
  return fromDate(new Date(y, m - 1 + n, Math.min(d, last)));
}

/** 令和8年 など。平成以前は空 */
export function eraYear(y: number): string { return y >= 2019 ? `令和${y === 2019 ? '元' : String(y - 2018)}年` : ''; }

/** 表示する月の 6 週 × 7 日 (日曜始まり) */
export function monthGrid(y: number, m: number): string[] {
  const start = new Date(y, m - 1, 1).getDay();
  return Array.from({ length: 42 }, (_, i) => fromDate(new Date(y, m - 1, 1 - start + i)));
}

export function weekdayOf(iso: string): number {
  const { y, m, d } = splitIso(iso);
  return new Date(y, m - 1, d).getDay();
}
