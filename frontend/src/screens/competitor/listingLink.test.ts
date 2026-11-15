import { describe, expect, it } from 'vitest';
import { DEFAULT_FORM } from './form';
import { competitorUrl, currentMonthLabel, fitRegion, readListingTarget, withListing } from './listingLink';

describe('求人から競合調査を開く URL', () => {
  it('求人の番号・名前・職種名・地域を付け、読み取ると同じ値に戻る', () => {
    const url = competitorUrl({ id: '30', name: '顧客A 配送スタッフ', prefecture: '大阪府', municipality: '堺市堺区', jobName: '配送ドライバー' });
    expect(url).toBe(`/app/competitor?${new URLSearchParams({ listing: '30', listing_name: '顧客A 配送スタッフ', job_name: '配送ドライバー', prefecture: '大阪府', municipality: '堺市堺区' }).toString()}`);
    expect(readListingTarget(url.slice(url.indexOf('?')))).toEqual({ id: '30', name: '顧客A 配送スタッフ', prefecture: '大阪府', municipality: '堺市堺区', jobName: '配送ドライバー' });
  });
  it('分からない条件は付けない。都道府県が無ければ市区町村も付けない。職種名は40文字まで', () => {
    expect(competitorUrl({ id: '30', municipality: '堺市堺区', jobName: null })).toBe('/app/competitor?listing=30');
    const long = competitorUrl({ id: '30', jobName: 'あ'.repeat(50) });
    expect(readListingTarget(long.slice(long.indexOf('?')))?.jobName).toBe('あ'.repeat(40));
  });
  it('求人の番号が無い・数字でないときは、ふつうの競合調査として開く', () => {
    expect(readListingTarget('?tab=excel')).toBeNull();
    expect(readListingTarget('?listing=abc&prefecture=大阪府')).toBeNull();
    expect(readListingTarget('?listing=')).toBeNull();
  });
  it('入力欄には職種名と地域だけを入れ、ほかの欄は変えない', () => {
    const base = { ...DEFAULT_FORM, searchKeyword: '配送 求人', topN: '30', prefecture: '北海道', municipality: '札幌市中央区' };
    expect(withListing(base, { id: '30', name: null, prefecture: '大阪府', municipality: '', jobName: '配送ドライバー' }))
      .toEqual({ ...base, jobName: '配送ドライバー', prefecture: '大阪府', municipality: '' });
  });
  it('選択肢に無い地域は外す（都道府県が無ければ全国、市区町村が無ければ指定しない）', () => {
    const cities = { 大阪府: ['大阪市北区', '堺市堺区'] };
    const values = { ...DEFAULT_FORM, prefecture: '大阪府', municipality: '大阪市' };
    expect(fitRegion(values, ['大阪府'], cities)).toEqual({ ...values, municipality: '' });
    expect(fitRegion({ ...values, municipality: '堺市堺区' }, ['大阪府'], cities).municipality).toBe('堺市堺区');
    expect(fitRegion({ ...values, prefecture: '大坂' }, ['大阪府'], cities)).toEqual({ ...values, prefecture: '', municipality: '' });
  });
  it('確かめる文の月は日本時間', () => {
    expect(currentMonthLabel(new Date('2026-11-30T15:30:00Z'))).toBe('12月');
    expect(currentMonthLabel(new Date('2026-11-15T07:20:00Z'))).toBe('11月');
  });
});
