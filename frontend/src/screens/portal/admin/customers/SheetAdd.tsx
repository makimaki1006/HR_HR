import { useEffect, useRef, useState, type ReactNode } from 'react';
import { sheetsOf, useData } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { Button, Field, Select, TextInput, cx } from '../../ui/kit';
import { IconCheck } from '../../ui/icons';
import type { ApplicantRow, ColumnConfig } from '../../types';
import { ShareHint } from './ShareHint';
import { guessInterviewColumns } from './interviewColumns';
import { AdminPage } from './shared';
import { actions, enterAs, useAdminState } from './state';

type Check = 'idle' | 'checking' | 'ok' | 'not_shared' | 'not_found';

const URL_PATTERN = /^https:\/\/docs\.google\.com\/spreadsheets\/d\/[\w-]+/;
const FOUND_TABS = ['応募者情報管理', '2025年度', '集計（社内用）'];
const HEADERS = ['応募発生日', '応募発生媒体', '応募者名', 'フリガナ', '年齢', '電話番号', 'メールアドレス', '職種', '雇用形態', '求職者経歴', '状況', '面接予定日', '時間', '備考', '次回アクション', '入社予定日'];
/** 実際のシートの 2 行目は「記入例」の行（灰色。応募者ではない）。 */
const EXAMPLE = ['記入例）2023年1月20日', 'indeed', '記入例 太郎', 'キニュウレイ タロウ', '35', '09000000000', 'example@example.com', 'ドライバー', '正社員', '最終学歴：〇〇高校卒業 ほか', '未連絡', '2023/01/30', '11:00', '（経緯のメモ）', '面接の前日に連絡', ''];
const SAMPLE_1 = ['2026年10月18日', 'engage', '山田 花子', 'ヤマダ ハナコ', '34', '09000000001', 'sample1@example.com', 'ドライバー', '正社員', '最終学歴：高校卒業 ほか', '応募者連絡待ち', '', '', '', '', ''];
const SAMPLE_2 = ['2026/10/19', 'indeed', '鈴木 次郎', 'スズキ ジロウ', '41', '09000000002', 'sample2@example.com', 'ドライバー', 'アルバイト', '最終学歴：専門学校卒業 ほか', '未連絡', '記載なし', '', '', '', ''];
const SAMPLE_3 = ['2026/10/19', '求人ボックス', '高橋 三郎', 'タカハシ サブロウ', '29', '09000000003', '記載なし', 'ドライバー', '正社員', '最終学歴：大学卒業 ほか', '未連絡', '', '', '', '', ''];
/** シートの先頭 5 行（見本）。 */
const TOP_ROWS: string[][] = [HEADERS, EXAMPLE, SAMPLE_1, SAMPLE_2, SAMPLE_3];
/** 書ける列（シートで見出しが緑の列）。 */
const WRITABLE = ['状況', '面接予定日', '時間', '備考', '次回アクション', '入社予定日'];

/** 見出しらしい行か。空のセルが多い行は見出しとして読めない。 */
const looksLikeHeader = (cells: string[]): boolean => cells.filter(Boolean).length >= 4;

/** 3 段の進み具合。今いる段を濃く、終わった段にチェック。 */
function Progress({ at }: { at: 1 | 2 | 3 }) {
  const steps = ['URL', 'タブと見出しの行', '項目とプルダウン'];
  return (
    <ol className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs" aria-label="進み具合">
      {steps.map((t, i) => {
        const n = i + 1;
        const done = n < at;
        const now = n === at;
        return (
          <li key={t} className="flex items-center gap-2">
            {i > 0 && <span className="h-px w-5 bg-pt-line-strong" aria-hidden="true" />}
            <span className={cx('inline-flex items-center gap-1.5', now ? 'font-semibold text-pt-ink' : done ? 'text-pt-muted' : 'text-pt-faint')}>
              <span className={cx('pt-num inline-flex h-5 w-5 items-center justify-center rounded-full text-[12px]', done ? 'bg-pt-ok text-white' : now ? 'bg-pt-ink text-white' : 'border border-pt-line-strong')}>
                {done ? <IconCheck size={10} /> : String(n)}
              </span>
              {t}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

function Step({ n, title, done, children }: { n: number; title: string; done?: boolean; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-pt-line bg-pt-surface">
      <h2 className="flex items-center gap-2 border-b border-pt-line px-4 py-2.5 text-[13px] font-semibold">
        <span className={cx('pt-num inline-flex h-5 w-5 items-center justify-center rounded-full text-[12px]', done ? 'bg-pt-ok text-white' : 'bg-pt-sunken text-pt-muted')}>{done ? <IconCheck size={12} /> : String(n)}</span>
        {title}
      </h2>
      <div className="p-4">{children}</div>
    </section>
  );
}

export function SheetAdd() {
  const { go } = usePreviewNav();
  const d = useData();
  const admin = useAdminState();
  const customer = d.customers.find((c) => c.id === admin.customerId);
  const [url, setUrl] = useState('');
  const [urlError, setUrlError] = useState('');
  const [check, setCheck] = useState<Check>('idle');
  const taken = (name: string): boolean => customer !== undefined && sheetsOf(d, customer.id).some((x) => x.tabName === name);
  const [tab, setTab] = useState(FOUND_TABS[0] ?? '');
  // 既定の名前は元のタブ名。すでに同じ名前があれば空にして、入力してもらう。
  const [tabName, setTabName] = useState(taken(FOUND_TABS[0] ?? '') ? '' : (FOUND_TABS[0] ?? ''));
  const [headerRow, setHeaderRow] = useState(1);
  const [exampleRows, setExampleRows] = useState(1);
  // 面接の列。undefined のあいだは見出しから当てた列を使い、社内の人が選んだら選んだ列にする。
  const [pickedDate, setPickedDate] = useState<string | undefined>(undefined);
  const [pickedTime, setPickedTime] = useState<string | undefined>(undefined);
  const [newId] = useState(() => 's' + String(Date.now()));
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => { window.clearTimeout(timer.current); }, []);

  const run = (): void => {
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
  const guess = guessInterviewColumns(headers);
  const interviewDate = pickedDate ?? guess.date;
  const interviewTime = pickedTime ?? guess.time;
  const finalTab = tabName.trim();
  const sameName = finalTab !== '' && taken(finalTab);
  const duplicate = Object.entries(admin.sheetUrls).some(([id, u]) => {
    const sh = d.sheets.find((x) => x.id === id);
    return sh !== undefined && u === url.trim() && sh.tabName === finalTab;
  });

  /** 「追加して…」を押せない理由。押せるときは空。 */
  const blockReason = duplicate
    ? 'このシートのこのタブは、すでに登録されている。'
    : sameName || (finalTab === '' && taken(tab))
      ? '同じ名前のシートがある。顧客に見せるタブの名前を変える。'
      : finalTab === ''
        ? '顧客に見せるタブの名前を入れる。'
        : !headerOk
          ? '見出しの行を選ぶ。'
          : '';

  const next = (): void => {
    if (!customer || !headerOk) return;
    const columns: ColumnConfig[] = headers.map((h) => ({
      header: h,
      label: '',
      visible: true,
      writable: WRITABLE.includes(h),
      options: h === '状況' ? ['書類選考落ち', '未連絡', '応募者連絡待ち', '面接辞退', '面接日時確定', '面接済', '内定', '不採用', '入社日調整中', '内定辞退'] : null,
      multiline: ['求職者経歴', '備考'].includes(h),
      inList: !['フリガナ', 'メールアドレス', '職種', '求職者経歴', '備考', '年齢', '応募発生媒体'].includes(h),
      // 見出しの言葉からの仮の設定。項目の設定の画面で直せる。
      kind: h.includes('電話') ? 'phone' : h.includes('メール') ? 'email' : h.endsWith('日') || h === interviewDate ? 'date' : h === '状況' ? 'choice' : 'text',
      group: ['応募者名', '応募発生日', '応募発生媒体', '職種', '雇用形態', '年齢', '電話番号', 'メールアドレス'].includes(h) ? 'summary' : WRITABLE.includes(h) ? 'customer' : h === '求職者経歴' ? 'staff' : 'other',
    }));
    const id = newId;
    // 記入例の行は応募者ではないので、読み込まない。
    const samples: ApplicantRow[] = TOP_ROWS.slice(headerRow + exampleRows).filter((cells) => cells.filter(Boolean).length >= 3).map((cells, i) => ({
      rowNumber: headerRow + exampleRows + 1 + i,
      changedSinceLastView: null,
      values: selectedCells.reduce<Record<string, string>>((acc, h, j) => (h === '' ? acc : { ...acc, [h]: cells[j] ?? '' }), {}),
    }));
    actions.addSheet({
      id, customerId: customer.id, tabName: finalTab, headerRow, exampleRows, columns, connection: 'ok',
      titleColumn: headers.includes('応募者名') ? '応募者名' : (headers[0] ?? ''),
      statusColumn: headers.includes('状況') ? '状況' : '',
      sortColumn: headers.includes('応募発生日') ? '応募発生日' : '',
      ...(interviewDate ? { interviewColumn: interviewDate } : {}),
      ...(interviewTime ? { interviewTimeColumn: interviewTime } : {}),
    }, url.trim(), `${customer.name}様_応募者管理`, samples);
    enterAs(customer.id, id);
    go(ROUTES.columns);
  };

  return (
    <AdminPage title={`シートの追加${customer ? ' ／ ' + customer.name : ''}`} current="customers" back={{ label: customer?.name ?? '顧客', to: 'admin-customer-detail' }}>
      <div className="flex max-w-3xl flex-col gap-4">
        <Progress at={check === 'ok' ? 2 : 1} />
        <Step n={1} title="シートの URL" done={check === 'ok'}>
          <Field label="スプレッドシートの URL" error={urlError || undefined}>
            <TextInput value={url} onChange={(e) => { setUrl(e.target.value); setCheck('idle'); setUrlError(''); }} placeholder="https://docs.google.com/spreadsheets/d/..." />
          </Field>
          <div className="mt-3 flex items-center gap-3">
            <Button variant="primary" size="sm" disabled={!url.trim() || check === 'checking'} onClick={run}>{check === 'checking' ? '確認中' : 'つながりを確認'}</Button>
            {check === 'ok' && <span className="text-[13px] text-pt-ok">つながった。シートを読み込める。</span>}
            {check === 'not_found' && <span className="text-[13px] text-pt-danger">シートが見つからない。URL と、削除されていないかを確かめる。</span>}
          </div>
          {check === 'not_shared' && <div className="mt-3"><ShareHint onRecheck={run} checking={false} /></div>}
        </Step>

        {check === 'ok' && (
          <Step n={2} title="タブと見出しの行">
            <div className="grid max-w-xl grid-cols-2 gap-4">
              <Field label="使うタブ">
                <Select value={tab} onChange={(e) => { setTab(e.target.value); setTabName(taken(e.target.value) ? '' : e.target.value); }}>
                  {FOUND_TABS.map((t) => <option key={t} value={t}>{t}</option>)}
                </Select>
              </Field>
              <Field label="顧客に見せるタブの名前" error={sameName ? '同じ名前のタブがある' : undefined}>
                <TextInput value={tabName} onChange={(e) => { setTabName(e.target.value); }} placeholder="例: 2026年度" />
              </Field>
            </div>
            {duplicate && <p className="mt-2 text-[13px] text-pt-warn">このシートのこのタブは、すでに登録されている。</p>}

            <p className="mt-5 mb-1.5 text-[13px] font-medium">見出しの行</p>
            <p className="mb-2 text-xs text-pt-muted">先頭 5 行。項目名が並んでいる行を選ぶ。</p>
            <div className="relative">
            <div className="overflow-x-auto rounded-md border border-pt-line">
              <table className="w-max min-w-full text-xs">
                <tbody>
                  {TOP_ROWS.map((cells, i) => {
                    const row = i + 1;
                    const on = row === headerRow;
                    const isExample = row > headerRow && row <= headerRow + exampleRows;
                    const sample = !isExample && row > headerRow + exampleRows && row <= headerRow + exampleRows + 2;
                    return (
                      <tr
                        key={row}
                        onClick={() => { setHeaderRow(row); }}
                        className={cx('cursor-pointer border-t border-pt-line first:border-t-0', on ? 'bg-pt-primary-soft font-medium' : 'hover:bg-pt-bg', (sample || isExample) && 'text-pt-faint')}
                      >
                        <td className="sticky left-0 bg-inherit px-2 py-1.5 text-pt-muted">
                          <label className="flex items-center gap-1.5 whitespace-nowrap">
                            <input type="radio" name="headerRow" checked={on} onChange={() => { setHeaderRow(row); }} />
                            <span className="pt-num">{String(row)}</span> 行目
                            {isExample && <span className="rounded bg-pt-sunken px-1 py-px text-[12px] text-pt-muted">記入例</span>}
                          </label>
                        </td>
                        {cells.map((c, j) => <td key={String(j) + c} className="border-l border-pt-line px-2 py-1.5 whitespace-nowrap">{c || '　'}</td>)}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <div className="pointer-events-none absolute inset-y-0 right-0 w-10 rounded-r-md bg-gradient-to-l from-pt-surface to-transparent" aria-hidden="true" />
            </div>
            <p className="mt-1 text-right text-[12px] text-pt-faint">横に動かせる</p>
            <Field label="記入例の行">
              <Select value={String(exampleRows)} onChange={(e) => { setExampleRows(Number(e.target.value)); }} className="w-56">
                <option value="0">なし（見出しの次の行から応募者）</option>
                <option value="1">見出しの下の 1 行</option>
                <option value="2">見出しの下の 2 行</option>
              </Select>
            </Field>
            <p className="mt-1 text-xs text-pt-muted">記入例の行は応募者ではないので、一覧・数字・記録に出さない。</p>
            {!headerOk
              ? <p className="mt-2 text-[13px] text-pt-danger">この行は見出しに見えない。項目名が並んでいる行を選び直す。</p>
              : <p className="mt-2 text-xs text-pt-muted"><span className="pt-num">{String(headers.length)}</span> 項目を読み込む。</p>}
            {headerOk && (
              <div className="mt-5">
                <p className="mb-1 text-[13px] font-medium">面接の日と時間の列</p>
                <p className="mb-2 text-xs text-pt-muted">
                  {interviewDate
                    ? `面接の日：${interviewDate}${interviewTime ? `／時間：${interviewTime}` : ''}。見出しから当てた。違えば選び直す。`
                    : '見出しから当てられなかった。面接の日が入る列を選ぶ。'}
                </p>
                <div className="grid max-w-xl grid-cols-2 gap-4">
                  <Field label="面接の日">
                    <Select value={interviewDate} onChange={(e) => { setPickedDate(e.target.value); }}>
                      <option value="">なし</option>
                      {headers.map((h) => <option key={h} value={h}>{h}</option>)}
                    </Select>
                  </Field>
                  <Field label="面接の時間">
                    <Select value={interviewTime} onChange={(e) => { setPickedTime(e.target.value); }}>
                      <option value="">なし</option>
                      {headers.map((h) => <option key={h} value={h}>{h}</option>)}
                    </Select>
                  </Field>
                </div>
              </div>
            )}
          </Step>
        )}

        {check === 'ok' && (
          <div className="flex items-center gap-3">
            <Button variant="primary" disabled={blockReason !== ''} onClick={next}>追加して、項目とプルダウンを設定する</Button>
            <Button variant="ghost" onClick={() => { go('admin-customer-detail'); }}>やめる</Button>
            {blockReason !== '' && <p className="text-[13px] text-pt-warn">{blockReason}</p>}
          </div>
        )}
      </div>
    </AdminPage>
  );
}
