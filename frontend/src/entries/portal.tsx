// /portal/*: 顧客向け応募者画面の本番の入口（ログイン・確認コード・招待のパスワード設定）。
// portal.css だけを読み、社内画面のコード・app.css は含めない。
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { PortalApp } from '../screens/portal/live/PortalApp';
import '../screens/portal/portal.css';

const root = document.getElementById('app-root');
if (root) createRoot(root).render(<StrictMode><PortalApp /></StrictMode>);
