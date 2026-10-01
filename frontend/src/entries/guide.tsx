import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { GuideScreen } from '../screens/guide/GuideScreen';

// The Rust HTML shell provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <GuideScreen />
    </StrictMode>,
  );
}
