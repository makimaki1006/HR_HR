import { describe, expect, it } from 'vitest';
import {
  closeLink, dealFixedLinks, dealJobSearchUrl, embedUrlFor, extractUrls, initialCenterTabs, isEmptyGoogleSearch, isHubspotUrl, makeLinkTab, rowJobSearchUrl, searchUrlFor, waitingFixedLinks,
  HOME_TAB, MAX_LINK_TABS, openLink, POSTING_TAB, safeHttpUrl, SEARCH_TAB, SITE_TAB,
} from './centerLinks';
import type { CenterTabsState } from './centerLinks';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';

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

describe('embedUrlFor with the frame extension', () => {
  it('embeds hubspot hosts only when allowed; other no-embed hosts stay blocked', () => {
    const e = (raw: string, ok: boolean) => { const u = safeHttpUrl(raw); if (u === null) throw new Error(raw); return embedUrlFor(u, ok); };
    expect(e('https://app.hubspot.com/contacts/1/', true)).toBe('https://app.hubspot.com/contacts/1/');
    expect(e('https://app.hubspot.com/contacts/1/', false)).toBeNull();
    expect(e('https://app.hubspot.jp/x', true)).toBe('https://app.hubspot.jp/x');
    expect(e('https://zoom.us/x', true)).toBeNull();
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

describe('extractUrls / searchUrlFor / isEmptyGoogleSearch', () => {
  it('splits several URLs and drops non-http values and duplicates', () => {
    expect(extractUrls('https://a.example.com/1\nhttps://b.example.com/2, javascript:x  https://a.example.com/1')).toEqual([
      'https://a.example.com/1', 'https://b.example.com/2']);
    expect(extractUrls(null)).toEqual([]);
  });
  it('電話番号: 番号 + 求人。番号が無いときだけ保存済みの URL_求人検索を使う', () => {
    const Q = 'https://www.google.com/search?q=03-0000-0001+%E6%B1%82%E4%BA%BA';
    expect(searchUrlFor('phone', { phone: '03-0000-0001', company: 'テスト商事', stored: STORED })).toBe(Q);
    expect(searchUrlFor('phone', { phone: null, company: 'テスト商事', stored: STORED })).toBe(STORED);
    expect(searchUrlFor('phone', { phone: null, company: null, stored: 'javascript:alert(1)' })).toBeNull();
  });
  it('会社名: 会社名 + 求人。切り替えると URL が変わる', () => {
    const input = { phone: '03-0000-0001', company: 'テスト商事 株式会社' };
    const phone = searchUrlFor('phone', input);
    const company = searchUrlFor('company', input);
    expect(new URL(company ?? '').searchParams.get('q')).toBe('テスト商事 株式会社 求人');
    expect(company).not.toBe(phone);
    expect(searchUrlFor('company', { phone: '03-0000-0001', company: '  ' })).toBeNull();
  });
  it('行 (詳細の前) と詳細が同じ URL になる。会社名が無ければ案件名', () => {
    const row = { phone: '+81312345678', company: { id: '1', name: 'テスト商事', phone: null }, deal_name: '案件A' };
    expect(rowJobSearchUrl(row, 'phone')).toBe('https://www.google.com/search?q=03-1234-5678+%E6%B1%82%E4%BA%BA');
    expect(new URL(rowJobSearchUrl(row, 'company') ?? '').searchParams.get('q')).toBe('テスト商事 求人');
    expect(new URL(rowJobSearchUrl({ ...row, company: null }, 'company') ?? '').searchParams.get('q')).toBe('案件A 求人');
    const deal = { job_search_url: null, name: '案件A' } as unknown as Parameters<typeof dealJobSearchUrl>[0]['deal'];
    const data = { deal, dial: { number: '+81312345678', source: 'contact' as const }, companies: [{ id: '1', name: 'テスト商事', is_primary: true }] } as unknown as Parameters<typeof dealJobSearchUrl>[0];
    expect(dealJobSearchUrl(data, 'phone')).toBe(rowJobSearchUrl(row, 'phone'));
    expect(dealJobSearchUrl(data, 'company')).toBe(rowJobSearchUrl(row, 'company'));
  });
  it('長野の 10 桁の番号はハイフン区切りで検索し、行を選んだ直後と詳細が届いた後で同じ URL', () => {
    const row = { phone: '0262765000', company: null, deal_name: '案件A' };
    const want = 'https://www.google.com/search?q=026-276-5000+%E6%B1%82%E4%BA%BA';
    expect(rowJobSearchUrl(row, 'phone')).toBe(want);
    const deal = { job_search_url: null, name: '案件A' } as unknown as Parameters<typeof dealJobSearchUrl>[0]['deal'];
    for (const number of ['0262765000', '+81262765000', '026-276-5000']) {
      const data = { deal, dial: { number, source: 'contact' as const }, companies: [] } as unknown as Parameters<typeof dealJobSearchUrl>[0];
      expect(dealJobSearchUrl(data, 'phone')).toBe(want);
    }
    expect(waitingFixedLinks(row, 'phone', '読み込み中')[0]?.url).toBe(want);
  });
  it('詳細を読む前でも、行があれば求人検索の URL がある', () => {
    const row = { phone: '03-0000-0001', company: null, deal_name: null };
    const l = waitingFixedLinks(row, 'phone', '待ち');
    expect(l[0]?.url).toContain('03-0000-0001');
    expect(l[1]?.url).toBeNull();
    expect(waitingFixedLinks(null, 'phone', '待ち')[0]?.url).toBeNull();
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
  const open = (s: CenterTabsState, n: number) => openLink(s, `https://site${String(n)}.example.com/`, undefined, []);
  it('opens and activates a tab; the same URL re-activates the existing tab', () => {
    const a = open(initialCenterTabs, 1);
    expect(a.links.map(l => l.host)).toEqual(['site1.example.com']);
    expect(a.active).toBe(a.links[0]?.id);
    const b = openLink({ ...a, active: SEARCH_TAB }, 'https://site1.example.com/', undefined, []);
    expect(b.links).toHaveLength(1);
    expect(b.active).toBe(a.links[0]?.id);
  });
  it('the job search URL activates the 求人検索 tab instead of a new tab', () => {
    const s = openLink(initialCenterTabs, STORED, '求人検索', [{ id: SEARCH_TAB, url: STORED }]);
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
    expect(openLink(initialCenterTabs, 'javascript:alert(1)', undefined, [])).toBe(initialCenterTabs);
  });
  it('closing the active tab activates its neighbour, then the 求人検索 tab', () => {
    let s = open(open(initialCenterTabs, 1), 2);
    const [first, second] = s.links;
    s = closeLink(s, second?.id ?? '');
    expect(s.active).toBe(first?.id);
    s = closeLink(s, first?.id ?? '');
    expect(s.active).toBe(SEARCH_TAB);
    expect(s.links).toHaveLength(0);
  });
});

describe('dealFixedLinks', () => {
  const base = (deal: Record<string, unknown>, website: string | null = null): WorkspaceResponse => ({
    deal: { job_search_url: null, homepage_url: null, media_job_urls: null, job_posting_url: null, ...deal },
    dial: { number: '03-0000-0001', source: 'contact' },
    companies: [{ is_primary: true, website }],
  }) as unknown as WorkspaceResponse;
  const byId = (l: ReturnType<typeof dealFixedLinks>, id: string) => l.find(f => f.id === id);

  it('always has 求人検索 / ホームページ / 求人票 in this order, even with no URLs', () => {
    const l = dealFixedLinks(base({}));
    expect(l.map(f => f.label)).toEqual(['求人検索', 'ホームページ', '求人票']);
    expect(l.map(f => f.id)).toEqual([SEARCH_TAB, HOME_TAB, POSTING_TAB]);
    expect(byId(l, HOME_TAB)?.url).toBeNull();
    expect(byId(l, HOME_TAB)?.missing).toBe('この案件にはホームページが登録されていません。');
    expect(byId(l, POSTING_TAB)?.missing).toBe('この案件には求人票が登録されていません。');
    expect(byId(l, SEARCH_TAB)?.url).toContain('https://www.google.com/search');
  });
  it('the homepage falls back to the primary company website; the 会社のサイト tab only appears when different', () => {
    const l = dealFixedLinks(base({}, 'https://www.example.com/'));
    expect(byId(l, HOME_TAB)?.url).toBe('https://www.example.com/');
    expect(byId(l, SITE_TAB)).toBeUndefined();
    const l2 = dealFixedLinks(base({ homepage_url: 'https://home.example.com/' }, 'https://www.example.com/'));
    expect(byId(l2, HOME_TAB)?.url).toBe('https://home.example.com/');
    expect(l2.map(f => f.label)).toEqual(['求人検索', 'ホームページ', '求人票', '会社のサイト']);
  });
  it('求人票 skips an empty Google search; 求人媒体 tabs are numbered only when several', () => {
    expect(byId(dealFixedLinks(base({ job_posting_url: 'https://www.google.com/search?q=+%E6%B1%82%E4%BA%BA' })), POSTING_TAB)?.url).toBeNull();
    expect(byId(dealFixedLinks(base({ job_posting_url: 'https://jobs.example.com/1' })), POSTING_TAB)?.url).toBe('https://jobs.example.com/1');
    expect(dealFixedLinks(base({ media_job_urls: 'https://a.example.com/' })).map(f => f.label).slice(3)).toEqual(['求人媒体']);
    expect(dealFixedLinks(base({ media_job_urls: 'https://a.example.com/ https://b.example.com/' })).map(f => f.label).slice(3)).toEqual(['求人媒体 1', '求人媒体 2']);
  });
  it('a link equal to a fixed tab URL focuses that tab instead of opening a new one', () => {
    const l = dealFixedLinks(base({ homepage_url: 'https://home.example.com/' }));
    const s = openLink(initialCenterTabs, 'https://home.example.com/', undefined, l);
    expect(s.active).toBe(HOME_TAB);
    expect(s.links).toHaveLength(0);
  });
});
