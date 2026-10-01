import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { RecruitmentDiagScreen } from '../screens/recruitment-diag/RecruitmentDiagScreen';
import '../styles/app.css';

// /app/recruitment-diag. The Rust HTML shell provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  createRoot(rootElement).render(
    <StrictMode>
      <RecruitmentDiagScreen />
    </StrictMode>,
  );
}
