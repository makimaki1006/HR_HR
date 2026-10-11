// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import type { ActivityRich } from '../../generated/ActivityRich';
import { groupActivityEmails, normalizedEmailSubject } from './emailThreads';
import { ActivityLog } from './DealDetail';
import { fixtureDetail } from './workspaceFixture';
import { apiGet } from '../../api/client';
vi.mock('../../api/client', () => ({ apiGet: vi.fn() }));
vi.mock('./ReplySyncStatus', () => ({ ReplySyncStatus: () => null }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const rich: ActivityRich = { body_html:null, body_full:null, from_name:null, from_email:'staff@example.invalid', to:['customer@example.invalid'], cc:[], thread_id:null, attachments_count:null, start_time:null, end_time:null, location:null, recording_url:null };
export function email(id: string, title: string, when: string, extra: Partial<WorkspaceActivity> = {}): WorkspaceActivity {
  return { id, kind:'email', title, timestamp:`2026-10-12T${when}:00Z`, body:`本文 ${id}`, direction:'EMAIL', status:'SENT', duration_ms:null, owner_id:null, source:null, via:'deal', via_id:'100', rich, ...extra };
}
const messages = [email('90003','Re: 採用のご相談','05:00'), email('90001','採用のご相談','03:00'), email('90002','RE : 採用のご相談','04:00', { direction:'INCOMING_EMAIL', status:null, rich:{ ...rich, from_email:'CUSTOMER@example.invalid', to:['staff@example.invalid'] } })];
function log(activities: WorkspaceActivity[]) {
  const data=fixtureDetail('f-1'); if (!data) throw new Error('fixture missing');
  return render(<ActivityLog data={{...data,activities}} placeholder="" ownerNames={new Map()} sample />);
}
describe('email threads', () => {
  it('groups explicit thread IDs regardless of subject, but keeps different IDs separate', () => {
    const list=groupActivityEmails([email('1','A','03:00',{rich:{...rich,thread_id:'shared'}}),email('2','別件名','04:00',{rich:{...rich,thread_id:'shared'}}),email('3','A','05:00',{rich:{...rich,thread_id:'other'}})]);
    expect(list.map(e => e.kind === 'thread' ? e.messages.map(a=>a.id) : [e.activity.id])).toEqual([['3'],['2','1']]);
  });
  it('normalizes repeated/case/spacing/fullwidth Re and Fwd prefixes and matches incoming/outgoing peer addresses', () => {
    expect(normalizedEmailSubject(' ＲＥ： Fwd: re[2]: 採用の  ご相談 ')).toBe('採用の ご相談');
    const list=groupActivityEmails(messages);
    expect(list).toHaveLength(1);
    expect(list[0]?.kind === 'thread' ? list[0].messages.map(a=>a.id) : []).toEqual(['90003','90002','90001']);
    expect(messages.map(a=>a.id)).toEqual(['90003','90001','90002']);
  });
  it('keeps another correspondent, missing addresses, and non-email activity separate', () => {
    const list=groupActivityEmails([...messages,email('4','Re: 採用のご相談','06:00',{rich:{...rich,to:['other@example.invalid']}}),email('5','Re: 採用のご相談','07:00',{rich:{...rich,to:[]}}),email('6','Re: 採用のご相談','08:00',{kind:'call'})]);
    expect(list.map(e=>e.kind==='thread'?e.messages.map(a=>a.id):[e.activity.id])).toEqual([['6'],['5'],['4'],['90003','90002','90001']]);
  });
  it('shows latest collapsed, allows both chronological orders, and reports grouped count plus email total with filtering', () => {
    log([...messages,email('90004','電話の履歴','06:00',{kind:'call'})]);
    expect(document.querySelector('.wd-log-head')?.textContent).toBe('2 件（メール 3 件）');
    expect(screen.getAllByTestId('activity-preview').map(e=>e.textContent)).toEqual(['本文 90004','本文 90003']);
    fireEvent.click(screen.getByRole('button',{name:'メール'}));
    expect(document.querySelector('.wd-log-head')?.textContent).toBe('1 件（メール 3 件）');
    fireEvent.click(screen.getByRole('button',{name:'やり取り 3 件を開く'}));
    expect(screen.getAllByTestId('activity-preview').map(e=>e.textContent)).toEqual(['本文 90003','本文 90002','本文 90001']);
    fireEvent.change(screen.getByLabelText('並び順'),{target:{value:'oldest'}});
    expect(screen.getAllByTestId('activity-preview').map(e=>e.textContent)).toEqual(['本文 90001','本文 90002','本文 90003']);
    expect(screen.getAllByTestId('activity-direction').map(e=>e.textContent)).toEqual(['送信','受信','送信']);
  });
  it('preserves per-message tracking values and never queries tracking for received emails', async () => {
    const count=(n:number)=>({count:n,first_at:null,last_at:null});
    vi.mocked(apiGet).mockImplementation(path => Promise.resolve({ok:true,data:{opens:count(path.includes('90001')?2:4),clicks:count(1),automatic_opens:count(0),automatic_clicks:count(0),links:[]}}));
    log(messages);
    await waitFor(()=>{ expect(screen.getByTestId('email-tracking-summary').textContent).toBe('開封 4 回・クリック 1 回'); });
    fireEvent.click(screen.getByRole('button',{name:'やり取り 3 件を開く'}));
    await waitFor(()=>{ expect(screen.getAllByTestId('email-tracking-summary').map(e=>e.textContent)).toEqual(['開封 4 回・クリック 1 回','開封 2 回・クリック 1 回']); });
    expect(vi.mocked(apiGet).mock.calls.map(c=>c[0])).toEqual(['/api/crm/emails/90003/tracking','/api/crm/emails/90001/tracking']);
    fireEvent.change(screen.getByLabelText('並び順'),{target:{value:'oldest'}});
    expect(screen.getAllByTestId('email-tracking-summary').map(e=>e.textContent)).toEqual(['開封 2 回・クリック 1 回','開封 4 回・クリック 1 回']);
    expect(vi.mocked(apiGet)).toHaveBeenCalledTimes(2);
  });
  it('does not show or fetch tracking when the latest collapsed message is a reply', async () => {
    const count=(n:number)=>({count:n,first_at:null,last_at:null});
    vi.mocked(apiGet).mockResolvedValue({ok:true,data:{opens:count(3),clicks:count(0),automatic_opens:count(0),automatic_clicks:count(0),links:[]}});
    log([email('90004','Re: 採用のご相談','07:00',{direction:'INCOMING_EMAIL',status:null,rich:{...rich,from_email:'customer@example.invalid',to:['staff@example.invalid']}}),...messages]);
    expect(screen.getByTestId('activity-direction').textContent).toBe('受信');
    expect(screen.queryByTestId('email-tracking')).toBeNull();
    expect(apiGet).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button',{name:'やり取り 4 件を開く'}));
    await waitFor(()=>{expect(screen.getAllByTestId('email-tracking-summary').map(e=>e.textContent)).toEqual(['開封 3 回・クリック 0 回','開封 3 回・クリック 0 回']);});
    expect(vi.mocked(apiGet).mock.calls.map(c=>c[0])).toEqual(['/api/crm/emails/90003/tracking','/api/crm/emails/90001/tracking']);
  });

});
