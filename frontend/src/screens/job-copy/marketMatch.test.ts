import { describe, expect, it } from 'vitest';
import { chooseMarket, matchMarketTitle, prefectureFromLocation } from './marketMatch';

describe('market choice for a job', () => {
  it('takes the prefecture from the work location', () => {
    expect(prefectureFromLocation('大分県大分市')).toBe('大分県');
    expect(prefectureFromLocation('東京都新宿区西新宿')).toBe('東京都');
    expect(prefectureFromLocation('神奈川県横浜市')).toBe('神奈川県');
    expect(prefectureFromLocation('北海道札幌市')).toBe('北海道');
    expect(prefectureFromLocation('大分市')).toBeNull();
    expect(prefectureFromLocation('大分県大分市', ['福岡県'])).toBeNull();
  });
  it('prefers an exact category, then the longest contained one', () => {
    expect(matchMarketTitle('ドライバー', ['ドライバー', '配送ドライバー'])).toEqual({ title: 'ドライバー', how: 'exact', candidates: ['ドライバー'] });
    expect(matchMarketTitle('地域配送ドライバー', ['ドライバー', '配送ドライバー', '事務'])).toEqual({ title: '配送ドライバー', how: 'partial', candidates: ['配送ドライバー', 'ドライバー'] });
  });
  it('returns null with no candidate instead of picking a near one', () => {
    expect(matchMarketTitle('倉庫内ピッキングスタッフ', ['倉庫作業', '製造スタッフ'])).toBeNull();
    expect(matchMarketTitle('', ['ドライバー'])).toBeNull();
    expect(chooseMarket({ title: '倉庫内ピッキングスタッフ', location: '大分県別府市' }, ['倉庫作業'], ['大分県'])).toEqual({ title: null, prefecture: '大分県', titleHow: null, candidates: [] });
  });
});
