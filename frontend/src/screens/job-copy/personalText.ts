/**
 * 応募理由の自由記述から、一人を指しうる部分を「＊＊」に置き換える。
 * サーバー（src/handlers/job_copy_live/applicant_reasons.rs の mask_personal_details）と同じ規則で、
 * 取り込んだファイルの記述にもここで同じ処理をかける（念のため）。
 *
 * 対象: 市区町村より細かい住所（丁目・番地・号・「3-10-1」「3の10の1」と、その直前の町名・建物名、漢数字の番地
 * 「三丁目十番一号」、部屋番号の付いた建物名「府内ビル201」、市区町村名の後ろの町名「大分市府内町」、大字と地番
 * 「大字松岡1234」、条丁目「北1条西2丁目」、郵便番号「〒8700021」）、電話番号（空白や点で区切ったものも）、
 * メールアドレス、LINE・ID の後ろのアカウント、生年月日、「さん」「様」「氏」「くん」「ちゃん」「君」「先生」の前・
 * 「と申します」の前・「紹介者」の後ろの名前。これらの付かない名前（姉の山田花子）は見つけられない。
 * 匿名化ではなく、できる範囲で隠す処理。
 */
import { isMunicipalityName } from './applicantArea';

export const MASK = '＊＊';

const isDigit = (c: string) => /^[0-9０-９]$/u.test(c);
const isKanjiDigit = (c: string) => '一二三四五六七八九十〇'.includes(c) && c !== '';
const isDash = (c: string) => '-－‐−―ー'.includes(c) && c !== '';
const isHan = (c: string) => /^[一-鿿々ヶケ]$/u.test(c);
const isKatakana = (c: string) => /^[゠-ヿ]$/u.test(c);
const isMailLocal = (c: string) => /^[A-Za-z0-9._%+-]$/u.test(c);
const isMailDomain = (c: string) => /^[A-Za-z0-9.-]$/u.test(c);
const HONORIFICS = ['さん', '様', '氏', 'くん', 'ちゃん', '君', '先生'];
/** 建物名に含まれる語。名前の直後の数字は部屋番号として扱う（府内ビル201）。 */
const BUILDING_WORDS = ['ビル', 'マンション', 'ハイツ', 'コーポ', 'アパート', 'レジデンス', 'メゾン', 'パレス', 'コート', 'ヒルズ', 'タワー', 'ハウス', '荘', '館', '棟', '寮'];
const COUNTERS = '年月日回件時分秒人名歳万円代階%％点位度倍本枚個';
const GENERIC_TOWNS = new Set(['町村', '区町村', '町内', '村内']);

/** index から始まる漢数字の並びの後ろに、番地の語（十番・一号・三丁目）が続くか。 */
function kanjiNumberContinuesAddress(chars: string[], index: number): boolean {
  let look = index;
  while (look < chars.length && isKanjiDigit(chars[look] ?? '')) look += 1;
  if (look === index) return false;
  const next = chars[look] ?? ''; const after = chars[look + 1] ?? '';
  if (next === '号') return true;
  if (next === '番') return after !== '目';
  if (next === '丁') return after === '目';
  return isDash(next) && isDigit(after);
}

/** 部屋番号: 建物の語を含む名前（漢字・カタカナ 12 文字まで）の直後の 2〜4 桁で、後ろが「年」「回」などでない。 */
function isRoomNumber(chars: string[], start: number, end: number, digits: number): boolean {
  if (digits < 2 || digits > 4 || COUNTERS.includes(chars[end] ?? '\u0000')) return false;
  let from = start;
  while (from > 0 && start - from < 12 && (isHan(chars[from - 1] ?? '') || isKatakana(chars[from - 1] ?? ''))) from -= 1;
  const name = chars.slice(from, start).join('');
  return BUILDING_WORDS.some(word => name.includes(word));
}

/** 市区町村名（マスタにある名前）の直後に続く町名（漢字・カタカナで「町」「村」で終わる）の終わりの位置。 */
function townAfterMunicipality(chars: string[], index: number): number | null {
  if (!'市区町村郡'.includes(chars[index] ?? '\u0000')) return null;
  let named = false;
  for (let length = 2; length <= 7 && !named; length++) {
    if (index + 1 >= length) named = isMunicipalityName(chars.slice(index + 1 - length, index + 1).join(''));
  }
  if (!named) return null;
  let end = index + 1;
  while (end < chars.length && end - index <= 10 && (isHan(chars[end] ?? '') || isKatakana(chars[end] ?? ''))) end += 1;
  const town = chars.slice(index + 1, end).join('');
  const last = chars[end - 1] ?? '';
  return end > index + 2 && !GENERIC_TOWNS.has(town) && (last === '町' || last === '村') ? end : null;
}
const NOT_NAMES = new Set(['皆', '客', 'お客', '奥', '神', '王', '利用者', '患者', '諸']);
/** さん・くん・ちゃんの前にあっても名前ではないひらがな（たくさん、みなさん、おかあさん など）。 */
const NOT_HIRAGANA_NAMES = ['みな', 'たく', 'みんな', 'おじい', 'おばあ', 'おかあ', 'おとう', 'おねえ', 'おにい', 'おば', 'おじ', 'あか', 'おく', 'おつかれ', 'ごくろう', 'あなた', 'どちら'];
const isHiragana = (c: string) => /^[ぁ-ゟ]$/u.test(c);
const isNameChar = (c: string) => isHan(c) || isKatakana(c);
/** 全角英数字（ＩＤ、ｔａｒｏ）を半角にする。 */
const asciiFold = (c: string) => (c >= '！' && c <= '～' ? String.fromCharCode(c.charCodeAt(0) - 0xfee0) : c);
const startsWith = (chars: string[], index: number, word: string) => Array.from(word).every((w, offset) => chars[index + offset] === w);

/** 見出しと値の間の空白・コロン・「は」を飛ばす（生年月日：、LINE ID は）。 */
function skipLabelGap(chars: string[], index: number): number {
  let at = index;
  while (at < chars.length && at - index < 4 && ' 　:：は'.includes(chars[at] ?? '\u0000')) at += 1;
  return at;
}
/** start から書かれた日付（1990年5月1日、1990/5/1、S55.3.1、平成2年5月1日）の終わり。数字を含むときだけ。 */
function dateRunEnd(chars: string[], start: number): number | null {
  let end = start; let digits = 0;
  while (end < chars.length && end - start < 20) {
    const c = chars[end] ?? '';
    if (isDigit(c) || isKanjiDigit(c)) digits += 1;
    else if (!('年月日/／.．-－ 　'.includes(c) || '昭和平成令元'.includes(c) || 'SHRshr'.includes(asciiFold(c)))) break;
    end += 1;
  }
  while (end > start && ' 　'.includes(chars[end - 1] ?? '\u0000')) end -= 1;
  return digits > 0 ? end : null;
}
/** 7 桁の数字の前が 〒 か 郵便番号（間に空白・コロン可）か。 */
function postalMarkBefore(chars: string[], index: number): boolean {
  let at = index;
  while (at > 0 && index - at < 3 && ' 　:：'.includes(chars[at - 1] ?? '\u0000')) at -= 1;
  return (at > 0 && chars[at - 1] === '〒') || (at >= 4 && startsWith(chars, at - 4, '郵便番号'));
}
/** 番号の直前の名前が「大字」を含む、「字」で始まる、市町村郡の直後に「字」がある（大字松岡1234、村字中原567）。文字・数字・赤字は除く。 */
function isRuralLotName(chars: string[], index: number): boolean {
  let from = index;
  while (from > 0 && index - from < 12 && isNameChar(chars[from - 1] ?? '')) from -= 1;
  const name = chars.slice(from, index);
  return name[0] === '字' || name.some((c, position) => position > 0 && c === '字' && ((name[position - 1] ?? '') === '大' || '市町村郡'.includes(name[position - 1] ?? '\u0000')));
}
/** end の直前に書かれた名前の始まり（漢字・カタカナ 8 文字まで、空白 1 つの前の姓も。hiragana のときはひらがな 6 文字まで）。 */
function nameBefore(chars: string[], end: number, hiragana: boolean): number | null {
  let start = end;
  while (start > 0 && end - start < 8 && isNameChar(chars[start - 1] ?? '')) start -= 1;
  if (start < end) {
    if (NOT_NAMES.has(chars.slice(start, end).join(''))) return null;
    if (start >= 2 && ' 　'.includes(chars[start - 1] ?? '\u0000') && isNameChar(chars[start - 2] ?? '')) {
      const gap = start - 1; let surname = gap;
      while (surname > 0 && gap - surname < 6 && isNameChar(chars[surname - 1] ?? '')) surname -= 1;
      return surname;
    }
    return start;
  }
  if (!hiragana) return null;
  while (start > 0 && end - start < 6 && isHiragana(chars[start - 1] ?? '')) start -= 1;
  const name = chars.slice(start, end).join('');
  return end - start >= 2 && !NOT_HIRAGANA_NAMES.some(word => name.endsWith(word)) ? start : null;
}
/** 見出しの後ろに書かれた名前（紹介者：佐藤一郎）の終わり。漢字・カタカナ 8 文字まで、空白 1 つの後ろの名も。 */
function nameAfter(chars: string[], start: number): number {
  let end = start;
  while (end < chars.length && end - start < 8 && isNameChar(chars[end] ?? '')) end += 1;
  if (end > start && end + 1 < chars.length && ' 　'.includes(chars[end] ?? '\u0000') && isNameChar(chars[end + 1] ?? '')) {
    const gap = end + 1; end = gap;
    while (end < chars.length && end - gap < 6 && isNameChar(chars[end] ?? '')) end += 1;
  }
  return end;
}
/** LINE や ID の後ろのアカウント（LINE ID: taro_yamada123）の範囲。3 文字以上続かなければ null。 */
function accountAfterLabel(chars: string[], index: number): [number, number] | null {
  const wordAt = (at: number, word: string) => Array.from(word).every((w, offset) => asciiFold(chars[at + offset] ?? '').toLowerCase() === w);
  const boundary = (at: number) => !/^[A-Za-z]$/u.test(asciiFold(chars[at] ?? ''));
  if (index > 0 && /^[A-Za-z0-9]$/u.test(asciiFold(chars[index - 1] ?? ''))) return null;
  let at: number;
  if (wordAt(index, 'line') && boundary(index + 4)) at = index + 4;
  else if (wordAt(index, 'id') && boundary(index + 2)) at = index + 2;
  else return null;
  at = skipLabelGap(chars, at);
  if (wordAt(at, 'id') && boundary(at + 2)) at = skipLabelGap(chars, at + 2);
  let end = at;
  while (end < chars.length && /^[A-Za-z0-9._\-@]$/u.test(asciiFold(chars[end] ?? ''))) end += 1;
  return end - at >= 3 ? [at, end] : null;
}

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
      if (start < index && end > index + 1) hide(start, end);
      index = Math.max(end, index + 1);
      continue;
    }
    // LINE・ID の後ろのアカウント
    const account = accountAfterLabel(chars, index);
    if (account) { hide(account[0], account[1]); index = account[1]; continue; }
    // 見出しの後ろの生年月日
    const birthLabel = ['生年月日', '誕生日'].find(label => startsWith(chars, index, label));
    if (birthLabel) {
      const start = skipLabelGap(chars, index + Array.from(birthLabel).length);
      const end = dateRunEnd(chars, start);
      if (end !== null) { hide(start, end); index = end; } else index = start;
      continue;
    }
    // 紹介者の後ろの名前
    if (startsWith(chars, index, '紹介者')) {
      const start = skipLabelGap(chars, index + 3);
      const end = nameAfter(chars, start);
      hide(start, end);
      index = Math.max(end, index + 3);
      continue;
    }
    let startsNumber = isDigit(c);
    if (!startsNumber && isKanjiDigit(c)) {
      let look = index;
      while (look < chars.length && isKanjiDigit(at(look))) look += 1;
      startsNumber = at(look) === '丁';
    }
    if (startsNumber) {
      // 「生まれ」の前の日付
      if (isDigit(c)) {
        const end = dateRunEnd(chars, index);
        if (end !== null) {
          let after = end;
          while (after < chars.length && ' 　'.includes(at(after) || '\u0000')) after += 1;
          if (startsWith(chars, after, '生まれ') || startsWith(chars, after, '生れ')) { hide(index, end); index = end; continue; }
        }
      }
      let end = index; let digits = 0; let group = 0; let longestGroup = 0;
      let address = false; let dashed = false; let spaced = false; let joins = 0;
      while (end < chars.length) {
        const here = at(end); const next = at(end + 1);
        const afterDigit = end > index && isDigit(at(end - 1));
        if (isDigit(here) || (isKanjiDigit(here) && (!address || kanjiNumberContinuesAddress(chars, end)))) {
          if (isDigit(here)) { digits += 1; group += 1; longestGroup = Math.max(longestGroup, group); }
          end += 1;
        }
        else if (isDash(here) && isDigit(next) && end > index) { dashed = true; end += 1; }
        else if (here === 'の' && afterDigit && isDigit(next)) { joins += 1; group = 0; end += 1; } // 3の10の1
        else if (' 　.．'.includes(here) && here !== '' && afterDigit && isDigit(next)) { spaced = true; group = 0; end += 1; } // 090 1234 5678
        else if (here === '丁' && next === '目') { address = true; end += 2; }
        else if (here === '番' && next === '地') { address = true; end += 2; }
        else if ((here === '番' && next !== '目') || here === '号') { address = true; end += 1; if (at(end) === '室') end += 1; }
        else if ('()（）'.includes(here) && here !== '' && digits > 0 && isDigit(next)) { dashed = true; end += 1; }
        else break;
      }
      const spacedPhone = spaced && joins === 0 && digits >= 10 && digits <= 11 && (c === '0' || c === '０');
      const lot = joins >= 2 || (joins === 1 && index > 0 && '町村字丁目通'.includes(at(index - 1) || '\u0000'));
      const rural = isRuralLotName(chars, index);
      const postal = !dashed && !address && digits === 7 && longestGroup === 7 && postalMarkBefore(chars, index);
      const room = !address && !dashed && joins === 0 && isRoomNumber(chars, index, end, digits);
      if (address || dashed || longestGroup >= 8 || room || spacedPhone || lot || rural || postal) {
        // 住所の番号の直前の町名・建物名（電話番号の前の「携帯」などは残す）。北1条西2丁目の「北1条」も
        const street = address || room || lot || rural || (dashed && digits < 10);
        let start = index; let taken = 0;
        while (street && start > 0 && taken < 12) {
          if (isNameChar(at(start - 1))) { start -= 1; taken += 1; }
          else if (isDigit(at(start - 1)) && at(start) === '条') { while (start > 0 && isDigit(at(start - 1))) { start -= 1; taken += 1; } }
          else break;
        }
        hide(start, end);
      }
      index = Math.max(end, index + 1);
      continue;
    }
    // 市区町村名の後ろの町名（大分市府内町に住んでいます: 市区町村名は残す）
    const town = townAfterMunicipality(chars, index);
    if (town !== null) { hide(index + 1, town); index = town; continue; }
    // 大字（大分市大字松岡: 市区町村名は残す）
    if (startsWith(chars, index, '大字')) {
      let end = index + 2;
      while (end < chars.length && end - index < 12 && isNameChar(at(end))) end += 1;
      if (end > index + 2) hide(index, end);
      index = end;
      continue;
    }
    // 「と申します」の前の名前
    if (['と申します', 'と申し', 'と言います', 'といいます'].some(word => startsWith(chars, index, word))) {
      const start = nameBefore(chars, index, false);
      if (start !== null) hide(start, index);
      index += 1;
      continue;
    }
    // 「さん」「様」「氏」「くん」「ちゃん」「君」「先生」の前の名前（「ちゃんと」は除く）
    const honorific = HONORIFICS.find(word => startsWith(chars, index, word));
    if (honorific) {
      const length = Array.from(honorific).length;
      const kana = ['さん', 'くん', 'ちゃん'].includes(honorific);
      if (!(honorific === 'ちゃん' && at(index + length) === 'と')) {
        const start = nameBefore(chars, index, kana);
        if (start !== null) hide(start, index);
      }
      index += length;
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
