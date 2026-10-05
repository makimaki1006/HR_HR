import { useEffect, useMemo, useState } from 'react';
import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueuePartial } from '../../generated/CallQueuePartial';
import { toDomesticPhone } from './phone';
import {
  DEFAULT_FILTERS, QUEUE_SORTS, QUEUE_STAGES, dateValue, filtersKey, parseFilters, parseMode, screenSearch,
} from './queueModel';
import type { QueueFilters, QueueMode, QueueSort } from './queueModel';
import { useCallQueue } from './useCallQueue';
import type { QueueFetch } from './useCallQueue';
import './crm.css';
import './queue.css';

const PHONE_SOURCE_LABELS: Record<string, string> = { deal: '案件', contact: '担当者', mobile: '担当者(携帯)', company: '会社' };

const ymd = (raw: string | null) => dateValue(raw)?.replaceAll('-', '/') ?? null;

export function partialNotes(p: CallQueuePartial | null): string[] {
  if (!p) return [];
  const notes: string[] = [];
  if (p.missing_contacts > 0) notes.push(`担当者情報を取得できなかった行が ${String(p.missing_contacts)} 件あります`);
  if (p.missing_companies > 0) notes.push(`会社情報を取得できなかった行が ${String(p.missing_companies)} 件あります`);
  if (p.failed.length > 0) notes.push(`取得に失敗した部分: ${p.failed.join('、')}(関連情報を表示できない行があります)`);
  if (p.excluded.no_phone > 0) notes.push(`電話番号がどこにも無いため ${String(p.excluded.no_phone)} 件を除きました`);
  if (p.excluded.stop_reason > 0) notes.push(`架電禁止・ブロック理由があるため ${String(p.excluded.stop_reason)} 件を除きました`);
  if (p.excluded.out_of_scope > 0) notes.push(`対象外(別パイプライン・対象外ステージ・アーカイブ)の ${String(p.excluded.out_of_scope)} 件を除きました`);
  return notes;
}

function QueueRow({ item }: { item: CallQueueItem }) {
  const phone = toDomesticPhone(item.phone);
  const next = ymd(item.next_call_date);
  const last = ymd(item.last_call_date);
  const stop = item.stop;
  return <tr>
    <td><strong>{item.company?.name ?? <span className="crm-muted">会社情報を取得できませんでした</span>}</strong>
      <small>{item.deal_name ?? '(案件名なし)'}</small></td>
    <td>{item.contact ? <>{item.contact.name ?? '(氏名なし)'}
      {item.contact.job_title && <small>{item.contact.job_title}</small>}
      {item.contact.extra_count > 0 && <small>ほか {item.contact.extra_count} 人</small>}</>
      : <span className="crm-muted">担当者情報を取得できませんでした</span>}</td>
    <td>{phone ? <><span className="cq-phone" title={item.phone ?? undefined}>{phone}</span>
      {item.phone_source && <small>{PHONE_SOURCE_LABELS[item.phone_source] ?? item.phone_source}の番号</small>}</>
      : <span className="crm-muted">番号を確認できません</span>}</td>
    <td><span className="crm-status">{item.stage_label ?? '(ステージ名を取得できません)'}</span>
      {stop.unreachable_check && <small className="cq-flag">不通時チェック: {stop.unreachable_check}</small>}</td>
    <td>{next ? <>{next}{item.next_call_time && <small>{item.next_call_time}</small>}</> : <span className="crm-muted">なし</span>}</td>
    <td>{last ?? <span className="crm-muted">未架電</span>}</td>
    <td>{item.owner_id ?? <span className="crm-muted">担当なし</span>}</td>
    <td><a href={item.deep_links.deal} target="_blank" rel="noreferrer">HubSpotで開く</a></td>
  </tr>;
}

export function CallQueueScreen({ fetcher, initialSearch }: { fetcher?: QueueFetch; initialSearch?: string }) {
  const search = initialSearch ?? window.location.search;
  const [mode, setMode] = useState<QueueMode>(() => parseMode(search));
  const [filters, setFilters] = useState<QueueFilters>(() => parseFilters(search));
  const [qDraft, setQDraft] = useState(filters.q);
  const [ownerIdMode, setOwnerIdMode] = useState(() => /^\d+$/.test(filters.owner));
  const { state, loadMore, reload } = useCallQueue(filters, mode, fetcher);

  function update(patch: Partial<QueueFilters>) {
    setFilters(prev => {
      const next = { ...prev, ...patch };
      return filtersKey(next) === filtersKey(prev) ? prev : next;
    });
  }

  // キーワードは入力が止まってから反映する (1 文字ごとに HubSpot を呼ばない)
  useEffect(() => {
    const t = window.setTimeout(() => { update({ q: qDraft }); }, 400);
    return () => { window.clearTimeout(t); };
  }, [qDraft]);

  // 条件を URL に残す (再読み込みで復元)。履歴は増やさない
  useEffect(() => {
    if (initialSearch !== undefined) return;
    try { window.history.replaceState(null, '', screenSearch(filters, mode)); } catch { /* URL を書けない環境では何もしない */ }
  }, [filters, mode, initialSearch]);

  const hasConditions = useMemo(() => filtersKey(filters) !== filtersKey(DEFAULT_FILTERS), [filters]);
  const isAdmin = state.role === 'admin';
  const notes = partialNotes(state.partial);
  const total = state.last?.total ?? null;

  function toggleStage(id: string) {
    update({ stages: filters.stages.includes(id) ? filters.stages.filter(s => s !== id) : [...filters.stages, id] });
  }
  function clearAll() { setQDraft(''); setFilters(DEFAULT_FILTERS); }

  return <div className="crm-app cq-app">
    <header className="crm-topbar"><a className="crm-home" href="/">HR_HR</a>
      <span className="crm-topbar-divider" /><strong>架電キュー</strong>
      <a className="crm-topbar-right" href="?view=calling">架電ワークスペースへ</a></header>

    <div className={`cq-mode cq-mode-${mode}`} role="status" aria-label="データの種類">
      <strong>{mode === 'live' ? '実データ(HubSpot)' : '架空サンプル'}</strong>
      <span>{mode === 'live' ? 'HubSpot の読み取りだけを行います。書き込み・発信はしません。'
        : '表示内容はすべて架空です。HubSpot には接続しません。'}</span>
      <span className="cq-mode-switch" role="group" aria-label="データの切り替え">
        <button aria-pressed={mode === 'live'} onClick={() => { setMode('live'); }}>実データ</button>
        <button aria-pressed={mode === 'fixture'} onClick={() => { setMode('fixture'); }}>架空サンプル</button>
      </span>
    </div>

    <form className="cq-filters" aria-label="絞り込みと並び替え" onSubmit={e => { e.preventDefault(); update({ q: qDraft }); }}>
      <label className="cq-wide">キーワード(会社名・案件名)<input type="search" value={qDraft} maxLength={100}
        placeholder="例: 架空商事" onChange={e => { setQDraft(e.target.value); }} /></label>
      <label>並び替え<select value={filters.sort} onChange={e => { update({ sort: e.target.value as QueueSort }); }}>
        {QUEUE_SORTS.map(s => <option key={s.value} value={s.value}>{s.label}</option>)}</select></label>
      <fieldset className="cq-range"><legend>次回架電日</legend>
        <label>から<input type="date" value={filters.nextFrom} onChange={e => { update({ nextFrom: e.target.value }); }} /></label>
        <label>まで<input type="date" value={filters.nextTo} onChange={e => { update({ nextTo: e.target.value }); }} /></label></fieldset>
      <fieldset className="cq-range"><legend>最終架電日</legend>
        <label>から<input type="date" value={filters.lastFrom} onChange={e => { update({ lastFrom: e.target.value }); }} /></label>
        <label>まで<input type="date" value={filters.lastTo} onChange={e => { update({ lastTo: e.target.value }); }} /></label></fieldset>
      <label className="cq-check"><input type="checkbox" checked={filters.due === 'today'}
        onChange={e => { update({ due: (e.target.checked ? 'today' : 'all') }); }} />次回日が来たものだけ</label>
      {isAdmin && <fieldset className="cq-owner"><legend>担当者(管理者のみ)</legend>
        <select aria-label="担当者" value={ownerIdMode ? 'id' : filters.owner === 'all' ? '' : filters.owner} onChange={e => {
          const v = e.target.value;
          setOwnerIdMode(v === 'id');
          update({ owner: v === 'id' ? '' : v });
        }}>
          <option value="">全員分</option><option value="unassigned">担当者なし</option>
          <option value="me">自分</option><option value="id">担当者IDを指定</option></select>
        {ownerIdMode && <input aria-label="担当者ID(HubSpot owner ID)" inputMode="numeric" placeholder="担当者ID(数字)"
          value={filters.owner} onChange={e => { update({ owner: e.target.value.replace(/\D/g, '').slice(0, 20) }); }} />}</fieldset>}
      <fieldset className="cq-stages"><legend>ステージ{filters.stages.length > 0 ? `(${String(filters.stages.length)} 件選択)` : '(すべて)'}</legend>
        {QUEUE_STAGES.map(s => <label key={s.id} className="cq-check"><input type="checkbox" checked={filters.stages.includes(s.id)}
          onChange={() => { toggleStage(s.id); }} />{s.label}</label>)}</fieldset>
      <div className="cq-actions"><button type="button" onClick={clearAll} disabled={!hasConditions && qDraft === ''}>条件をクリア</button></div>
    </form>

    <main className="cq-main" aria-live="polite" aria-busy={state.phase === 'loading'}>
      {state.phase === 'invalid' && <div className="cq-notice cq-error" role="alert"><strong>条件を確認してください</strong>
        <ul>{state.invalid.map(m => <li key={m}>{m}</li>)}</ul></div>}
      {state.phase === 'loading' && <p role="status" className="cq-loading">読み込み中…</p>}
      {state.phase === 'unauthorized' && <div className="cq-notice cq-error" role="alert"><strong>表示できません</strong><p>{state.message}</p>
        {state.errorKind === 'forbidden_owner' && <button onClick={() => { update({ owner: '' }); }}>担当者の指定を外す</button>}</div>}
      {state.phase === 'error' && <div className="cq-notice cq-error" role="alert"><strong>取得できませんでした</strong><p>{state.message}</p>
        <button onClick={reload}>再試行</button></div>}

      {state.phase === 'ready' && <>
        <p className="cq-count" role="status">{state.items.length} 件を表示
          {total !== null && <span>(HubSpot の検索結果は {total} 件。電話番号なし等を除く前の参考値)</span>}</p>
        {state.last?.truncated && <div className="cq-notice cq-warn" role="status">HubSpot の検索は 1 万件までしか取得できないため、これより先は表示できません。条件を絞ってください。</div>}
        {notes.length > 0 && <div className="cq-notice cq-warn" role="status"><strong>一部の情報が欠けています</strong>
          <ul>{notes.map(n => <li key={n}>{n}</li>)}</ul></div>}
        {state.items.length === 0 && <div className="cq-notice cq-empty">
          {state.nextCursor ? <p>このページには表示できる行がありません。続きを読み込んでください。</p>
            : hasConditions ? <><strong>条件に一致する架電先がありません</strong><p>条件を変えるか、クリアしてください。</p>
              <button onClick={clearAll}>条件をクリア</button></>
              : <><strong>いま架電キューに出ている架電先はありません</strong></>}</div>}
        {state.items.length > 0 && <div className="cq-table-wrap"><table className="cq-table">
          <thead><tr><th>会社 / 案件</th><th>担当者</th><th>電話番号</th><th>ステージ</th><th>次回架電</th><th>最終架電</th><th>担当</th><th>リンク</th></tr></thead>
          <tbody>{state.items.map(item => <QueueRow key={item.deal_id} item={item} />)}</tbody></table></div>}
        {state.moreError && <div className="cq-notice cq-error" role="alert"><strong>続きを読み込めませんでした</strong><p>{state.moreError.message}</p>
          {state.moreError.kind === 'cursor_mismatch' && <button onClick={reload}>最初から読み直す</button>}</div>}
        {state.nextCursor && <button className="cq-more" disabled={state.loadingMore} onClick={loadMore}>
          {state.loadingMore ? '読み込み中…' : 'さらに読み込む'}</button>}
        {!state.nextCursor && state.items.length > 0 && <p className="cq-end">これで最後です。</p>}
      </>}
    </main>
  </div>;
}
