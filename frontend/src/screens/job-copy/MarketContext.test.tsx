// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MarketContext } from './MarketContext';
import { ApplicationTrend } from './ApplicationTrend';
import type { JobCopyRecord } from './data';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
const metadata = { source: '合成市場', titles: ['合成職種'], prefectures: ['合成県'], ctk_basis: '応募数ではありません', series: null };
const job: JobCopyRecord = { id: 'synthetic', title: '合成求人', company: '合成取引先', media: '合成媒体', mediaJobId: 'synthetic', location: '合成県', versions: [] };
afterEach(() => { cleanup(); vi.resetAllMocks(); });

describe('market UI evidence and recovery', () => {
  it('distinguishes a verified zero total from absent daily aggregates and all missing dates', () => {
    api.mockResolvedValue({ ok: true, data: metadata });
    const view = render(<ApplicationTrend job={{ ...job, overallApplications: { total: 0, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {}, byDate: {} } }} />);
    expect(screen.getByText(/取得済み応募は0件です/)).toBeTruthy();
    view.rerender(<ApplicationTrend job={{ ...job, id: 'missing', overallApplications: { total: 3, missingDate: 3, fetchedAt: '2026-10-05T00:00:00Z', distributions: {}, byDate: {} } }} />);
    expect(screen.getByText(/応募日も作成日もすべて不明/)).toBeTruthy();
    view.rerender(<ApplicationTrend job={{ ...job, id: 'uncollected' }} />);
    expect(screen.getByText(/応募日別集計が未取得/)).toBeTruthy();
    expect(api).not.toHaveBeenCalled();
  });
  const designated: JobCopyRecord = { ...job, indeedTitle: '合成職種', indeedTitleState: 'set' };
  it('can retry the initial failed market request without navigating away', async () => {
    api.mockResolvedValueOnce({ ok: false }).mockResolvedValueOnce({ ok: true, data: metadata });
    render(<MarketContext job={designated} />);
    expect(await screen.findByRole('alert')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '市場データを再読み込み' }));
    expect(await screen.findByText('市場と比べる職種：合成職種（人が指定）')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(api).toHaveBeenCalledTimes(2);
  });
  it('fetches only the designated title, and nothing for a job without a designation', async () => {
    api.mockResolvedValue({ ok: true, data: metadata });
    const view = render(<MarketContext job={designated} />);
    await waitFor(() => { expect(api).toHaveBeenCalledTimes(1); });
    expect(api.mock.calls[0]?.[0]).toBe(`/api/job-copy/market?${new URLSearchParams({ title: '合成職種', prefecture: '合成県' }).toString()}`);
    // Another job nobody designated: the note replaces the data, and no request is made.
    view.rerender(<MarketContext job={{ ...job, id: 'other-job', title: '合成職種' }} />);
    expect(await screen.findByText('市場と比べる職種が未指定です')).toBeTruthy();
    expect(screen.queryByText(/人が指定/)).toBeNull();
    view.rerender(<MarketContext job={{ ...job, id: 'third-job', indeedTitleState: 'none' }} />);
    expect(await screen.findByText('市場と比べないと指定されています')).toBeTruthy();
    expect(api).toHaveBeenCalledTimes(1);
  });
});
