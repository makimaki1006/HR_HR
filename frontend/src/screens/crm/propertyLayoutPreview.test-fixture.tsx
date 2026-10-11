import { createRoot } from 'react-dom/client';
import { PropertyPanel } from './PropertyPanel';
import { useRolePropertyLayout } from './useRolePropertyLayout';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { catalogIndex } from './propertyModel';
import { fixtureDetail } from './workspaceFixture';
import './crm.css';
import './queue.css';
function Preview() {
  const layout = useRolePropertyLayout('live', localStorage);
  return <main className="crm-app cq-app" style={{ padding: 24, maxWidth: 650, background: '#f8fafc', fontFamily: 'sans-serif' }}>
    <h1>プロパティ</h1><p>表示確認用の架空データです。</p>
    <PropertyPanel roleLayout={layout} catalog={{ phase: 'ready', catalog: FIXTURE_CATALOG, index: catalogIndex(FIXTURE_CATALOG) }}
      selection={layout.selection} onApply={layout.apply} onReloadCatalog={() => { /* static fixture */ }} data={fixtureDetail('f-1', layout.selection)} ownerNames={new Map()} hasSelection placeholder="案件を選んでください" />
  </main>;
}
const root = document.getElementById('root');
if (!root) throw new Error('preview root missing');
createRoot(root).render(<Preview />);
