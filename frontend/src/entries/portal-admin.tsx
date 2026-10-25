// /app/portal-admin: 顧客向け応募者画面の社内の管理（管理者だけ。データは /api/admin/portal/*）。
// 顧客の画面と同じ見た目にするため portal.css を読む（社内画面の app.css は読まない）。
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { PortalAdminApp } from '../screens/portal/live-admin/PortalAdminApp';
import '../screens/portal/portal.css';

const root = document.getElementById('app-root');
if (root) createRoot(root).render(<StrictMode><PortalAdminApp /></StrictMode>);
