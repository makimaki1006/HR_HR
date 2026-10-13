// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import type { JobCopyRecord } from './data';
import { MarketOccupationSlot } from './MarketOccupationSlot';

const get = vi.hoisted(() => vi.fn());
const put = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async importOriginal => ({ ...await importOriginal<typeof import('../../api/client')>(), apiGet: get, apiPut: put }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const titles = ['配送ドライバー', '看護師', '介護職'];
const record: JobCopyRecord = {
  id: 'hubspot-history-30', hubspotId: '30', title: '架空配送スタッフ', company: '', media: 'HRハッカー', mediaJobId: 'J-30', location: '大分県大分市', versions: [],
  currentListingFields: { 職種: '配送スタッフ', Indeed表示職種名: '配送ドライバー' },
};
const open = async (job: JobCopyRecord, onChanged = vi.fn()) => {
  get.mockResolvedValue({ ok: true, data: { titles, prefectures: ['大分県'], series: null } });
  render(<MarketOccupationSlot record={job} prefecture="大分県" onChanged={onChanged} />);
  fireEvent.click(screen.getByRole('button', { name: /職種を(指定する|変える)/u }));
  const dialog = await screen.findByRole('dialog', { name: '市場と比べる職種を指定する' });
  await within(dialog).findByLabelText('看護師');
  return { dialog, onChanged };
};

describe('MarketOccupationSlot', () => {
  it('shows 未指定 with a button to designate, and 指定済み with a button to change', () => {
    const { unmount } = render(<MarketOccupationSlot record={record} prefecture="大分県" onChanged={() => undefined} />);
    expect(screen.getByText('未指定', { selector: '.mo-chip' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '職種を指定する' })).toBeTruthy();
    unmount();
    render(<MarketOccupationSlot record={{ ...record, indeedTitle: '看護師', indeedTitleState: 'set' }} prefecture="大分県" onChanged={() => undefined} />);
    expect(screen.getByText('指定済み')).toBeTruthy();
    expect(screen.getByText('看護師', { selector: '.mo-value' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '職種を変える' })).toBeTruthy();
  });

  it('opens with hints, no pre-selection and a disabled save button; nothing is guessed from the name', async () => {
    const { dialog } = await open(record);
    expect(within(dialog).getByText('選ぶときの手がかり（自動では決めません）')).toBeTruthy();
    expect(within(dialog).getByText('配送スタッフ')).toBeTruthy();
    expect(within(dialog).getAllByText('配送ドライバー').length).toBeGreaterThan(0);
    expect(within(dialog).getAllByRole<HTMLInputElement>('radio').every(radio => !radio.checked)).toBe(true);
    expect(within(dialog).getByRole<HTMLButtonElement>('button', { name: 'この職種を指定する' }).disabled).toBe(true);
    expect(within(dialog).getByText('この求人だけに指定します。HubSpot の求人の項目「比べる Indeed 職種」に入り、HubSpot の画面からも直せます。')).toBeTruthy();
    expect(put).not.toHaveBeenCalled();
  });

  it('searches the titles, saves the chosen one and reports it', async () => {
    put.mockResolvedValue({ ok: true, data: { indeed_title: '看護師', indeed_title_state: 'set' } });
    const { dialog, onChanged } = await open(record);
    fireEvent.change(within(dialog).getByLabelText('職種を探す'), { target: { value: '護' } });
    expect(within(dialog).queryByLabelText('配送ドライバー')).toBeNull();
    expect(within(dialog).getByLabelText('介護職')).toBeTruthy();
    fireEvent.click(within(dialog).getByLabelText('看護師'));
    const save = within(dialog).getByRole<HTMLButtonElement>('button', { name: 'この職種を指定する' });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await waitFor(() => { expect(onChanged).toHaveBeenCalledWith({ indeedTitle: '看護師', indeedTitleState: 'set' }); });
    expect(put).toHaveBeenCalledWith('/api/job-copy/listings/30/indeed-title', { title: '看護師', compare: true });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('saves 市場と比べない as compare:false with no title', async () => {
    put.mockResolvedValue({ ok: true, data: { indeed_title: null, indeed_title_state: 'none' } });
    const { dialog, onChanged } = await open(record);
    fireEvent.click(within(dialog).getByLabelText('市場と比べない'));
    fireEvent.click(within(dialog).getByRole('button', { name: 'この職種を指定する' }));
    await waitFor(() => { expect(onChanged).toHaveBeenCalledWith({ indeedTitle: null, indeedTitleState: 'none' }); });
    expect(put).toHaveBeenCalledWith('/api/job-copy/listings/30/indeed-title', { title: null, compare: false });
  });

  it('says plainly when the HubSpot property does not exist yet, and changes nothing', async () => {
    put.mockResolvedValue({ ok: false, error: new ApiHttpError(409, { code: 'indeed_title_property_missing' }) });
    const { dialog, onChanged } = await open(record);
    fireEvent.click(within(dialog).getByLabelText('看護師'));
    fireEvent.click(within(dialog).getByRole('button', { name: 'この職種を指定する' }));
    expect((await within(dialog).findByRole('alert')).textContent).toBe('保存先の項目がまだ用意されていません');
    expect(onChanged).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeTruthy();
  });
});
