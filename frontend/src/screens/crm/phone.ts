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

/**
 * 市外局番の桁数が番号の先頭だけで決まるもの (10 桁の固定電話)。
 * 日本の市外局番は 2〜5 桁で、先頭が同じでも桁数が違う局番が混在する (例: 042 と 0422)。
 * 総務省の番号表を持たずに区切ると誤った位置にハイフンが入るため、先頭が同じ別の局番が無いと
 * 分かっているものだけを載せる (03 東京 / 06 大阪 / 011 札幌 / 045 横浜 / 052 名古屋)。
 * ここに無い固定電話は区切らずに数字のまま出す。
 */
const FIXED_AREA_CODES: readonly string[] = ['03', '06', '011', '045', '052'];

/**
 * 画面に出すための電話番号 (ハイフン区切り)。発信・コピーに使う値は変えない (元の値 / `toDomesticPhone` を使う)。
 *
 * 規則 (`toDomesticPhone` で国内形式の数字にしてから区切る):
 * - 携帯・IP・M2M 等 (020 / 050 / 060 / 070 / 080 / 090 で 11 桁) → 3-4-4 (`090-1234-5678`)
 * - 0120 / 0570 / 0990 で 10 桁 → 4-3-3 (`0120-123-456`)
 * - 0800 で 11 桁 → 4-3-4 (`0800-123-4567`)
 * - 10 桁の固定電話で `FIXED_AREA_CODES` の局番 → 局番-市内局番-4 桁 (`03-1234-5678` / `045-123-4567`)
 * - それ以外 (局番の桁数を先頭だけで決められない固定電話、桁数が合わない番号) → `toDomesticPhone` の数字のまま
 * - 国内の番号と判断できないもの (`+1…` / `サンプル`) → `toDomesticPhone` と同じ (前後の空白を除いた元の値)
 */
export function formatPhoneForDisplay(raw: string | null | undefined): string | null {
  const d = toDomesticPhone(raw);
  if (d === null || !/^0\d+$/.test(d)) return d;
  const split = (...sizes: number[]) => {
    const parts: string[] = [];
    let i = 0;
    for (const n of sizes) { parts.push(d.slice(i, i + n)); i += n; }
    return parts.join('-');
  };
  if (d.length === 11) {
    if (d.startsWith('0800')) return split(4, 3, 4);
    if (/^0[256789]0/.test(d)) return split(3, 4, 4);
    return d;
  }
  if (d.length === 10) {
    if (/^0(120|570|990)/.test(d)) return split(4, 3, 3);
    const area = FIXED_AREA_CODES.find(code => d.startsWith(code));
    if (area !== undefined) return split(area.length, 6 - area.length, 4);
  }
  return d;
}
