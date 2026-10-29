import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { CallPracticeScreen } from '../screens/call-practice/CallPracticeScreen';
import '../screens/admin/w8.css';
import '../screens/call-practice/call-practice.css';

// /app/call-practice (架電の練習)。The Rust HTML shell provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <CallPracticeScreen />
    </StrictMode>,
  );
}
