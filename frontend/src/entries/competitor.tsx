import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { CompetitorScreen } from '../screens/competitor/CompetitorScreen';

// The Rust HTML shell (GET /app/competitor) provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  document.title = '競合調査 | HR_HR';
  createRoot(rootElement).render(
    <StrictMode>
      <CompetitorScreen />
    </StrictMode>,
  );
}
