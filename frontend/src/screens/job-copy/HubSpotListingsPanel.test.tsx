// @vitest-environment happy-dom
import { beforeEach as pinSidebarBeforeEach, afterEach as clearSidebarAfterEach } from 'vitest';
pinSidebarBeforeEach(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
clearSidebarAfterEach(() => { localStorage.removeItem('hrhr-job-copy-sidebar-pinned'); });
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { HubSpotListingsPanel } from './HubSpotListingsPanel';
import { JobCopyScreen } from './JobCopyScreen';
import type { JobCopyRecord } from './data';
import { listingRecord } from './hubspotListings';
import type { HubSpotListing, HubSpotVersions } from './hubspotListings';
import { CAUSAL_PATTERN, JARGON_PATTERN } from './format';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async importOriginal => ({ ...await importOriginal<typeof import('../../api/client')>(), apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const row: HubSpotListing = { id: '10', media: 'airwork', media_job_id: 'AW-10', account_id: 'synthetic-account', title: '合成配送求人', prefecture: '大分県', municipality: '大分市', category: 'ドライバー', publication_status: null, last_csv_detected_at: null, application_count: null };
const history: HubSpotVersions = {
  listing: row, history_counts: { shigotonaiyou: 20 }, history_may_be_incomplete: true,
  versions: [{ written_at: '2026-10-01T00:00:00Z', body: '合成の旧本文', image_urls: null }, { written_at: '2026-10-02T00:00:00Z', body: '合成の新本文', image_urls: ['https://example.invalid/1.png'] }],
};
const page = { listings: [row, ...Array.from({ length: 49 }, (_, index) => ({ ...row, id: String(index + 11), title: `合成求人${String(index + 11)}`, media_job_id: `AW-${String(index + 11)}` }))], titles: ['ドライバー', '看護師'], status: 'ready', total: 51, index_built_at: '2026-10-10T00:00:00Z', offset: 0, next_offset: 50, refreshing: false, refresh_failed: false };
function mock() {
  api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: path.endsWith('/applications') ? { listing_id: row.id, application_count: null } : path.includes('/market') ? { titles: [], prefectures: [], series: null } : path.includes('/versions') ? history : path.includes('offset=50') ? { ...page, listings: [{ ...row, id: '60', title: '合成の次ページ求人', media_job_id: 'AW-60' }], offset: 50, next_offset: null } : page }));
}
describe('HubSpot listing history', () => {
  it('preserves body, ordered URLs and unknown dates and counts in the existing record shape', () => {
    const record = listingRecord(history);
    expect(record.accountId).toBe('synthetic-account');
    expect(record.mediaJobId).toBe('AW-10');
    expect(record.location).toBe('大分県大分市');
    expect(record.company).toBe('');
    expect(listingRecord({ ...history, company_name: '合成運輸株式会社' }).company).toBe('合成運輸株式会社');
    expect(record.versions[1]?.label).toBe('2026/10/02 09:00 JST にHubSpotに記録された本文');
    expect(record.versions.map(v => [v.observedAt, v.body])).toEqual([['2026-10-01T00:00:00Z', '合成の旧本文'], ['2026-10-02T00:00:00Z', '合成の新本文']]);
    expect(record.versions[0]?.images).toBeUndefined();
    expect(record.versions[1]?.images?.map(image => image.url)).toEqual(['https://example.invalid/1.png']);
    expect(record.versions[0]?.publishedFrom).toBeUndefined();
    expect(record.versions[0]?.applications).toBeNull();
    expect(record.overallApplications).toBeUndefined();
    expect(listingRecord({ ...history, versions: [{ written_at: '2026-10-01T00:00:00Z', body: '合成本文', image_urls: [] }] }).versions[0]?.images).toEqual([]);
  });
  it.each(['hrh', 'airwork'] as const)('preserves known listing counts for %s without replacing zero or mixing jobs', media => {
    const data = { ...history, listing: { ...row, media } };
    expect(listingRecord(data, { ...row, application_count: 12 }).listingApplicationCount).toBe(12);
    expect(listingRecord(data, { ...row, application_count: 0 }).listingApplicationCount).toBe(0);
    expect(listingRecord(data, row).listingApplicationCount).toBeNull();
    expect(listingRecord(data).listingApplicationCount).toBeNull();
    expect(listingRecord(data, { ...row, id: '11', application_count: 12 }).listingApplicationCount).toBeNull();
    expect(listingRecord({ ...data, listing: { ...data.listing, application_count: 0 } }, { ...row, application_count: 12 }).listingApplicationCount).toBe(0);
    expect(listingRecord({ ...data, listing: { ...data.listing, application_count: 3 } }, { ...row, application_count: 12 }).listingApplicationCount).toBe(3);
  });
  it.each([12, 0])('carries %i applications from the selected list row when detail has no count', async count => {
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: path.includes('/versions') ? history : { ...page, listings: [{ ...row, application_count: count }] } }));
    const onOpen = vi.fn<(job: JobCopyRecord) => void>(); render(<HubSpotListingsPanel onOpen={onOpen} />);
    fireEvent.click(await screen.findByRole('button', { name: '合成配送求人の本文を見る' }));
    await waitFor(() => { expect(onOpen).toHaveBeenCalledOnce(); });
    expect(onOpen.mock.calls[0]?.[0].listingApplicationCount).toBe(count);
  });
  it('loads on entry, filters, pages and passes the actual body to detail', async () => {
    mock(); const onOpen = vi.fn<(job: JobCopyRecord) => void>(); render(<HubSpotListingsPanel onOpen={onOpen} />);
    await screen.findByRole('button', { name: '合成配送求人の本文を見る' });
    expect(screen.getByText('51件')).toBeTruthy();
    fireEvent.click(screen.getByText('絞り込み・並び順'));
    expect(screen.getByRole('button', { name: '合成配送求人の本文を見る' }).textContent).toContain('応募 未取得');
    fireEvent.change(screen.getByLabelText('都道府県'), { target: { value: '大分県' } });
    fireEvent.change(screen.getByLabelText('職種の分類'), { target: { value: 'ドライバー' } });
    fireEvent.change(screen.getByLabelText('媒体', { selector: 'select' }), { target: { value: 'airwork' } });
    await waitFor(() => { const query = new URL(String(api.mock.calls.at(-1)?.[0]), 'https://example.test').searchParams;
      expect(query.get('prefecture')).toBe('大分県'); expect(query.get('title')).toBe('ドライバー'); expect(query.get('media')).toBe('airwork'); });
    fireEvent.click(screen.getByRole('button', { name: '合成配送求人の本文を見る' }));
    await waitFor(() => { expect(onOpen).toHaveBeenCalledOnce(); });
    expect(onOpen.mock.calls[0]?.[0].versions[1]?.body).toBe('合成の新本文');
    expect(onOpen.mock.calls[0]?.[0].historyMayBeIncomplete).toBe(true);
    expect(screen.queryByText(/文面の版は/)).toBeNull();
    expect(screen.getByRole('region').textContent).not.toMatch(CAUSAL_PATTERN);
    expect(screen.getByRole('region').textContent).not.toMatch(JARGON_PATTERN);
    fireEvent.click(screen.getByRole('button', { name: '次の求人' }));
    await screen.findByRole('button', { name: '合成の次ページ求人の本文を見る' });
    expect(new URL(String(api.mock.calls.filter(call => String(call[0]).includes('/listings?')).at(-1)?.[0]), 'https://example.test').searchParams.get('offset')).toBe('50');
  });
  it('shows preparation without zero results and preserves the dated list after refresh failure', async () => {
    api.mockResolvedValue({ ok: true, data: { ...page, status: 'preparing', listings: [], total: null, index_built_at: null, next_offset: null, refreshing: true } });
    render(<HubSpotListingsPanel onOpen={vi.fn()} />);
    await screen.findByText('求人の一覧を準備しています');
    expect(screen.queryByRole('button', { name: '合成配送求人の本文を見る' })).toBeNull();
    expect(screen.getByRole('region').textContent).not.toContain('0件');
    expect(screen.getByText(/準備が終わると自動で/)).toBeTruthy();
    api.mockResolvedValue({ ok: true, data: { ...page, refresh_failed: true } });
    fireEvent.click(screen.getByRole('button', { name: '求人を読み込む' }));
    await screen.findByRole('button', { name: '合成配送求人の本文を見る' });
    expect(screen.getByText(/2026.*時点/)).toBeTruthy();
    expect(screen.getByRole('status').textContent).toContain('一覧を更新できませんでした');
  });
  it('hides the warning for nineteen history entries and shows an empty matching result', async () => {
    mock(); const onOpen = vi.fn<(job: JobCopyRecord) => void>(); render(<HubSpotListingsPanel onOpen={onOpen} />);
    await screen.findByRole('button', { name: '合成配送求人の本文を見る' });
    api.mockResolvedValueOnce({ ok: true, data: { ...history, history_counts: { shigotonaiyou: 19 }, history_may_be_incomplete: false } });
    fireEvent.click(screen.getByRole('button', { name: '合成配送求人の本文を見る' }));
    await waitFor(() => { expect(onOpen).toHaveBeenCalledOnce(); });
    expect(onOpen.mock.calls[0]?.[0].historyMayBeIncomplete).toBe(false);
    api.mockResolvedValue({ ok: true, data: { ...page, listings: [], total: 0, next_offset: null } });
    fireEvent.click(screen.getByRole('button', { name: '求人を読み込む' }));
    await screen.findByText('条件に合う求人はありません。');
  });
  it('opens versions in the existing body comparison and restores the fixed list', async () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1'); mock();
    const { container } = render(<JobCopyScreen />);
    const originalCount = container.querySelectorAll('.jc-fixed-list .jc-job').length;
    fireEvent.click(screen.getByRole('button', { name: 'HubSpot の求人' }));
    fireEvent.click(await screen.findByRole('button', { name: '合成配送求人の本文を見る' }));
    await waitFor(() => { expect(container.querySelectorAll('.jc-fixed-list .jc-job')).toHaveLength(1); });
    const main = within(screen.getByRole('article'));
    fireEvent.click(main.getByRole('tab', { name: '求人内容' }));
    expect(main.getByText('合成の新本文', { exact: true })).toBeTruthy();
    fireEvent.click(main.getByRole('tab', { name: '比較・報告' }));
    fireEvent.click(main.getByRole('tab', { name: '変更差分' }));
    expect(container.querySelector('[id$="-panel-diff"]')?.textContent).toContain('合成の旧本文');
    expect(container.querySelector('[id$="-panel-diff"]')?.textContent).toContain('合成の新本文');
    fireEvent.click(screen.getByRole('button', { name: '固定一覧を表示' }));
    await waitFor(() => { expect(container.querySelectorAll('.jc-fixed-list .jc-job')).toHaveLength(originalCount); });
  });
  it('restores the fixed file records and their acquisition date after opening HubSpot history', async () => {
    window.history.replaceState(null, '', '/app/job-copy?data=actual');
    const capturedAt = '2026-10-06T00:00:00Z';
    const fixed = { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt,
      jobs: [{ id: 'synthetic-fixed', hubspotListingId: '30', title: '合成の固定求人', company: '合成会社', media: 'HRハッカー', mediaJobId: 'HR-30', location: '大分県', body: '合成の固定本文', images: [] }] },
      results: [{ listing_id: '30', summary: { total: 2, missing_date: 0, by_date: { '2026-10-05': 2 }, dimensions: {} }, dated_comparison: null }] };
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: path.endsWith('/moc') ? fixed : path.endsWith('/listing-status') ? { listings: {} } : path.endsWith('/applications') ? { listing_id: row.id, application_count: null } : path.includes('/market') ? { titles: [], prefectures: [], series: null } : path.includes('/versions') ? history : page }));
    const { container } = render(<JobCopyScreen />);
    await waitFor(() => { expect(container.querySelector('.jc-fixed-list .jc-job')?.textContent).toContain('合成の固定求人'); });
    fireEvent.click(screen.getByRole('button', { name: 'HubSpot の求人' }));
    fireEvent.click(await screen.findByRole('button', { name: '合成配送求人の本文を見る' }));
    await waitFor(() => { expect(container.querySelector('.jc-fixed-list .jc-job')?.textContent).toContain('合成配送求人'); });
    expect(container.querySelectorAll('.jc-fixed-list .jc-job')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: '固定一覧を表示' }));
    expect(container.querySelector('.jc-fixed-list .jc-job')?.textContent).toContain('合成の固定求人');
    expect(screen.getByRole('region', { name: '取り込んだ内容の内訳' }).textContent).toContain('2応募');
    expect(screen.getByText('実データ（取得済み）')).toBeTruthy();
  });

});

it('ignores a delayed count for the previous job after switching jobs', async () => {
  let resolveCount!: (value: unknown) => void;
  const delayed = new Promise(resolve => { resolveCount = resolve; });
  const onOpen = vi.fn<(job: JobCopyRecord) => void>();
  const onApplicationsLoaded = vi.fn();
  api.mockImplementation((path: string) => {
    if (path.endsWith('/applications')) return delayed;
    if (path.includes('/versions')) return Promise.resolve({ ok: true, data: path.includes('/11/') ? { ...history, listing: { ...row, id: '11', application_count: 7 } } : history });
    return Promise.resolve({ ok: true, data: { ...page, listings: page.listings.filter(item => item.id !== '10') } });
  });
  render(<HubSpotListingsPanel initialListingId="10" onOpen={onOpen} onApplicationsLoaded={onApplicationsLoaded} />);
  await waitFor(() => { expect(api.mock.calls.some(call => String(call[0]).endsWith('/10/applications'))).toBe(true); });
  const countCall = api.mock.calls.find(call => String(call[0]).endsWith('/10/applications'));
  fireEvent.click(screen.getByRole('button', { name: '合成求人11の本文を見る' }));
  await waitFor(() => { expect(onOpen.mock.calls.at(-1)?.[0].hubspotId).toBe('11'); });
  const countOptions = countCall?.[1] as { signal: AbortSignal } | undefined;
  expect(countOptions?.signal.aborted).toBe(true);
  await act(async () => { resolveCount({ ok: true, data: { listing_id: '10', application_count: 0 } }); await delayed; });
  expect(onApplicationsLoaded).not.toHaveBeenCalled();
  expect(onOpen.mock.calls.at(-1)?.[0].listingApplicationCount).toBe(7);
});

describe('opening a listing quickly', () => {
  const versionCalls = (id: string) => api.mock.calls.filter(call => String(call[0]) === `/api/job-copy/listings/${id}/versions`).length;
  const reply = (path: string, company?: Promise<unknown>) => {
    if (path.endsWith('/company')) return company ?? Promise.resolve({ ok: true, data: { listing_id: row.id, company_name: '合成運輸株式会社' } });
    const id = /listings\/(\d+)\/versions/.exec(path)?.[1];
    if (id) return Promise.resolve({ ok: true, data: { ...history, listing: id === row.id ? row : { ...row, id, title: `合成求人${id}` } } });
    return Promise.resolve({ ok: true, data: path.endsWith('/applications') ? { listing_id: row.id, application_count: null } : path.includes('/market') ? { titles: [], prefectures: [], series: null } : path.endsWith('/moc') ? { error: 'x' } : page });
  };
  it('draws the body before the customer name and fills the name in when it arrives', async () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1');
    let resolveCompany!: (value: unknown) => void;
    const company = new Promise(resolve => { resolveCompany = resolve; });
    api.mockImplementation((path: string) => reply(path, company));
    const { container } = render(<JobCopyScreen />);
    fireEvent.click(screen.getByRole('button', { name: 'HubSpot の求人' }));
    fireEvent.click(await screen.findByRole('button', { name: '合成配送求人の本文を見る' }));
    await waitFor(() => { expect(container.querySelector('.jc-band h1')?.textContent).toBe('合成配送求人'); });
    // The body is on screen and the name has been asked for, but has not arrived.
    expect(api.mock.calls.some(call => String(call[0]) === '/api/job-copy/listings/10/company')).toBe(true);
    expect(container.querySelector('.jc-detail-heading p')?.textContent).not.toContain('合成運輸株式会社');
    await act(async () => { resolveCompany({ ok: true, data: { listing_id: '10', company_name: '合成運輸株式会社' } }); await company; });
    await waitFor(() => { expect(container.querySelector('.jc-detail-heading p')?.textContent).toContain('合成運輸株式会社'); });
  });
  it('shows a reopened listing from the kept result without asking the server again, and warms only its neighbours', async () => {
    api.mockImplementation((path: string) => reply(path));
    const onOpen = vi.fn<(job: JobCopyRecord) => void>();
    render(<HubSpotListingsPanel onOpen={onOpen} />);
    fireEvent.click(await screen.findByRole('button', { name: '合成求人12の本文を見る' }));
    await waitFor(() => { expect(onOpen).toHaveBeenCalledTimes(1); });
    // Only the next and the previous listing (13 and 11) are read in the background.
    await waitFor(() => { expect(versionCalls('11')).toBe(1); expect(versionCalls('13')).toBe(1); });
    expect(['10', '14', '15', '59'].map(versionCalls)).toEqual([0, 0, 0, 0]);
    expect(versionCalls('12')).toBe(1);
    // The next listing was warmed: opening it adds no request; neither does going back.
    fireEvent.click(screen.getByRole('button', { name: '合成求人13の本文を見る' }));
    await waitFor(() => { expect(onOpen).toHaveBeenCalledTimes(2); });
    expect(onOpen.mock.calls[1]?.[0].hubspotId).toBe('13');
    fireEvent.click(screen.getByRole('button', { name: '合成求人12の本文を見る' }));
    await waitFor(() => { expect(onOpen).toHaveBeenCalledTimes(3); });
    expect(versionCalls('13')).toBe(1);
    expect(versionCalls('12')).toBe(1);
  });
});
