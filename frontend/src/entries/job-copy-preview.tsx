// Local preview only (npx vite, then open /static/app/job-copy-preview.html). Not a build entry.
// Renders the job copy screen with the fictional demo data and demo market data, without the
// shared App Shell (which needs /api/nav from Rust). Sends no request to the backend.
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { JobCopyScreen } from '../screens/job-copy/JobCopyScreen';
import '../styles/app.css';

const url = new URL(window.location.href);
if (url.searchParams.get('demo') !== '1') {
  url.searchParams.set('demo', '1');
  window.history.replaceState(null, '', url);
}
const root = document.getElementById('app-root');
if (root) createRoot(root).render(<StrictMode><div className="jc-shell"><JobCopyScreen /></div></StrictMode>);
