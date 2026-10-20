// 試作用のメモリ上の状態。サーバーの代わりに、シートの読み書き・同時編集・遅れを再現する。
// 顧客の一覧（list）と編集（edit）がこれを共有する。社内の管理画面は各自の画面の中で状態を持つ。
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';
import { CURRENT_ACCOUNT_ID, NOW, accounts, customers, initialRows } from './fixtures';
import { useAdminState } from './admin/customers/state';
import type { ApplicantRow, PortalAccount, SaveResult, SheetConfig, SheetSnapshot } from './types';

interface PortalStore {
  account: PortalAccount;
  customerName: string;
  /** ログイン中の顧客に見せるシート（タブ）。 */
  mySheets: SheetConfig[];
  snapshot(sheetId: string): SheetSnapshot;
  /** 保存。base は開いたときの行の値。シートの行と食い違えば conflict。 */
  saveRow(sheetId: string, rowNumber: number, base: Record<string, string>, next: Record<string, string>): Promise<SaveResult>;
  /** 「前回から増えた・変わった」の印を消す（行を開いたとき）。 */
  markSeen(sheetId: string, rowNumber: number): void;
  /** 試作の操作: 社内の人がシートで 1 行を書き換えた（次の確認で画面に出る）。 */
  simulateSheetEdit(sheetId: string, rowNumber?: number): void;
  /** 試作の操作: シートに新しい応募者の行が足された。 */
  simulateNewApplicant(sheetId: string): void;
  /** 試作の操作: 次の保存を「シートで変更されています」にする / 「保存できませんでした」にする。 */
  nextSave: 'ok' | 'conflict' | 'failed';
  setNextSave(v: 'ok' | 'conflict' | 'failed'): void;
}

const Ctx = createContext<PortalStore | null>(null);

const nowIso = () => new Date().toISOString();

export function PortalStoreProvider({ children }: { children: ReactNode }) {
  const account = accounts.find((a) => a.id === CURRENT_ACCOUNT_ID) ?? accounts[0];
  const customer = customers.find((c) => c.id === account?.customerId) ?? customers[0];
  if (!account || !customer) throw new Error('試作のサンプルが無い');
  // 社内の「列の設定」で変えた内容が、そのまま顧客の画面に出る（試作）。
  const adminSheets = useAdminState().sheets;
  const mySheets = useMemo(() => adminSheets.filter((s) => s.customerId === customer.id), [adminSheets, customer.id]);
  const [rows, setRows] = useState<Record<string, ApplicantRow[]>>(initialRows);
  const [readAt, setReadAt] = useState<Record<string, string>>(() =>
    Object.fromEntries(Object.keys(initialRows).map((k) => [k, NOW])),
  );
  const [nextSave, setNextSave] = useState<'ok' | 'conflict' | 'failed'>('ok');

  const snapshot = useCallback(
    (sheetId: string): SheetSnapshot => ({ sheetId, readAt: readAt[sheetId] ?? NOW, rows: rows[sheetId] ?? [] }),
    [rows, readAt],
  );

  const touch = (sheetId: string) => { setReadAt((r) => ({ ...r, [sheetId]: nowIso() })); };

  const saveRow = useCallback<PortalStore['saveRow']>(
    async (sheetId, rowNumber, base, next) => {
      await new Promise((r) => setTimeout(r, 700));
      const current = (rows[sheetId] ?? []).find((r) => r.rowNumber === rowNumber);
      if (!current) return { kind: 'failed', message: 'この行がシートに見つかりません。' };
      if (nextSave === 'failed') {
        setNextSave('ok');
        return { kind: 'failed', message: 'シートに保存できませんでした。' };
      }
      const changedOnSheet =
        nextSave === 'conflict' || Object.keys(base).some((k) => (current.values[k] ?? '') !== (base[k] ?? ''));
      if (changedOnSheet) {
        setNextSave('ok');
        const latest: ApplicantRow =
          nextSave === 'conflict'
            ? { ...current, values: { ...current.values, 選考状況: '面接調整中', 貴社記入欄: (current.values.貴社記入欄 ?? '') + '（弊社で追記）候補日を 3 つ送付済み。' } }
            : current;
        setRows((all) => ({ ...all, [sheetId]: (all[sheetId] ?? []).map((r) => (r.rowNumber === rowNumber ? latest : r)) }));
        touch(sheetId);
        return { kind: 'conflict', latest };
      }
      const saved: ApplicantRow = { ...current, values: { ...current.values, ...next }, changedSinceLastView: null };
      setRows((all) => ({ ...all, [sheetId]: (all[sheetId] ?? []).map((r) => (r.rowNumber === rowNumber ? saved : r)) }));
      touch(sheetId);
      return { kind: 'saved', row: saved };
    },
    [rows, nextSave],
  );

  const markSeen = useCallback((sheetId: string, rowNumber: number) => {
    setRows((all) => ({
      ...all,
      [sheetId]: (all[sheetId] ?? []).map((r) => (r.rowNumber === rowNumber ? { ...r, changedSinceLastView: null } : r)),
    }));
  }, []);

  const simulateSheetEdit = useCallback((sheetId: string, rowNumber?: number) => {
    setRows((all) => {
      const list = all[sheetId] ?? [];
      const target = rowNumber ?? list[2]?.rowNumber;
      return {
        ...all,
        [sheetId]: list.map((r) =>
          r.rowNumber === target
            ? { ...r, changedSinceLastView: 'updated', values: { ...r.values, 一次対応の結果: (r.values.一次対応の結果 ?? '') + ' 10/20 再度電話し、10/24 14:00 で面接を設定。', 選考状況: '面接予定', 面接日: '2026/10/24 14:00' } }
            : r,
        ),
      };
    });
    touch(sheetId);
  }, []);

  const simulateNewApplicant = useCallback((sheetId: string) => {
    setRows((all) => {
      const list = all[sheetId] ?? [];
      const max = list.reduce((m, r) => Math.max(m, r.rowNumber), 1);
      const row: ApplicantRow = {
        rowNumber: max + 1,
        changedSinceLastView: 'new',
        values: {
          応募日: '2026/10/20', 応募媒体: 'Indeed', 氏名: '新井 ひなた', ふりがな: 'あらい ひなた', 年齢: '27',
          電話番号: '090-0000-9999', メールアドレス: 'sample-new@example.com', 応募職種: '介護職（正社員）',
          一次対応日: '', 一次対応の結果: '', 選考状況: '未対応', 面接日: '', 面接結果: '', 貴社記入欄: '', 社内メモ: '',
        },
      };
      return { ...all, [sheetId]: [row, ...list] };
    });
    touch(sheetId);
  }, []);

  const value: PortalStore = {
    account,
    customerName: customer.name,
    mySheets,
    snapshot,
    saveRow,
    markSeen,
    simulateSheetEdit,
    simulateNewApplicant,
    nextSave,
    setNextSave,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function usePortalStore(): PortalStore {
  const v = useContext(Ctx);
  if (!v) throw new Error('PortalStoreProvider が無い');
  return v;
}
