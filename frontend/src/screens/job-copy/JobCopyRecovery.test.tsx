// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { JobCopyScreen } from './JobCopyScreen';

const response = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json' } });
const snapshot = (total = 2) => {
  const capturedAt = '2026-10-06T00:00:00Z';
  return { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{ id: 'synthetic-recovery', hubspotListingId: '30', title: `合成の復帰確認${String(total)}`, company: '合成会社', media: 'HRハッカー', mediaJobId: '12345678', location: '大分県', body: '再取得した合成の全文です。', images: [] }] }, results: [{ listing_id: '30', summary: { total, missing_date: 0, by_date: { '2026-10-05': total }, dimensions: { gender: { 男性: total } } }, dated_comparison: null }] };
};
// The timeline tab (first tab) also asks for market data; those calls are answered here and not counted.
const market = (path: string) => path.startsWith('/api/job-copy/market') ? Promise.resolve(response({ source: '合成', titles: [], prefectures: [], ctk_basis: '', series: null })) : null;
const loading = () => screen.queryByText(/求人一覧・本文・応募集計を読み込んでいます|読み込みに時間がかかっています/);
const retry = () => screen.getByRole('button', { name: '求人データを再取得' });

beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy'); });
afterEach(() => { cleanup(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('snapshot recovery without fictional replacement', () => {
  it('retries a 503 into the actual response count and clears the failure and retry control', async () => {
    let count = 0;
    vi.stubGlobal('fetch', vi.fn((path: string) => market(path) ?? Promise.resolve(++count === 1 ? response({ code: 'moc_drive_snapshot_unavailable' }, 503) : response(snapshot()))));
    const { container } = render(<JobCopyScreen />);
    await screen.findByRole('alert');
    expect(container.querySelectorAll('.jc-job')).toHaveLength(0);
    expect(loading()).toBeNull();
    await act(async () => { fireEvent.click(retry()); await Promise.resolve(); });
    expect(screen.getByRole('region', { name: '実データの取得範囲' }).textContent).toContain('2応募レコード');
    expect(container.querySelectorAll('.jc-job')).toHaveLength(1);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByRole('button', { name: '求人データを再取得' })).toBeNull();
    expect(count).toBe(2);
  });

  it('aborts a slow request on retry and ignores its late successful response', async () => {
    vi.useFakeTimers();
    let finishOld: (value: Response) => void = () => { throw new Error('Request not started'); };
    let oldSignal: AbortSignal | null | undefined;
    let count = 0;
    vi.stubGlobal('fetch', vi.fn((_path: string, options: RequestInit) => {
      if (++count > 1) return Promise.resolve(response(snapshot()));
      oldSignal = options.signal;
      // Deliberately ignore abort in this adversarial transport to prove the
      // component also rejects a late response rather than trusting cancellation.
      return new Promise<Response>(resolve => { finishOld = resolve; });
    }));
    await act(async () => { render(<JobCopyScreen />); await Promise.resolve(); });
    expect(screen.getByRole('heading', { name: '求人レコード' }).parentElement?.textContent).toContain('取得中');
    expect(screen.getByRole('heading', { name: '求人レコード' }).parentElement?.textContent).not.toMatch(/0\s*\/\s*0/);
    expect(screen.queryByRole('button', { name: '求人データを再取得' })).toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
    expect(retry()).toBeDefined();
    expect(loading()).not.toBeNull();
    await act(async () => { fireEvent.click(retry()); await Promise.resolve(); });
    expect(oldSignal?.aborted).toBe(true);
    expect(screen.getByRole('region', { name: '実データの取得範囲' }).textContent).toContain('2応募レコード');
    await act(async () => { finishOld(response(snapshot(9))); await Promise.resolve(); });
    expect(screen.getByRole('region', { name: '実データの取得範囲' }).textContent).toContain('2応募レコード');
    expect(screen.queryByRole('heading', { name: '合成の復帰確認9' })).toBeNull();
    expect(loading()).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('finishes loading after the real 30-second API deadline and can retry', async () => {
    vi.useFakeTimers();
    let count = 0;
    vi.stubGlobal('fetch', vi.fn((_path: string, options: RequestInit) => {
      if (++count > 1) return Promise.resolve(response(snapshot()));
      return new Promise<Response>((_resolve, reject) => {
        options.signal?.addEventListener('abort', () => { reject(new DOMException('Aborted', 'AbortError')); }, { once: true });
      });
    }));
    const { container } = render(<JobCopyScreen />);
    await act(async () => { await vi.advanceTimersByTimeAsync(29_999); });
    expect(loading()).not.toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(loading()).toBeNull();
    expect(screen.getByRole('alert').textContent).toContain('時間がかかっています');
    expect(container.querySelectorAll('.jc-job')).toHaveLength(0);
    await act(async () => { fireEvent.click(retry()); await Promise.resolve(); });
    expect(screen.getByRole('region', { name: '実データの取得範囲' }).textContent).toContain('2応募レコード');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('retains the login link beside the recovery action for an expired session', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(response({ code: 'login_required' }, 401))));
    render(<JobCopyScreen />);
    await screen.findByRole('alert');
    expect(screen.getByRole('link', { name: '再ログインする' }).getAttribute('href')).toBe('/login');
    expect(retry()).toBeDefined();
    expect(loading()).toBeNull();
  });

  it('cancels the pending snapshot when a validated media capture is explicitly chosen', async () => {
    let finishOld: (value: Response) => void = () => { throw new Error('Request not started'); };
    const transport: { signal?: AbortSignal | null | undefined } = {};
    vi.stubGlobal('fetch', vi.fn((path: string, options: RequestInit) => {
      const answered = market(path);
      if (answered) return answered;
      transport.signal = options.signal;
      return new Promise<Response>(resolve => { finishOld = resolve; });
    }));
    render(<JobCopyScreen />);
    const capture = snapshot().capture_bundle;
    const job = capture.jobs[0];
    if (!job) throw new Error('Missing synthetic capture');
    job.title = '合成の手動取込';
    const file = new File([JSON.stringify(capture)], 'synthetic-capture.json', { type: 'application/json' });
    await act(async () => {
      fireEvent.change(screen.getByLabelText('媒体取得データを読み込む'), { target: { files: [file] } });
      await Promise.resolve();
    });
    await screen.findByRole('button', { name: '取得データを表示' });
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '取得データを表示' })); await Promise.resolve(); });
    expect(transport.signal?.aborted).toBe(true);
    expect(loading()).toBeNull();
    expect(screen.getByRole('heading', { name: '合成の手動取込' })).toBeDefined();
    await act(async () => { finishOld(response(snapshot(9))); await Promise.resolve(); });
    expect(screen.getByRole('heading', { name: '合成の手動取込' })).toBeDefined();
    expect(screen.queryByRole('region', { name: '実データの取得範囲' })).toBeNull();
    expect(screen.queryByRole('heading', { name: '合成の復帰確認9' })).toBeNull();
  });
});
