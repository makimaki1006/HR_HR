// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { JobCopyRecord } from './data';
import { HubSpotReadPanel } from './HubSpotReadPanel';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

// Synthetic street-level addresses (not real applicants). Only 都道府県 + 市区町村 may reach the screen.
const street = '東京都新宿区西新宿2-8-1 都庁ビル';
const leaks = /西新宿|都庁ビル|2-8-1|府内町|1-1/u;
const fetchedAt = '2026-10-06T00:00:00.000Z';
const summary = {
  total: 6, duplicate_ids: 0, missing_date: 0, by_date: { '2026-09-10': 6 },
  dimensions: {
    prefecture: { [street]: 4, '大分県大分市府内町1-1': 2 },
    municipality: { [`東京都 / ${street.slice(3)}`]: 4, '大分県 / 大分市府内町1-1': 2 },
  },
};
const customers = { customers: [{ id: 'c1', properties: { name: '合成取引先' } }], next_after: null, total_ms: 1 };
const jobsPage = { company_id: 'c1', contracts: [], jobs: [{ record: { id: 'L1', properties: { hs_name: '合成求人', shigotonaiyou: '合成の本文', id_hrhakkaa: '12345678', qinwude: '東京都' } }, deal_ids: [] }], total: 1, next_offset: null, total_ms: 1, fetched_at: fetchedAt };
const applicants = { metric: '応募', summary, total_ms: 1, fetched_at: fetchedAt, version_attribution: '合成の説明', attribute_basis: '合成の説明' };

function mockApi(page: Record<string, unknown>) {
  api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: path.includes('listing=') ? page : path.includes('company=') ? jobsPage : customers }));
}

async function openJob(onOpen: (job: JobCopyRecord) => void) {
  render(<HubSpotReadPanel onOpen={onOpen} />);
  fireEvent.click(screen.getByText('取引先を取得'));
  await screen.findByText('取引先を1件取得しました。');
  fireEvent.change(screen.getByLabelText('HubSpotの取引先'), { target: { value: 'c1' } });
  fireEvent.click(screen.getByText('関連する求人を取得'));
  fireEvent.click(await screen.findByText('合成求人（関連契約0件）'));
  await screen.findByText('応募を取得しました。');
}

const categories = (record: JobCopyRecord | undefined, dimension: 'prefecture' | 'municipality') =>
  record?.overallApplications?.distributions[dimension]?.categories.map(row => [row.category, row.count]);

describe('HubSpotReadPanel applicant addresses', () => {
  it('rounds addresses to 都道府県 + 市区町村 in the record it opens and in the table (no capture data)', async () => {
    mockApi(applicants);
    const onOpen = vi.fn<(job: JobCopyRecord) => void>();
    await openJob(onOpen);
    const opened = onOpen.mock.calls.at(-1)?.[0];
    expect(categories(opened, 'prefecture')).toEqual([['東京都', 4], ['その他', 2]]);
    expect(categories(opened, 'municipality')).toEqual([['東京都新宿区', 4], ['その他', 2]]);
    for (const [job] of onOpen.mock.calls) expect(JSON.stringify(job)).not.toMatch(leaks);
    const table = screen.getByText('求人全体の応募属性を開く').closest('details');
    expect(table?.textContent).toContain('東京都新宿区');
    expect(table?.textContent).not.toMatch(leaks);
    expect(document.body.textContent).not.toMatch(leaks);
  });

  it('rounds addresses in the record built from media capture data and its per-version shares', async () => {
    const capturedAt = '2026-10-05T00:00:00.000Z';
    const versionId = `capture-synthetic-${capturedAt}`;
    mockApi({ ...applicants,
      capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{ id: 'synthetic', hubspotListingId: 'L1', title: '合成求人', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678', location: '東京都', body: '合成の本文', images: [] }] },
      dated_comparison: { total: 6, unknown: 2, basis: '合成の日付対応', daily_representatives: {},
        by_version: { [versionId]: { count: 4, dimensions: { gender: null, age: null, prefecture: null, municipality: { denominator: 4, categories: [{ category: `東京都 / ${street.slice(3)}`, count: 4, percentage: 100 }] } } } } } });
    const onOpen = vi.fn<(job: JobCopyRecord) => void>();
    await openJob(onOpen);
    await waitFor(() => { expect(onOpen).toHaveBeenCalledTimes(2); });
    const opened = onOpen.mock.calls.at(-1)?.[0];
    expect(opened?.versions[0]?.id).toBe(versionId);
    expect(opened?.versions[0]?.distributions?.municipality?.categories.map(row => [row.category, row.count])).toEqual([['東京都新宿区', 4]]);
    expect(categories(opened, 'municipality')).toEqual([['東京都新宿区', 4], ['その他', 2]]);
    for (const [job] of onOpen.mock.calls) expect(JSON.stringify(job)).not.toMatch(leaks);
    expect(document.body.textContent).not.toMatch(leaks);
  });
});
