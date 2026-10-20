import { useEffect, useRef, useState, type ReactNode } from 'react';
import { sheetsOf, useData } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { Badge, Button, Dialog, IconButton, TextInput, cx, formatShortDateTime } from '../../ui/kit';
import { IconEye, IconExternal, IconPlus, IconSheet } from '../../ui/icons';
import type { CustomerStage, SheetConfig } from '../../types';
import { AccountStatusBadge } from '../accounts/shared';
import { entrySentence } from '../accounts/activityView';
import { setActivityAccount, setActivityCustomer } from '../accounts/store';
import { IconPencil } from './icons';
import { ShareHint } from './ShareHint';
import { AdminPage, MoreMenu, StatusDot, Switch, TD, TH, Table } from './shared';
import { CONNECTION_LABEL, CONNECTION_TONE, STAGE_LABEL, STAGE_MEANING, STAGE_TONE, actions, enterAs, stageBlockReason, useAdminState } from './state';

const STAGES: CustomerStage[] = ['sheet_only', 'both', 'portal_only'];

function SectionTitle({ children, right }: { children: string; right?: ReactNode }) {
  return (
    <div className="mb-2 flex items-center gap-2 border-b border-pt-line pb-1.5">
      <h2 className="text-[13px] font-semibold">{children}</h2>
      {right && <div className="ml-auto flex items-center gap-1">{right}</div>}
    </div>
  );
}

export function CustomerDetail() {
  const { go } = usePreviewNav();
  const d = useData();
  const admin = useAdminState();
  const customer = d.customers.find((c) => c.id === admin.customerId);
  const [editingName, setEditingName] = useState(false);
  const [nameDraft, setNameDraft] = useState('');
  const [pending, setPending] = useState<CustomerStage | null>(null);
  const [shareChecked, setShareChecked] = useState(false);
  const [removing, setRemoving] = useState<SheetConfig | null>(null);
  const [stopping, setStopping] = useState(false);
  const [rechecking, setRechecking] = useState<string | null>(null);
  const [notes, setNotes] = useState<Record<string, string>>({});
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => { window.clearTimeout(timer.current); }, []);

  if (!customer) {
    return <AdminPage title="顧客の詳細" current="customers" back={{ label: '顧客とシート', to: ROUTES.adminCustomers }}><p className="text-[13px] text-pt-muted">顧客が選ばれていません。</p></AdminPage>;
  }
  const stopped = customer.disabled;
  const mine = sheetsOf(d, customer.id);
  const accts = d.accounts.filter((a) => a.customerId === customer.id);
  const recent = d.activity.filter((e) => e.customerId === customer.id).slice(0, 5);
  const block = stageBlockReason(mine);

  const startEdit = (): void => { setNameDraft(customer.name); setEditingName(true); };
  const commitName = (): void => {
    const n = nameDraft.trim();
    if (n !== '' && n !== customer.name) actions.renameCustomer(customer.id, n);
    setEditingName(false);
  };
  const choose = (st: CustomerStage): void => {
    if (st === customer.stage || stopped) return;
    if (st === 'portal_only' || customer.stage === 'portal_only') {
      setShareChecked(false);
      setPending(st);
    } else {
      actions.setStage(customer.id, st);
    }
  };
  const recheck = (sh: SheetConfig): void => {
    setRechecking(sh.id);
    setNotes((n) => ({ ...n, [sh.id]: '' }));
    timer.current = window.setTimeout(() => {
      setRechecking(null);
      if (sh.connection === 'not_shared') actions.setConnection(sh, 'ok');
      else setNotes((n) => ({ ...n, [sh.id]: sh.connection === 'not_found' ? 'シートがまだ見つからない。URL と削除の有無を確かめる。' : '見出しが変わっている。項目とプルダウンで、増えた項目と見つからない項目を確かめる。' }));
    }, 700);
  };
  const openColumns = (sh: SheetConfig): void => {
    enterAs(customer.id, sh.id);
    go(ROUTES.columns);
  };
  const view = (sh?: SheetConfig): void => {
    enterAs(customer.id, sh?.id);
    go(ROUTES.list);
  };

  const title = editingName ? (
    <span className="flex items-center gap-1.5">
      <TextInput
        autoFocus
        aria-label="顧客名"
        value={nameDraft}
        onChange={(e) => { setNameDraft(e.target.value); }}
        onKeyDown={(e) => { if (e.key === 'Enter') commitName(); if (e.key === 'Escape') setEditingName(false); }}
        className="!h-8 !w-72 !text-[15px]"
      />
      <Button size="sm" variant="primary" disabled={nameDraft.trim() === ''} onClick={commitName}>保存</Button>
      <Button size="sm" variant="ghost" onClick={() => { setEditingName(false); }}>やめる</Button>
    </span>
  ) : (
    <span className="inline-flex items-center gap-1">
      {customer.name}
      <IconButton label="顧客名を変更" className="!h-7 !w-7" onClick={startEdit}><IconPencil size={14} /></IconButton>
    </span>
  );

  return (
    <AdminPage
      title={title}
      {...(stopped ? { titleBadge: <Badge tone="neutral">停止中</Badge> } : {})}
      current="customers"
      back={{ label: '顧客とシート', to: ROUTES.adminCustomers }}
      actions={
        <>
          <Button size="sm" disabled={mine.length === 0} onClick={() => { view(); }}><IconEye size={14} />この顧客の画面を見る</Button>
          <MoreMenu items={[stopped ? { label: '再開する', onSelect: () => { actions.setDisabled(customer.id, false); } } : { label: 'この顧客を停止', danger: true, onSelect: () => { setStopping(true); } }]} />
        </>
      }
    >
      {stopped && (
        <p className="-mt-2 mb-4 flex items-center gap-3 text-[13px] text-pt-muted">
          停止中。この顧客の人はログインできない。
          <Button size="sm" onClick={() => { actions.setDisabled(customer.id, false); }}>再開する</Button>
        </p>
      )}
      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="flex min-w-0 flex-col gap-6">
          <section>
            <SectionTitle right={<Button size="sm" variant="primary" disabled={stopped} onClick={() => { actions.selectCustomer(customer.id); go('admin-sheet-add'); }}><IconPlus size={14} />シートを追加</Button>}>
              {`シート（${String(mine.length)} 枚）`}
            </SectionTitle>
            {mine.length === 0 ? (
              <p className="rounded-lg border border-dashed border-pt-line-strong py-8 text-center text-[13px] text-pt-muted">シートはまだ登録されていない</p>
            ) : (
              <Table minWidth={820}>
                <thead>
                  <tr>
                    <th className={TH}>タブ</th>
                    <th className={TH}>つながり</th>
                    <th className={TH + ' text-right'}>応募者</th>
                    <th className={TH + ' text-right'}>画面に出す項目</th>
                    <th className={TH}>顧客も項目を変更</th>
                    <th className={TH} />
                  </tr>
                </thead>
                <tbody>
                  {mine.map((sh) => {
                    const shown = sh.columns.filter((c) => c.visible).length;
                    const url = admin.sheetUrls[sh.id];
                    return (
                      <tr key={sh.id} className={cx('group', sh.id === d.currentSheetId && 'bg-pt-primary-soft/40')}>
                        <td className={TD}>
                          <span className="font-medium">{sh.tabName}</span>
                          <span className="flex items-center gap-1 text-[11px] text-pt-faint">
                            <IconSheet size={11} />
                            <span className="max-w-56 truncate">{admin.sheetFiles[sh.id] ?? ''}</span>
                            {url && (
                              <a href={url} target="_blank" rel="noreferrer" aria-label="スプレッドシートを開く" title="スプレッドシートを開く" className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:text-pt-primary"><IconExternal size={11} /></a>
                            )}
                          </span>
                        </td>
                        <td className={TD}>
                          <StatusDot tone={CONNECTION_TONE[sh.connection]}>{CONNECTION_LABEL[sh.connection]}</StatusDot>
                          {notes[sh.id] && <p className="mt-0.5 max-w-60 text-xs text-pt-muted">{notes[sh.id]}</p>}
                        </td>
                        <td className={TD + ' pt-num text-right'}>{String(d.rows[sh.id]?.length ?? 0)}</td>
                        <td className={TD + ' text-right whitespace-nowrap'}><span className="pt-num">{String(shown)}</span> 項目<span className="pt-num text-pt-faint">（全 {String(sh.columns.length)}）</span></td>
                        <td className={TD}>
                          <span className="inline-flex items-center gap-2">
                            <Switch on={sh.customerCanEditColumns === true} label={`${sh.tabName}の項目を顧客も変更できる`} onChange={(v) => { actions.setCustomerCanEdit(sh, v); }} />
                            <span className="text-xs text-pt-muted">{sh.customerCanEditColumns === true ? '許可' : '社内のみ'}</span>
                          </span>
                        </td>
                        <td className={TD + ' text-right whitespace-nowrap'}>
                          <Button size="sm" variant="ghost" onClick={() => { openColumns(sh); }}>項目とプルダウン</Button>
                          <MoreMenu
                            items={[
                              { label: 'この画面を開く', onSelect: () => { view(sh); } },
                              ...(sh.connection !== 'ok' ? [{ label: rechecking === sh.id ? '確認中' : '確かめ直す', disabled: rechecking === sh.id, onSelect: () => { recheck(sh); } }] : []),
                              { label: 'シートを外す', danger: true, onSelect: () => { setRemoving(sh); } },
                            ]}
                          />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </Table>
            )}
            {mine.some((s) => s.connection === 'not_shared') && (
              <div className="mt-3">
                <ShareHint
                  checking={rechecking !== null}
                  onRecheck={() => { const t = mine.find((s) => s.connection === 'not_shared'); if (t) recheck(t); }}
                />
              </div>
            )}
          </section>

          <section>
            <SectionTitle
              right={
                <>
                  <Button size="sm" variant="ghost" onClick={() => { setActivityAccount(''); actions.setAccountsFilter(customer.id); go(ROUTES.adminAccounts); }}>すべて見る</Button>
                  <Button size="sm" variant="ghost" disabled={stopped} onClick={() => { actions.setInviteCustomer(customer.id); go('admin-invite'); }}>招待</Button>
                </>
              }
            >
              {`アカウント（${String(accts.length)}）`}
            </SectionTitle>
            {accts.length === 0 ? (
              <p className="text-xs text-pt-muted">アカウントはまだない。</p>
            ) : (
              <Table minWidth={640}>
                <thead>
                  <tr>
                    <th className={TH}>アカウント</th>
                    <th className={TH}>状態</th>
                    <th className={TH}>最後のログイン</th>
                  </tr>
                </thead>
                <tbody>
                  {accts.map((a) => (
                    <tr key={a.id}>
                      <td className={TD}>
                        <span className="font-medium">{a.displayName !== '' ? a.displayName : a.email}</span>
                        {a.displayName !== '' && <span className="ml-2 text-xs text-pt-faint">{a.email}</span>}
                      </td>
                      <td className={TD}><AccountStatusBadge account={a} /></td>
                      <td className={TD + ' pt-num text-pt-muted'}>{a.lastLoginAt ? formatShortDateTime(a.lastLoginAt) : <span className="text-pt-faint">まだない</span>}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            )}
          </section>
        </div>

        <aside className="flex flex-col gap-6">
          <section>
            <SectionTitle>使い方の段階</SectionTitle>
            <div role="radiogroup" aria-label="使い方の段階" className="inline-flex rounded-md border border-pt-line bg-pt-sunken p-0.5">
              {STAGES.map((st) => {
                const current = customer.stage === st;
                const locked = stopped || (st !== 'sheet_only' && block !== null && !current);
                return (
                  <button
                    key={st}
                    type="button"
                    role="radio"
                    aria-checked={current}
                    disabled={locked}
                    onClick={() => { choose(st); }}
                    className={cx('h-7 rounded px-3 text-[13px] font-medium whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-45', current ? 'bg-pt-surface text-pt-ink shadow-pt-card' : 'text-pt-muted hover:text-pt-ink')}
                  >
                    {STAGE_LABEL[st]}
                  </button>
                );
              })}
            </div>
            <p className="mt-2 text-xs text-pt-muted"><Badge tone={STAGE_TONE[customer.stage]}>{STAGE_LABEL[customer.stage]}</Badge> {STAGE_MEANING[customer.stage]}</p>
            {block && !stopped && <p className="mt-1.5 text-xs text-pt-warn">「両方」「画面だけ」は選べない。{block}</p>}
            <p className="mt-1.5 text-xs text-pt-faint">最後に使われた {customer.lastUsedAt ? <span className="pt-num">{formatShortDateTime(customer.lastUsedAt)}</span> : 'まだ使われていない'}</p>
          </section>

          <section>
            <SectionTitle right={<Button size="sm" variant="ghost" onClick={() => { setActivityAccount(''); setActivityCustomer(customer.id); go('admin-activity'); }}>すべて見る</Button>}>最近の記録</SectionTitle>
            {recent.length === 0 ? (
              <p className="text-xs text-pt-muted">記録はまだない。</p>
            ) : (
              <ul className="divide-y divide-pt-line">
                {recent.map((e) => (
                  <li key={e.id} className="flex gap-3 py-1.5 text-[13px]">
                    <span className="pt-num w-24 shrink-0 text-xs leading-5 text-pt-muted">{formatShortDateTime(e.at)}</span>
                    <span className="min-w-0 flex-1 [overflow-wrap:anywhere]">{entrySentence(d, e)}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </aside>
      </div>

      <Dialog
        open={pending !== null}
        width="sm"
        title={pending ? `段階を「${STAGE_LABEL[pending]}」に変更` : ''}
        onClose={() => { setPending(null); }}
        footer={
          <>
            <Button variant="ghost" onClick={() => { setPending(null); }}>やめる</Button>
            <Button variant="primary" disabled={!shareChecked} onClick={() => { if (pending) actions.setStage(customer.id, pending); setPending(null); }}>変更する</Button>
          </>
        }
      >
        <p className="text-[13px]">{pending === 'portal_only' ? '顧客はシートを使えなくなる。' : '顧客がシートも使えるようになる。'}</p>
        <label className="mt-3 flex items-center gap-2 text-[13px]">
          <input type="checkbox" checked={shareChecked} onChange={(e) => { setShareChecked(e.target.checked); }} className="h-4 w-4" />
          {pending === 'portal_only' ? 'この顧客へのシートの共有を外した' : 'この顧客へのシートの共有を戻した'}
        </label>
      </Dialog>

      <Dialog
        open={removing !== null}
        width="sm"
        title="シートを外す"
        onClose={() => { setRemoving(null); }}
        footer={
          <>
            <Button variant="ghost" onClick={() => { setRemoving(null); }}>やめる</Button>
            <Button variant="danger" onClick={() => { if (removing) actions.removeSheet(removing); setRemoving(null); }}>外す</Button>
          </>
        }
      >
        <p className="text-[13px]">「{removing?.tabName}」を画面から外す。シートの中身は消えない。顧客の画面には出なくなる。</p>
      </Dialog>

      <Dialog
        open={stopping}
        width="sm"
        title="この顧客を停止"
        onClose={() => { setStopping(false); }}
        footer={
          <>
            <Button variant="ghost" onClick={() => { setStopping(false); }}>やめる</Button>
            <Button variant="danger" onClick={() => { actions.setDisabled(customer.id, true); setStopping(false); }}>停止する</Button>
          </>
        }
      >
        <p className="text-[13px]">この顧客のアカウントは、すぐにログインできなくなる。再開はいつでもできる。</p>
      </Dialog>
    </AdminPage>
  );
}
