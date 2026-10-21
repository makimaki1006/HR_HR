// @vitest-environment happy-dom
import { useState } from 'react';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import type { JobCopyRecord } from './data';
import { ConsultantReview, type ConsultantDraft } from './ConsultantReview';
import { MarketFactors } from './MarketFactors';
import { ReceivedTextSaver } from './ReceivedTextSaver';
import { persistableListingId } from './savedNotes';

const get = vi.hoisted(() => vi.fn());
const put = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async importOriginal => ({ ...await importOriginal<typeof import('../../api/client')>(), apiGet: get, apiPut: put }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const live: JobCopyRecord = { id: 'hubspot-history-30', hubspotId: '30', dataSource: 'hubspot', title: '架空配送スタッフ', company: '架空商事', media: 'HRハッカー', mediaJobId: 'J-30', location: '大分県大分市', versions: [] };
const demo: JobCopyRecord = { ...live, id: 'demo-job-1' };
delete demo.dataSource;
delete demo.hubspotId;
const barStatus = () => within(screen.getByRole('group', { name: '記録の保存' })).getByRole('status');
const wire = (value: string, by = 'a@example.test', at = '2026-10-21T01:30:00+00:00') => ({ value, updated_by: by, updated_at: at });
const notes = (n: Record<string, unknown>) => ({ ok: true, data: { notes: n } });

function Review({ job }: { job: JobCopyRecord }) {
  const [draft, setDraft] = useState<ConsultantDraft>({ stage: 'plan', target: 'both', fields: {}, selection: ['', ''] });
  return <ConsultantReview job={job} draft={draft} onDraft={setDraft} />;
}

describe('persistableListingId', () => {
  it('saves only for HubSpot listings with a numeric id', () => {
    expect(persistableListingId(live)).toBe('30');
    expect(persistableListingId(demo)).toBeNull();
    expect(persistableListingId({ ...live, hubspotId: 'abc' })).toBeNull();
    expect(persistableListingId({ dataSource: 'hubspot' })).toBeNull();
  });
});

describe('検証記録の保存', () => {
  it('loads the saved record into empty fields, shows who saved it and when, and saves one PUT', async () => {
    get.mockResolvedValue(notes({ review: wire(JSON.stringify({ stage: 'check', target: 'body', fields: { hypothesis: '給与を上げると応募が増える' } })) }));
    put.mockImplementation((_path: string, body: { value: string }) => Promise.resolve({ ok: true, data: { field: 'review', ...wire(body.value, 'b@example.test', '2026-10-21T02:00:00+00:00') } }));
    render(<Review job={live} />);
    expect(get).toHaveBeenCalledWith('/api/job-copy/listings/30/notes');
    await waitFor(() => { expect(screen.getByLabelText<HTMLTextAreaElement>('仮説').value).toBe('給与を上げると応募が増える'); });
    expect(screen.getByDisplayValue('Check · 結果を確認')).toBeTruthy();
    expect(barStatus().textContent).toContain('最後に保存：a@example.test 2026/10/21 10:30');
    const save = screen.getByRole<HTMLButtonElement>('button', { name: '保存' });
    expect(save.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('次に試すこと'), { target: { value: '来週の月曜に再確認' } });
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    await waitFor(() => { expect(barStatus().textContent).toContain('最後に保存：b@example.test 2026/10/21 11:00'); });
    expect(put).toHaveBeenCalledTimes(1);
    expect(put.mock.calls[0]?.[0]).toBe('/api/job-copy/listings/30/notes/review');
    expect(JSON.parse((put.mock.calls[0]?.[1] as { value: string }).value)).toEqual({ stage: 'check', target: 'body', fields: { hypothesis: '給与を上げると応募が増える', action: '来週の月曜に再確認' } });
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '保存' }).disabled).toBe(true);
  });

  it('keeps the input and says 保存できません when the save fails', async () => {
    get.mockResolvedValue(notes({}));
    put.mockResolvedValue({ ok: false, error: new ApiHttpError(503, { code: 'notes_unavailable' }) });
    render(<Review job={live} />);
    await waitFor(() => { expect(barStatus().textContent).toContain('まだ保存されていません'); });
    fireEvent.change(screen.getByLabelText('結果と根拠'), { target: { value: '応募が12件から9件' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect((await screen.findByRole('alert')).textContent).toBe('いまは保存できません。入力はこの画面に残っています。');
    expect(screen.getByLabelText<HTMLTextAreaElement>('結果と根拠').value).toBe('応募が12件から9件');
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '保存' }).disabled).toBe(false);
  });

  it('does not overwrite what the user already typed with the saved record', async () => {
    let resolve: (v: unknown) => void = () => undefined;
    get.mockReturnValue(new Promise(r => { resolve = r; }));
    render(<Review job={live} />);
    fireEvent.change(screen.getByLabelText('仮説'), { target: { value: '入力中の仮説' } });
    resolve(notes({ review: wire(JSON.stringify({ stage: 'do', target: 'image', fields: { hypothesis: '保存済みの仮説' } })) }));
    await waitFor(() => { expect(barStatus().textContent).toContain('最後に保存'); });
    expect(screen.getByLabelText<HTMLTextAreaElement>('仮説').value).toBe('入力中の仮説');
    expect(barStatus().textContent).toContain('保存していない変更があります');
  });

  it('says 保存できません when the record cannot be read, and the fields stay usable', async () => {
    get.mockResolvedValue({ ok: false, error: new ApiHttpError(503, { code: 'notes_unavailable' }) });
    render(<Review job={live} />);
    expect((await screen.findByRole('alert')).textContent).toBe('いまは保存できません。入力はこの画面に残っています。');
    fireEvent.change(screen.getByLabelText('仮説'), { target: { value: 'まだ書ける' } });
    expect(screen.getByLabelText<HTMLTextAreaElement>('仮説').value).toBe('まだ書ける');
  });

  it('does not call the server for the demo and explains that nothing is saved', () => {
    render(<Review job={demo} />);
    expect(get).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: '保存' })).toBeNull();
    expect(screen.getByRole('note').textContent).toContain('保存されません');
    expect(document.body.textContent).toContain('入力はこの画面の中だけに残り');
  });
});

describe('要因メモの保存', () => {
  it('loads, edits and saves the memo as plain text', async () => {
    get.mockResolvedValue(notes({ factor: wire('企業名検索が増えている') }));
    put.mockResolvedValue({ ok: true, data: { field: 'factor', ...wire('企業名検索が増えている。10/20確認') } });
    render(<MarketFactors job={live} />);
    await waitFor(() => { expect(screen.getByLabelText<HTMLTextAreaElement>('会社のブランド力の根拠').value).toBe('企業名検索が増えている'); });
    fireEvent.change(screen.getByLabelText('会社のブランド力の根拠'), { target: { value: '企業名検索が増えている。10/20確認' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => { expect(put).toHaveBeenCalledWith('/api/job-copy/listings/30/notes/factor', { value: '企業名検索が増えている。10/20確認' }); });
    expect(document.body.textContent).not.toContain('再読み込みで消えます');
  });
});

describe('受け取った文面の保存', () => {
  function Harness({ job }: { job: JobCopyRecord }) {
    const [incoming, setIncoming] = useState('');
    const [source, setSource] = useState('外部文面の貼り付け');
    return <><textarea aria-label="文面" value={incoming} onChange={e => { setIncoming(e.target.value); }} /><span data-testid="source">{source}</span>
      <ReceivedTextSaver job={job} source={source} incoming={incoming} onLoad={(s, t) => { setSource(s); setIncoming(t); }} /></>;
  }
  it('restores the saved text and source, and saves both together', async () => {
    get.mockResolvedValue(notes({ incoming: wire(JSON.stringify({ source: '架空の制作会社', text: '仕事内容：配送\n給与：月給30万円' })) }));
    put.mockResolvedValue({ ok: true, data: { field: 'incoming', ...wire('x') } });
    render(<Harness job={live} />);
    await waitFor(() => { expect(screen.getByLabelText<HTMLTextAreaElement>('文面').value).toBe('仕事内容：配送\n給与：月給30万円'); });
    expect(screen.getByTestId('source').textContent).toBe('架空の制作会社');
    fireEvent.change(screen.getByLabelText('文面'), { target: { value: '仕事内容：配送\n給与：月給32万円' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => { expect(put).toHaveBeenCalledTimes(1); });
    expect(JSON.parse((put.mock.calls[0]?.[1] as { value: string }).value)).toEqual({ source: '架空の制作会社', text: '仕事内容：配送\n給与：月給32万円' });
  });
});
