// 項目とプルダウンは、顧客の画面と同じ「項目の設定」で行う。ここは開いたらそちらへ移す。
import { useEffect } from 'react';
import { sheetsOf, useData } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { enterAs, useAdminState } from './state';

export function SheetColumns() {
  const { go } = usePreviewNav();
  const d = useData();
  const admin = useAdminState();
  const customerId = admin.customerId;
  const sheetId = (sheetsOf(d, customerId).find((s) => s.id === admin.sheetId) ?? sheetsOf(d, customerId)[0])?.id;
  useEffect(() => {
    enterAs(customerId, sheetId);
    go(ROUTES.columns);
    // 開いたときに 1 回だけ移す。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return <p className="p-6 text-[13px] text-pt-muted">項目とプルダウンの画面へ移ります。</p>;
}
