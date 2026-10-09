import { describe, expect, it } from 'vitest';
import {
  closeLink, DEAL_TAB, dealJobSearchUrl, embedUrlFor, extractUrls, initialCenterTabs, isEmptyGoogleSearch, isHubspotUrl, jobSearchUrl, makeLinkTab,
  MAX_LINK_TABS, openLink, safeHttpUrl, SEARCH_TAB,
} from './centerLinks';
import type { CenterTabsState } from './centerLinks';

// HubSpot の「URL_求人検索」(bpo_32) の実際の形 (番号は架空)。古いセッションの項目が多く付いている
const STORED = 'https://www.google.com/search?q=022-000-0001+%E6%B1%82%E4%BA%BA&sca_esv=658d&rlz=1C1&ei=abc&gs_lp=Egx&sclient=gws-wiz-serp';

const embed = (raw: string) => { const u = safeHttpUrl(raw); if (u === null) throw new Error(raw); return embedUrlFor(u); };

describe('safeHttpUrl', () => {
  it('accepts only http(s) and rejects script / data / credentials', () => {
    expect(safeHttpUrl(' https://example.com/a ')?.toString()).toBe('https://example.com/a');
    expect(safeHttpUrl('http://example.com/')?.toString()).toBe('http://example.com/');
    expect(safeHttpUrl('javascript:alert(1)')).toBeNull();
    expect(safeHttpUrl('JavaScript:alert(1)')).toBeNull();
    expect(safeHttpUrl('data:text/html,<script>alert(1)</script>')).toBeNull();
    expect(safeHttpUrl('ftp://example.com/')).toBeNull();
    expect(safeHttpUrl('//example.com/')).toBeNull();
    expect(safeHttpUrl('https://user:pass@example.com/')).toBeNull();
    expect(safeHttpUrl('')).toBeNull();
    expect(safeHttpUrl(null)).toBeNull();
  });
});

describe('isHubspotUrl', () => {
  const hs = (raw: string) => { const u = safeHttpUrl(raw); if (u === null) throw new Error(raw); return isHubspotUrl(u); };
  it('is true only for hubspot hosts (not for yahoo / duckduckgo / f-a-c.co.jp / lookalikes)', () => {
    expect(hs('https://app.hubspot.com/contacts/1/record/0-3/2/')).toBe(true);
    expect(hs('https://app-eu1.hubspot.com/x')).toBe(true);
    expect(hs('https://search.yahoo.co.jp/search?p=a')).toBe(false);
    expect(hs('https://duckduckgo.com/?q=a')).toBe(false);
    expect(hs('https://www.f-a-c.co.jp/')).toBe(false);
    expect(hs('https://evilhubspot.com/')).toBe(false);
  });
});

describe('embedUrlFor', () => {
  it('rebuilds a Google search with igu=1, keeping the query and dropping old session parameters', () => {
    const e = embed(STORED);
    expect(e).toBe('https://www.google.com/search?q=022-000-0001+%E6%B1%82%E4%BA%BA&igu=1');
    const u = new URL(e ?? '');
    expect(u.searchParams.get('q')).toBe('022-000-0001 求人');
    expect(u.searchParams.get('igu')).toBe('1');
    expect(u.searchParams.has('ei')).toBe(false);
  });
  it('handles google.co.jp and an unencoded Japanese query', () => {
    expect(embed('https://www.google.co.jp/search?q=022-346-0590+求人&hl=ja')).toBe(
      'https://www.google.co.jp/search?q=022-346-0590+%E6%B1%82%E4%BA%BA&hl=ja&igu=1');
  });
  it('keeps Bing and ordinary https pages as they are', () => {
    expect(embed('https://www.bing.com/search?q=abc+%E6%B1%82%E4%BA%BA')).toBe('https://www.bing.com/search?q=abc+%E6%B1%82%E4%BA%BA');
    expect(embed('https://www.example.com/recruit/')).toBe('https://www.example.com/recruit/');
  });
  it('does not frame known refusing sites, other Google pages or plain http', () => {
    expect(embed('https://app.hubspot.com/contacts/1/record/0-3/2/')).toBeNull();
    expect(embed('https://app-eu1.hubspot.com/x')).toBeNull();
    expect(embed('https://search.yahoo.co.jp/search?p=a')).toBeNull();
    expect(embed('https://www.google.com/maps/place/x')).toBeNull();
    expect(embed('http://www.example.com/')).toBeNull();
  });
});

describe('makeLinkTab', () => {
  it('keeps the original URL for "open in a new tab" and the rewritten one for the frame', () => {
    const t = makeLinkTab('link-1', STORED, '求人検索');
    expect(t?.url).toBe(STORED);
    expect(t?.embed).toContain('igu=1');
    expect(t?.host).toBe('www.google.com');
    expect(t?.label).toBe('求人検索');
    expect(makeLinkTab('link-1', 'https://www.example.com/x')?.label).toBe('www.example.com');
    expect(makeLinkTab('link-1', 'javascript:alert(1)')).toBeNull();
  });
});

describe('extractUrls / jobSearchUrl / isEmptyGoogleSearch', () => {
  it('splits several URLs and drops non-http values and duplicates', () => {
    expect(extractUrls('https://a.example.com/1\nhttps://b.example.com/2, javascript:x  https://a.example.com/1')).toEqual([
      'https://a.example.com/1', 'https://b.example.com/2']);
    expect(extractUrls(null)).toEqual([]);
  });
  it('uses the stored search URL, else builds "<number> 求人"', () => {
    expect(jobSearchUrl(STORED, '03-0000-0001')).toBe(STORED);
    expect(jobSearchUrl('javascript:alert(1)', '03-0000-0001')).toBe('https://www.google.com/search?q=03-0000-0001+%E6%B1%82%E4%BA%BA');
    expect(jobSearchUrl(null, null)).toBeNull();
  });
  it('formats the dial number with hyphens when deriving the search', () => {
    const deal = { job_search_url: null } as unknown as Parameters<typeof dealJobSearchUrl>[0]['deal'];
    expect(dealJobSearchUrl({ deal, dial: { number: '+81312345678', source: 'contact' } })).toBe(
      'https://www.google.com/search?q=03-1234-5678+%E6%B1%82%E4%BA%BA');
  });
  it('treats a search for just "求人" (no number) as empty', () => {
    expect(isEmptyGoogleSearch('https://www.google.com/search?q=+求人')).toBe(true);
    expect(isEmptyGoogleSearch(STORED)).toBe(false);
    expect(isEmptyGoogleSearch('https://www.example.com/')).toBe(false);
  });
});

describe('openLink / closeLink', () => {
  const open = (s: CenterTabsState, n: number) => openLink(s, `https://site${String(n)}.example.com/`, undefined, null);
  it('opens and activates a tab; the same URL re-activates the existing tab', () => {
    const a = open(initialCenterTabs, 1);
    expect(a.links.map(l => l.host)).toEqual(['site1.example.com']);
    expect(a.active).toBe(a.links[0]?.id);
    const b = openLink({ ...a, active: DEAL_TAB }, 'https://site1.example.com/', undefined, null);
    expect(b.links).toHaveLength(1);
    expect(b.active).toBe(a.links[0]?.id);
  });
  it('the job search URL activates the 求人検索 tab instead of a new tab', () => {
    const s = openLink(initialCenterTabs, STORED, '求人検索', STORED);
    expect(s.active).toBe(SEARCH_TAB);
    expect(s.links).toHaveLength(0);
  });
  it(`keeps at most ${String(MAX_LINK_TABS)} link tabs, closing the oldest`, () => {
    let s = initialCenterTabs;
    for (let i = 1; i <= MAX_LINK_TABS + 1; i += 1) s = open(s, i);
    expect(s.links).toHaveLength(MAX_LINK_TABS);
    expect(s.links.map(l => l.host)).toEqual(['site2.example.com', 'site3.example.com', 'site4.example.com', 'site5.example.com', 'site6.example.com']);
    expect(s.active).toBe(s.links[4]?.id);
  });
  it('ignores javascript: links', () => {
    expect(openLink(initialCenterTabs, 'javascript:alert(1)', undefined, null)).toBe(initialCenterTabs);
  });
  it('closing the active tab activates its neighbour, then the 案件 tab', () => {
    let s = open(open(initialCenterTabs, 1), 2);
    const [first, second] = s.links;
    s = closeLink(s, second?.id ?? '');
    expect(s.active).toBe(first?.id);
    s = closeLink(s, first?.id ?? '');
    expect(s.active).toBe(DEAL_TAB);
    expect(s.links).toHaveLength(0);
  });
});
