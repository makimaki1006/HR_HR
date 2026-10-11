import { createRoot } from 'react-dom/client';
import { ActivityLog } from './DealDetail';
import { fixtureDetail } from './workspaceFixture';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import './crm.css';
import './queue.css';
const data=fixtureDetail('f-1'); if (!data) throw new Error('fixture missing');
const rich={body_html:null,body_full:null,from_name:'架空 花子',from_email:'staff@example.invalid',to:['customer@example.invalid'],cc:[],thread_id:'fake-conversation',attachments_count:null,start_time:null,end_time:null,location:null,recording_url:null};
const activities: WorkspaceActivity[] = [
  {id:'90003',kind:'email',timestamp:'2026-10-12T05:00:00Z',title:'Re: 採用のご相談（架空）',body:'ご返信ありがとうございます。\n来週火曜日の14時にお打ち合わせをお願いいたします。',direction:'EMAIL',status:'SENT',duration_ms:null,owner_id:'9001',source:null,via:'deal',via_id:'100',rich},
  {id:'90002',kind:'email',timestamp:'2026-10-12T04:00:00Z',title:'Re: 採用のご相談（架空）',body:'資料を確認しました。\n来週火曜日にお打ち合わせできますでしょうか。',direction:'INCOMING_EMAIL',status:null,duration_ms:null,owner_id:null,source:null,via:'deal',via_id:'100',rich:{...rich,from_name:'架空 太郎',from_email:'customer@example.invalid',to:['staff@example.invalid']}},
  {id:'90001',kind:'email',timestamp:'2026-10-12T03:00:00Z',title:'採用のご相談（架空）',body:'お世話になっております。\n採用支援の資料をお送りします。ご都合をお知らせください。',direction:'EMAIL',status:'SENT',duration_ms:null,owner_id:'9001',source:null,via:'deal',via_id:'100',rich},
];
const root=document.getElementById('root'); if (!root) throw new Error('preview root missing');
createRoot(root).render(<main className="crm-app cq-app" style={{padding:32,maxWidth:960,background:'#f1f5f9',minHeight:'100vh',fontFamily:'sans-serif'}}>
  <h1 style={{fontSize:20}}>活動ログ</h1><p style={{color:'#64748b'}}>表示確認用の架空データです。</p>
  <ActivityLog data={{...data,activities,activity_scope:'案件に関連する活動履歴（架空）'}} placeholder="" ownerNames={new Map([['9001','架空 花子']])} sample />
</main>);
