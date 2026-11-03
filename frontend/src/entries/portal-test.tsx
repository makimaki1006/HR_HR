// /app/portal-test: 顧客向け応募者画面の社内向けテスト版（本番の HR_HR に載せる。ナビには出さない）。
// 中身は手元の試作（portal-preview）と同じで、試作の帯つき。本物のシート（テスト用の 1 枚）を
// 同じオリジンの /api/portal-test/* から読み書きする（既定で ON、?live=0 で試作のサンプルだけ）。
// portal.css だけを読み、社内画面の app.css は読まない。
import { StrictMode } from 'react';
import { setPortalPreviewClock } from '../screens/portal/clock';
import { NOW } from '../screens/portal/now';
import { createRoot } from 'react-dom/client';
import { PortalPreview } from '../screens/portal/PortalPreview';
import '../screens/portal/portal.css';

setPortalPreviewClock(NOW);

const root = document.getElementById('app-root');
if (root) createRoot(root).render(<StrictMode><PortalPreview /></StrictMode>);
