import { useEffect, useRef, useState } from 'react';
import { AdminLayout } from '../../ui/layouts';
import { Badge, Button, Card, Field, Notice, TextInput, cx, formatShortDateTime } from '../../ui/kit';
import { ROUTES, usePreviewNav } from '../../preview';
import type { CustomerStage, SheetConfig } from '../../types';
import { ShareHint } from './ShareHint';
import { CONNECTION_LABEL, CONNECTION_TONE, STAGE_LABEL, STAGE_MEANING, STAGE_TONE, actions, isReady, stageBlockReason, useAdminState } from './state';

const STAGES: CustomerStage[] = ['sheet_only', 'both', 'portal_only'];
type Msg = { tone: 'ok' | 'danger'; text: string } | null;

function Result({ msg }: { msg: Msg }) {
  if (!msg) return null;
  return <div className="mt-3"><Notice tone={msg.tone}>{msg.text}</Notice></div>;
}

export function CustomerDetail() {
  const { go } = usePreviewNav();
  const s = useAdminState();
  const customer = s.customers.find((c) => c.id === s.customerId);
  const [pending, setPending] = useState<CustomerStage | null>(null);
  const [checked, setChecked] = useState(false);
  const [confirmStop, setConfirmStop] = useState(false);
  const [name, setName] = useState(customer?.name ?? '');
  const [nameMsg, setNameMsg] = useState<Msg>(null);
  const [nameSaving, setNameSaving] = useState(false);
  const [stageMsg, setStageMsg] = useState<Msg>(null);
  const [stopMsg, setStopMsg] = useState<Msg>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [menu, setMenu] = useState<string | null>(null);
  const [rechecking, setRechecking] = useState<string | null>(null);
  const [recheckMsg, setRecheckMsg] = useState<Record<string, string>>({});
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => { window.clearTimeout(timer.current); }, []);

  const nav = (to: 'customers' | 'accounts') => { go(to === 'customers' ? ROUTES.adminCustomers : ROUTES.adminAccounts); };
  if (!customer) {
    return <AdminLayout title="顧客の詳細" current="customers" onNavigate={nav}><p>顧客が選ばれていません。</p></AdminLayout>;
  }
  const mine = s.sheets.filter((sh) => sh.customerId === customer.id);
  const accts = s.accounts.filter((a) => a.customerId === customer.id);
  const block = stageBlockReason(mine);

  const saveName = () => {
    setNameSaving(true);
    setNameMsg(null);
    timer.current = window.setTimeout(() => {
      setNameSaving(false);
      if (name.includes('失敗')) {
        setNameMsg({ tone: 'danger', text: '保存できませんでした。もう一度押してください。入力した名前はそのまま残っています。' });
        return;
      }
      actions.updateCustomer(customer.id, { name: name.trim() });
      setNameMsg({ tone: 'ok', text: '顧客名を保存しました。' });
    }, 500);
  };

  const choose = (st: CustomerStage) => {
    if (st === customer.stage) { setPending(null); return; }
    setStageMsg(null);
    setChecked(false);
    setPending(st);
  };
  const needsCheck = pending !== null && (pending === 'portal_only' || customer.stage === 'portal_only');
  const applyStage = () => {
    if (!pending) return;
    actions.setStage(customer.id, pending);
    setStageMsg({ tone: 'ok', text: '段階を「' + STAGE_LABEL[pending] + '」に変更しました。' });
    setPending(null);
  };

  const recheck = (sh: SheetConfig) => {
    setRechecking(sh.id);
    setRecheckMsg((m) => ({ ...m, [sh.id]: '' }));
    timer.current = window.setTimeout(() => {
      setRechecking(null);
      if (sh.connection === 'not_shared') {
        actions.updateSheet(sh.id, { connection: 'ok' });
      } else if (sh.connection === 'not_found') {
        setRecheckMsg((m) => ({ ...m, [sh.id]: 'シートがまだ見つかりません。URL とシートの削除の有無を確かめてください。' }));
      } else {
        setRecheckMsg((m) => ({ ...m, [sh.id]: '見出しが変わっています。列の設定で、増えた列と見つからない列を確かめてください。' }));
      }
    }, 700);
  };

  return (
    <AdminLayout title="顧客の詳細" current="customers" onNavigate={nav}>
      <button type="button" className="mb-3 text-sm text-pt-primary hover:underline" onClick={() => { go(ROUTES.adminCustomers); }}>顧客の一覧に戻る</button>

      <div className="grid grid-cols-[1fr_340px] gap-5">
        <div className="flex flex-col gap-5">
          <Card className="p-5">
            <div className="flex items-end gap-3">
              <div className="w-96"><Field label="顧客名"><TextInput value={name} onChange={(e) => { setName(e.target.value); setNameMsg(null); }} /></Field></div>
              <Button disabled={!name.trim() || name === customer.name || nameSaving} onClick={saveName}>{nameSaving ? '保存しています' : '名前を保存'}</Button>
              {customer.disabled && <Badge tone="danger">停止中</Badge>}
            </div>
            <p className="mt-3 text-sm text-pt-muted">最後に使われた日時: {customer.lastUsedAt ? formatShortDateTime(customer.lastUsedAt) : 'まだ使われていません'}</p>
            <Result msg={nameMsg} />
          </Card>

          <Card className="p-5">
            <h2 className="mb-3 text-base font-semibold">段階</h2>
            <div className="grid grid-cols-3 gap-3">
              {STAGES.map((st) => {
                const current = customer.stage === st;
                const selected = pending === st;
                const locked = st !== 'sheet_only' && block !== null && !current;
                return (
                  <button
                    key={st}
                    type="button"
                    disabled={locked}
                    onClick={() => { choose(st); }}
                    aria-pressed={selected || (current && pending === null)}
                    className={cx('flex flex-col items-start justify-start rounded-lg border-2 p-3 text-left disabled:cursor-not-allowed disabled:opacity-50', selected ? 'border-pt-primary bg-pt-surface' : current ? 'border-pt-line-strong bg-pt-primary-soft' : 'border-pt-line hover:bg-pt-bg')}
                  >
                    <Badge tone={STAGE_TONE[st]}>{STAGE_LABEL[st]}</Badge>
                    <p className="mt-2 text-xs leading-relaxed text-pt-muted">{STAGE_MEANING[st]}</p>
                    {current && <p className="mt-2 text-xs font-medium text-pt-primary">現在の段階</p>}
                    {selected && <p className="mt-2 text-xs font-medium text-pt-primary">この段階に変更します</p>}
                  </button>
                );
              })}
            </div>
            {block && <p className="mt-3 text-sm text-pt-warn">「シートと画面の両方」「画面だけ」は選べません。{block}</p>}
            {pending && (
              <div className="mt-4">
                <Notice tone="warn" title={'段階を「' + STAGE_LABEL[pending] + '」に変更します'}>
                  {pending === 'sheet_only' && <p>顧客は画面にログインしても応募者を見られなくなります。</p>}
                  {pending === 'both' && customer.stage === 'sheet_only' && <p>顧客はシートに加えて、画面でも応募者を見て書けるようになります。</p>}
                  {pending === 'both' && customer.stage === 'portal_only' && <p>顧客がシートも使えるようになります。</p>}
                  {pending === 'portal_only' && <p>顧客はシートを使えなくなります。</p>}
                  {needsCheck && (
                    <label className="mt-2 flex items-center gap-2">
                      <input type="checkbox" checked={checked} onChange={(e) => { setChecked(e.target.checked); }} className="h-4 w-4" />
                      {pending === 'portal_only' ? 'この顧客へのシートの共有を外しました' : 'この顧客へのシートの共有を戻しました'}
                    </label>
                  )}
                  <div className="mt-3 flex gap-2">
                    <Button variant="primary" size="sm" disabled={needsCheck && !checked} onClick={applyStage}>変更する</Button>
                    <Button variant="ghost" size="sm" onClick={() => { setPending(null); }}>やめる</Button>
                  </div>
                </Notice>
              </div>
            )}
            <Result msg={stageMsg} />
          </Card>

          <Card className="overflow-hidden">
            <div className="flex items-center justify-between p-5 pb-3">
              <h2 className="text-base font-semibold">登録したシート</h2>
              <Button variant="primary" size="sm" onClick={() => { go('admin-sheet-add'); }}>シートの追加</Button>
            </div>
            <table className="w-full text-sm">
              <thead className="bg-pt-bg text-left text-xs text-pt-muted">
                <tr><th className="px-5 py-2 font-medium">タブ名</th><th className="px-3 py-2 font-medium">つながり</th><th className="px-3 py-2 font-medium">列の数</th><th className="px-5 py-2" /></tr>
              </thead>
              <tbody>
                {mine.map((sh) => (
                  <tr key={sh.id} className="border-t border-pt-line align-top">
                    <td className="whitespace-nowrap px-5 py-3 font-medium">{sh.tabName}</td>
                    <td className="px-3 py-3">
                      <Badge tone={CONNECTION_TONE[sh.connection]}>{CONNECTION_LABEL[sh.connection]}</Badge>
                      {sh.connection === 'ok' && !isReady(sh) && <div className="mt-1 text-xs text-pt-warn">列の設定が済んでいません</div>}
                      {recheckMsg[sh.id] && <div className="mt-1 text-xs text-pt-danger">{recheckMsg[sh.id]}</div>}
                      {sh.connection === 'not_shared' && <div className="mt-2 max-w-md"><ShareHint onRecheck={() => { recheck(sh); }} checking={rechecking === sh.id} /></div>}
                    </td>
                    <td className="px-3 py-3">{sh.columns.length > 0 ? String(sh.columns.length) + ' 列（顧客に見せるのは ' + String(sh.columns.filter((c) => c.visible).length) + ' 列）' : '未設定'}</td>
                    <td className="px-5 py-3 text-right">
                      {removing === sh.id ? (
                        <div className="flex flex-col items-end gap-2">
                          <span className="text-xs text-pt-muted">このシートを外します。シート自体は消えません。</span>
                          <span className="flex gap-2">
                            <Button variant="danger" size="sm" onClick={() => { actions.removeSheet(sh.id); setRemoving(null); }}>外す</Button>
                            <Button variant="ghost" size="sm" onClick={() => { setRemoving(null); }}>やめる</Button>
                          </span>
                        </div>
                      ) : (
                        <div className="flex flex-nowrap items-center justify-end gap-2">
                          {sh.connection !== 'ok' && sh.connection !== 'not_shared' && (
                            <Button size="sm" disabled={rechecking === sh.id} onClick={() => { recheck(sh); }}>{rechecking === sh.id ? '確認しています' : 'つながりを確かめ直す'}</Button>
                          )}
                          <Button size="sm" onClick={() => { actions.selectSheet(sh.id); go('admin-sheet-columns'); }}>列の設定</Button>
                          <div className="relative">
                            <Button size="sm" aria-expanded={menu === sh.id} onClick={() => { setMenu(menu === sh.id ? null : sh.id); }}>その他</Button>
                            {menu === sh.id && (
                              <div className="absolute right-0 top-9 z-10 flex w-44 flex-col rounded-md border border-pt-line bg-pt-surface py-1 text-left shadow-md">
                                <a href="https://docs.google.com/spreadsheets/" target="_blank" rel="noreferrer" className="px-3 py-2 text-sm text-pt-primary hover:bg-pt-bg">シートを開く</a>
                                <button type="button" className="px-3 py-2 text-left text-sm text-pt-danger hover:bg-pt-bg" onClick={() => { setMenu(null); setRemoving(sh.id); }}>このシートを外す</button>
                              </div>
                            )}
                          </div>
                        </div>
                      )}
                    </td>
                  </tr>
                ))}
                {mine.length === 0 && <tr><td colSpan={4} className="px-5 py-8 text-center text-pt-muted">まだシートが登録されていません。</td></tr>}
              </tbody>
            </table>
          </Card>
        </div>

        <div className="flex flex-col gap-5">
          <Card className="p-5">
            <h2 className="mb-2 text-base font-semibold">アカウント</h2>
            <p className="text-sm text-pt-muted">使える {accts.filter((a) => a.status !== 'disabled').length} 件 ／ 使えない {accts.filter((a) => a.status === 'disabled').length} 件</p>
            <ul className="mt-3 flex flex-col divide-y divide-pt-line">
              {accts.slice(0, 3).map((a) => (
                <li key={a.id} className="py-2 text-sm">
                  <div className="font-medium">{a.displayName || a.email}</div>
                  {a.displayName && <div className="text-xs text-pt-muted">{a.email}</div>}
                  <div className="text-xs text-pt-faint">{a.lastLoginAt ? '最後のログイン ' + formatShortDateTime(a.lastLoginAt) : 'まだログインしていません'}</div>
                </li>
              ))}
              {accts.length === 0 && <li className="py-2 text-sm text-pt-muted">アカウントはまだありません。</li>}
            </ul>
            <div className="mt-3"><Button onClick={() => { actions.setAccountsFilter(customer.id); go(ROUTES.adminAccounts); }}>{accts.length > 3 ? 'すべて見る' : 'アカウントの画面へ'}</Button></div>
          </Card>
          <Card className="p-5">
            <h2 className="mb-2 text-base font-semibold">利用の停止</h2>
            {customer.disabled ? (
              <>
                <p className="mb-3 text-sm text-pt-muted">この顧客のアカウントは、いまログインできません。</p>
                <Button onClick={() => { actions.updateCustomer(customer.id, { disabled: false }); setStopMsg({ tone: 'ok', text: '停止を解除しました。' }); }}>停止を解除する</Button>
              </>
            ) : confirmStop ? (
              <Notice tone="warn" title="この顧客を停止します">
                <p>この顧客のアカウントは、すべてログインできなくなります。</p>
                <div className="mt-3 flex gap-2">
                  <Button variant="danger" size="sm" onClick={() => { actions.updateCustomer(customer.id, { disabled: true }); setConfirmStop(false); setStopMsg({ tone: 'ok', text: 'この顧客を停止しました。' }); }}>停止する</Button>
                  <Button variant="ghost" size="sm" onClick={() => { setConfirmStop(false); }}>やめる</Button>
                </div>
              </Notice>
            ) : (
              <>
                <p className="mb-3 text-sm text-pt-muted">契約の終了や、一時的に止めたいときに使います。</p>
                <Button variant="danger" onClick={() => { setStopMsg(null); setConfirmStop(true); }}>この顧客を停止する</Button>
              </>
            )}
            <Result msg={stopMsg} />
          </Card>
        </div>
      </div>
    </AdminLayout>
  );
}
