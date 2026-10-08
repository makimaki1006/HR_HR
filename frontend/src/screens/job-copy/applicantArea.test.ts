import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { AREA_MASTER } from './areaMaster';
import { parseApplicantArea, roundAreaCounts, roundAreaDistribution, roundAreaLabel, roundApplicantAreasInRecord, roundJointDemographics } from './applicantArea';
import { buildDistribution } from './applicantCompositionModel';
import { parseJointDemographics, reverseSearch, reverseSearchOptions } from './reverseSearchModel';
import { parseRealMoc } from './realMoc';
import type { JobCopyRecord } from './data';

const rawAddress = '東京都新宿区西新宿2-8-1 ○○ビル301';
const leaks = /西新宿|○○ビル|301|芝公園|府内町|天神|2-8-1/u;

describe('applicant address rounding', () => {
  it('keeps only the prefecture and municipality of a full street address', () => {
    expect(parseApplicantArea(rawAddress, null)).toEqual({ prefecture: '東京都', municipality: '新宿区' });
    expect(parseApplicantArea('東京都', '新宿区西新宿2-8-1 ○○ビル301')).toEqual({ prefecture: '東京都', municipality: '新宿区' });
    expect(parseApplicantArea('東京都', rawAddress)).toEqual({ prefecture: '東京都', municipality: '新宿区' });
    expect(roundAreaLabel('municipality', `東京都 / ${rawAddress.slice(3)}`)).toBe('東京都新宿区');
    expect(roundAreaLabel('municipality', rawAddress)).toBe('東京都新宿区');
    expect(roundAreaLabel('prefecture', rawAddress)).toBe('東京都');
  });

  it('handles designated-city wards, omitted counties, ヶ/ケ and full-width spaces with the master names', () => {
    expect(roundAreaLabel('municipality', '神奈川県 / 横浜市保土ヶ谷区岩間町1-1')).toBe('神奈川県横浜市保土ケ谷区');
    expect(roundAreaLabel('municipality', '神奈川県 / 横浜市')).toBe('神奈川県横浜市');
    // マスタより新しい区名は市までにする
    expect(roundAreaLabel('municipality', '静岡県 / 浜松市中央区元城町103-2')).toBe('静岡県浜松市');
    expect(roundAreaLabel('municipality', '北海道 / 当別町白樺町58-9')).toBe('北海道石狩郡当別町');
    expect(roundAreaLabel('municipality', '大分県　大分市府内町1-1')).toBe('大分県大分市');
    expect(roundAreaLabel('prefecture', '大分')).toBe('大分県');
  });

  it('reads half-width katakana and full-width digits as the master name (NFKC)', () => {
    expect(roundAreaLabel('municipality', '北海道 / ﾆｾｺ町富士見１２３')).toBe('北海道虻田郡ニセコ町');
    expect(parseApplicantArea('北海道', 'ﾆｾｺ町')).toEqual({ prefecture: '北海道', municipality: '虻田郡ニセコ町' });
  });

  it('never guesses: unreadable cities become （市区町村不明） and ambiguous cities without a prefecture become 不明', () => {
    expect(roundAreaLabel('municipality', '東京都 / ○○ビル301')).toBe('東京都（市区町村不明）');
    expect(roundAreaLabel('municipality', '都道府県不明 / 新宿区西新宿2-8-1')).toBe('東京都新宿区');
    expect(roundAreaLabel('municipality', '都道府県不明 / 府中市宮西町1')).toBe('不明');
    expect(roundAreaLabel('prefecture', 'ビル301')).toBe('不明');
    expect(roundAreaLabel('municipality', '不明')).toBe('不明');
    // 丸め済みのラベルは何度通しても同じ
    for (const label of ['東京都新宿区', '東京都（市区町村不明）', 'その他', '不明']) expect(roundAreaLabel('municipality', label)).toBe(label);
  });

  it('merges areas with fewer than 3 applicants into その他 and keeps the total', () => {
    const result = roundAreaDistribution({ total: 7, categories: [
      { category: `東京都 / 新宿区西新宿2-8-1 ○○ビル301`, count: 1, percentage: null },
      { category: '東京都 / 新宿区歌舞伎町1-1', count: 2, percentage: null },
      { category: '大分県 / 大分市府内町1-1', count: 2, percentage: null },
      { category: '福岡県 / 福岡市中央区天神1', count: 1, percentage: null },
      { category: '不明', count: 1, percentage: null },
    ] }, 'municipality');
    expect(result.categories.map(row => [row.category, row.count])).toEqual([['東京都新宿区', 3], ['その他', 3], ['不明', 1]]);
    expect(result.categories.reduce((sum, row) => sum + row.count, 0)).toBe(7);
    expect(result.categories[0]?.percentage).toBeCloseTo(300 / 7, 10);
    expect(JSON.stringify(result)).not.toMatch(leaks);
    expect(roundAreaCounts('prefecture', { 東京都: 3, 大分県: 2, 不明: 1 })).toEqual({ 東京都: 3, その他: 2, 不明: 1 });
  });

  it('rounds and merges the demo applicant rows (12 applications of demo-001 v1)', () => {
    const rows = [
      ['大分県', '大分市'], ['大分県', '大分市'], ['大分県', '別府市'], ['大分県', '大分市'], ['大分県', '別府市'], ['福岡県', '福岡市'],
      ['大分県', '大分市'], ['大分県', '中津市'], ['大分県', '大分市'], ['福岡県', '福岡市'], [null, null], ['大分県', null],
    ].map(([prefecture, municipality]) => ({ gender: null, age: null, prefecture: prefecture ?? null, municipality: municipality ?? null }));
    expect(buildDistribution(rows, 'municipality')?.categories.map(row => [row.category, row.count])).toEqual([['大分県大分市', 5], ['その他', 6], ['不明', 1]]);
    expect(buildDistribution(rows, 'prefecture')?.categories.map(row => [row.category, row.count])).toEqual([['大分県', 9], ['その他', 2], ['不明', 1]]);
  });

  it('rounds joint cells, merges small areas per job and never offers a raw address as a reverse-search option', () => {
    const joint = parseJointDemographics({ total: 6, cells: [
      { gender: '男性', age: '30代', prefecture: '東京都', municipality: '東京都 / 新宿区西新宿2-8-1 ○○ビル301', count: 2 },
      { gender: '男性', age: '30代', prefecture: '東京都', municipality: '東京都 / 新宿区歌舞伎町1-1', count: 2 },
      { gender: '女性', age: '20代', prefecture: '大分県', municipality: '大分県 / 大分市府内町1-1', count: 1 },
      { gender: '女性', age: '20代', prefecture: '福岡県', municipality: '福岡県 / 福岡市中央区天神1', count: 1 },
    ] }, 6);
    expect(joint).toEqual({ total: 6, cells: [
      { gender: '男性', age: '30代', prefecture: '東京都', municipality: '東京都新宿区', count: 4 },
      { gender: '女性', age: '20代', prefecture: 'その他', municipality: 'その他', count: 2 },
    ] });
    const job = { id: 'synthetic', jointDemographics: joint } as JobCopyRecord;
    expect(reverseSearchOptions([job], 'municipality')).toEqual(['東京都新宿区', 'その他']);
    expect(reverseSearch([job], { gender: '', age: '', prefecture: '', municipality: '東京都新宿区', minimum: 1 }).map(row => [row.count, row.denominator])).toEqual([[4, 6]]);
    // 丸める前のデータが直接入っていても、選択肢と検索は丸めた値で答える
    const unrounded = { id: 'raw', jointDemographics: { total: 3, cells: [{ gender: '男性', age: '30代', prefecture: '東京都', municipality: '東京都 / 新宿区西新宿2-8-1', count: 3 }] } } as JobCopyRecord;
    expect(reverseSearchOptions([unrounded], 'municipality')).toEqual(['東京都新宿区']);
    expect(roundJointDemographics(joint)).toEqual(joint);
  });

  it('rounds every applicant area in a captured snapshot before it reaches the screen', () => {
    const capturedAt = '2026-10-06T00:00:00.000Z';
    const id = `capture-synthetic-privacy-${capturedAt}`;
    const municipality = { [`東京都 / ${rawAddress.slice(3)}`]: 31, '東京都 / 港区芝公園4-2-8': 1, '大分県 / 大分市府内町1-1': 2 };
    const versionMunicipality = { denominator: 3, categories: [{ category: `東京都 / ${rawAddress.slice(3)}`, count: 3, percentage: 100 }] };
    const snapshot = { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{ id: 'synthetic-privacy', hubspotListingId: '30', title: '合成の住所確認', company: '合成会社', media: 'HRハッカー', mediaJobId: '12345678', location: '東京都', body: '合成の本文', images: [] }] },
      results: [{ listing_id: '30', summary: { total: 34, missing_date: 0, by_date: { '2026-09-10': 34 }, dimensions: { prefecture: { [rawAddress]: 32, 大分県: 2 }, municipality },
        joint_demographics: { total: 34, cells: Object.entries(municipality).map(([label, count]) => ({ gender: '男性', age: '30代', prefecture: label.split(' / ')[0], municipality: label, count })) } },
      dated_comparison: { total: 34, unknown: 31, basis: '合成の日付対応', by_version: { [id]: { count: 3, dimensions: { gender: null, age: null, prefecture: null, municipality: versionMunicipality } } }, daily_representatives: {} } }] };
    const [job] = parseRealMoc(JSON.stringify(snapshot));
    expect(job?.overallApplications?.distributions.municipality?.categories.map(row => [row.category, row.count])).toEqual([['東京都新宿区', 31], ['その他', 3]]);
    // 東京都 is counted from the protected cells (31), not taken from the stored total (32): with
    // 「東京都 32」 next to 「男性・30代・東京都新宿区 31」, the hidden 港区 applicant would be in 東京都.
    expect(job?.overallApplications?.distributions.prefecture?.categories.map(row => [row.category, row.count])).toEqual([['東京都', 31], ['その他', 3]]);
    expect(job?.versions[0]?.distributions?.municipality?.categories.map(row => [row.category, row.count])).toEqual([['東京都新宿区', 3]]);
    expect(job?.jointDemographics?.cells.map(cell => [cell.prefecture, cell.municipality, cell.count])).toEqual([['東京都', '東京都新宿区', 31], ['その他', 'その他', 3]]);
    expect(JSON.stringify(job)).not.toMatch(leaks);
    if (job) expect(roundApplicantAreasInRecord(job)).toEqual(job);
  });
});

describe('area master', () => {
  it('matches the Rust city master src/geo/master_city.csv exactly', () => {
    const csv = readFileSync(new URL('../../../../src/geo/master_city.csv', import.meta.url), 'utf-8');
    const byPrefecture = new Map<number, string[]>();
    for (const line of csv.trim().split('\n').slice(1)) {
      const [, prefcode, name] = line.split(',');
      const code = Number(prefcode);
      const names = byPrefecture.get(code) ?? [];
      if (name && !names.includes(name)) names.push(name);
      byPrefecture.set(code, names);
    }
    expect(AREA_MASTER).toHaveLength(47);
    expect(AREA_MASTER.map((_, index) => byPrefecture.get(index + 1)?.join('|'))).toEqual(AREA_MASTER.map(([, joined]) => joined));
    expect(AREA_MASTER.reduce((sum, [, joined]) => sum + joined.split('|').length, 0)).toBe(1896);
  });
});
