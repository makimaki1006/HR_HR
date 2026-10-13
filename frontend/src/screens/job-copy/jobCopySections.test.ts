import { describe, expect, it } from 'vitest';
import columns from '../../generated/jobgen/columns.json';
import { jobs } from './data';
import { jobCopySections } from './JobCopyBody';
import { versionItems } from './fullTextCompareModel';

describe('本文の項目の区切り', () => {
  it('デモの本文: 見出しだけの行の次から本文が続く形を、項目ごとに分ける', () => {
    const body = jobs[0]?.versions[0]?.body ?? '';
    const sections = jobCopySections(body);
    expect(sections.map(item => item.heading)).toEqual(expect.arrayContaining(['キャッチコピー', '仕事内容', '仕事の進め方', '職場紹介', '入社後の流れ', '応募後の流れ']));
    expect(sections.find(item => item.heading === 'キャッチコピー')?.text.trim()).toBe('いつもの道で、地域の暮らしを支える配送の仕事。');
    expect(sections.find(item => item.heading === '仕事内容')?.text).toContain('大分市周辺の店舗へ日用品を届けます。');
    expect(sections.find(item => item.heading === '仕事内容')?.text).not.toContain('朝礼でルート');
    expect(sections.find(item => item.heading === '仕事の進め方')?.text.trim().startsWith('朝礼でルート')).toBe(true);
  });
  it('「見出し：値」と、見出しだけの行の両方が同じ本文にあっても分かれる', () => {
    const body = 'キャッチコピー：地域を支える仕事。\n\n仕事内容\n荷物を届けます。\n\n職場紹介\n少人数のチームです。\n\n給与形態：月給';
    const items = versionItems({ id: 'v', label: 'v', body, source: 'x', observedAt: '2026-10-01T00:00:00Z', kind: 'received' } as never);
    expect([...items.entries()]).toEqual([['キャッチコピー', '地域を支える仕事。'], ['仕事内容', '荷物を届けます。'], ['職場紹介', '少人数のチームです。'], ['給与形態', '月給']]);
  });
  it('HRハッカーの本文（列名：値）は列ごとに分かれ、値の中の短い行は見出しにならない', () => {
    const lines = ['案件名：配送ドライバー', '仕事内容：決まったルートで日用品を届けます。', 'チームで支えます', '', '先輩と回ります', '給与形態：月給', '基本給与 最小：250000'];
    for (const key of ['案件名', '仕事内容', '給与形態', '基本給与 最小']) expect(columns).toContain(key);
    expect(jobCopySections(lines.join('\n')).map(item => [item.heading, item.text.trim()])).toEqual([
      ['案件名', '配送ドライバー'], ['仕事内容', '決まったルートで日用品を届けます。\nチームで支えます\n\n先輩と回ります'], ['給与形態', '月給'], ['基本給与 最小', '250000'],
    ]);
  });
});
