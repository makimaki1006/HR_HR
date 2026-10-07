import { StrictMode, Suspense, lazy } from 'react';
import { createRoot } from 'react-dom/client';
import { crmView } from '../screens/crm/crmView';
import type { CrmView } from '../screens/crm/crmView';

// 表示する画面だけを読み込む (既定の架電画面を開いたときに、見本・MOC の画面とその CSS を読まない)
const SCREENS: Record<CrmView, React.LazyExoticComponent<React.ComponentType>> = {
  queue: lazy(() => import('../screens/crm/CallQueueScreen').then(m => ({ default: () => <m.CallQueueScreen /> }))),
  moc: lazy(() => import('../screens/crm/CrmScreen').then(m => ({ default: m.CrmScreen }))),
  reference: lazy(() => import('../screens/crm/CrmScreen').then(m => ({ default: m.CrmReferenceScreen }))),
  single: lazy(() => import('../screens/crm/CallWorkspace').then(m => ({ default: () => <m.CallWorkspace /> }))),
};

const rootElement = document.getElementById('app-root');
const Screen = SCREENS[crmView(window.location.search)];
if (rootElement) createRoot(rootElement).render(<StrictMode>
  <Suspense fallback={<p role="status" style={{ padding: 16 }}>読み込み中…</p>}><Screen /></Suspense>
</StrictMode>);
