import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { CrmReferenceScreen, CrmScreen } from '../screens/crm/CrmScreen';
import { CallWorkspace } from '../screens/crm/CallWorkspace';
import { CallQueueScreen } from '../screens/crm/CallQueueScreen';
import { crmView } from '../screens/crm/crmView';

const rootElement = document.getElementById('app-root');
const view = crmView(window.location.search);
if (rootElement) createRoot(rootElement).render(<StrictMode>
  {view === 'moc' ? <CrmScreen /> : view === 'reference' ? <CrmReferenceScreen /> : view === 'single' ? <CallWorkspace /> : <CallQueueScreen />}
</StrictMode>);
