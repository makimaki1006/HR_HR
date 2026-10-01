import { useEffect, useState } from 'react';
import { SAMPLE_RECORDS } from './fixtures';
import { filterActivities, formatDate } from './model';
import { CALL_RESULTS, durationLabel, emptyDraft, nextContact, validateDraft } from './callModel';
import type { CallDraft, CallResult, DemoCallEntry } from './callModel';
import './call.css';

const contacts = SAMPLE_RECORDS.filter(record => record.objectType === 'contacts');

export function CallWorkspace() {
  const [selectedId, setSelectedId] = useState('sample-contact-1');
  const [drafts, setDrafts] = useState<Record<string, CallDraft>>({});
  const [entries, setEntries] = useState<Record<string, DemoCallEntry>>({});
  const [skipped, setSkipped] = useState<string[]>([]);
  const [callState, setCallState] = useState<'ready' | 'calling' | 'ended'>('ready');
  const [startedAt, setStartedAt] = useState(0);
  const [seconds, setSeconds] = useState(0);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [showHistory, setShowHistory] = useState(false);
  const [finished, setFinished] = useState(false);
  const selected = contacts.find(record => record.id === selectedId);
  const completed = Object.keys(entries);
  const draft = drafts[selectedId] ?? emptyDraft();
  const phone = selected?.properties.find(p => p.name === 'phone')?.value;
  const company = selected?.associations.find(a => a.objectType === 'companies');
  const companyRecord = SAMPLE_RECORDS.find(r => r.objectType === 'companies' && r.id === company?.id);
  const lastCall = selected ? filterActivities(selected.activities, 'call', '', '', '')[0] : undefined;
  const handover = selected ? filterActivities(selected.activities, 'note', '', '', '')[0] : undefined;
  const isCalling = callState === 'calling';

  useEffect(() => {
    if (!isCalling) return;
    const timer = window.setInterval(() => { setSeconds(Math.floor((Date.now() - startedAt) / 1000)); }, 1000);
    return () => { window.clearInterval(timer); };
  }, [isCalling, startedAt]);

  function updateDraft(patch: Partial<CallDraft>) {
    setDrafts(previous => ({ ...previous, [selectedId]: { ...(previous[selectedId] ?? emptyDraft()), ...patch } }));
    setError('');
  }
  function select(id: string) {
    if (isCalling) return;
    setSelectedId(id); setCallState('ready'); setSeconds(0); setShowHistory(false); setFinished(false); setError('');
  }
  function moveNext(done: string[], skip: string[]) {
    const next = nextContact(contacts, selectedId, done, skip);
    if (next) select(next.id);
    else { setFinished(true); setCallState('ready'); }
  }
  function recordDemo() {
    const validation = validateDraft(draft);
    if (validation) { setError(validation); return; }
    setEntries(previous => ({ ...previous, [selectedId]: { ...draft, duration: seconds } }));
    setSkipped(previous => previous.filter(id => id !== selectedId));
    setMessage(`${selected?.name ?? ''}さんの結果をデモ記録しました。HubSpotには保存していません。`);
    moveNext([...completed, selectedId], skipped.filter(id => id !== selectedId));
  }

  return <div className="call-app">
    <header className="call-header"><a href="/" className="call-brand">HR_HR</a><strong>架電ワークスペース</strong>
      <a className="call-reference" href="?view=reference">基準のCRM画面</a></header>
    <div className="call-demo-banner"><strong>操作デモ</strong>
      <span>架空の架電先です。実際の発信・HubSpot保存は行いません。下書きとデモ記録は再読み込みで消えます。</span></div>
    <div className="call-layout">
      <aside className="call-queue" aria-label="架電先一覧">
        <div className="call-queue-heading"><span className="call-eyebrow">CALL QUEUE</span><h2>今回の架電先</h2>
          <p><strong>{completed.length}</strong> / {contacts.length} 件をデモ記録</p></div>
        <div className="call-progress" aria-hidden="true"><span style={{ width: `${String(completed.length / contacts.length * 100)}%` }} /></div>
        <div className="call-queue-items">{contacts.map((record, index) => <button key={record.id}
          disabled={isCalling} aria-current={selectedId === record.id && !finished ? 'true' : undefined}
          className="call-queue-item" onClick={() => { select(record.id); setMessage(''); }}>
          <span className="call-queue-number">{String(index + 1).padStart(2, '0')}</span>
          <span><strong>{record.name}</strong><small>{record.subtitle.split(' / ')[1]}</small>
            <span className="call-queue-state">{entries[record.id] ? 'デモ記録済み' : skipped.includes(record.id) ? '後回し' :
              !record.properties.find(p => p.name === 'phone')?.value ? '電話番号なし' : '未対応'}
              {drafts[record.id] && !entries[record.id] && (drafts[record.id]?.memo || drafts[record.id]?.result) ? ' · 下書きあり' : ''}</span></span>
        </button>)}</div>
        <p className="call-queue-note">通話中は架電先を固定します。<br />入力途中でも下書きはこの画面内に残ります。</p>
      </aside>

      {finished ? <main className="call-finished"><span className="call-eyebrow">QUEUE COMPLETE</span>
        <h1>今回のリストを確認しました</h1><p>デモ記録 {completed.length}件 / 後回し {skipped.length}件</p>
        <p>HubSpotへの保存や実際の発信は行っていません。</p>
        <button onClick={() => { select(contacts[0]?.id ?? ''); }}>架電先を見直す</button>
      </main> : selected && <>
        <main className="call-context">
          <div className="call-context-top"><span className="call-eyebrow">通話前に確認</span><span>{contacts.findIndex(r => r.id === selectedId) + 1} / {contacts.length} 件目</span></div>
          <h1>{companyRecord?.name ?? selected.name}</h1>
          <p className="call-contact-name">{selected.name}<span>{selected.properties.find(p => p.name === 'jobtitle')?.value ?? '担当者'}</span></p>
          <section className="call-dial-card" aria-label="発信操作">
            <div><span className="call-eyebrow">電話番号</span><strong className="call-number">{phone ? 'サンプル番号' : '電話番号未設定'}</strong>
              <p>{phone ? '実接続時はHubSpotの電話番号を表示' : '番号を確認してから架電してください。'}</p></div>
            <div className="call-dial-controls"><span className="call-timer" aria-live="off">{durationLabel(seconds)}</span>
              {isCalling ? <button className="call-end" onClick={() => { setSeconds(Math.floor((Date.now() - startedAt) / 1000)); setCallState('ended'); }}>通話デモを終了</button> :
                <button className="call-start" disabled={!phone} onClick={() => { setStartedAt(Date.now()); setSeconds(0); setCallState('calling'); setError(''); }}>発信デモを試す</button>}
              <span className="call-dial-state" role="status">{isCalling ? '通話デモ中 · 実際には発信していません' : callState === 'ended' ? 'デモ終了 · 結果を入力してください' : 'Zoom Phoneは未接続'}</span>
            </div>
          </section>
          <section className="call-handover"><span className="call-eyebrow">前回からの申し送り</span>
            <h2>{handover?.title ?? '申し送りはありません'}</h2><p>{handover?.body ?? '初回のご連絡として、担当者と採用状況を確認してください。'}</p></section>
          <section className="call-last"><div className="call-section-heading"><h2>前回の電話</h2>
            {lastCall && <span>{lastCall.outcome}</span>}</div>
            {lastCall ? <><p className="call-last-date">{formatDate(lastCall.occurredAt)} · {lastCall.owner}</p><p>{lastCall.body}</p></> :
              <p className="call-muted">このサンプルには前回の架電履歴がありません。</p>}
          </section>
          <button className="call-history-toggle" aria-expanded={showHistory} onClick={() => { setShowHistory(!showHistory); }}>
            {showHistory ? 'その他の履歴を閉じる' : 'その他の履歴を見る'}</button>
          {showHistory && <div className="call-history">{filterActivities(selected.activities, 'all', '', '', '').map(activity =>
            <article key={activity.id}><strong>{activity.title}</strong><small>{formatDate(activity.occurredAt)}</small><p>{activity.body}</p></article>)}</div>}
          {selected.deepLink && <a href={selected.deepLink} target="_blank" rel="noreferrer">HubSpotで詳細を確認</a>}
        </main>

        <aside className="call-recording" aria-label="通話結果の入力">
          <span className="call-eyebrow">AFTER CALL</span><h2>今回の結果</h2><p className="call-muted">結果を選んで、次の架電先へ。</p>
          <fieldset className="call-result-options" disabled={isCalling || !phone}><legend>架電結果（必須）</legend>
            {(Object.keys(CALL_RESULTS) as CallResult[]).map(result => <label key={result} className={draft.result === result ? 'is-selected' : ''}>
              <input type="radio" name={`call-result-${selectedId}`} checked={draft.result === result}
                onChange={() => { updateDraft({ result, ...(result === 'do_not_call' ? { nextCallAt: '' } : {}) }); }} />{CALL_RESULTS[result]}</label>)}
          </fieldset>
          <label className="call-input-label" htmlFor="call-memo">申し送りメモ<span>任意</span></label>
          <textarea id="call-memo" rows={4} value={draft.memo} placeholder="会話の要点、次の人に伝えたいこと"
            onChange={e => { updateDraft({ memo: e.target.value }); }} />
          <label className="call-input-label" htmlFor="call-next">次回の架電日時<span>{draft.result === 'callback' ? '必須' : '任意'}</span></label>
          <input id="call-next" type="datetime-local" disabled={draft.result === 'do_not_call'} value={draft.nextCallAt}
            onChange={e => { updateDraft({ nextCallAt: e.target.value }); }} />
          {draft.result === 'do_not_call' && <p className="call-stop-note">架電停止の希望です。実接続時は再架電の対象から外す必要があります。</p>}
          {error && <p className="call-error" role="alert">{error}</p>}
          <div className="call-recording-footer"><button className="call-next-button" disabled={isCalling || !phone} onClick={recordDemo}>デモ記録して次へ</button>
            <button className="call-skip" disabled={isCalling} onClick={() => { const nextSkipped = [...new Set([...skipped, selectedId])]; setSkipped(nextSkipped); setMessage(`${selected.name}さんを後回しにしました。下書きは残っています。`); moveNext(completed, nextSkipped); }}>今は後回しにする</button>
            <p>デモ記録はこの画面内のみ。HubSpot未保存です。</p></div>
        </aside>
      </>}
    </div>
    {message && <div className="call-feedback" role="status">{message}<button aria-label="通知を閉じる" onClick={() => { setMessage(''); }}>閉じる</button></div>}
  </div>;
}
