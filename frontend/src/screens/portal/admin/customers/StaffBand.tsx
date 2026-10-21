import { currentCustomer, useData } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { actions } from './state';

/** 社内の人が顧客の画面を見ているあいだ、上に出す細い濃い帯（顧客の担当者の画面と見分けるため）。 */
export function StaffBand() {
  const d = useData();
  const { go } = usePreviewNav();
  if (d.viewer.kind !== 'staff') return null;
  const customer = currentCustomer(d);
  return (
    <div className="sticky top-0 z-40 flex h-9 items-center gap-3 bg-pt-ink px-4 text-[13px] text-white">
      <span className="text-xs font-semibold tracking-[0.12em] text-white/60">社内</span>
      <span className="min-w-0 flex-1 truncate">{customer?.name ?? ''}の画面を見ている</span>
      <button
        type="button"
        onClick={() => {
          if (customer) {
            actions.selectCustomer(customer.id);
            go('admin-customer-detail');
          } else {
            go(ROUTES.adminCustomers);
          }
        }}
        className="shrink-0 rounded px-2 py-1 text-xs font-medium text-white/85 hover:bg-white/12 hover:text-white"
      >
        管理画面に戻る
      </button>
    </div>
  );
}
