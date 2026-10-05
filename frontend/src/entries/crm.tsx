import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { CrmReferenceScreen, CrmScreen } from '../screens/crm/CrmScreen';
import { CallWorkspace } from '../screens/crm/CallWorkspace';
import { CallQueueScreen } from '../screens/crm/CallQueueScreen';

const rootElement = document.getElementById('app-root');
const view = new URLSearchParams(window.location.search).get('view');
if (rootElement) createRoot(rootElement).render(<StrictMode>
  {view === 'queue' ? <CallQueueScreen /> : view === 'reference' ? <CrmReferenceScreen /> : view === 'single' ? <CallWorkspace /> : <CrmScreen />}
</StrictMode>);
