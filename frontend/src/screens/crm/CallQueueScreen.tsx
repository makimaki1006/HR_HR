import { useEffect, useMemo, useRef, useState } from 'react';
import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueuePartial } from '../../generated/CallQueuePartial';
import { OwnerFilter } from './OwnerFilter';
import { ownerNameMap } from './ownerModel';
import { formatPhoneForDisplay } from './phone';
import {
  DEFAULT_FILTERS, QUEUE_SORTS, QUEUE_STAGES, dateValue, filtersKey, parseFilters, parseMode, screenSearch,
} from './queueModel';
import type { QueueFilters, QueueMode, QueueSort } from './queueModel';
import { useCallQueue } from './useCallQueue';
import { DealDetail } from './DealDetail';
import { ZoomPhonePanel } from './ZoomPhonePanel';
import { useDealDetail } from './useDealDetail';
import type { DetailFetch } from './useDealDetail';
import { useZoomPhone } from './useZoomPhone';
import type { ZoomOptions } from './useZoomPhone';
import type { QueueFetch } from './useCallQueue';
import { fixtureOwnersFetch, liveOwnersFetch, useOwners } from './useOwners';
import type { OwnersFetch } from './useOwners';
import './crm.css';
import './queue.css';

const PHONE_SOURCE_LABELS: Record<string, string> = { deal: '案件', contact: '担当者', mobile: '担当者(携帯)', company: '会社' };

/** 矢印キーで行を移ったとき、詳細の取得を待つ時間 (押しっぱなしで HubSpot を連続で呼ばない) */
export const KEY_SELECT_DELAY_MS = 300;

const ymd = (raw: string | null) => dateValue(raw)?.replaceAll('-', '/') ?? null;
/** 一覧の行は幅が狭いので月日だけ (年は title に残す) */
const md = (raw: string | null) => dateValue(raw)?.slice(5).replace('-', '/') ?? null;

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

/** 応答の scope.owner (all / me / unassigned / owner ID) を、画面の注記に出す名前にする */
export function ownerScopeLabel(scopeOwner: string, names: ReadonlyMap<string, string>): string {
  if (scopeOwner === 'all') return '全員';
  if (scopeOwner === 'me') return '自分';
  if (scopeOwner === 'unassigned') return '担当者なし';
  return names.get(scopeOwner) ?? `ID ${scopeOwner}`;
}

export interface ConditionChip { key: string; label: string; clear: Partial<QueueFilters> }

const range = (from: string, to: string) => `${from ? from.replaceAll('-', '/') : ''}〜${to ? to.replaceAll('-', '/') : ''}`;

/** 既定と違う絞り込み条件を、外せるチップとして並べる (詳細条件を閉じていても見えるように)。並び替えは選択欄に出ているので含めない */
export function conditionChips(f: QueueFilters, ownerNames: ReadonlyMap<string, string>): ConditionChip[] {
  const chips: ConditionChip[] = [];
  if (f.q.trim()) chips.push({ key: 'q', label: `キーワード: ${f.q.trim()}`, clear: { q: '' } });
  if (f.owner) chips.push({ key: 'owner', label: `所有者: ${ownerScopeLabel(f.owner, ownerNames)}`, clear: { owner: '' } });
  if (f.due === 'today') chips.push({ key: 'due', label: '次回日が来たものだけ', clear: { due: 'all' } });
  for (const id of f.stages) {
    const label = QUEUE_STAGES.find(s => s.id === id)?.label ?? id;
    chips.push({ key: `stage-${id}`, label: `ステージ: ${label}`, clear: { stages: f.stages.filter(s => s !== id) } });
  }
  if (f.nextFrom || f.nextTo) chips.push({ key: 'next', label: `次回架電日: ${range(f.nextFrom, f.nextTo)}`, clear: { nextFrom: '', nextTo: '' } });
  if (f.lastFrom || f.lastTo) chips.push({ key: 'last', label: `最終架電日: ${range(f.lastFrom, f.lastTo)}`, clear: { lastFrom: '', lastTo: '' } });
  return chips;
}

function QueueRow({ item, ownerName, selected, focusable, onSelect }: {
  item: CallQueueItem; ownerName?: string | undefined; selected: boolean; focusable: boolean; onSelect: (id: string) => void;
}) {
  const phone = formatPhoneForDisplay(item.phone);
  const next = md(item.next_call_date);
  const last = md(item.last_call_date);
  const dates = [ymd(item.next_call_date) && `次回架電 ${ymd(item.next_call_date) ?? ''}`, ymd(item.last_call_date) && `最終架電 ${ymd(item.last_call_date) ?? ''}`]
    .filter(Boolean).join(' / ');
  const flag = item.stop.unreachable_check;
  const source = item.phone_source ? `${PHONE_SOURCE_LABELS[item.phone_source] ?? item.phone_source}の番号` : undefined;
  return <li className={`cq-row${selected ? ' is-selected' : ''}`}>
    <button type="button" className="cq-row-button" aria-pressed={selected} tabIndex={focusable ? 0 : -1}
      title={item.deal_name ?? undefined} onClick={() => { onSelect(item.deal_id); }}>
      <span className="cq-row-l1">
        <strong className="cq-row-company">{item.company?.name ?? <span className="crm-muted">会社情報を取得できませんでした</span>}</strong>
        {flag && <span className="cq-flag" title={`不通時チェック: ${flag}`} aria-label={`不通時チェック: ${flag}`}>不通チェック</span>}
        <span className="cq-stage">{item.stage_label ?? '(ステージ不明)'}</span>
      </span>
      <span className="cq-row-l2">
        {item.contact ? <span className="cq-row-contact">{item.contact.name ?? '(氏名なし)'}
          {item.contact.extra_count > 0 && <small> ほか{item.contact.extra_count}人</small>}</span>
          : <span className="crm-muted">担当者情報を取得できませんでした</span>}
        <span className="cq-sep" aria-hidden="true">·</span>
        {phone ? <span className="cq-phone" title={item.phone ?? undefined} data-source={source}>{phone}</span>
          : <span className="crm-muted">番号を確認できません</span>}
      </span>
      <span className="cq-row-l3" title={dates || undefined}>
        <span>次回 {next ? <><span>{next}</span>{item.next_call_time && <> <span>{item.next_call_time}</span></>}</> : 'なし'}</span>
        <span>最終 {last ? <span>{last}</span> : '未架電'}</span>
        <span>担当 {item.owner_id ? <span>{ownerName ?? item.owner_id}</span> : 'なし'}</span>
      </span>
    </button>
  </li>;
}

export function CallQueueScreen({ fetcher, ownersFetcher, detailFetcher, zoomOptions, initialSearch }: {
  fetcher?: QueueFetch; ownersFetcher?: OwnersFetch; detailFetcher?: DetailFetch; zoomOptions?: ZoomOptions | undefined; initialSearch?: string;
}) {
  const search = initialSearch ?? window.location.search;
  const [mode, setMode] = useState<QueueMode>(() => parseMode(search));
  const [filters, setFilters] = useState<QueueFilters>(() => parseFilters(search));
  const [qDraft, setQDraft] = useState(filters.q);
  const [panelOpen, setPanelOpen] = useState(false);
  const { state, loadMore, reload } = useCallQueue(filters, mode, fetcher);
  // 選んだ案件。モードを切り替えたら選び直す (実データの ID と架空の ID を取り違えない)
  const [selection, setSelection] = useState<{ id: string; mode: QueueMode } | null>(null);
  const selectedId = selection !== null && selection.mode === mode ? selection.id : null;
  // 詳細を読む案件。クリックはすぐ、矢印キーは少し待ってから (selection と違う間は古い詳細を出さない)
  const [detailSel, setDetailSel] = useState<{ id: string; mode: QueueMode } | null>(null);
  const detailId = detailSel !== null && detailSel.mode === mode ? detailSel.id : null;
  const keyTimer = useRef<number | null>(null);
  const detail = useDealDetail(detailId, mode, detailFetcher);
  // Zoom Phone は常駐 (案件を切り替えても作り直さない)。架空サンプルでは出さず、発信もしない
  const { zoom, iframeRef } = useZoomPhone(mode === 'live', zoomOptions);
  const listRef = useRef<HTMLUListElement | null>(null);

  useEffect(() => () => { if (keyTimer.current !== null) window.clearTimeout(keyTimer.current); }, []);

  function select(id: string, via: 'click' | 'key') {
    const sel = { id, mode };
    setSelection(sel);
    if (keyTimer.current !== null) { window.clearTimeout(keyTimer.current); keyTimer.current = null; }
    if (via === 'click') { setDetailSel(sel); return; }
    keyTimer.current = window.setTimeout(() => { keyTimer.current = null; setDetailSel(sel); }, KEY_SELECT_DELAY_MS);
  }

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
    try { window.history.replaceState(null, '', screenSearch(filters, mode) || window.location.pathname); } catch { /* URL を書けない環境では何もしない */ }
  }, [filters, mode, initialSearch]);

  const hasConditions = useMemo(() => filtersKey(filters) !== filtersKey(DEFAULT_FILTERS), [filters]);
  // 所有者の既定 (条件の owner が '' のとき、サーバが実際に使った所有者 = 管理者は all、それ以外は me)。
  // 条件を変えて読み直している間は応答が空になるので、同じモードで分かった値を覚えておく (選択欄がちらつかないように)
  const [seenOwner, setSeenOwner] = useState<{ mode: QueueMode; owner: string } | null>(null);
  if (state.last !== null && filters.owner === '' && (seenOwner?.mode !== mode || seenOwner.owner !== state.last.scope.owner)) {
    setSeenOwner({ mode, owner: state.last.scope.owner });
  }
  const effectiveOwner = seenOwner?.mode === mode ? seenOwner.owner : null;
  // 所有者の一覧は CRM の利用者全員が使える。実データでは HubSpot、架空サンプルでは架空の一覧
  const owners = useOwners(true, mode === 'fixture' ? fixtureOwnersFetch : (ownersFetcher ?? liveOwnersFetch));
  const ownerNames = useMemo(() => ownerNameMap(owners.state.phase === 'ready' ? owners.state.owners : []), [owners.state]);
  const needsOwnerPick = state.phase === 'error' && state.errorKind === 'owner_not_resolved';
  const notes = partialNotes(state.partial);
  const total = state.last?.total ?? null;
  const chips = conditionChips(filters, ownerNames);
  const detailCount = filters.stages.length + (filters.nextFrom || filters.nextTo ? 1 : 0) + (filters.lastFrom || filters.lastTo ? 1 : 0);

  function toggleStage(id: string) {
    update({ stages: filters.stages.includes(id) ? filters.stages.filter(s => s !== id) : [...filters.stages, id] });
  }
  function removeChip(c: ConditionChip) {
    if (c.clear.q !== undefined) setQDraft('');
    update(c.clear);
  }
  function clearAll() { setQDraft(''); setFilters(DEFAULT_FILTERS); }

  // 一覧にフォーカスがあるとき、上下の矢印キーで選択を移す
  function onListKey(e: React.KeyboardEvent<HTMLUListElement>) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    if (state.phase !== 'ready' || state.items.length === 0) return;
    e.preventDefault();
    const items = state.items;
    const focused = items.findIndex((_, i) => listRef.current?.querySelectorAll('.cq-row-button')[i] === document.activeElement);
    const cur = focused !== -1 ? focused : items.findIndex(i => i.deal_id === selectedId);
    const step = e.key === 'ArrowDown' ? 1 : -1;
    const nextIdx = cur === -1 ? (step === 1 ? 0 : items.length - 1) : Math.min(items.length - 1, Math.max(0, cur + step));
    const target = items[nextIdx];
    if (!target) return;
    if (target.deal_id !== selectedId) select(target.deal_id, 'key');
    const btn = listRef.current?.querySelectorAll<HTMLButtonElement>('.cq-row-button')[nextIdx];
    btn?.focus();
    if (typeof btn?.scrollIntoView === 'function') btn.scrollIntoView({ block: 'nearest' });
  }

  const selectedOwner = state.items.find(i => i.deal_id === selectedId)?.owner_id;
  const waitingKey = selectedId !== null && selectedId !== detailId;
  const anyFocusable = state.items.some(i => i.deal_id === selectedId);

  return <div className="crm-app cq-app">
    <header className="crm-topbar cq-topbar"><a className="crm-home" href="/">HR_HR</a>
      <span className="crm-topbar-divider" /><h1 className="cq-title">架電</h1>
      <div className={`cq-mode cq-mode-${mode}`} role="status" aria-label="データの種類">
        <strong className="cq-mode-badge">{mode === 'live' ? '実データ(HubSpot)' : '架空サンプル'}</strong>
        <span className="cq-mode-note">{mode === 'live' ? 'HubSpot への書き込みはしません' : '表示内容はすべて架空です。HubSpot には接続しません。'}</span>
      </div>
      <span className="cq-mode-switch" role="group" aria-label="データの切り替え">
        <button type="button" aria-pressed={mode === 'live'} onClick={() => { setMode('live'); }}>実データ</button>
        <button type="button" aria-pressed={mode === 'fixture'} onClick={() => { setMode('fixture'); }}>架空サンプル</button>
      </span>
    </header>

    <form className="cq-filters" aria-label="絞り込みと並び替え" onSubmit={e => { e.preventDefault(); update({ q: qDraft }); }}>
      <div className="cq-bar">
        <input className="cq-search" type="search" aria-label="キーワード(会社名・案件名)" value={qDraft} maxLength={100}
          placeholder="会社名・案件名で検索" onChange={e => { setQDraft(e.target.value); }} />
        <OwnerFilter owner={filters.owner} effective={effectiveOwner} needsPick={needsOwnerPick}
          onChange={owner => { update({ owner }); }} owners={owners.state} onReload={owners.reload} />
        <select className="cq-sort" aria-label="並び替え" value={filters.sort} onChange={e => { update({ sort: e.target.value as QueueSort }); }}>
          {QUEUE_SORTS.map(s => <option key={s.value} value={s.value}>{s.label}</option>)}</select>
        <label className="cq-toggle"><input type="checkbox" checked={filters.due === 'today'}
          onChange={e => { update({ due: (e.target.checked ? 'today' : 'all') }); }} />次回日が来たものだけ</label>
        <button type="button" className="cq-btn cq-more-toggle" aria-expanded={panelOpen} aria-controls="cq-advanced"
          onClick={() => { setPanelOpen(o => !o); }}>詳細条件{detailCount > 0 && <span className="cq-badge">{detailCount}</span>}</button>
        <button type="button" className="cq-btn cq-btn-quiet" onClick={clearAll} disabled={!hasConditions && qDraft === ''}>条件をクリア</button>
      </div>
      {chips.length > 0 && <ul className="cq-chips" aria-label="適用中の条件">
        {chips.map(c => <li key={c.key} className="cq-chip"><span>{c.label}</span>
          <button type="button" aria-label={`「${c.label}」を外す`} onClick={() => { removeChip(c); }}>×</button></li>)}
      </ul>}
      <div id="cq-advanced" className="cq-advanced" hidden={!panelOpen}>
        <fieldset className="cq-stages"><legend>ステージ{filters.stages.length > 0 ? `(${String(filters.stages.length)} 件選択)` : '(すべて)'}</legend>
          {QUEUE_STAGES.map(s => <label key={s.id} className="cq-stage-chip"><input type="checkbox" checked={filters.stages.includes(s.id)}
            onChange={() => { toggleStage(s.id); }} />{s.label}</label>)}</fieldset>
        <div className="cq-ranges">
          <fieldset className="cq-range"><legend>次回架電日</legend>
            <label>から<input type="date" value={filters.nextFrom} onChange={e => { update({ nextFrom: e.target.value }); }} /></label>
            <label>まで<input type="date" value={filters.nextTo} onChange={e => { update({ nextTo: e.target.value }); }} /></label></fieldset>
          <fieldset className="cq-range"><legend>最終架電日</legend>
            <label>から<input type="date" value={filters.lastFrom} onChange={e => { update({ lastFrom: e.target.value }); }} /></label>
            <label>まで<input type="date" value={filters.lastTo} onChange={e => { update({ lastTo: e.target.value }); }} /></label></fieldset>
        </div>
      </div>
    </form>

    <div className="cq-body">
      <section className="cq-col cq-list-col" aria-label="架電先の一覧" aria-live="polite" aria-busy={state.phase === 'loading'}>
        <div className="cq-list-head">
          {state.phase === 'ready' && <p className="cq-count" role="status">{state.items.length} 件を表示
            {total !== null && <span title="電話番号なし等を除く前の参考値">(検索結果 {total} 件)</span>}</p>}
          {mode === 'live' && state.last !== null && <p className="cq-scope-note" data-testid="scope-note"
            title="HubSpot の全件から、上の所有者の選択で切り替えられます">所有者: {ownerScopeLabel(state.last.scope.owner, ownerNames)} を表示中</p>}
        </div>
        <div className="cq-list-scroll">
          {state.phase === 'invalid' && <div className="cq-notice cq-error" role="alert"><strong>条件を確認してください</strong>
            <ul>{state.invalid.map(m => <li key={m}>{m}</li>)}</ul></div>}
          {state.phase === 'loading' && <p role="status" className="cq-loading">読み込み中…</p>}
          {state.phase === 'unauthorized' && <div className="cq-notice cq-error" role="alert"><strong>表示できません</strong><p>{state.message}</p>
          </div>}
          {state.phase === 'error' && needsOwnerPick && <div className="cq-notice cq-warn" role="status" data-testid="owner-pick-prompt">
            <strong>所有者を選んでください</strong><p>{state.message}</p></div>}
          {state.phase === 'error' && !needsOwnerPick && <div className="cq-notice cq-error" role="alert"><strong>取得できませんでした</strong><p>{state.message}</p>
            <button type="button" onClick={reload}>再試行</button></div>}

          {state.phase === 'ready' && <>
            {state.last?.truncated && <div className="cq-notice cq-warn" role="status">HubSpot の検索は 1 万件までしか取得できないため、これより先は表示できません。条件を絞ってください。</div>}
            {notes.length > 0 && <div className="cq-notice cq-warn" role="status"><strong>一部の情報が欠けています</strong>
              <ul>{notes.map(n => <li key={n}>{n}</li>)}</ul></div>}
            {state.items.length === 0 && <div className="cq-notice cq-empty">
              {state.nextCursor ? <p>このページには表示できる行がありません。続きを読み込んでください。</p>
                : hasConditions ? <><strong>条件に一致する架電先がありません</strong><p>条件を変えるか、クリアしてください。</p>
                  <button type="button" onClick={clearAll}>条件をクリア</button></>
                  : <><strong>いま架電キューに出ている架電先はありません</strong></>}</div>}
            {state.items.length > 0 && <ul className="cq-list" aria-label="架電キュー" ref={listRef} onKeyDown={onListKey}>
              {state.items.map((item, i) => <QueueRow key={item.deal_id} item={item}
                selected={item.deal_id === selectedId} focusable={anyFocusable ? item.deal_id === selectedId : i === 0}
                onSelect={id => { select(id, 'click'); }} ownerName={item.owner_id ? ownerNames.get(item.owner_id) : undefined} />)}</ul>}
            {state.moreError && <div className="cq-notice cq-error" role="alert"><strong>続きを読み込めませんでした</strong><p>{state.moreError.message}</p>
              {state.moreError.kind === 'cursor_mismatch' && <button type="button" onClick={reload}>最初から読み直す</button>}</div>}
            {state.nextCursor && <button type="button" className="cq-btn cq-load-more" disabled={state.loadingMore} onClick={loadMore}>
              {state.loadingMore ? '読み込み中…' : 'さらに読み込む'}</button>}
            {!state.nextCursor && state.items.length > 0 && <p className="cq-end">これで最後です。</p>}
          </>}
        </div>
      </section>
      <section className="cq-col cq-detail" aria-label="選んだ架電先の詳細">
        {waitingKey ? <div className="cq-detail-scroll"><p role="status" className="cq-loading">詳細を読み込み中…</p></div>
          : <DealDetail state={detail.state} reload={detail.reload} zoom={zoom}
            ownerName={selectedOwner ? ownerNames.get(selectedOwner) : undefined} />}
        {/* 架電結果の入力欄を後の PR でここに置く (中央の列の下端に固定)。案件を選んでいるときだけ出す */}
        {selectedId !== null && <div className="cq-result-slot" data-testid="result-slot" data-deal-id={selectedId} />}
      </section>
      <div className="cq-col cq-phone-col">
        <ZoomPhonePanel zoom={zoom} iframeRef={iframeRef} />
      </div>
    </div>
  </div>;
}
