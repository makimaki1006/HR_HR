import { describe, expect, it } from 'vitest';
import { freshnessLabel } from './workspaceModel';
import { detailUrl } from './useDealDetail';

describe('freshnessLabel (詳細を読んだ時刻からの経過)', () => {
  const at = '2026-10-05T03:00:00Z';
  const base = Date.parse(at);
  it('秒・分・時間・日で表す', () => {
    expect(freshnessLabel(at, base)).toBe('0秒前の情報');
    expect(freshnessLabel(at, base + 59_999)).toBe('59秒前の情報');
    expect(freshnessLabel(at, base + 60_000)).toBe('1分前の情報');
    expect(freshnessLabel(at, base + 59 * 60_000)).toBe('59分前の情報');
    expect(freshnessLabel(at, base + 3 * 3_600_000)).toBe('3時間前の情報');
    expect(freshnessLabel(at, base + 50 * 3_600_000)).toBe('2日前の情報');
  });
  it('端末の時計が遅れていても負にしない / 読めない時刻は出さない', () => {
    expect(freshnessLabel(at, base - 5_000)).toBe('0秒前の情報');
    expect(freshnessLabel('not a date', base)).toBeNull();
  });
});

describe('detailUrl (詳細の URL)', () => {
  it('fresh=1 を選んだ項目と並べて付ける', () => {
    expect(detailUrl('123')).toBe('/api/crm/workspace/deals/123');
    expect(detailUrl('123', undefined, { fresh: true })).toBe('/api/crm/workspace/deals/123?fresh=1');
    expect(detailUrl('123', { deals: ['bpo_10'], contacts: [], companies: [] }, { fresh: true }))
      .toBe('/api/crm/workspace/deals/123?deal_props=bpo_10&fresh=1');
    expect(detailUrl('123', { deals: ['bpo_10', 'bpo_32'], contacts: ['phone'], companies: [] }))
      .toBe('/api/crm/workspace/deals/123?deal_props=bpo_10%2Cbpo_32&contact_props=phone');
  });
});
