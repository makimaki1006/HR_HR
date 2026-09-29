import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { DummyScreen } from '../screens/dummy/DummyScreen';
import { AppShell } from '../shell';
import '../styles/app.css';

// The Rust HTML shell (Phase 0-3) provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <AppShell screen="dummy" filters>
        <DummyScreen />
      </AppShell>
    </StrictMode>,
  );
}
