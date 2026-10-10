// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { HubSpotListingsPanel } from './HubSpotListingsPanel';
import { JobCopyScreen } from './JobCopyScreen';
import type { JobCopyRecord } from './data';
import { listingRecord } from './hubspotListings';
import type { HubSpotListing, HubSpotVersions } from './hubspotListings';
import { matchMarketTitle } from './marketMatch';
import { CAUSAL_PATTERN, JARGON_PATTERN } from './format';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async importOriginal => ({ ...await importOriginal<typeof import('../../api/client')>(), apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const row: HubSpotListing = { id: '10', media: 'airwork', media_job_id: 'AW-10', account_id: 'synthetic-account', title: '合成配送求人', prefecture: '大分県', municipality: '大分市', category: 'ドライバー', publication_status: null, last_csv_detected_at: null, application_count: null };
const history: HubSpotVersions = {
  listing: row, history_counts: { shigotonaiyou: 3 }, history_may_be_incomplete: true,
  versions: [{ written_at: '2026-10-01T00:00:00Z', body: '合成の旧本文', image_urls: null }, { written_at: '2026-10-02T00:00:00Z', body: '合成の新本文', image_urls: ['https://example.invalid/1.png'] }],
};
const page = { listings: [row], titles: ['ドライバー', '看護師'], next_after: '11', scanned: 50 };
function mock() {
  api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: path.includes('/market') ? { titles: [], prefectures: [], series: null } : path.includes('/versions') ? history : path.includes('after=11') ? { ...page, listings: [], next_after: null } : page }));
}
describe('HubSpot listing history', () => {
  it('preserves body, ordered URLs and unknown dates and counts in the existing record shape', () => {
    expect(matchMarketTitle('看護師・介護職', ['看護師', '介護職'])?.title).toBe('介護職');
    const record = listingRecord(history);
    expect(record.accountId).toBe('synthetic-account');
    expect(record.mediaJobId).toBe('AW-10');
    expect(record.location).toBe('大分県大分市');
    expect(record.versions.map(v => [v.observedAt, v.body])).toEqual([['2026-10-01T00:00:00Z', '合成の旧本文'], ['2026-10-02T00:00:00Z', '合成の新本文']]);
    expect(record.versions[0]?.images).toBeUndefined();
    expect(record.versions[1]?.images?.map(image => image.url)).toEqual(['https://example.invalid/1.png']);
    expect(record.versions[0]?.publishedFrom).toBeUndefined();
    expect(record.versions[0]?.applications).toBeNull();
    expect(record.overallApplications).toBeUndefined();
    expect(listingRecord({ ...history, versions: [{ written_at: '2026-10-01T00:00:00Z', body: '合成本文', image_urls: [] }] }).versions[0]?.images).toEqual([]);
  });
  it('filters, pages, shows unknown counts and opens history with its actual counts and warning', async () => {
    mock(); const onOpen = vi.fn<(job: JobCopyRecord) => void>(); render(<HubSpotListingsPanel onOpen={onOpen} />);
    fireEvent.click(screen.getByRole('button', { name: '求人を取得' }));
    await screen.findByText('synthetic-account / AW-10');
    expect(screen.getByRole('table').textContent).not.toContain('0件');
    fireEvent.change(screen.getByLabelText('都道府県'), { target: { value: '大分県' } });
    fireEvent.change(screen.getByLabelText('職種の分類'), { target: { value: 'ドライバー' } });
    fireEvent.change(screen.getByLabelText('媒体', { selector: 'select' }), { target: { value: 'airwork' } });
    fireEvent.click(screen.getByRole('button', { name: '求人を取得' }));
    await screen.findByText('synthetic-account / AW-10');
    const query = new URL(String(api.mock.calls.at(-1)?.[0]), 'https://example.test').searchParams;
    expect(query.get('prefecture')).toBe('大分県'); expect(query.get('title')).toBe('ドライバー'); expect(query.get('media')).toBe('airwork');
    fireEvent.click(screen.getByRole('button', { name: '合成配送求人の版を見る' }));
    await waitFor(() => { expect(onOpen).toHaveBeenCalledOnce(); });
    expect(onOpen.mock.calls[0]?.[0].versions[1]?.body).toBe('合成の新本文');
    const warning = screen.getByRole('status').textContent;
    expect(warning).toContain('文面の版は2件'); expect(warning).toContain('本文の履歴：3件'); expect(warning).toContain('20件までの可能性');
    expect(screen.getByRole('region').textContent).not.toMatch(CAUSAL_PATTERN);
    expect(screen.getByRole('region').textContent).not.toMatch(JARGON_PATTERN);
    fireEvent.click(screen.getByRole('button', { name: '次の求人' }));
    await screen.findByText('今回確認した範囲では、条件に一致する求人はありません。');
    expect(new URL(String(api.mock.calls.at(-1)?.[0]), 'https://example.test').searchParams.get('after')).toBe('11');
  });
  it('opens versions in the existing body comparison and restores the fixed list', async () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1'); mock();
    const { container } = render(<JobCopyScreen />);
    const originalCount = container.querySelectorAll('.jc-job').length;
    fireEvent.click(screen.getByRole('button', { name: '求人を取得' }));
    fireEvent.click(await screen.findByRole('button', { name: '合成配送求人の版を見る' }));
    await waitFor(() => { expect(container.querySelectorAll('.jc-job')).toHaveLength(1); });
    const main = within(screen.getByRole('article'));
    fireEvent.click(main.getByRole('tab', { name: '求人内容' }));
    expect(main.getByText('合成の新本文', { exact: true })).toBeTruthy();
    fireEvent.click(main.getByRole('tab', { name: '比較・報告' }));
    fireEvent.click(main.getByRole('tab', { name: '変更差分' }));
    expect(container.querySelector('[id$="-panel-diff"]')?.textContent).toContain('合成の旧本文');
    expect(container.querySelector('[id$="-panel-diff"]')?.textContent).toContain('合成の新本文');
    fireEvent.click(screen.getByRole('button', { name: '固定一覧を表示' }));
    await waitFor(() => { expect(container.querySelectorAll('.jc-job')).toHaveLength(originalCount); });
  });
  it('restores the fixed file records and their acquisition date after opening HubSpot history', async () => {
    window.history.replaceState(null, '', '/app/job-copy');
    const capturedAt = '2026-10-06T00:00:00Z';
    const fixed = { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt,
      jobs: [{ id: 'synthetic-fixed', hubspotListingId: '30', title: '合成の固定求人', company: '合成会社', media: 'HRハッカー', mediaJobId: 'HR-30', location: '大分県', body: '合成の固定本文', images: [] }] },
      results: [{ listing_id: '30', summary: { total: 2, missing_date: 0, by_date: { '2026-10-05': 2 }, dimensions: {} }, dated_comparison: null }] };
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: path.endsWith('/moc') ? fixed : path.endsWith('/listing-status') ? { listings: {} } : path.includes('/market') ? { titles: [], prefectures: [], series: null } : path.includes('/versions') ? history : page }));
    const { container } = render(<JobCopyScreen />);
    await waitFor(() => { expect(container.querySelector('.jc-job')?.textContent).toContain('合成の固定求人'); });
    fireEvent.click(screen.getByRole('button', { name: '求人を取得' }));
    fireEvent.click(await screen.findByRole('button', { name: '合成配送求人の版を見る' }));
    await waitFor(() => { expect(container.querySelector('.jc-job')?.textContent).toContain('合成配送求人'); });
    expect(container.querySelectorAll('.jc-job')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: '固定一覧を表示' }));
    expect(container.querySelector('.jc-job')?.textContent).toContain('合成の固定求人');
    expect(screen.getByRole('region', { name: '実データの取得範囲' }).textContent).toContain('2応募');
    expect(screen.getByText('実データ（取得済み）')).toBeTruthy();
  });

});
