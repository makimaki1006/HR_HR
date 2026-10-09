import { describe, expect, it } from 'vitest';
import { FIXTURE_PAGE_SIZE, fixtureQueuePage } from './queueFixture';
import { DEFAULT_FILTERS, scopeMatches } from './queueModel';
import type { QueueFilters } from './queueModel';
import type { CallQueueItem } from '../../generated/CallQueueItem';

const f = (patch: Partial<QueueFilters>): QueueFilters => ({ ...DEFAULT_FILTERS, ...patch });
const all = (filters: QueueFilters): CallQueueItem[] => {
  const out: CallQueueItem[] = [];
  let cursor: string | null = null;
  for (let i = 0; i < 20; i++) {
    const page = fixtureQueuePage(filters, cursor);
    out.push(...page.items);
    if (!page.next_cursor) return out;
    cursor = page.next_cursor;
  }
  throw new Error('cursor did not end');
};
const ids = (filters: QueueFilters) => all(filters).map(i => i.deal_id);

describe('fixtureQueuePage (fictional data)', () => {
  it('pages by cursor and its scope matches the requested filters', () => {
    const first = fixtureQueuePage(DEFAULT_FILTERS, null);
    expect(first.items).toHaveLength(FIXTURE_PAGE_SIZE);
    expect(first.next_cursor).not.toBeNull();
    // 全体の件数は全ページの行数と同じ (画面の「全 N 件中」に使う)
    expect(first.total).toBe(ids(DEFAULT_FILTERS).length);
    for (const x of [DEFAULT_FILTERS, f({ q: '架空', sort: 'last_call_desc', nextFrom: '2026-10-01', stages: ['1095387442'] })]) {
      expect(scopeMatches(fixtureQueuePage(x, null).scope, x)).toBe(true);
    }
  });

  it('excludes rows without a phone and non-unprocessed rows whose next call date has not come', () => {
    const got = ids(DEFAULT_FILTERS);
    expect(got).not.toContain('f-12'); // 番号なし
    expect(got).not.toContain('f-9'); // 不通以外のステージで次回日が未来
    expect(got).toContain('f-6'); // 未済は次回日が未来でも出る
  });

  it('due=today keeps only rows whose next call date has come', () => {
    expect(ids(f({ due: 'today' })).sort()).toEqual(['f-11', 'f-2', 'f-3', 'f-5', 'f-7']);
  });

  it('filters by stage, keyword, owner and date ranges', () => {
    expect(ids(f({ stages: ['1095387445'] }))).toEqual(['f-2']);
    expect(ids(f({ q: '運輸' }))).toEqual(['f-3']);
    expect(ids(f({ owner: 'unassigned' })).sort()).toEqual(['f-10', 'f-4']);
    expect(ids(f({ owner: '9002' })).sort()).toEqual(['f-3', 'f-6']);
    expect(ids(f({ nextFrom: '2026-10-03', nextTo: '2026-10-05' })).sort()).toEqual(['f-11', 'f-3', 'f-7']);
    expect(ids(f({ lastFrom: '2026-10-02', lastTo: '2026-10-03' })).sort()).toEqual(['f-3', 'f-7']);
    expect(ids(f({ nextFrom: '2027-01-01' }))).toEqual([]);
  });

  it('sorts: default puts due rows first by next date, then unprocessed by last call (never-called first)', () => {
    expect(ids(DEFAULT_FILTERS)).toEqual(['f-5', 'f-2', 'f-11', 'f-7', 'f-3', 'f-1', 'f-10', 'f-8', 'f-4', 'f-6']);
    expect(ids(f({ sort: 'next_call_desc' })).slice(0, 5)).toEqual(['f-3', 'f-7', 'f-11', 'f-2', 'f-5']);
    expect(ids(f({ sort: 'last_call_asc' })).slice(0, 3)).toEqual(['f-1', 'f-10', 'f-8']);
    const desc = all(f({ sort: 'last_call_desc' })).map(i => i.last_call_date);
    expect(desc.slice(0, 3)).toEqual(['2026-10-03', '2026-10-02', '2026-10-01']);
    expect(desc.at(-1)).toBeNull();
  });

  it('never contains a real-looking phone number or id (0300000000 range only, example.invalid links)', () => {
    for (const i of all(DEFAULT_FILTERS)) {
      expect(i.deep_links.deal.startsWith('https://example.invalid/')).toBe(true);
      const digits = (i.phone ?? '').replace(/\D/g, '');
      expect(/^(81)?0?(3|90|50)0000\d+$/.test(digits), digits).toBe(true);
    }
  });
});

describe('fixtureQueuePage: the second fictional pipeline', () => {
  it('shows the always-shown stage and due rows only; never another pipeline, an excluded stage or a future date', () => {
    const fx = f({ pipeline: 'fx-sample' });
    expect(ids(fx)).toEqual(['f-14', 'f-13', 'f-17']);
    expect(ids(f({ pipeline: 'fx-sample', stages: ['fx-follow'] }))).toEqual(['f-14']);
    expect(ids(DEFAULT_FILTERS)).not.toContain('f-13');
    // bpo_リクロジの行にもステージ名が付く (架空サンプルは固定の名前)
    expect(fixtureQueuePage(DEFAULT_FILTERS, null).items.map(i => i.stage_label)).toEqual(['担当者ブロック', '不在', '受付ブロック', '日程確保', '不通']);
    const page = fixtureQueuePage(fx, null);
    expect(page.scope.pipeline).toBe('fx-sample');
    expect(page.items.find(i => i.deal_id === 'f-13')?.stage_label).toBe('新規');
    expect(scopeMatches(page.scope, fx)).toBe(true);
    expect(scopeMatches(page.scope, DEFAULT_FILTERS)).toBe(false);
  });
});
