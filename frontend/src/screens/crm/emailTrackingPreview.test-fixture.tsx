// E2E-only entry. Not imported by any production entry.
import { createRoot } from 'react-dom/client';
import { ActivityCard } from './ActivityCard';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import './crm.css';
import './queue.css';

const tracked = new URLSearchParams(window.location.search).get('tracked') === '1';
const trackingHtml = new URLSearchParams(window.location.search).get('html') === '1'
  ? '<p>資料 <a href="https://hr.example.invalid/t/c/0123456789abcdef0123456789abcdef/1" data-hr-email-url="https://example.invalid/docs?a=1&amp;b=2">資料リンク</a></p><img data-hr-email-open="1" src="https://hr.example.invalid/t/o/0123456789abcdef0123456789abcdef.gif" width="1" height="1">'
  : null;
const activity: WorkspaceActivity = {
  id: tracked ? '90001' : '90002', kind: 'email', timestamp: '2026-10-12T03:00:00Z',
  title: tracked ? 'ご案内資料をお送りします（架空）' : '先日のご相談について（架空）',
  body: 'お世話になっております。\nご相談いただいた資料をお送りします。\nご不明な点があればお気軽にご返信ください。',
  direction: 'EMAIL', status: 'SENT', duration_ms: null, owner_id: '9001', source: null, via: 'deal', via_id: '100',
  rich: { body_html: trackingHtml, body_full: null, from_name: '架空 花子', from_email: 'hanako@example.invalid', to: ['contact@example.invalid'], cc: [], thread_id: null, attachments_count: null, start_time: null, end_time: null, location: null, recording_url: null },
};
const root = document.getElementById('root');
if (!root) throw new Error('preview root missing');
createRoot(root).render(<main className="crm-app cq-app" style={{ padding: 32, background: '#f1f5f9', minHeight: '100vh', fontFamily: 'sans-serif' }}>
  <h1 style={{ fontSize: 20 }}>活動ログ</h1>
  <p style={{ color: '#64748b' }}>表示確認用の架空データです。</p>
  <ul style={{ padding: 0, listStyle: 'none' }}><ActivityCard a={activity} ownerNames={new Map([['9001', '架空 花子']])} /></ul>
</main>);
