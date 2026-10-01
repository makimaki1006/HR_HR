import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { SalesKpiScreen, applySavedTheme } from '../screens/sales-kpi/SalesKpiScreen';
import '../screens/sales-kpi/sales-kpi.css';

// 旧画面は <head> 先頭で暗い画面を適用していた。ここでは描画の直前に当てる。
applySavedTheme();

// The Rust HTML shell (GET /app/sales-kpi) provides <div id="app-root">.
const rootElement = document.getElementById('app-root');
if (rootElement) {
  document.title = '営業KPI 現場版';
  createRoot(rootElement).render(
    <StrictMode>
      <SalesKpiScreen />
    </StrictMode>,
  );
}
