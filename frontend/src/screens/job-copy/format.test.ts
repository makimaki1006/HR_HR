import { describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { createElement } from 'react';
import {
  CAUSAL_PATTERN, HUBSPOT_ONLY_NOTE, JARGON_PATTERN, NOT_CAUSAL_NOTE, codeLabel, formatCount, formatDateJst, formatDateTimeJst,
  formatPeriodJst, formatYen, joinPresent, labelConditionLine, orderAgeBands, orderCategories, orderPrefectures, plainWording,
} from './format';
import { AssumptionsNote } from './AssumptionsNote';

describe('formatDateTimeJst', () => {
  it('converts a UTC timestamp with microseconds to JST minutes', () => {
    expect(formatDateTimeJst('2026-10-05T07:19:48.222798+00:00')).toBe('2026/10/05 16:19 JST');
  });
  it('rolls the date over at JST midnight', () => {
    expect(formatDateTimeJst('2026-09-30T15:00:00Z')).toBe('2026/10/01 00:00 JST');
    expect(formatDateTimeJst('2026-09-15T10:00:00+09:00')).toBe('2026/09/15 10:00 JST');
  });
  it('keeps a date-only value as a date without inventing a time', () => {
    expect(formatDateTimeJst('2026-09-01')).toBe('2026/09/01');
  });
  it('returns the fallback for missing or unreadable values instead of "Invalid Date"', () => {
    expect(formatDateTimeJst(null)).toBe('');
    expect(formatDateTimeJst('not a date', '未取得')).toBe('未取得');
  });
});

describe('formatDateJst / formatPeriodJst', () => {
  it('formats dates, months and periods', () => {
    expect(formatDateJst('2026-10-05T16:00:00Z')).toBe('2026/10/06');
    expect(formatDateJst('2026-08')).toBe('2026/08');
    expect(formatPeriodJst('2026-09-01', '2026-09-15')).toBe('2026/09/01〜2026/09/15');
    expect(formatPeriodJst('2026-09-25', null)).toBe('2026/09/25〜継続中');
  });
});

describe('formatYen', () => {
  it.each([
    [4_000_000, '400万円'], [250_000, '25万円'], [1_100, '1,100円'], [253_400, '25万3,400円'],
    [120_000_000, '1億2,000万円'], [100_000_000, '1億円'], [0, '0円'], [123.6, '124円'], [-30_000, '−3万円'],
  ])('%d → %s', (value, expected) => {
    expect(formatYen(value)).toBe(expected);
  });
  it('does not turn missing values into 0円', () => {
    expect(formatYen(null)).toBe('');
    expect(formatYen(undefined, '未接続')).toBe('未接続');
    expect(formatYen(Number.NaN, '未接続')).toBe('未接続');
  });
});

describe('formatCount / joinPresent', () => {
  it('formats counts with separators', () => {
    expect(formatCount(12345.678, '件')).toBe('12,345.68件');
    expect(formatCount(null, '件', '未取得')).toBe('未取得');
  });
  it('drops separators around empty values', () => {
    expect(joinPresent(['大分県大分市', '', null, 'HRハッカー'])).toBe('大分県大分市 · HRハッカー');
    expect(joinPresent([undefined, '  ', false])).toBe('');
    expect(joinPresent(['A', 0, 'B'], ' / ')).toBe('A / 0 / B');
  });
});

describe('code labels', () => {
  it('maps trial / training codes to labels', () => {
    expect(codeLabel('trial', '1')).toBe('あり');
    expect(codeLabel('training', 0)).toBe('なし');
    expect(codeLabel('training', true)).toBe('あり');
    expect(codeLabel('employment', 'part_time')).toBe('パート・アルバイト');
  });
  it('returns null for unknown codes instead of echoing them', () => {
    expect(codeLabel('trial', '9')).toBeNull();
    expect(codeLabel('trial', '')).toBeNull();
    expect(codeLabel('trial', null)).toBeNull();
  });
  it('relabels condition lines and leaves other lines untouched', () => {
    expect(labelConditionLine('試用期間：1')).toBe('試用期間：あり');
    expect(labelConditionLine('  研修: 0')).toBe('  研修: なし');
    expect(labelConditionLine('試用期間：3か月（条件同じ）')).toBe('試用期間：3か月（条件同じ）');
    expect(labelConditionLine('給与：月給250,000円')).toBe('給与：月給250,000円');
  });
});

describe('ordering', () => {
  it('puts 20歳未満 before 20代 and unknown last', () => {
    expect(orderAgeBands(['30代', '不明', '20代', '70歳以上', '20歳未満', '60代'])).toEqual(['20歳未満', '20代', '30代', '60代', '70歳以上', '不明']);
    expect(orderAgeBands(['20代', '19歳以下'])).toEqual(['19歳以下', '20代']);
  });
  it('orders prefectures north to south and keeps unknown last', () => {
    expect(orderPrefectures(['大分県', '不明', '東京都', '北海道', '沖縄県', '福岡県'])).toEqual(['北海道', '東京都', '福岡県', '大分県', '沖縄県', '不明']);
  });
  it('orders municipality categories by their prefecture and keeps the original order inside one prefecture', () => {
    const items = [{ category: '大分県 / 別府市' }, { category: '福岡県 / 福岡市' }, { category: '大分県 / 大分市' }, { category: '不明' }];
    expect(orderCategories('municipality', items).map(item => item.category)).toEqual(['福岡県 / 福岡市', '大分県 / 別府市', '大分県 / 大分市', '不明']);
  });
  it('leaves other dimensions in their incoming order', () => {
    const items = [{ category: '女性' }, { category: '男性' }];
    expect(orderCategories('gender', items)).toEqual(items);
  });
});

describe('plainWording', () => {
  it('rewrites developer terms that come from server strings', () => {
    expect(plainWording('既存市場レポートのctk_countで、応募者数ではありません。')).toBe('既存市場レポートのIndeed閲覧者指標で、応募者数ではありません。');
    expect(plainWording('変更検知日を基準に集計。媒体生成日が不明の観測は鮮度未確認。')).toBe('変更検知日を基準に集計。媒体生成日が不明の取得は鮮度未確認。');
    expect(plainWording('版対応不明3件')).toBe('どの版への応募か不明3件');
    expect(plainWording('過去CSV観測版')).toBe('過去CSV取得した版');
    expect(plainWording('source filename acquisition label; not publication timestamp')).toBe('ファイルを取得した日時（掲載が変わった日時ではありません）');
    expect(plainWording('求人文面（MOC）')).toBe('求人文面');
  });
  it('leaves no jargon behind', () => {
    const samples = ['観測ラベル', '本文観測', '掲載観測版', '複合集計', 'snapshot', 'スナップショット', '市場閲覧者指標（ctk）', 'fixture', '実データMOC'];
    for (const sample of samples) expect(plainWording(sample)).not.toMatch(JARGON_PATTERN);
  });
  it('returns an empty string for missing text', () => {
    expect(plainWording(undefined)).toBe('');
  });
});

describe('AssumptionsNote', () => {
  it('shows one line and keeps the HubSpot-only and not-causal notes in the disclosure', () => {
    const html = renderToStaticMarkup(createElement(AssumptionsNote, { summary: '応募はHubSpot記録分のみです。', items: ['期間は取得日です。', '', null] }));
    expect(html.match(/<p /g)).toHaveLength(1);
    expect(html).toContain('ⓘ 集計の前提');
    expect(html).toContain('期間は取得日です。');
    expect(html).toContain(HUBSPOT_ONLY_NOTE);
    expect(html).toContain(NOT_CAUSAL_NOTE);
    expect(html).not.toMatch(CAUSAL_PATTERN);
    expect(html).not.toMatch(JARGON_PATTERN);
    expect((html.match(/<li>/g) ?? []).length).toBe(3);
  });
  it('can drop the HubSpot note for panels without applications but keeps the not-causal note', () => {
    const html = renderToStaticMarkup(createElement(AssumptionsNote, { summary: '市場の数字です。', includeHubSpot: false }));
    expect(html).not.toContain(HUBSPOT_ONLY_NOTE);
    expect(html).toContain(NOT_CAUSAL_NOTE);
  });
});
