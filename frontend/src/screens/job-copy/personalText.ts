/**
 * 応募理由の自由記述から、一人を指しうる部分を「＊＊」に置き換える。
 * サーバー（src/handlers/job_copy_live/applicant_reasons.rs の mask_personal_details）と同じ規則で、
 * 取り込んだファイルの記述にもここで同じ処理をかける（念のため）。
 *
 * 対象: 市区町村より細かい住所（丁目・番地・号・「3-10-1」と、その直前の町名・建物名）、電話番号、
 * メールアドレス、「さん」「様」「氏」などの付いた名前。匿名化ではなく、できる範囲で隠す処理。
 */
export const MASK = '＊＊';

const isDigit = (c: string) => /^[0-9０-９]$/u.test(c);
const isKanjiDigit = (c: string) => '一二三四五六七八九十〇'.includes(c) && c !== '';
const isDash = (c: string) => '-－‐−―ー'.includes(c) && c !== '';
const isHan = (c: string) => /^[一-鿿々ヶケ]$/u.test(c);
const isKatakana = (c: string) => /^[゠-ヿ]$/u.test(c);
const isMailLocal = (c: string) => /^[A-Za-z0-9._%+-]$/u.test(c);
const isMailDomain = (c: string) => /^[A-Za-z0-9.-]$/u.test(c);
const HONORIFICS = ['さん', '様', '氏', 'くん', 'ちゃん'];
const NOT_NAMES = new Set(['皆', '客', 'お客', '奥', '神', '王']);

export function maskPersonalDetails(text: string): string {
  const chars = Array.from(text);
  const masked = chars.map(() => false);
  const at = (index: number) => chars[index] ?? '';
  const hide = (start: number, end: number) => { for (let index = start; index < end; index++) masked[index] = true; };
  let index = 0;
  while (index < chars.length) {
    const c = at(index);
    if (c === '@') {
      let start = index;
      while (start > 0 && isMailLocal(at(start - 1))) start -= 1;
      let end = index + 1;
      while (end < chars.length && isMailDomain(at(end))) end += 1;
      if (start < index && chars.slice(index + 1, end).includes('.')) hide(start, end);
      index = Math.max(end, index + 1);
      continue;
    }
    let startsNumber = isDigit(c);
    if (!startsNumber && isKanjiDigit(c)) {
      let look = index;
      while (look < chars.length && isKanjiDigit(at(look))) look += 1;
      startsNumber = at(look) === '丁';
    }
    if (startsNumber) {
      let end = index; let digits = 0; let address = false; let dashed = false;
      while (end < chars.length) {
        const here = at(end); const next = at(end + 1);
        if (isDigit(here) || (isKanjiDigit(here) && !address)) { if (isDigit(here)) digits += 1; end += 1; }
        else if (isDash(here) && isDigit(next) && end > index) { dashed = true; end += 1; }
        else if (here === '丁' && next === '目') { address = true; end += 2; }
        else if (here === '番' && next === '地') { address = true; end += 2; }
        else if ((here === '番' && next !== '目') || here === '号') { address = true; end += 1; if (at(end) === '室') end += 1; }
        else if ('()（）'.includes(here) && digits > 0 && isDigit(next)) { dashed = true; end += 1; }
        else break;
      }
      if (address || dashed || digits >= 8) {
        // 住所の番号の直前の町名・建物名（電話番号の前の「携帯」などは残す）
        const street = address || (dashed && digits < 10);
        let start = index; let taken = 0;
        while (street && start > 0 && taken < 12 && (isHan(at(start - 1)) || isKatakana(at(start - 1)))) { start -= 1; taken += 1; }
        hide(start, end);
      }
      index = Math.max(end, index + 1);
      continue;
    }
    const honorific = HONORIFICS.find(word => chars.slice(index, index + Array.from(word).length).join('') === word);
    if (honorific) {
      let start = index;
      while (start > 0 && index - start < 4 && (isHan(at(start - 1)) || isKatakana(at(start - 1)))) start -= 1;
      if (start < index && !NOT_NAMES.has(chars.slice(start, index).join(''))) hide(start, index);
      index += Array.from(honorific).length;
      continue;
    }
    index += 1;
  }
  let result = ''; let previous = false;
  chars.forEach((char, position) => {
    if (masked[position]) { if (!previous) result += MASK; } else result += char;
    previous = masked[position] ?? false;
  });
  return result;
}
