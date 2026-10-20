import { useEffect } from 'react';
import { ROUTES, usePreviewNav } from '../../preview';
import { usePortalStore } from '../../store';
import { CustomerLayout } from '../../ui/layouts';
import { RowEditor } from './RowEditor';

export function EditScreen({ next }: { next: 'ok' | 'conflict' | 'failed' }) {
  const store = usePortalStore();
  const { go } = usePreviewNav();
  // eslint-disable-next-line @typescript-eslint/unbound-method
  const { setNextSave } = store;
  useEffect(() => { setNextSave(next); }, [setNextSave, next]);
  const sheet = store.mySheets[0];
  const rows = sheet ? store.snapshot(sheet.id).rows : [];
  const row = rows.find((r) => r.rowNumber === 5) ?? rows[0];
  return (
    <CustomerLayout
      customerName={store.customerName}
      accountLabel={store.account.displayName}
      current="list"
      onNavigate={(to) => { go(to === 'logout' ? ROUTES.loggedOut : to === 'account' ? ROUTES.account : ROUTES.list); }}
    >
      <p className="py-10 text-center text-sm text-pt-faint">応募者の一覧</p>
      {sheet && row && <RowEditor sheet={sheet} row={row} onClose={() => { go(ROUTES.list); }} />}
    </CustomerLayout>
  );
}
