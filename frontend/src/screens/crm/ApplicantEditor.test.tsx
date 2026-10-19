// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApplicantEditor } from './ApplicantEditor';
import { applicantDetailFixture } from './applicantFixture';
import { APPLICANT_DRAFT_KEY } from './applicantResultModel';
afterEach(() => { cleanup(); sessionStorage.clear(); vi.unstubAllGlobals(); });
function mockFetch(status: number, body: unknown) {
  const fetcher = vi.fn().mockImplementation((url: string) => Promise.resolve(new Response(JSON.stringify(url === '/api/nav' ? { user_email: 'test@example.invalid' } : body), { status: url === '/api/nav' ? 200 : status, headers: {'Content-Type':'application/json'} })));
  vi.stubGlobal('fetch',fetcher);return fetcher;
}
async function edit() {
  fireEvent.change(await screen.findByLabelText('対応種別'), { target: {value:'done'} });
  fireEvent.change(screen.getByLabelText('連絡・確認目的詳細'), {target:{value:'条件を確認'}});
}
describe('応募の記録', () => {
  it.each([['saved',200,'saved'],['queued',202,'queued']] as const)('%s の色で表示し次へ進む', async (kind,http,state) => {
    const fetcher=mockFetch(http,kind === 'saved' ? {status:'saved',values:{taioushubetsu:'done'},objects_values:{},fetched_at:'2026-10-19T00:00:00Z'} : {status:'queued',operation_id:'app-pending-0001'});
    const next=vi.fn(); render(<ApplicantEditor detail={applicantDetailFixture('90001')} fixture={false} onNext={next}/>);
    await edit();fireEvent.change(screen.getByLabelText('ステージ'), {target:{value:'fixture-stage-0'}});fireEvent.click(screen.getByRole('button',{name:'記録して次へ'}));
    await waitFor(() => {expect(next).toHaveBeenCalledWith('90001');});
    expect(document.querySelector(`[data-state="${state}"]`)).not.toBeNull();
    const patch=fetcher.mock.calls.find(([,init]) => (init as RequestInit | undefined)?.method === 'PATCH');
    const sent=JSON.parse((patch?.[1] as RequestInit).body as string) as {set:Record<string,string>;base:Record<string,string>;stage:{pipeline_id:string;stage_id:string}};
    expect(sent.set.taioushubetsu).toBe('done');expect(sent.base.hs_pipeline).toBe('fixture');
    expect(sent.stage).toEqual({pipeline_id:'fixture',stage_id:'fixture-stage-0'});expect(sent.base.hs_pipeline_stage).toBe('1431574804');
    expect(sent.set.renrakukakuninmokuteki).toMatch(/^10\/18 午後の連絡を希望\n\d{2}\/\d{2} 条件を確認$/);
    expect(Object.keys(sent.set).sort()).toEqual(['renrakukakuninmokuteki','taioushubetsu']);
    expect(sessionStorage.getItem(APPLICANT_DRAFT_KEY)).not.toContain('条件を確認');
  });
  it.each([403,409,503])('HTTP %s は赤を出し次へ進まない', async status => {
    mockFetch(status,status === 409 ? {status:'conflict',object:'applicant',current:{taioushubetsu:'waiting'},changed_by_hubspot:['taioushubetsu']} : status === 403 ? {error_kind:'applicants_write_scope'} : {error:'hubspot_busy'});
    const next=vi.fn();render(<ApplicantEditor detail={applicantDetailFixture('90001')} fixture={false} onNext={next}/>);
    await edit();fireEvent.click(screen.getByRole('button',{name:'記録して次へ'}));
    expect(await screen.findByRole('alert')).toBeTruthy();expect(next).not.toHaveBeenCalled();
    if (status === 403) expect(screen.getByRole('alert').textContent).toContain('HubSpot の鍵に応募を書き込む権限がありません。何も保存されていません。');
    expect(sessionStorage.getItem(APPLICANT_DRAFT_KEY)).toContain('条件を確認');
  });
  it('下書きを同じ利用者の同じ応募にだけ復元する', async () => {
    const detail=applicantDetailFixture('90001');
    sessionStorage.setItem(APPLICANT_DRAFT_KEY,JSON.stringify({user:'fixture@example.invalid',drafts:{'fixture:90001':{stageId:'fixture-stage-0',values:{renrakukakuninmokuteki:'復元する入力'}}},recorded:{}}));
    const view=render(<ApplicantEditor detail={detail} fixture onNext={vi.fn()}/>);
    await waitFor(() => {expect(screen.getByLabelText<HTMLTextAreaElement>('連絡・確認目的詳細').value).toBe('復元する入力');});
    expect(screen.getByLabelText<HTMLSelectElement>('ステージ').value).toBe('fixture-stage-0');
    view.rerender(<ApplicantEditor detail={applicantDetailFixture('90002')} fixture onNext={vi.fn()}/>);
    expect(screen.getByLabelText<HTMLTextAreaElement>('連絡・確認目的詳細').value).toBe('');
    view.unmount();
    sessionStorage.setItem(APPLICANT_DRAFT_KEY,JSON.stringify({user:'other@example.invalid',drafts:{'fixture:90001':{stageId:'',values:{renrakukakuninmokuteki:'別の人の入力'}}},recorded:{}}));
    render(<ApplicantEditor detail={detail} fixture onNext={vi.fn()}/>);
    await waitFor(() => {expect(screen.getByLabelText<HTMLTextAreaElement>('連絡・確認目的詳細').value).toBe('');});
  });
  it('OFF は読み取り表示で保存ボタンを出さない', () => {
    const fetcher=mockFetch(200,{});render(<ApplicantEditor detail={{...applicantDetailFixture('90001'),writes_enabled:false}} fixture onNext={vi.fn()}/>);
    expect(screen.getByText('いまは HubSpot に保存できない設定です。見るだけになります。')).toBeTruthy();
    expect(screen.queryByRole('button',{name:'記録して次へ'})).toBeNull();expect(fetcher).not.toHaveBeenCalled();
  });
  it('日時入力の通常の Enter では記録しない', async () => {
    const next=vi.fn();render(<ApplicantEditor detail={applicantDetailFixture('90001')} fixture onNext={next}/>);
    const input=await screen.findByLabelText('約束架電日時');
    fireEvent.keyDown(input,{key:'Enter'});
    expect(next).not.toHaveBeenCalled();
  });
  it('無変更なら PATCH せず Ctrl+Enter で次へ進む', async () => {
    const fetcher=mockFetch(200,{});const next=vi.fn();render(<ApplicantEditor detail={applicantDetailFixture('90001')} fixture={false} onNext={next}/>);
    const button=await screen.findByRole('button',{name:'記録して次へ'});fireEvent.keyDown(button,{key:'Enter',ctrlKey:true});
    await waitFor(() => {expect(next).toHaveBeenCalledWith('90001');});
    expect(fetcher.mock.calls.filter(([,init]) => (init as RequestInit | undefined)?.method === 'PATCH')).toHaveLength(0);
  });
});
