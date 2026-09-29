import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { JobgenScreen } from '../screens/jobgen/JobgenScreen';

// The Rust HTML shell (Phase 0-3) provides <div id="app-root">.
// /app/jobgen: 求人票生成パイプライン (W8)。旧 /jobgen (static/jobgen.html) の React 版。
const rootElement = document.getElementById('app-root');
if (rootElement) {
  document.title = '求人票生成パイプライン';
  createRoot(rootElement).render(
    <StrictMode>
      <JobgenScreen />
    </StrictMode>,
  );
}
