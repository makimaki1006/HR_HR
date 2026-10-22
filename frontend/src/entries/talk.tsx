import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { TalkScreen } from '../screens/talk/TalkScreen';
import '../screens/admin/w8.css';

// /app/talk (トークの振り返り)。The Rust HTML shell provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <TalkScreen />
    </StrictMode>,
  );
}
