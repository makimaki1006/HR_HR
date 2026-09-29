import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { DummyScreen } from '../screens/dummy/DummyScreen';

// The Rust HTML shell (Phase 0-3) provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <DummyScreen />
    </StrictMode>,
  );
}
