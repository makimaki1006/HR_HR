import { useEffect, useRef, useState } from 'react';
import { AdminLayout } from '../../ui/layouts';
import { Button, Card, Field, Notice, Select, TextInput, cx } from '../../ui/kit';
import { ROUTES, usePreviewNav } from '../../preview';
import type { ColumnConfig } from '../../types';
import { ShareHint } from './ShareHint';
import { actions, useAdminState } from './state';

type Check = 'idle' | 'checking' | 'ok' | 'not_shared' | 'not_found';

const URL_PATTERN = /^https:\/\/docs\.google\.com\/spreadsheets\/d\/[\w-]+/;
const FOUND_TABS = ['応募者一覧', '2025年度', '集計（社内用）'];
const HEADERS = ['応募日', '応募媒体', '氏名', 'ふりがな', '年齢', '電話番号', 'メールアドレス', '応募職種', '選考状況', '面接日', '貴社記入欄', '社内メモ'];
const TITLE = ['応募者管理表', '', '', '', '', '', '', '', '', '', '', ''];
const NOTE = ['※毎週月曜に更新', '', '', '', '', '', '', '', '', '', '', ''];
const SAMPLE_1 = ['2026/10/18', '求人媒体A', '山田 花子', 'やまだ はなこ', '34', '090-0000-0001', 'sample1@example.com', '介護職員', '書類選考中', '', '', ''];
const SAMPLE_2 = ['2026/10/19', '求人媒体B', '鈴木 次郎', 'すずき じろう', '41', '090-0000-0002', 'sample2@example.com', '看護助手', '未対応', '', '', ''];
/** シートの先頭 5 行（見本）。 */
const TOP_ROWS: string[][] = [TITLE, NOTE, HEADERS, SAMPLE_1, SAMPLE_2];

/** 見出しらしい行か。空のセルが多い行は見出しとして読めない。 */
const looksLikeHeader = (cells: string[]) => cells.filter(Boolean).length >= 4;

export function SheetAdd() {
  const { go } = usePreviewNav();
  const s = useAdminState();
  const customer = s.customers.find((c) => c.id === s.customerId);
  const [url, setUrl] = useState('');
  const [urlError, setUrlError] = useState('');
  const [check, setCheck] = useState<Check>('idle');
  const [tab, setTab] = useState(FOUND_TABS[0] ?? '');
  const [tabName, setTabName] = useState(FOUND_TABS[0] ?? '');
  const [headerRow, setHeaderRow] = useState(3);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => { window.clearTimeout(timer.current); }, []);

  const nav = (to: 'customers' | 'accounts') => { go(to === 'customers' ? ROUTES.adminCustomers : ROUTES.adminAccounts); };

  const run = () => {
    if (!URL_PATTERN.test(url.trim())) {
      setUrlError('Google スプレッドシートの URL を貼ってください。');
      setCheck('idle');
      return;
    }
    setUrlError('');
    setCheck('checking');
    timer.current = window.setTimeout(() => {
      setCheck(url.includes('notshared') ? 'not_shared' : url.includes('missing') ? 'not_found' : 'ok');
    }, 700);
  };

  const selectedCells = TOP_ROWS[headerRow - 1] ?? [];
  const headerOk = looksLikeHeader(selectedCells);
  const headers = selectedCells.filter(Boolean);
  const finalTab = tabName.trim() || tab;
  const duplicate = Object.entries(s.sheetUrls).some(([id, u]) => {
    const sh = s.sheets.find((x) => x.id === id);
    return sh !== undefined && u === url.trim() && sh.tabName === finalTab;
  });

  const next = () => {
    if (!customer || !headerOk) return;
    const columns: ColumnConfig[] = headers.map((h) => ({
      header: h,
      label: '',
      visible: h !== '社内メモ',
      writable: false,
      options: h === '選考状況' ? ['未対応', '書類選考中', '面接調整中', '面接予定', '内定', '不採用', '辞退'] : null,
      multiline: h === '貴社記入欄',
      inList: !['ふりがな', 'メールアドレス', '貴社記入欄', '社内メモ'].includes(h),
      // 見出しの言葉からの仮の設定。列の設定の画面で直せる。
      kind: h.includes('電話') ? 'phone' : h.includes('メール') ? 'email' : h.includes('日') ? 'date' : h === '選考状況' ? 'choice' : 'text',
      group: ['氏名', '応募日', '応募媒体', '応募職種', '年齢', '電話番号', 'メールアドレス'].includes(h) ? 'summary' : ['選考状況', '面接日', '貴社記入欄'].includes(h) ? 'customer' : 'other',
    }));
    const id = 's' + String(Date.now());
    actions.addSheet({
      id, customerId: customer.id, tabName: finalTab, headerRow, columns, connection: 'ok',
      titleColumn: headers.includes('氏名') ? '氏名' : (headers[0] ?? ''),
      statusColumn: headers.includes('選考状況') ? '選考状況' : '',
      sortColumn: headers.includes('応募日') ? '応募日' : '',
    }, url.trim());
    go('admin-sheet-columns');
  };

  return (
    <AdminLayout title="シートの追加" current="customers" onNavigate={nav}>
      <button type="button" className="mb-3 text-sm text-pt-primary hover:underline" onClick={() => { go('admin-customer-detail'); }}>{customer?.name ?? '顧客'}の詳細に戻る</button>
      <div className="flex max-w-4xl flex-col gap-5">
        <Card className="p-5">
          <h2 className="mb-3 text-base font-semibold">1. シートの URL</h2>
          <Field label="スプレッドシートの URL" hint="ブラウザでシートを開いたときのアドレスを貼ってください。" error={urlError || undefined}>
            <TextInput value={url} onChange={(e) => { setUrl(e.target.value); setCheck('idle'); setUrlError(''); }} placeholder="https://docs.google.com/spreadsheets/d/..." />
          </Field>
          <div className="mt-3"><Button variant="primary" disabled={!url.trim() || check === 'checking'} onClick={run}>{check === 'checking' ? '確認しています' : 'つながりを確認'}</Button></div>

          <div className="mt-4">
            {check === 'ok' && <Notice tone="ok" title="つながりました">シートを読み込めます。</Notice>}
            {check === 'not_shared' && <ShareHint onRecheck={run} checking={false} />}
            {check === 'not_found' && <Notice tone="danger" title="シートが見つかりません">URL に誤りがないか、シートが削除されていないか確かめてください。</Notice>}
          </div>
        </Card>

        {check === 'ok' && (
          <Card className="p-5">
            <h2 className="mb-3 text-base font-semibold">2. タブと見出しの行</h2>
            <div className="grid max-w-2xl grid-cols-2 gap-4">
              <Field label="使うタブ">
                <Select value={tab} onChange={(e) => { setTab(e.target.value); setTabName(e.target.value); }}>
                  {FOUND_TABS.map((t) => <option key={t} value={t}>{t}</option>)}
                </Select>
              </Field>
              <Field label="顧客に見せるタブの名前"><TextInput value={tabName} onChange={(e) => { setTabName(e.target.value); }} /></Field>
            </div>
            {duplicate && <div className="mt-3 max-w-2xl"><Notice tone="warn">このシートのこのタブは、すでに登録されています。</Notice></div>}

            <p className="mb-1 mt-5 text-sm font-medium">見出しの行を選ぶ</p>
            <p className="mb-2 text-xs text-pt-muted">シートの先頭 5 行です。列の名前が並んでいる行を押してください。選んだ行の下 2 行は、データの見本として薄く出します。</p>
            <div className="overflow-x-auto rounded-md border border-pt-line">
              <table className="w-max min-w-full text-xs">
                <tbody>
                  {TOP_ROWS.map((cells, i) => {
                    const row = i + 1;
                    const on = row === headerRow;
                    const sample = row > headerRow && row <= headerRow + 2;
                    return (
                      <tr
                        key={row}
                        onClick={() => { setHeaderRow(row); }}
                        className={cx('cursor-pointer border-t border-pt-line first:border-t-0', on ? 'bg-pt-primary-soft font-medium' : 'hover:bg-pt-bg', sample && 'text-pt-faint')}
                      >
                        <td className="sticky left-0 bg-inherit px-2 py-1.5 text-pt-muted">
                          <label className="flex items-center gap-1.5 whitespace-nowrap">
                            <input type="radio" name="headerRow" checked={on} onChange={() => { setHeaderRow(row); }} />{row} 行目
                          </label>
                        </td>
                        {cells.map((c, j) => <td key={String(j) + c} className="whitespace-nowrap border-l border-pt-line px-2 py-1.5">{c || '　'}</td>)}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {!headerOk && <div className="mt-3"><Notice tone="danger" title="この行は見出しではなさそうです">列の名前が並んでいる行を選び直してください。</Notice></div>}
            {headerOk && <p className="mt-2 text-xs text-pt-muted">{headers.length} 列を読み込みます。</p>}
            <div className="mt-4"><Button variant="primary" disabled={!headerOk || duplicate} onClick={next}>列の設定へ進む</Button></div>
          </Card>
        )}
      </div>
    </AdminLayout>
  );
}
