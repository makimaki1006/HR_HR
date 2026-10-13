import { describe, expect, it } from 'vitest';
import { chooseMarket, indeedTitleState, marketTitleNote, prefectureFromLocation } from './marketMatch';

describe('market choice for a job', () => {
  it('takes the prefecture from the work location', () => {
    expect(prefectureFromLocation('大分県大分市')).toBe('大分県');
    expect(prefectureFromLocation('東京都新宿区西新宿')).toBe('東京都');
    expect(prefectureFromLocation('神奈川県横浜市')).toBe('神奈川県');
    expect(prefectureFromLocation('北海道札幌市')).toBe('北海道');
    expect(prefectureFromLocation('大分市')).toBeNull();
    expect(prefectureFromLocation('大分県大分市', ['福岡県'])).toBeNull();
  });
  it('uses only the designated title and never guesses one from the job', () => {
    const job = { title: '地域配送ドライバー', location: '大分県大分市' };
    expect(chooseMarket({ ...job, indeedTitle: '配送ドライバー', indeedTitleState: 'set' }, ['大分県'])).toEqual({ title: '配送ドライバー', titleState: 'set', prefecture: '大分県' });
    // The job's own name equals a market title, but nothing was designated: still no title.
    expect(chooseMarket({ location: '大分県大分市', indeedTitleState: 'unset' }, ['大分県'])).toEqual({ title: null, titleState: 'unset', prefecture: '大分県' });
    expect(chooseMarket({ ...job, indeedTitle: null, indeedTitleState: 'none' }, ['大分県'])).toEqual({ title: null, titleState: 'none', prefecture: '大分県' });
    expect(chooseMarket(job, ['大分県']).titleState).toBe('unset');
  });
  it('treats a record without a title as 未指定 and names each state plainly', () => {
    expect(indeedTitleState({ indeedTitleState: 'set', indeedTitle: null })).toBe('unset');
    // 前後の空白は取り除き、空白だけの職種は未指定。
    expect(chooseMarket({ location: '大分県大分市', indeedTitle: '  看護師　', indeedTitleState: 'set' }, ['大分県']).title).toBe('看護師');
    expect(indeedTitleState({ indeedTitleState: 'set', indeedTitle: ' 　 ' })).toBe('unset');
    expect(chooseMarket({ location: '大分県大分市', indeedTitle: '   ', indeedTitleState: 'set' }, ['大分県'])).toEqual({ title: null, titleState: 'unset', prefecture: '大分県' });
    expect(marketTitleNote('unset')).toBe('市場と比べる職種が未指定です');
    expect(marketTitleNote('none')).toBe('市場と比べないと指定されています');
    expect(marketTitleNote('set')).toBeNull();
  });
});
