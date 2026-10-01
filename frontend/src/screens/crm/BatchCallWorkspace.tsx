import { Fragment, useEffect, useRef, useState } from 'react';
import { ContactProperties, MocPropertyInput } from './ContactProperties';
import { propertyDraftKey, mockDealId } from './mocProperties';
import type { CrmMetadataResponse } from '../../generated/CrmMetadataResponse';
import { CrmMetadataContext, metadataDealDefinitions } from './liveMetadata';
import { CrmMetadataPanel } from './CrmMetadataPanel';
import { SAMPLE_RECORDS } from './fixtures';
import { filterActivities, formatDate } from './model';
import { CALL_RESULTS, durationLabel, nextContact } from './callModel';
import type { CallResult } from './callModel';
import { BATCH_CONTACTS, emptyBatchDraft, emptyBatchFilters, filterBatchContacts, validateBatchDraft } from './batchModel';
import type { BatchDraft } from './batchModel';
import './call.css';
import './batch-call.css';

const contacts = BATCH_CONTACTS;

export function BatchCallWorkspace() {
  const [metadata, setMetadata] = useState<CrmMetadataResponse | null>(null);
  const definitions = metadataDealDefinitions(metadata);
  const [drafts, setDrafts] = useState<Record<string, BatchDraft>>({});
  const [entries, setEntries] = useState<Record<string, BatchDraft & { duration: number; propertyChanges: Record<string, string> }>>({});
  const [filters, setFilters] = useState(emptyBatchFilters);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [propertyDrafts, setPropertyDrafts] = useState<Record<string, string>>(() => {
    const stopped = contacts[contacts.length - 1];
    return stopped ? { [propertyDraftKey(stopped, 'bpo_3')]: '架電停止の希望（架空のサンプル）' } : {};
  });
  const [skipped, setSkipped] = useState<string[]>([]);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [activeCall, setActiveCall] = useState<{ id: string; startedAt: number } | null>(null);
  const [durations, setDurations] = useState<Record<string, number>>({});
  const [seconds, setSeconds] = useState(0);
  const [nextId, setNextId] = useState('sample-contact-1');
  const [message, setMessage] = useState('');
  const callButtons = useRef<Record<string, HTMLButtonElement | null>>({});
  const completed = Object.keys(entries);
  const callableContacts = contacts.filter(record => (propertyDrafts[`${record.id}:phone`] ?? record.properties.find(p => p.name === 'phone')?.value)?.trim()
    && !propertyDrafts[propertyDraftKey(record, 'bpo_3')]?.trim() && drafts[record.id]?.result !== 'do_not_call');
  const visibleContacts = filterBatchContacts(contacts, filters, completed, skipped, propertyDrafts);
  const visibleCallable = visibleContacts.filter(record => callableContacts.includes(record));
  const visiblePending = visibleCallable.filter(record => !completed.includes(record.id) && !skipped.includes(record.id));
  const highlightedId = visiblePending.find(record => record.id === nextId)?.id ?? visiblePending[0]?.id ?? '';
  const remaining = callableContacts.filter(record => !completed.includes(record.id) && !skipped.includes(record.id));
  const hasFilters = Object.values(filters).some(value => value !== '');

  useEffect(() => {
    if (!activeCall) return;
    const timer = window.setInterval(() => { setSeconds(Math.floor((Date.now() - activeCall.startedAt) / 1000)); }, 1000);
    return () => { window.clearInterval(timer); };
  }, [activeCall]);

  function updateDraft(id: string, patch: Partial<BatchDraft>) {
    setDrafts(previous => ({ ...previous, [id]: { ...(previous[id] ?? emptyBatchDraft()), ...patch } }));
    setErrors(previous => ({ ...previous, [id]: '' }));
  }
  function focusNext(id: string, done: string[], deferred: string[]) {
    const next = nextContact(visibleCallable, id, done, deferred);
    setNextId(next?.id ?? '');
    if (next) {
      const button = callButtons.current[next.id];
      button?.focus({ preventScroll: true });
      button?.scrollIntoView({ block: 'nearest', behavior: 'auto' });
    }
  }
  function startCall(id: string, startedAt: number) {
    if (activeCall) return;
    setActiveCall({ id, startedAt }); setSeconds(0); setNextId(id); setMessage('');
  }
  function endCall() {
    if (!activeCall) return;
    const duration = Math.floor((Date.now() - activeCall.startedAt) / 1000);
    setDurations(previous => ({ ...previous, [activeCall.id]: duration }));
    setActiveCall(null);
  }
  function recordDemo(id: string) {
    if (activeCall) return;
    const draft = drafts[id] ?? emptyBatchDraft();
    const record = contacts.find(contact => contact.id === id);
    if (!record) return;
    let error = validateBatchDraft(draft, definitions);
    if (!error && draft.result === 'appointment' && ['bpo_23', 'bpo__', 'bpo_33'].some(name => !propertyDrafts[propertyDraftKey(record, name)])) {
      error = '詳細欄で商談予定日・時間・方法を入力してください（MOCの必須条件）。';
      setExpanded(previous => ({ ...previous, [id]: true }));
    }
    if (!error && draft.result === 'do_not_call' && !propertyDrafts[propertyDraftKey(record, 'bpo_3')]?.trim()) {
      error = '詳細欄で架電禁止理由を入力してください（MOCの必須条件）。';
      setExpanded(previous => ({ ...previous, [id]: true }));
    }
    if (error) { setErrors(previous => ({ ...previous, [id]: error })); return; }
    const relatedCompanyId = record.associations.find(association => association.objectType === 'companies')?.id;
    const propertyChanges = {
      ...Object.fromEntries(Object.entries(propertyDrafts).filter(([key]) =>
        [record.id, mockDealId(record), relatedCompanyId].some(objectId => !!objectId && key.startsWith(`${objectId}:`)))),
      ...Object.fromEntries(Object.entries({ bpo_40: draft.spokeTo, bpo_42: draft.interest, bpo_45: draft.nextAction,
        bpo_13: draft.nextCallDate, bpo_14: draft.nextCallTime, bpo_16: draft.memo }).map(([name, value]) => [propertyDraftKey(record, name), value])),
    };
    setEntries(previous => ({ ...previous, [id]: { ...draft, duration: durations[id] ?? 0, propertyChanges } }));
    setMessage('この行をデモ記録しました。次の対象は現在の絞り込み内から選びます。HubSpot未保存です。');
    focusNext(id, [...completed, id], skipped);
  }

  return <CrmMetadataContext value={metadata}><div className="call-app batch-app">
    <header className="call-header"><a href="/" className="call-brand">HR_HR</a><strong>連続架電ワークスペース</strong><span className="batch-demo-tag">{metadata ? 'API定義使用' : '固定定義使用'} · 架空案件MOC · HubSpot未保存</span>
      <nav className="batch-view-links" aria-label="画面表示"><a href="?view=single">1件ずつの表示</a><a href="?view=reference">基準のCRM画面</a></nav></header>
    <main className="batch-main">
      <CrmMetadataPanel metadata={metadata} onLoaded={setMetadata} disabled={!!activeCall} />
      <details className="batch-controls"><summary><span>絞り込み・集計を開く / 畳む</span><span className="batch-control-count">{visibleContacts.length} / {contacts.length}件{hasFilters ? ' · 絞り込み中' : ''} · 記録 {completed.length}件</span></summary>
      <div className="batch-control-content">
      <div className="call-demo-banner"><strong>操作デモ</strong><span>12件の架空案件です。実HubSpotの項目定義に合わせたMOCで、発信・記録はデモです。HubSpotには保存せず、再読み込みで入力内容は消えます。</span></div>
      <section className="batch-heading"><div><span className="call-eyebrow">CALL WORKLIST</span><h1>一覧のまま、次の電話へ。</h1>
        <p>相手の情報を見ながら、この行で発信・結果・申し送りを完了。</p></div>
        <div className="batch-counts"><div><strong>{contacts.length}</strong><span>架電先</span></div><div><strong>{completed.length}</strong><span>デモ記録</span></div>
          <div><strong>{remaining.length}</strong><span>発信可能・未対応</span></div></div></section>
      <div className="batch-filters">
        <label>会社・担当者を検索<input type="search" value={filters.query} disabled={!!activeCall} placeholder="会社名 / 担当者名"
          onChange={e => { setFilters(previous => ({ ...previous, query: e.target.value })); }} /></label>
        <label>担当オーナー<select value={filters.owner} disabled={!!activeCall} onChange={e => { setFilters(previous => ({ ...previous, owner: e.target.value })); }}>
          <option value="">全員</option>{[...new Set(contacts.map(c => c.owner))].map(owner => <option key={owner}>{owner}</option>)}</select></label>
        <label>対応状態<select value={filters.state} disabled={!!activeCall} onChange={e => { setFilters(previous => ({ ...previous, state: e.target.value })); }}>
          <option value="">すべて</option><option value="pending">未対応</option><option value="done">デモ記録済み</option><option value="skipped">後回し</option></select></label>
        <label>電話番号<select value={filters.phone} disabled={!!activeCall} onChange={e => { setFilters(previous => ({ ...previous, phone: e.target.value })); }}>
          <option value="">すべて</option><option value="yes">番号あり</option><option value="no">番号なし</option></select></label>
        <button disabled={!!activeCall} onClick={() => { setFilters(emptyBatchFilters()); }}>絞り込みを解除</button>
        <span role="status">{visibleContacts.length} / {contacts.length}件を表示</span>
      </div>
      </div></details>
      <div className="batch-call-status" role="status"><span className={activeCall ? 'batch-status-active' : ''}>
        {activeCall ? `${contacts.find(c => c.id === activeCall.id)?.name ?? ''}さん · 通話デモ中 ${durationLabel(seconds)}` : 'Zoom Phone未接続 · 発信デモを利用できます'}</span>
        <span>{activeCall ? '別の顧客への発信を停止中' : '結果を記録すると、次の発信ボタンへ移動します。'}</span></div>
      <div className="batch-table-container" role="region" aria-label="架電一覧（スクロール可能）" tabIndex={0}><table className="batch-table"><caption className="batch-sr-only">複数顧客の架電一覧。各行で発信と結果入力を行います。</caption>
        <colgroup><col className="batch-col-customer" /><col className="batch-col-history" /><col className="batch-col-call" /><col className="batch-col-record" /></colgroup>
        <thead><tr><th scope="col">架電案件・担当者</th><th scope="col">前回の電話・申し送り</th><th scope="col">発信</th><th scope="col">今回の結果・タスクメモ</th></tr></thead>
        <tbody>{visibleContacts.map(record => {
          const index = contacts.indexOf(record);
          const draft = drafts[record.id] ?? emptyBatchDraft();
          const entry = entries[record.id];
          const phone = (propertyDrafts[`${record.id}:phone`] ?? record.properties.find(p => p.name === 'phone')?.value)?.trim();
          const company = record.associations.find(a => a.objectType === 'companies');
          const companyRecord = SAMPLE_RECORDS.find(r => r.objectType === 'companies' && r.id === company?.id);
          const lastCall = filterActivities(record.activities, 'call', '', '', '')[0];
          const handover = filterActivities(record.activities, 'note', '', '', '')[0];
          const isActive = activeCall?.id === record.id;
          const isSkipped = skipped.includes(record.id);
          const enteredStopReason = propertyDrafts[propertyDraftKey(record, 'bpo_3')]?.trim() ?? '';
          const stopReason = enteredStopReason.length > 0 ? enteredStopReason : (draft.result === 'do_not_call' ? '架電停止の希望（MOC入力）' : '');
          const blocked = !!activeCall || !!entry || isSkipped || !phone;
          return <Fragment key={record.id}><tr data-deal-id={mockDealId(record)} className={isActive ? 'batch-row-active' : highlightedId === record.id ? 'batch-row-next' : entry ? 'batch-row-done' : ''}>
            <th scope="row"><div className="batch-customer-top"><span>{String(index + 1).padStart(2, '0')}</span>
              <span className="batch-row-state">{isActive ? '通話デモ中' : entry ? 'デモ記録済み' : isSkipped ? '後回し' : stopReason ? '架電停止' : !phone ? '番号未設定' : highlightedId === record.id ? '次に架電' : '未対応'}</span></div>
              <button className="batch-property-toggle" aria-label={`${record.name}さんの詳細プロパティ`} aria-expanded={!!expanded[record.id]}
                aria-controls={`properties-${record.id}`} onClick={() => { setExpanded(previous => ({ ...previous, [record.id]: !previous[record.id] })); }}>
                <strong className="batch-company">{companyRecord?.name ?? record.name}</strong><span className="batch-contact">{record.name}</span>
                <span className="batch-toggle-hint">{expanded[record.id] ? '− 詳細を畳む' : '＋ 詳細・入力'}</span></button>
              <small>{record.properties.find(p => p.name === 'jobtitle')?.value ?? '担当者'}</small>
              {record.deepLink && <a href={record.deepLink} target="_blank" rel="noreferrer">HubSpotで開く</a>}
            </th>
            <td><span className="batch-last-outcome">{lastCall?.outcome ?? '前回の電話なし'}</span>
              {lastCall && <small className="batch-last-date">{formatDate(lastCall.occurredAt)}</small>}
              <p className="batch-handover">{handover?.body ?? lastCall?.body ?? '初回の連絡です。担当者と採用状況を確認してください。'}</p>
              {record.activities.length > 0 && <details className="batch-details"><summary>{record.name}さんの履歴</summary>
                {filterActivities(record.activities, 'all', '', '', '').map(activity => <article key={activity.id}><strong>{activity.title}</strong>
                  <small>{formatDate(activity.occurredAt)} · {activity.owner}</small><p>{activity.body}</p></article>)}</details>}
            </td>
            <td><strong className="batch-number">{phone ? 'サンプル番号' : '電話番号なし'}</strong>
              <span className="batch-duration">{durationLabel(isActive ? seconds : entry?.duration ?? durations[record.id] ?? 0)}</span>
              {isActive ? <button className="batch-end" onClick={endCall}>通話デモを終了</button> :
                <button className="batch-dial" disabled={blocked || !!stopReason} aria-label={`${record.name}さんへ発信デモ`}
                  ref={element => { callButtons.current[record.id] = element; }} onClick={() => { startCall(record.id, Date.now()); }}>発信デモ</button>}
              {!entry && <button className="batch-defer" disabled={!!activeCall} onClick={() => {
                if (isSkipped) { setSkipped(previous => previous.filter(id => id !== record.id)); setNextId(record.id); }
                else { const deferred = [...skipped, record.id]; setSkipped(deferred); focusNext(record.id, completed, deferred); }
              }}>{isSkipped ? '対象に戻す' : '後回し'}</button>}
              {stopReason && <p className="batch-stop-note">{stopReason}</p>}
            </td>
            <td>{entry ? <div className="batch-recorded"><strong>{entry.result ? CALL_RESULTS[entry.result] : ''}</strong>
              <p>{entry.memo || 'メモなし'}</p>{entry.nextCallDate && <small>次回: {entry.nextCallDate} {entry.nextCallTime}</small>}
              {entry.spokeTo && <small>会話相手: {entry.spokeTo}</small>}{entry.interest && <small>関心度: {entry.interest}</small>}
              {entry.nextAction && <small>次のアクション: {entry.nextAction}</small>}
              <details className="batch-details"><summary>記録した入力の確認</summary><dl className="moc-change-preview">{Object.entries(entry.propertyChanges).map(([key, value]) => {
                const name = key.slice(key.lastIndexOf(':') + 1);
                const label = definitions[name]?.label ?? [...record.properties, ...(companyRecord?.properties ?? [])].find(property => property.name === name)?.label ?? name;
                return <div key={key}><dt>{label}</dt><dd>{value || '未設定'}</dd></div>;
              })}</dl></details>
              <span>この画面内のデモ記録 · HubSpot未保存</span></div> : <>
              <div className="batch-result-row"><label className="batch-sr-only" htmlFor={`batch-result-${record.id}`}>{record.name}さんの架電結果</label>
                <select id={`batch-result-${record.id}`} value={draft.result} disabled={blocked}
                  onChange={e => { const result = e.target.value as CallResult | ''; updateDraft(record.id, { result, ...(result === 'do_not_call' ? { nextCallDate: '', nextCallTime: '', nextAction: '' } : {}) }); }}>
                  <option value="">操作結果（MOC・必須）</option>{(Object.keys(CALL_RESULTS) as CallResult[]).map(result => <option key={result} value={result}>{CALL_RESULTS[result]}</option>)}
                </select><button className="batch-record-button" disabled={blocked} aria-label={`${record.name}さんの結果をデモ記録して次へ`}
                  onClick={() => { recordDemo(record.id); }}>デモ記録して次へ</button></div>
              <label className="batch-sr-only" htmlFor={`batch-memo-${record.id}`}>{record.name}さんの申し送りメモ</label>
              <textarea id={`batch-memo-${record.id}`} rows={2} placeholder="タスクメモ・次の人への申し送り" value={draft.memo}
                disabled={isSkipped} onChange={e => { updateDraft(record.id, { memo: e.target.value }); }} />
              <details className="batch-extra"><summary>ヒアリング情報（任意）</summary><div>
                <MocPropertyInput recordName={record.name} definitionName="bpo_40" value={draft.spokeTo} disabled={isSkipped}
                  onChange={value => { updateDraft(record.id, { spokeTo: value }); }} />
                <MocPropertyInput recordName={record.name} definitionName="bpo_42" value={draft.interest} disabled={isSkipped}
                  onChange={value => { updateDraft(record.id, { interest: value }); }} />
                {draft.result !== 'do_not_call' && <MocPropertyInput recordName={record.name} definitionName="bpo_45" value={draft.nextAction} disabled={isSkipped}
                  onChange={value => { updateDraft(record.id, { nextAction: value }); }} />}
              </div></details>
              {draft.result !== 'do_not_call' && <details className="batch-next-date" open={draft.result === 'callback' || draft.nextAction === '再架電' || !!draft.nextCallDate || !!draft.nextCallTime}>
                <summary>次回の架電日時{draft.result === 'callback' || draft.nextAction === '再架電' ? '（MOC必須）' : '（任意）'}</summary>
                <div className="moc-next-fields"><MocPropertyInput recordName={record.name} definitionName="bpo_13" disabled={isSkipped} value={draft.nextCallDate}
                  onChange={value => { updateDraft(record.id, { nextCallDate: value }); }} />
                <MocPropertyInput recordName={record.name} definitionName="bpo_14" disabled={isSkipped} value={draft.nextCallTime}
                  onChange={value => { updateDraft(record.id, { nextCallTime: value }); }} /></div></details>}
              {draft.result === 'do_not_call' && <p className="batch-stop-note">架電停止の希望。実接続時は再架電対象から除外します。</p>}
              {errors[record.id] && <p className="batch-error" role="alert">{errors[record.id]}</p>}
            </>}</td>
          </tr>{expanded[record.id] && <tr className="batch-property-row"><td colSpan={4}>
            <ContactProperties record={record} values={propertyDrafts} onChange={(key, value) => {
              setPropertyDrafts(previous => ({ ...previous, [key]: value }));
            }} />
          </td></tr>}</Fragment>;
        })}{visibleContacts.length === 0 && <tr><td colSpan={4} className="batch-empty">条件に一致する顧客はいません。絞り込みを解除してください。</td></tr>}</tbody>
      </table></div>
      <div className="batch-footer"><span role="status">{message || '入力はデモ下書きです。再読み込みで消えます。'}</span>
        {remaining.length === 0 && <strong role="status">発信可能な未対応の顧客はありません。</strong>}</div>
    </main>
  </div></CrmMetadataContext>;
}
