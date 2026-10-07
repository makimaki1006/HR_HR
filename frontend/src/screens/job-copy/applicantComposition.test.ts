import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ApplicantComposition } from './ApplicantComposition';
import { buildDistribution, compareDistributions, compositionRows } from './applicantCompositionModel';
import type { ApplicantAttributes, ApplicantDimension } from './applicantCompositionModel';
import { jobs } from './data';
import type { JobCopyRecord } from './data';

const rows: ApplicantAttributes[] = [
  { gender: '女性', age: 29, prefecture: '大分県', municipality: '大分市' },
  { gender: '男性', age: 30, prefecture: '大分県', municipality: '府中市' },
  { gender: '女性', age: 61, prefecture: '東京都', municipality: '府中市' },
  { gender: null, age: null, prefecture: null, municipality: null },
];

describe('applicant distributions', () => {
  it('shows acquired zero total before absent attribute buckets, while positive totals keep absent attributes unavailable', () => {
    const base = jobs[0];
    if (!base) throw new Error('Missing synthetic job');
    const zero: JobCopyRecord = { ...base, dataSource: 'hubspot', overallApplications: { total: 0, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {} } };
    const zeroMarkup = renderToStaticMarkup(createElement(ApplicantComposition, { job: zero }));
    const overall = zeroMarkup.split('<section class="ac-overall"')[1]?.split('<header>')[0] ?? '';
    expect(overall.match(/求人全体の応募は0件です。割合は算出できません。/g)).toHaveLength(4);
    expect(overall).not.toContain('この属性は未取得です');
    const positive = { ...zero, overallApplications: { total: 2, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {} } };
    const positiveMarkup = renderToStaticMarkup(createElement(ApplicantComposition, { job: positive }));
    const positiveOverall = positiveMarkup.split('<section class="ac-overall"')[1]?.split('<header>')[0] ?? '';
    expect(positiveOverall.match(/この属性は未取得です/g)).toHaveLength(4);
    expect(positiveOverall).not.toContain('求人全体の応募は0件です');
  });
  it('orders age bands independently of incoming applicant order', () => {
    const result = buildDistribution([61, null, 19, 42, 29].map(age => ({ gender: null, age, prefecture: null, municipality: null })), 'age');
    expect(result?.categories.map(item => item.category)).toEqual(['19歳以下', '20代', '40代', '60歳以上', '不明']);
  });
  it('keeps impossible and fractional ages unknown', () => {
    const result = buildDistribution([999, 35.5, Number.NaN].map(age => ({ gender: null, age, prefecture: null, municipality: null })), 'age');
    expect(result?.categories).toEqual([{ category: '不明', count: 3, percentage: 100 }]);
  });
  it.each<ApplicantDimension>(['gender', 'age', 'prefecture', 'municipality'])('uses every attributed application as the %s denominator, including missing attributes', dimension => {
    const distribution = buildDistribution(rows, dimension);
    expect(distribution?.total).toBe(4);
    expect(distribution?.categories.reduce((sum, item) => sum + item.count, 0)).toBe(4);
    expect(distribution?.categories.reduce((sum, item) => sum + (item.percentage ?? 0), 0)).toBeCloseTo(100);
    expect(distribution?.categories.find(item => item.category === '不明')).toMatchObject({ count: 1, percentage: 25 });
  });

  it('groups supplied ages into bands without inferring absent ages', () => {
    const result = buildDistribution(rows, 'age');
    expect(result?.categories.map(item => item.category)).toEqual(['20代', '30代', '60歳以上', '不明']);
    expect(buildDistribution([{ ...rows[0], gender: null, age: -1, prefecture: null, municipality: null }], 'age')?.categories[0]?.category).toBe('不明');
  });

  it('keeps cities with identical names distinct using their supplied prefectures', () => {
    const fuchu = (prefecture: string) => Array.from({ length: 3 }, () => ({ gender: null, age: null, prefecture, municipality: '府中市' }));
    const result = buildDistribution([...fuchu('東京都'), ...fuchu('広島県')], 'municipality');
    expect(result?.categories.map(item => [item.category, item.count])).toEqual([['東京都府中市', 3], ['広島県府中市', 3]]);
    // 都道府県が無いと府中市は 2 つあるので決めない。大分県に府中市は無いので市区町村不明にする
    expect(buildDistribution([{ gender: null, age: null, prefecture: null, municipality: '府中市' }], 'municipality')?.categories[0]?.category).toBe('不明');
    expect(buildDistribution(Array.from({ length: 3 }, () => ({ gender: null, age: null, prefecture: '大分県', municipality: '府中市' })), 'municipality')?.categories[0]?.category).toBe('大分県（市区町村不明）');
  });

  it('compares the union of categories and computes percentage-point rather than relative growth', () => {
    const before = buildDistribution(rows, 'gender');
    const after = buildDistribution([{ gender: '男性', age: null, prefecture: null, municipality: null }, { gender: 'その他', age: null, prefecture: null, municipality: null }], 'gender');
    const result = compareDistributions(before, after);
    expect(result?.find(item => item.category === '女性')).toMatchObject({ beforeCount: 2, afterCount: 0, beforePercentage: 50, afterPercentage: 0, deltaPp: -50 });
    expect(result?.find(item => item.category === '男性')).toMatchObject({ beforePercentage: 25, afterPercentage: 50, deltaPp: 25 });
    expect(result?.find(item => item.category === 'その他')).toMatchObject({ beforeCount: 0, afterCount: 1, beforePercentage: 0, afterPercentage: 50, deltaPp: 50 });
  });

  it('keeps a known zero denominator distinct from missing data and never divides by zero', () => {
    expect(buildDistribution([], 'gender')).toEqual({ total: 0, categories: [] });
    expect(buildDistribution(null, 'gender')).toBeNull();
    expect(compareDistributions(null, buildDistribution(rows, 'gender'))).toBeNull();
    const result = compareDistributions(buildDistribution([], 'gender'), buildDistribution(rows, 'gender'));
    expect(result?.every(item => item.beforePercentage === null && item.deltaPp === null)).toBe(true);
    expect(compareDistributions(buildDistribution([], 'gender'), buildDistribution([], 'gender'))).toEqual([]);
  });
});

describe('composition fixture boundary and UI', () => {
  const demo = jobs.find(job => job.id === 'demo-job-001');
  if (!demo) throw new Error('Missing demo fixture.');
  const driver: JobCopyRecord = demo;

  it('attributes exactly 12, 9, and 4 synthetic applications to the three published fixtures', () => {
    const published = driver.versions.filter(version => version.kind === 'published');
    expect(published.map(version => compositionRows(driver, version)?.length)).toEqual([12, 9, 4]);
    const compared = compareDistributions(buildDistribution(compositionRows(driver, published[0]), 'gender'), buildDistribution(compositionRows(driver, published[1]), 'gender'));
    expect(compared?.find(item => item.category === '男性')?.deltaPp).toBeCloseTo(-25);
    expect(compared?.find(item => item.category === '女性')?.deltaPp).toBeCloseTo(11.111111);
  });

  it('does not apply synthetic attributes to real imported jobs, AI proposals, received drafts, or mismatched counts', () => {
    const first = driver.versions[0];
    if (!first) throw new Error('Missing demo version.');
    expect(compositionRows({ ...driver, id: 'imported-private-job' }, first)).toBeNull();
    expect(compositionRows(driver, { ...first, kind: 'ai_draft' })).toBeNull();
    expect(compositionRows(driver, { ...first, kind: 'received' })).toBeNull();
    expect(compositionRows(driver, { ...first, applications: null })).toBeNull();
    expect(compositionRows(driver, { ...first, applications: { confirmed: 11, estimated: 0, unknown: 0 } })).toBeNull();
  });

  it('renders images before charts, exact textual values, dates, and observational caveats', () => {
    const html = renderToStaticMarkup(createElement(ApplicantComposition, { job: driver }));
    expect(html.indexOf('構成比較元の掲載画像')).toBeLessThan(html.indexOf('性別の構成比較'));
    expect(html).toContain('58.3%'); expect(html).toContain('33.3%'); expect(html).toContain('-25.0pt');
    expect(html).toContain('2026/09/01');
    expect(html).toContain('架空の応募者属性');
    expect(html).toContain('属性不明も含めます');
    expect(html).toContain('変更効果を示すものではありません');
    expect(html).toContain('課金情報は未取得');
    expect(html).not.toContain('<option value="demo-001-draft"');
  });

  it('shows unavailability rather than synthetic graphs for imported records', () => {
    const html = renderToStaticMarkup(createElement(ApplicantComposition, { job: { ...driver, id: 'imported-private-job' } }));
    expect(html).toContain('応募者の属性データは未取得');
    expect(html).toContain('0件・0%とは判定していません');
    expect(html).not.toContain('class="ac-chart"');
  });
});
