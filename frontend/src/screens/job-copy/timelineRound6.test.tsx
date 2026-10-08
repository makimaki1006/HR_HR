// @vitest-environment happy-dom
/**
 * Review round 6 (2026-10-08): the 課金 lane names only the sources it shows, the demo's made-up
 * HRハッカー amounts say so (also in print), one end month for the market data written one way,
 * the 「約」 note for dummy amounts, small groups of applicants in the reverse search and in
 * comparisons, masked reason texts, and the 日給 range written with 万 on both ends.
 */
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import { renderToStaticMarkup } from 'react-dom/server';
import { createElement } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { JobTimeline } from './JobTimeline';
import { ApplicantReasons } from './ApplicantReasons';
import { parseJointDemographics, reverseSearch, reverseSearchOptions } from './reverseSearchModel';
import { roundAreaDistribution } from './applicantArea';
import { compareDistributions } from './applicantCompositionModel';
import { maskPersonalDetails } from './personalText';
import { parseSalaryText } from './salaryExtract';
import { JARGON_PATTERN } from './format';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const demo = (id: string): JobCopyRecord => {
  const job = jobs.find(item => item.id === id);
  if (!job) throw new Error(`Missing ${id}`);
  return job;
};
const lane = (name: string) => screen.getByRole('group', { name });

describe('課金 lane source', () => {
  it('does not name HRハッカー for an Airワーク job that has only dummy billing', () => {
    const job = { ...demo('demo-job-002'), hrhPerformance: undefined };
    render(<JobTimeline job={job} marketMode="demo" />);
    const text = lane('課金').textContent;
    expect(text).toContain('仮の課金データ（ダミー）');
    expect(text).not.toContain('HRハッカー');
  });

  it('names HRハッカー only when HRハッカー rows exist, and marks the demo amounts as made up', () => {
    render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" />);
    const text = lane('課金').textContent;
    expect(text).toContain('HRハッカーの期間別実績（デモ用の架空の金額）');
    expect([...lane('課金').querySelectorAll('.jt-billing-hrhacker')].map(bar => bar.textContent)).toEqual(['架空 3万円', '架空 4万5,000円', '架空 1万2,000円']);
    // The note sits in the timeline (printed), not only in the demo banner (not printed).
    const note = document.querySelector('.jt-demo-billing');
    expect(note?.textContent).toContain('実際の請求額ではありません');
    expect(note?.closest('.jc-demo, .jc-no-print')).toBeNull();
  });

  it('does not mark a real HRハッカー job as made up', () => {
    const job = { ...demo('demo-job-001'), id: 'hubspot-30', dataSource: 'hubspot' as const };
    render(<JobTimeline job={job} marketMode="demo" />);
    expect(lane('課金').textContent).not.toContain('架空');
    expect(document.querySelector('.jt-demo-billing')).toBeNull();
    const cells = within(within(screen.getByRole('region', { name: '期間比較表の数値' })).getByRole('table')).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[3]?.textContent);
    expect(cells[0]).toBe('3万円');
  });
});

describe('「約」 note', () => {
  it('does not split or add dummy amounts in the period table, so no 約 note comes from them', () => {
    const job = { ...demo('demo-job-002'), hrhPerformance: undefined };
    render(<JobTimeline job={job} marketMode="demo" />);
    const cells = within(within(screen.getByRole('region', { name: '期間比較表の数値' })).getByRole('table')).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[3]?.textContent ?? '');
    expect(cells.every(cell => cell === '実際の課金データなし（仮の課金データ（ダミー）は合計しません）')).toBe(true);
    expect(screen.queryByText('「約」の付いた課金額は、課金の期間と版の期間がずれているため、日数で割って配分した金額です。')).toBeNull();
  });
});

describe('the end of the market data', () => {
  it('says how far 市場求人数 runs when 閲覧者指標 runs one month further, in one date form', async () => {
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: { source: '合成', titles: ['ドライバー'], prefectures: ['大分県'], ctk_basis: '応募数ではありません',
      series: path.includes('title=') ? { prefecture: '大分県', months: ['2026-07', '2026-08', '2026-09'], job_count: [220, 230, null], ctk_count: [300, 310, 320], employer_count: [1, 1, 1], seekers_per_posting: [3, 3, 3] } : null } }));
    render(<JobTimeline job={demo('demo-job-001')} />);
    expect(await screen.findByText('市場データは2026年9月まで（毎月更新）。市場求人数は2026年8月まで。2026年10月以降はデータなしとして表示しています')).toBeTruthy();
    await waitFor(() => { expect(within(within(screen.getByRole('region', { name: '期間比較表の数値' })).getByRole('table')).getAllByRole('row')[1]?.querySelectorAll('td')[4]?.textContent).toBe('データなし（市場求人数は2026年8月まで）'); });
    const text = document.body.textContent;
    // No sentence writes the end month as YYYY/MM.
    expect(text).not.toMatch(/\d{4}\/\d{2}(以降|まで)/u);
    expect(text).not.toMatch(JARGON_PATTERN);
  });
});

describe('small groups of applicants', () => {
  const cells = [
    { gender: '女性', age: '60代', prefecture: '大分県', municipality: '大分県 / 由布市湯布院町1234-5', count: 1 },
    { gender: '男性', age: '30代', prefecture: '大分県', municipality: '大分県 / 由布市挾間町', count: 2 },
    { gender: '男性', age: '20代', prefecture: '大分県', municipality: '大分県 / 大分市府内町', count: 3 },
  ];
  it('never shows one applicant by gender × age × city in the reverse search', () => {
    const joint = parseJointDemographics({ total: 6, cells }, 6);
    expect(joint.cells.every(cell => cell.count >= 3 || (cell.municipality === 'その他' && cell.prefecture === 'その他'))).toBe(true);
    const job = { ...demo('demo-job-001'), jointDemographics: joint };
    expect(reverseSearch([job], { gender: '女性', age: '60代', prefecture: '', municipality: '大分県由布市', minimum: 1 })).toEqual([]);
    expect(reverseSearchOptions([job], 'municipality')).toEqual(['大分県大分市', 'その他']);
    // The total is kept.
    expect(joint.cells.reduce((sum, cell) => sum + cell.count, 0)).toBe(6);
  });

  it('takes the prefecture from the rounded city so a search by 大分県 counts the same people', () => {
    const joint = parseJointDemographics({ total: 4, cells: [
      { gender: '女性', age: '20代', prefecture: '大分県', municipality: '大分県 / 大分市', count: 3 },
      { gender: '女性', age: '20代', prefecture: '不明', municipality: '都道府県不明 / 別府市北浜2-9-1', count: 1 },
    ] }, 4);
    expect(joint.cells.some(cell => cell.prefecture === '不明' && cell.municipality.startsWith('大分県'))).toBe(false);
  });

  it('does not compare an area merged into その他 as 0件 / −100 points', () => {
    const before = roundAreaDistribution({ total: 3, categories: [{ category: '大分県由布市', count: 3, percentage: null }] }, 'municipality');
    const after = roundAreaDistribution({ total: 4, categories: [{ category: '大分県由布市', count: 1, percentage: null }, { category: '大分県大分市', count: 3, percentage: null }] }, 'municipality');
    const rows = compareDistributions(before, after, { areas: true }) ?? [];
    expect(rows.map(row => [row.category, row.beforeCount, row.afterCount])).toEqual([['その他', 3, 1], ['大分県大分市', 0, 3]]);
    expect(rows.some(row => row.deltaPp === -100)).toBe(false);
    // Gender keeps その他 as a real answer.
    const genders = compareDistributions({ total: 2, categories: [{ category: '男性', count: 2, percentage: 100 }] }, { total: 2, categories: [{ category: 'その他', count: 2, percentage: 100 }] });
    expect(genders?.map(row => [row.category, row.beforeCount, row.afterCount])).toEqual([['男性', 2, 0], ['その他', 0, 2]]);
  });
});

describe('reason texts', () => {
  it('masks addresses, phone numbers, mail addresses and names, and keeps ordinary reasons', () => {
    expect(maskPersonalDetails('大分市府内町3丁目から近いため')).toBe('＊＊から近いため');
    expect(maskPersonalDetails('自宅は府内町3-10-1 府内ビル201号室です')).toBe('自宅は＊＊ ＊＊です');
    expect(maskPersonalDetails('連絡は097-123-4567まで')).toBe('連絡は＊＊まで');
    expect(maskPersonalDetails('携帯09012345678')).toBe('携帯＊＊');
    expect(maskPersonalDetails('mail: taro.yamada@example.co.jp でお願いします')).toBe('mail: ＊＊ でお願いします');
    expect(maskPersonalDetails('山田さんの紹介で応募')).toBe('＊＊さんの紹介で応募');
    expect(maskPersonalDetails('三丁目の店舗に近い')).toBe('＊＊の店舗に近い');
    for (const text of ['週3日から働けるため', '月給25万円以上で、土日休みだったので', '1日8件程度の配送なら続けられそう', '皆さんの雰囲気が良さそうだった', 'お客様と話す仕事がしたい', '応募は2回目です。3番目に見た求人でした']) {
      expect(maskPersonalDetails(text)).toBe(text);
    }
  });

  it('shows no HubSpot property name and no street address on the screen', () => {
    const job: JobCopyRecord = { ...demo('demo-job-001'), applicantReasons: { available: true, basis: 'recorded_applicant_reason', fetchedAt: '2026-10-05T00:00:00Z', totalApplicants: 1, totalSourceValues: 3,
      sourceCounts: { oubodouki: { missing: 0, blank: 0, nonblank: 1 }, ouboriyuu_baitaikisai: { missing: 1, blank: 0, nonblank: 0 }, ouboriyuu_hiaringu: { missing: 1, blank: 0, nonblank: 0 } },
      items: [{ id: 'a'.repeat(64), applicant: null, text: '大分市府内町3丁目から近いため', sourceProperty: 'oubodouki', applicationDate: '2026-09-02', collectedAt: null, versionId: null }], missing: 2, blank: 0, truncated: false, selections: null } };
    const html = renderToStaticMarkup(createElement(ApplicantReasons, { job }));
    expect(html).not.toContain('府内町3丁目');
    expect(html).toContain('＊＊から近いため');
    // Shown text and tooltips (the <option> values are form values, never shown).
    expect(html.replace(/ value="[^"]*"/gu, '')).not.toMatch(/oubodouki|ouboriyuu|HubSpotの項目名/u);
  });
});

describe('salary reader', () => {
  it('reads a 日給 range written with 万 on both ends and 円 only at the end', () => {
    expect(parseSalaryText('日給1万2000〜1万5000円')).toMatchObject({ kind: '日給', min: 12000, max: 15000 });
    expect(parseSalaryText('日給1万2000円〜1万5000円')).toMatchObject({ kind: '日給', min: 12000, max: 15000 });
    // Without 円 anywhere it is still not read.
    expect(parseSalaryText('日給1万2000')).toMatchObject({ kind: '不明' });
    expect(parseSalaryText('日給1万2000〜1万5000')).toMatchObject({ kind: '不明' });
  });
});
