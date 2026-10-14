import { describe, expect, it } from 'vitest';
import { appsChange, filterCustomers, sortCustomers, yen } from './homeModel';
import type { HomeCustomer } from './homeModel';
import { buildCrumbs, readTrail, trailAt, writeTrail } from './trail';

const c = (id: string, name: string | null, over: Partial<HomeCustomer> = {}): HomeCustomer => ({ company_id: id, name, deals: 1, live_listings: 1, recent_apps: 0, prev_apps: 0, recent_cost: 0, cost_per_app: null, ...over });
const customers = [
  c('1', '合成運輸', { recent_apps: 20, recent_cost: 40000, cost_per_app: 2000, deals: 3 }),
  c('2', '合成介護', { recent_apps: 5, recent_cost: 50000, cost_per_app: 10000, deals: 1 }),
  c('3', '合成倉庫', { recent_apps: 0, recent_cost: 8000, cost_per_app: null, deals: 2 }),
];
describe('顧客の表', () => {
  it('応募の多い順・少ない順に並べ、1件あたりが出ない行は最後に置く', () => {
    expect(sortCustomers(customers, 'apps', true).map(x => x.company_id)).toEqual(['1', '2', '3']);
    expect(sortCustomers(customers, 'cpa', false).map(x => x.company_id)).toEqual(['1', '2', '3']);
    expect(sortCustomers(customers, 'cpa', true).map(x => x.company_id)).toEqual(['2', '1', '3']);
    expect(sortCustomers(customers, 'deals', true).map(x => x.company_id)).toEqual(['1', '3', '2']);
    const ascending = sortCustomers(customers, 'name', false).map(x => x.name);
    expect(sortCustomers(customers, 'name', true).map(x => x.name)).toEqual([...ascending].reverse());
  });
  it('顧客名で絞り込む', () => {
    expect(filterCustomers(customers, ' 介護 ').map(x => x.company_id)).toEqual(['2']);
    expect(filterCustomers(customers, '').length).toBe(3);
  });
  it('金額と応募の表記', () => {
    expect(yen(12345)).toBe('¥12,345');
    expect(yen(null)).toBe('—');
    expect(appsChange(41, 58)).toBe('41 → 58 件');
    expect(appsChange(null, 3)).toBe('—');
  });
});
describe('階層の URL', () => {
  it('数字の ID だけを読み、ほかの指定は残して書き戻す', () => {
    expect(readTrail('?company=12&deal=x&listing=7&demo=1')).toEqual({ company: '12', listing: '7' });
    expect(writeTrail('?job=a&company=1&deal=2', { company: '1', listing: '9' })).toBe('?job=a&company=1&listing=9');
    expect(writeTrail('?company=1', {})).toBe('');
  });
  it('上の階層へ戻ると下の階層が消える', () => {
    const trail = { company: '1', deal: '2', listing: '3' };
    expect(trailAt(trail, 'home')).toEqual({});
    expect(trailAt(trail, 'company')).toEqual({ company: '1' });
    expect(trailAt(trail, 'deal')).toEqual({ company: '1', deal: '2' });
    expect(trailAt(trail, 'listing')).toEqual(trail);
  });
  it('パンくずは最後だけがリンクにならず、名前が分からない階層は仮の名前になる', () => {
    expect(buildCrumbs({}, {})).toEqual([{ level: 'home', label: '案件管理', current: true }]);
    expect(buildCrumbs({ company: '1', deal: '2', listing: '3' }, { company: '合成運輸', listing: '配送ドライバー' })).toEqual([
      { level: 'home', label: '案件管理', current: false },
      { level: 'company', label: '合成運輸', current: false },
      { level: 'deal', label: '案件', current: false },
      { level: 'listing', label: '配送ドライバー', current: true },
    ]);
  });
});
