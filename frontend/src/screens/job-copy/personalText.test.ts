import { describe, expect, it } from 'vitest';
import { maskPersonalDetails } from './personalText';

// The same cases as review_round_4_* in src/handlers/job_copy_live/applicant_reasons.rs: the screen
// masks imported files with the same rules as the server.
describe('maskPersonalDetails (review round 4)', () => {
  it.each([
    ['090 1234 5678に連絡ください', '＊＊に連絡ください'],
    ['電話は090.1234.5678', '電話は＊＊'],
    ['097 123 4567', '＊＊'],
    ['大分市大字松岡1234', '＊＊'],
    ['大分市大字松岡', '大分市＊＊'],
    ['大字松岡1234番地', '＊＊'],
    ['府内町3の10の1に住んでいます', '＊＊に住んでいます'],
    ['荷揚町2の31', '＊＊'],
    ['〒8700021', '〒＊＊'],
    ['郵便番号：8700021です', '郵便番号：＊＊です'],
    ['ヤマダタロウさん', '＊＊さん'],
    ['東郷平八郎さん', '＊＊さん'],
    ['山田 太郎さん', '＊＊さん'],
    ['田中君の紹介', '＊＊君の紹介'],
    ['やまださんの紹介', '＊＊さんの紹介'],
    ['鈴木先生の紹介', '＊＊先生の紹介'],
    ['生年月日1990年5月1日', '生年月日＊＊'],
    ['誕生日は1990/5/1です', '誕生日は＊＊です'],
    ['1990年5月1日生まれです', '＊＊生まれです'],
    ['LINE ID: taro_yamada123', 'LINE ID: ＊＊'],
    ['ID：yamada01です', 'ID：＊＊です'],
    ['taro@example', '＊＊'],
    ['札幌市中央区北1条西2丁目', '＊＊'],
    ['紹介者：佐藤一郎', '紹介者：＊＊'],
    ['山田太郎と申します', '＊＊と申します'],
  ])('masks %s', (raw, expected) => {
    expect(maskPersonalDetails(raw)).toBe(expected);
  });

  it.each([
    'たくさんの求人から選びました',
    '皆さんの雰囲気が良さそう',
    'ちゃんと休みが取れるため',
    'おばあちゃんの介護経験があります',
    '時給1.5倍になるため',
    '2026.10.08に応募',
    'LINEで連絡しやすい',
    '文字数200字程度',
    '学校の先生になりたい',
    '週3の2日だけ',
    '電話番号は伝えていません',
    '1日8 9件',
  ])('leaves %s as it is', text => {
    expect(maskPersonalDetails(text)).toBe(text);
  });
});
