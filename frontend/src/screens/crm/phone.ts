/**
 * 電話番号の表示用の整形。HubSpot の値は `+81…` と `0…` が混在するので、画面では国内形式
 * (0 始まり、数字だけ) にそろえる。元の値は呼び出し側が保持し、ここでは書き換えない。
 *
 * - `+81 3-1234-5678` / `+81(0)3…` / `０３－１２３４…` (全角) → `0312345678`
 * - 国内形式 `03-1234-5678` → `0312345678`
 * - 日本以外の国番号 (`+1…`) と、番号と判断できない文字列 (`サンプル`) は、前後の空白を除いてそのまま返す
 * - 桁数が国内の番号として不自然 (9〜11 桁の外) なときも、そのまま返す (推測で直さない)
 */
export function toDomesticPhone(raw: string | null | undefined): string | null {
  if (raw === null || raw === undefined) return null;
  const original = raw.trim();
  if (original === '') return null;
  const s = original.normalize('NFKC');
  if (!/^[+\d\s\-().]+$/.test(s)) return original;
  const hasPlus = s.startsWith('+');
  let digits = s.replace(/\D/g, '');
  if (hasPlus) {
    if (!digits.startsWith('81')) return original;
    digits = digits.slice(2);
    if (!digits.startsWith('0')) digits = `0${digits}`;
  } else if (!digits.startsWith('0')) {
    return original;
  }
  return digits.length >= 9 && digits.length <= 11 ? digits : original;
}
