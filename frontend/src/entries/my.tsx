import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { MyScreen } from '../screens/my/MyScreen';
import '../screens/admin/w8.css';

// W8 (2026-09-29): /app/my. The Rust HTML shell provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <MyScreen />
    </StrictMode>,
  );
}
