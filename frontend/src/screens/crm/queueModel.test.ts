import { describe, expect, it } from 'vitest';
import {
  DEFAULT_FILTERS, QUEUE_STAGE_IDS, dateValue, errorMessage, filtersKey, isValidDate, mergeItems, parseFilters,
  queueApiPath, scopeMatches, screenSearch, unauthorizedMessage, validateFilters,
} from './queueModel';
import type { QueueFilters } from './queueModel';
import { makeItem, makeResponse } from './queueTestUtil';

const f = (patch: Partial<QueueFilters>): QueueFilters => ({ ...DEFAULT_FILTERS, ...patch });

describe('isValidDate / validateFilters', () => {
  it.each(['2026-13-01', '2026-02-30', '20261005', '2026-1-5', '', '1999-12-31', '2101-01-01', '2026-10-05T00:00', 'abc'])(
    'rejects %j', v => { expect(isValidDate(v)).toBe(false); });
  it.each(['2026-10-05', '2024-02-29', '2000-01-01', '2100-12-31'])('accepts %s', v => { expect(isValidDate(v)).toBe(true); });
  it('2025-02-29 is not a date', () => { expect(isValidDate('2025-02-29')).toBe(false); });

  it('reports invalid dates and reversed ranges, and accepts a one-day range', () => {
    expect(validateFilters(DEFAULT_FILTERS)).toEqual([]);
    expect(validateFilters(f({ nextFrom: '2026-10-10', nextTo: '2026-10-09' }))).toEqual(['次回架電日の開始日が終了日より後になっています']);
    expect(validateFilters(f({ lastFrom: '2026-10-10', lastTo: '2026-10-09' }))).toEqual(['最終架電日の開始日が終了日より後になっています']);
    expect(validateFilters(f({ nextFrom: '2026-02-30' }))).toEqual(['次回架電日の開始日が正しい日付ではありません']);
    expect(validateFilters(f({ lastTo: 'abc' }))).toEqual(['最終架電日の終了日が正しい日付ではありません']);
    expect(validateFilters(f({ nextFrom: '2026-10-01', nextTo: '2026-10-01' }))).toEqual([]);
    expect(validateFilters(f({ owner: 'abc' }))).toEqual(['担当者の指定が正しくありません']);
    expect(validateFilters(f({ q: 'あ'.repeat(101) }))).toEqual(['キーワードは 100 文字までです']);
  });
});

describe('URL <-> filters', () => {
  it('round-trips every control, and an untouched screen has an empty query', () => {
    expect(filtersKey(DEFAULT_FILTERS)).toBe('');
    const all = f({
      q: '架空', stages: ['1095387442', '1095387445'], owner: 'unassigned', due: 'today', sort: 'last_call_desc',
      nextFrom: '2026-10-01', nextTo: '2026-10-31', lastFrom: '2026-09-01', lastTo: '2026-09-30',
    });
    expect(parseFilters(screenSearch(all, 'live'))).toEqual(all);
    expect(parseFilters(screenSearch(f({ owner: '123456' }), 'live')).owner).toBe('123456');
  });

  it('the screen URL has no view (the call screen is the default of /app/crm); defaults give an empty string', () => {
    expect(screenSearch(DEFAULT_FILTERS, 'live')).toBe('');
    expect(screenSearch(DEFAULT_FILTERS, 'fixture')).toBe('?mode=fixture');
    expect(screenSearch(f({ due: 'today', stages: ['1095387445'] }), 'live')).toBe('?stage=1095387445&due=today');
  });

  it('drops invalid URL values to defaults instead of breaking the screen', () => {
    const got = parseFilters('?stage=999&stage=1095387445&sort=bogus&owner=<x>&due=never&next_from=2026-13-01&last_to=zzz&q=' + 'あ'.repeat(150));
    expect(got.stages).toEqual(['1095387445']);
    expect(got.sort).toBe('default');
    expect(got.owner).toBe('');
    expect(got.due).toBe('all');
    expect(got.nextFrom).toBe('');
    expect(got.lastTo).toBe('');
    expect(got.q).toHaveLength(100);
  });

  it('builds the API path with repeated stage, ranges and the cursor only when given', () => {
    const path = queueApiPath(f({ stages: ['1095387442', '1095387445'], nextFrom: '2026-10-01', sort: 'next_call_asc' }), null);
    expect(path).toBe('/api/crm/call-queue?stage=1095387442&stage=1095387445&sort=next_call_asc&next_from=2026-10-01&limit=25');
    expect(queueApiPath(DEFAULT_FILTERS, 'abc.def')).toBe('/api/crm/call-queue?limit=25&cursor=abc.def');
  });
});

describe('scopeMatches', () => {
  it('matches the scope built from the same filters', () => {
    for (const x of [DEFAULT_FILTERS, f({ stages: ['1095387445'], q: ' 架空 ', due: 'today', sort: 'next_call_asc', nextFrom: '2026-10-01' })]) {
      expect(scopeMatches(makeResponse(x, []).scope, x)).toBe(true);
    }
  });
  it('rejects a response for any other condition', () => {
    const base = f({ q: '架空', stages: ['1095387445'], nextFrom: '2026-10-01', lastTo: '2026-09-30', owner: 'unassigned' });
    const scope = makeResponse(base, []).scope;
    for (const other of [
      { q: '別' }, { stages: ['1095387443'] }, { stages: [] }, { due: 'today' as const }, { sort: 'last_call_asc' as const },
      { nextFrom: '2026-10-02' }, { nextTo: '2026-10-31' }, { lastFrom: '2026-09-01' }, { lastTo: '2026-09-29' }, { owner: 'me' },
    ]) {
      expect(scopeMatches(scope, { ...base, ...other }), JSON.stringify(other)).toBe(false);
    }
  });
  it('accepts the server default owner when the screen has no explicit owner', () => {
    const scope = { ...makeResponse(DEFAULT_FILTERS, []).scope, owner: 'me', role: 'own' };
    expect(scopeMatches(scope, DEFAULT_FILTERS)).toBe(true);
    expect(scopeMatches(scope, f({ owner: 'all' }))).toBe(false);
  });
  it('knows all queue stages', () => {
    expect(QUEUE_STAGE_IDS).toHaveLength(16);
  });
});

describe('mergeItems', () => {
  it('appends only unseen deal ids, keeping the first copy, and dedupes inside the incoming page', () => {
    const a = makeItem('1'); const b = makeItem('2'); const b2 = makeItem('2', { deal_name: '更新後' }); const c = makeItem('3');
    const merged = mergeItems([a, b], [b2, c, c]);
    expect(merged.map(i => i.deal_id)).toEqual(['1', '2', '3']);
    expect(merged[1]?.deal_name).toBe(b.deal_name);
    expect(mergeItems([], [])).toEqual([]);
  });
});

describe('messages and dates', () => {
  it('has a distinct message for each known error_kind and a generic one otherwise', () => {
    const kinds = ['hubspot_rate_limited', 'hubspot_timeout', 'crm_timeout', 'hubspot_auth', 'hubspot_upstream', 'hubspot_decode', 'not_configured', 'cursor_mismatch', 'invalid_param', 'owner_not_resolved'];
    const msgs = kinds.map(k => errorMessage(k, 500));
    expect(new Set(msgs).size).toBe(kinds.length);
    expect(errorMessage(null, null)).toContain('ネットワーク');
    expect(errorMessage('weird', 500)).toContain('500');
    expect(errorMessage('hubspot_rate_limited', 503)).toContain('上限');
  });
  it('explains 401 and forbidden kinds; the owner_not_resolved 409 asks the person to pick an owner', () => {
    expect(unauthorizedMessage(null, 401)).toContain('ログイン');
    expect(unauthorizedMessage('forbidden', 403)).toContain('権限');
    expect(errorMessage('owner_not_resolved', 409)).toContain('所有者を選んでください');
  });
  it('reads HubSpot date values in the date and epoch-ms forms', () => {
    expect(dateValue('2026-10-05')).toBe('2026-10-05');
    expect(dateValue('1790000000000')).toBe(new Date(1790000000000).toISOString().slice(0, 10));
    expect(dateValue('2026-10-05T00:00:00Z')).toBe('2026-10-05');
    expect(dateValue('')).toBeNull();
    expect(dateValue(null)).toBeNull();
    expect(dateValue('そのうち')).toBeNull();
  });
});
