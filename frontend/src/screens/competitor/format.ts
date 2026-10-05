// 旧 HTML (Rust の format! / format_number) と同じ文字列を作る。
// Rust の `{:.N}` は同点 (ちょうど半分) を偶数へ丸め、JS の toFixed は大きい方へ丸める。
// 旧新の表示を 1 文字も変えないため、同点だけ Rust に合わせる。

/** 欠測の表示。0 とは区別する。 */
export const MISSING = '—';

/** 小数 `digits` 桁の文字列。同点は偶数へ (Rust の format!("{:.N}") と同じ)。 */
export function fixed(x: number, digits: number): string {
  if (!Number.isFinite(x)) return String(x);
  const scale = 10 ** digits;
  const scaled = x * scale;
  // x が 2 進で正確に表せ、かつ scaled がちょうど .5 のときだけ同点。
  if (Math.abs(scaled % 1) === 0.5) {
    const abs = Math.abs(scaled);
    const lower = Math.floor(abs);
    const even = lower % 2 === 0 ? lower : lower + 1;
    const sign = x < 0 ? '-' : '';
    return sign + (even / scale).toFixed(digits);
  }
  return x.toFixed(digits);
}

/** 整数の桁区切り (Rust の format_number)。 */
export function fmtInt(n: number): string {
  const s = String(Math.trunc(n));
  const neg = s.startsWith('-');
  const digits = neg ? s.slice(1) : s;
  let out = '';
  for (let i = 0; i < digits.length; i += 1) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += ',';
    out += digits.charAt(i);
  }
  return neg ? `-${out}` : out;
}

/** 整数は桁区切り、小数は 2 桁。未取得 (null) は —。旧 number()。 */
export function fmtNumber(v: number | null): string {
  if (v === null) return MISSING;
  return Number.isInteger(v) ? fmtInt(v) : fixed(v, 2);
}

/** 給与。小数桁は Rust が返した `decimals` (月給 2、時給 0)。未取得 (null) は —。 */
export function fmtSalary(v: number | null | undefined, decimals: number): string {
  if (v === null || v === undefined) return MISSING;
  return fixed(v, decimals);
}

/** 占有率 (小数 0 桁の %)。 */
export function fmtPct0(v: number): string {
  return `${fixed(v, 0)}%`;
}
