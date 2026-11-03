// Local preview only (npx vite, then open /static/app/portal-preview.html). Not a build entry.
// 顧客向け応募者画面（/portal）の UI 試作。架空のサンプルだけで動き、サーバーへは何も送らない。
import { StrictMode } from 'react';
import { setPortalPreviewClock } from '../screens/portal/clock';
import { NOW } from '../screens/portal/now';
import { createRoot } from 'react-dom/client';
import { PortalPreview } from '../screens/portal/PortalPreview';
import '../screens/portal/portal.css';

setPortalPreviewClock(NOW);

const root = document.getElementById('app-root');
if (root) createRoot(root).render(<StrictMode><PortalPreview /></StrictMode>);
