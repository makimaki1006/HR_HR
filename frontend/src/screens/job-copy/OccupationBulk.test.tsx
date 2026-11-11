// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import { OccupationBulk } from './OccupationBulk';
import { ConsultantMenu } from './ConsultantMenu';

const get = vi.hoisted(() => vi.fn());
const put = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async importOriginal => ({ ...await importOriginal<typeof import('../../api/client')>(), apiGet: get, apiPut: put }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const titles = ['配送ドライバー', '看護師'];
const row = (id: string, title: string, apps: number, state: 'set' | 'none' | 'unset', pub = '公開中', media = 'hrh') => ({
  id, media, media_job_id: `J-${id}`, account_id: null, title, prefecture: '大分県', municipality: null, category: null,
  indeed_title: state === 'set' ? '看護師' : null, indeed_title_state: state, publication_status: pub, last_csv_detected_at: null,
  application_count: 999, recent_performance: { period_start: '2026-10-13', period_end: '2026-11-09', applications: apps, cost_yen: null },
  summary: { occupation: `${title}の表示名`, job_kind: `${title}の職種`, location: null, salary_kind: null, salary_min: null, salary_max: null, hours: null, holidays: null, body_changed_at: null },
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
    expect(listCalls()[0]).toBe('/api/job-copy/listings?owner=me&sort=bulk&offset=0&indeed_state=unset');
    const input = await screen.findByLabelText('配送A と比べる Indeed の職種');
    expect((input as HTMLInputElement).value).toBe('');
    // 直近4週の応募（全期間の 999 件ではない）と、手がかり 2 つ。
    expect(screen.getByText('20 件').getAttribute('title')).toBe('10/13〜11/09');
    expect(screen.queryByText('999 件')).toBeNull();
    const tr = input.closest('tr') as HTMLElement;
    expect(within(tr).getByText('求人票の職種').nextElementSibling?.textContent).toBe('配送Aの職種');
    expect(within(tr).getByText('Indeed 表示職種名').nextElementSibling?.textContent).toBe('配送Aの表示名');
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
    await waitFor(() => { expect(listCalls().at(-1)).toBe('/api/job-copy/listings?owner=me&sort=bulk&offset=0&indeed_state=assigned'); });
    expect(await screen.findByText('指定済み', { selector: '.mo-chip' })).toBeTruthy();
    expect(screen.getAllByRole('button', { name: '指定済み' }).every(b => (b as HTMLButtonElement).disabled)).toBe(true);
    fireEvent.click(screen.getByRole('tab', { name: 'すべて（10）' }));
    fireEvent.change(screen.getByPlaceholderText('求人名・顧客名で探す'), { target: { value: '看護' } });
    await waitFor(() => { expect(listCalls().at(-1)).toBe('/api/job-copy/listings?owner=me&sort=bulk&offset=0&q=%E7%9C%8B%E8%AD%B7'); });
  });

  it('AirWork は求人票の職種だけ、実績の無い求人は「—」', async () => {
    const aw = { ...row('5', '倉庫E', 0, 'unset', '公開中', 'airwork'), recent_performance: null, summary: { occupation: '倉庫管理', job_kind: '倉庫管理', location: null, salary_kind: null, salary_min: null, salary_max: null, hours: null, holidays: null, body_changed_at: null } };
    serve([aw]);
    render(<OccupationBulk />);
    const tr = (await screen.findByLabelText('倉庫E と比べる Indeed の職種')).closest('tr') as HTMLElement;
    expect(within(tr).getByText('求人票の職種').nextElementSibling?.textContent).toBe('倉庫管理');
    expect(within(tr).queryByText('Indeed 表示職種名')).toBeNull();
    expect(within(tr).getAllByText('—')).toHaveLength(1);
  });

  it('deal=<取引ID> で絞り、案件名と「すべてに戻す」を出す', async () => {
    window.history.replaceState(null, '', '/app/job-copy?view=occupations&deal=123&owner=me');
    get.mockImplementation((path: string) => {
      if (path.startsWith('/api/job-copy/listings/companies')) return Promise.resolve({ ok: true, data: { companies: {} } });
      if (path.includes('publication=')) return Promise.resolve({ ok: true, data: page([], { set: 1, none: 0, unset: 2 }, { total: 0 }) });
      return Promise.resolve({ ok: true, data: page([row('1', '配送A', 3, 'unset')], { set: 1, none: 0, unset: 2 }, { selected_deal: { id: '123', name: '架空の案件' } }) });
    });
    render(<OccupationBulk />);
    expect((await screen.findByText('架空の案件')).closest('[role="status"]')?.textContent).toContain('架空の案件 の求人だけを表示しています');
    // 担当では絞らず、取引で絞る。進み具合も同じ取引。
    expect(listCalls()[0]).toBe('/api/job-copy/listings?deal=123&sort=bulk&offset=0&indeed_state=unset');
    expect(get.mock.calls.map(c => String(c[0])).find(p => p.includes('publication='))).toBe('/api/job-copy/listings?deal=123&sort=bulk&publication=%E6%8E%B2%E8%BC%89%E4%B8%AD&offset=100000');
    fireEvent.click(screen.getByRole('button', { name: 'すべてに戻す' }));
    expect(new URLSearchParams(window.location.search).get('deal')).toBeNull();
    await waitFor(() => { expect(listCalls().at(-1)).toBe('/api/job-copy/listings?owner=me&sort=bulk&offset=0&indeed_state=unset'); });
    expect(screen.queryByText('すべてに戻す')).toBeNull();
    window.history.replaceState(null, '', '/');
  });

  it('見つからない案件では案内を出す', async () => {
    window.history.replaceState(null, '', '/app/job-copy?view=occupations&deal=999');
    get.mockImplementation(() => Promise.resolve({ ok: false, error: new ApiHttpError(400, { code: 'listing_filter_invalid' }) }));
    render(<OccupationBulk />);
    expect((await screen.findByRole('alert')).textContent).toContain('この案件の求人が見つかりませんでした。');
    window.history.replaceState(null, '', '/');
  });

  it('案件管理のメニューに「職種の指定」があり、押すと一括指定の画面へ移る', () => {
    window.history.replaceState(null, '', '/app/job-copy?view=deals&deal=55&owner=11');
    get.mockImplementation(() => new Promise(() => undefined));
    render(<ConsultantMenu><p>画面</p></ConsultantMenu>);
    fireEvent.click(screen.getByRole('button', { name: '職種の指定' }));
    const query = new URLSearchParams(window.location.search);
    expect([query.get('view'), query.get('deal'), query.get('owner')]).toEqual(['occupations', null, '11']);
    expect(screen.getByRole('button', { name: '職種の指定' }).getAttribute('aria-current')).toBe('page');
    window.history.replaceState(null, '', '/');
  });
});
