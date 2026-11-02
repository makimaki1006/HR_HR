import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { ConsultingV2Screen } from '../screens/consulting-v2/ConsultingV2Screen';
import '../screens/consulting-v2/consulting-v2.css';

// /app/consulting-v2 (コンサルKPI v2)。The Rust HTML shell provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  document.title = 'コンサルKPI v2';
  createRoot(rootElement).render(
    <StrictMode>
      <ConsultingV2Screen />
    </StrictMode>,
  );
}
