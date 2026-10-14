// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import { OccupationBulk } from './OccupationBulk';

const get = vi.hoisted(() => vi.fn());
const put = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async importOriginal => ({ ...await importOriginal<typeof import('../../api/client')>(), apiGet: get, apiPut: put }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const titles = ['配送ドライバー', '看護師'];
const row = (id: string, title: string, apps: number, state: 'set' | 'none' | 'unset', pub = '公開中') => ({
  id, media: 'hrh', media_job_id: `J-${id}`, account_id: null, title, prefecture: '大分県', municipality: null, category: null,
  indeed_title: state === 'set' ? '看護師' : null, indeed_title_state: state, publication_status: pub, last_csv_detected_at: null,
  application_count: apps, summary: { occupation: `${title}の職種`, location: null, salary_kind: null, salary_min: null, salary_max: null, hours: null, holidays: null, body_changed_at: null },
});
const page = (listings: unknown[], counts: { set: number; none: number; unset: number }, extra: object = {}) => ({
  status: 'ready', listings, total: listings.length, index_built_at: 'x', offset: 0, next_offset: null, titles, refreshing: false, refresh_failed: false, indeed_title_counts: counts, ...extra,
});
function serve(rows: unknown[]) {
  get.mockImplementation((path: string) => {
    if (path.startsWith('/api/job-copy/listings/companies')) return Promise.resolve({ ok: true, data: { companies: { 1: '架空顧客A', 2: '架空顧客B' } } });
    if (path.includes('publication=')) return Promise.resolve({ ok: true, data: page([], { set: 4, none: 1, unset: 5 }, { total: 0 }) });
    return Promise.resolve({ ok: true, data: page(rows, { set: 3, none: 1, unset: 6 }) });
  });
}
const listCalls = () => get.mock.calls.map(call => String(call[0])).filter(path => path.startsWith('/api/job-copy/listings?') && !path.includes('publication='));

describe('OccupationBulk', () => {
  it('shows tab counts, progress for 掲載中, and requests 未指定 sorted for bulk with no pre-selection', async () => {
    serve([row('1', '配送A', 20, 'unset'), row('2', '配送B', 9, 'unset')]);
    render(<OccupationBulk />);
    expect(await screen.findByRole('tab', { name: '未指定（6）' })).toBeTruthy();
    expect(screen.getByRole('tab', { name: '指定済み（4）' })).toBeTruthy();
    expect(screen.getByRole('tab', { name: 'すべて（10）' })).toBeTruthy();
    expect(screen.getByText('5 / 10 件')).toBeTruthy();
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('5');
    expect(listCalls()[0]).toBe('/api/job-copy/listings?sort=bulk&offset=0&indeed_state=unset');
    const input = await screen.findByLabelText('配送A と比べる Indeed の職種');
    expect((input as HTMLInputElement).value).toBe('');
    expect(screen.getByText('20 件')).toBeTruthy();
    expect(screen.getByText('配送Aの職種')).toBeTruthy();
    await waitFor(() => { expect(screen.getByText(/架空顧客A/u)).toBeTruthy(); });
  });

  it('keeps 指定する disabled until a listed title is chosen, then PUTs and turns the row into 指定済み', async () => {
    serve([row('1', '配送A', 20, 'unset'), row('2', '配送B', 9, 'unset')]);
    put.mockResolvedValue({ ok: true, data: { indeed_title: '配送ドライバー', indeed_title_state: 'set' } });
    render(<OccupationBulk />);
    const input = await screen.findByLabelText('配送A と比べる Indeed の職種');
    const tr = input.closest('tr') as HTMLElement;
    const button = within(tr).getByRole<HTMLButtonElement>('button', { name: '指定する' });
    expect(button.disabled).toBe(true);
    fireEvent.change(input, { target: { value: '存在しない職種' } });
    expect(button.disabled).toBe(true);
    expect(within(tr).getByText('一覧にある職種を選んでください。')).toBeTruthy();
    fireEvent.change(input, { target: { value: '配送ドライバー' } });
    expect(button.disabled).toBe(false);
    fireEvent.click(button);
    await waitFor(() => { expect(within(tr).getByText('指定済み', { selector: '.mo-chip' })).toBeTruthy(); });
    expect(put).toHaveBeenCalledWith('/api/job-copy/listings/1/indeed-title', { title: '配送ドライバー', compare: true });
    expect(put).toHaveBeenCalledTimes(1);
    expect(within(tr).getByText('配送ドライバー', { selector: 'strong' })).toBeTruthy();
    // 6 → 5 件、指定済み 4 → 5 件、掲載中の進み 5 → 6 / 10。もう片方の行は触らない。
    expect(screen.getByRole('tab', { name: '未指定（5）' })).toBeTruthy();
    expect(screen.getByRole('tab', { name: '指定済み（5）' })).toBeTruthy();
    expect(screen.getByText('6 / 10 件')).toBeTruthy();
    expect((screen.getByLabelText<HTMLInputElement>('配送B と比べる Indeed の職種')).value).toBe('');
  });

  it('「市場と比べない」 is saved as title null and compare false', async () => {
    serve([row('1', '配送A', 20, 'unset')]);
    put.mockResolvedValue({ ok: true, data: { indeed_title: null, indeed_title_state: 'none' } });
    render(<OccupationBulk />);
    const input = await screen.findByLabelText('配送A と比べる Indeed の職種');
    fireEvent.change(input, { target: { value: '市場と比べない' } });
    fireEvent.click(within(input.closest('tr') as HTMLElement).getByRole('button', { name: '指定する' }));
    await waitFor(() => { expect(put).toHaveBeenCalledWith('/api/job-copy/listings/1/indeed-title', { title: null, compare: false }); });
    expect(await screen.findByText('市場と比べない', { selector: 'strong' })).toBeTruthy();
  });

  it('shows the plain message when the save property is missing and leaves the row unchanged', async () => {
    serve([row('1', '配送A', 20, 'unset')]);
    put.mockResolvedValue({ ok: false, error: new ApiHttpError(409, { code: 'indeed_title_property_missing' }) });
    render(<OccupationBulk />);
    const input = await screen.findByLabelText('配送A と比べる Indeed の職種');
    fireEvent.change(input, { target: { value: '看護師' } });
    fireEvent.click(within(input.closest('tr') as HTMLElement).getByRole('button', { name: '指定する' }));
    expect((await screen.findByRole('alert')).textContent).toBe('保存先の項目がまだ用意されていません');
    expect(screen.getByRole('tab', { name: '未指定（6）' })).toBeTruthy();
  });

  it('switches tabs and searches by listing name with the matching query', async () => {
    serve([row('3', '看護C', 5, 'set')]);
    render(<OccupationBulk />);
    await screen.findByRole('tab', { name: '未指定（6）' });
    fireEvent.click(screen.getByRole('tab', { name: '指定済み（4）' }));
    await waitFor(() => { expect(listCalls().at(-1)).toBe('/api/job-copy/listings?sort=bulk&offset=0&indeed_state=assigned'); });
    expect(await screen.findByText('指定済み', { selector: '.mo-chip' })).toBeTruthy();
    expect(screen.getAllByRole('button', { name: '指定済み' }).every(b => (b as HTMLButtonElement).disabled)).toBe(true);
    fireEvent.click(screen.getByRole('tab', { name: 'すべて（10）' }));
    fireEvent.change(screen.getByPlaceholderText('求人名で探す'), { target: { value: '看護' } });
    await waitFor(() => { expect(listCalls().at(-1)).toBe('/api/job-copy/listings?sort=bulk&offset=0&q=%E7%9C%8B%E8%AD%B7'); });
  });
});
