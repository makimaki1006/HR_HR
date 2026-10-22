import { formatPhoneForDisplay } from './phone';
import { useEffect, useEffectEvent, useReducer, useRef, useState } from 'react';
import type { ApplicantQueue } from '../../generated/ApplicantQueue';
import type { ApplicantDetail } from '../../generated/ApplicantDetail';
import type { ApplicantRelatedCard } from '../../generated/ApplicantRelatedCard';
import type { ApplicantRow } from '../../generated/ApplicantRow';
import type { ReactNode } from 'react';
import { Dock } from './Dock';
import { DEFAULT_LAYOUT, TAB_LABELS, dockReducer, parseLayout, serializeLayout } from './dockModel';
import type { DockLayout, PanelId } from './dockModel';
import { PhoneRow, ZOOM_LOGIN_DONE, ZOOM_LOGIN_REQUIRED, ZOOM_NO_RESPONSE } from './DealDetail';
import { ZoomPhonePanel } from './ZoomPhonePanel';
import type { ApplicantSection } from '../../generated/ApplicantSection';
import { CrmModeSwitch } from './CrmModeSwitch';
import { applicantQueueFixture, applicantAllQueueFixture, applicantDetailFixture } from './applicantFixture';
import { StageFilter } from './StageFilter';
import { CenterTabBar, CenterPanel, LinkView, PropLink, useCenterTabs } from './CenterTabs';
import { makeLinkTab, safeHttpUrl } from './centerLinks';
import { sanitizeHtml } from './activityHtml';
import { useZoomPhone } from './useZoomPhone';
import type { FixedLink } from './centerLinks';
import { toE164Jp } from './smartEmbed';
import { ApplicantEditor } from './ApplicantEditor';
import { ApplicantReminderBand } from './ApplicantReminders';
import { presenceText, useApplicantPresence, viewersOf } from './ApplicantPresence';
import './crm.css';
import './queue.css';
import './applicants.css';

function PresenceTag({ text }: { text: string }) {
  return text ? <span className="applicant-presence-tag">{text}</span> : null;
}
async function read<T>(url: string, signal: AbortSignal): Promise<T> {
  const response = await fetch(url, { signal, credentials: 'same-origin' });
  const body: unknown = await response.json();
  if (!response.ok) {
    const message = typeof body === 'object' && body !== null && 'message' in body && typeof body.message === 'string' ? body.message : '応募を読み込めません。時間をおいて再試行してください。';
    throw new Error(message);
  }
  return body as T;
}
function date(raw: string): string {
  if (!raw) return '—';
  const value = new Date(/^\d{11,}$/.test(raw) ? Number(raw) : raw);
  return Number.isNaN(value.getTime()) ? raw : new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', dateStyle: 'short', ...(raw.length > 10 ? { timeStyle: 'short' as const } : {}) }).format(value);
}
function Fields({ section, collapse = true }: { section: ApplicantSection; collapse?: boolean }) {
  return <dl className="crm-property-list">{section.fields.map((f, i) => <div key={`${f.label}-${String(i)}`} className={f.label.startsWith('以前の') ? 'applicant-legacy' : undefined}><dt>{f.label}</dt><dd>
    {safeHttpUrl(f.value) ? <PropLink url={f.value} /> : collapse && f.value.length > 200 ? <details><summary>内容を表示</summary><p>{f.value}</p></details> : f.value}
  </dd></div>)}</dl>;
}
export function ApplicantSheet({ detail }: { detail: ApplicantDetail | null }) {
  const url = detail?.sheet_url;
  if (!detail) return <p>応募を選択してください。</p>;
  if (!url) return <p>この応募には報告シートの URL がありません</p>;
  const tab = makeLinkTab('applicant-sheet', url, '報告シート');
  if (!tab || !safeHttpUrl(url)) return <p>報告シートの URL を確認してください。</p>;
  // サーバの判定に加えてブラウザでも埋め込み先を制限する。
  tab.embed = detail.sheet_embed && url.startsWith('https://docs.google.com/spreadsheets/') ? url : null;
  return <LinkView key={url} tab={tab} showFrameHint={false} />;
}
const LAYOUT_KEY = 'hrhr.crm.applicantsDock.v1';
/** 応募によらない固定の資料。URL が決まったらここに入れる */
const SHARED_LINKS: Record<'script' | 'customerSheet', string | null> = { script: null, customerSheet: null };
const SHARED_TABS = new Set(['applicant-script', 'applicant-customer-sheet']);
const DEFAULT_APPLICANT_LAYOUT: DockLayout = { ...DEFAULT_LAYOUT, columns: [DEFAULT_LAYOUT.columns[0], { panels: ['overview', 'result', 'activity'], active: 'result' }, DEFAULT_LAYOUT.columns[2]] };
const LABELS = { ...TAB_LABELS, queue: '応募一覧', overview: '応募の概要', result: '対応の入力', activity: 'メモ', links: '求人票・シート・スクリプト' };
function loadApplicantLayout(): DockLayout {
  try { const raw = localStorage.getItem(LAYOUT_KEY); const saved = raw === null ? DEFAULT_APPLICANT_LAYOUT : parseLayout(raw, DEFAULT_APPLICANT_LAYOUT); return new URLSearchParams(window.location.search).get('applicant') ? saved : dockReducer(saved, { type: 'activate', panel: 'queue' }); } catch { return DEFAULT_APPLICANT_LAYOUT; }
}
function ApplicantRules({ section }: { section: ApplicantSection | undefined }) {
  const [expanded, setExpanded] = useState(false);
  const long = (section?.fields.reduce((n, f) => n + f.label.length + f.value.length, 0) ?? 0) > 100 || (section?.fields.length ?? 0) > 2;
  return <section className="applicant-rules" aria-label="この求人の対応ルール"><h3>この求人の対応ルール</h3>
    <div className={!expanded && long ? 'applicant-rules-clamped' : undefined}>{section?.fields.length ? <Fields section={section} collapse={false} /> : <p>この求人の対応ルールは登録されていません</p>}</div>
    {long && <button type="button" aria-expanded={expanded} onClick={() => { setExpanded(v => !v); }}>{expanded ? '畳む' : '全部表示'}</button>}
  </section>;
}
function RelatedCard({ card }: { card: ApplicantRelatedCard }) {
  return <article className="applicant-related-record">
    <h4>{card.title || '名称未登録'}{card.current && <span className="applicant-current">今の契約</span>}</h4>
    <Fields section={{ title: card.title, fields: card.fields.slice(0, 4) }} />
    {card.fields.length > 4 && <details><summary>ほかの項目 {card.fields.length - 4} 件</summary><Fields section={{ title: card.title, fields: card.fields.slice(4) }} /></details>}
    <div className="applicant-related-links"><a href={card.deep_link} target="_blank" rel="noopener noreferrer">HubSpotで開く</a>
      {card.jobcopy_url && safeHttpUrl(card.jobcopy_url) && <a href={card.jobcopy_url} target="_blank" rel="noopener noreferrer">案件管理で見る</a>}</div>
  </article>;
}
export function ApplicantsScreen({ fixture = new URLSearchParams(window.location.search).get('mode') === 'fixture' }: { fixture?: boolean }) {
  const [layout, dispatch] = useReducer(dockReducer, undefined, loadApplicantLayout);
  useEffect(() => { try { localStorage.setItem(LAYOUT_KEY, serializeLayout(layout)); } catch { /* 保存できない場合も操作は可能 */ } }, [layout]);
  const relatedRefs = useRef<Record<string, HTMLElement | null>>({});
  const [relatedFocus, setRelatedFocus] = useState<{ kind: 'listings' | 'deals' } | null>(null);
  useEffect(() => {
    if (!relatedFocus) return;
    const el = relatedRefs.current[relatedFocus.kind];
    if (typeof el?.scrollIntoView === 'function') el.scrollIntoView({ block: 'start' });
    el?.focus({ preventScroll: true });
  }, [relatedFocus]);
  function showRelated(kind: 'listings' | 'deals') {
    dispatch({ type: 'activate', panel: 'properties' });
    setRelatedFocus({ kind });
  }
  const selectedRef = useRef<string | null>(null);
  const [contactTarget, setContactTarget] = useState<HTMLDivElement | null>(null);
  const [hearingTarget, setHearingTarget] = useState<HTMLDivElement | null>(null);
  const queueRefreshSeen = useRef(0);
  const detailRefreshSeen = useRef(0);
  const [queue, setQueue] = useState<ApplicantQueue | null>(null);
  const [scope, setScope] = useState(() => new URLSearchParams(window.location.search).get('scope') === 'all');
  const [target, setTarget] = useState(() => { const qt = new URLSearchParams(window.location.search).get('qt'); return qt === 'deal' || qt === 'listing' ? qt : 'applicant'; });
  const [query, setQuery] = useState(() => new URLSearchParams(window.location.search).get('q') ?? '');
  const [search, setSearch] = useState(query);
  const [stages, setStages] = useState<string[]>(() => new URLSearchParams(window.location.search).get('stages')?.split(',').filter(Boolean) ?? []);
  const [media, setMedia] = useState(() => new URLSearchParams(window.location.search).get('media') ?? '');
  const [selectedId, setSelectedId] = useState<string | null>(() => new URLSearchParams(window.location.search).get('applicant'));
  const commitSearch = useEffectEvent((value: string) => { applySearch(value); });
  useEffect(() => { const timer = window.setTimeout(() => { commitSearch(search); }, 300); return () => { window.clearTimeout(timer); }; }, [search]);
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    for (const [key, value] of Object.entries({ scope: scope ? 'all' : '', q: query, qt: target === 'applicant' ? '' : target, stages: stages.join(','), media, applicant: selectedId ?? '' })) {
      if (value) params.set(key, value); else params.delete(key);
    }
    window.history.replaceState(null, '', `${window.location.pathname}?${params.toString()}`);
  }, [scope, query, target, stages, media, selectedId]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [selected, setSelected] = useState<ApplicantRow | null>(null);
  const [detail, setDetail] = useState<ApplicantDetail | null>(null);
  const [error, setError] = useState('');
  const [detailError, setDetailError] = useState('');
  const [loading, setLoading] = useState(true);
  const [refresh, setRefresh] = useState(0);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [loginPrompt, setLoginPrompt] = useState<{ restore: boolean } | null>(null);
  const [dialNotice, setDialNotice] = useState('');
  const { zoom, iframeRef } = useZoomPhone(!fixture);
  useEffect(() => { selectedRef.current = selectedId; }, [selectedId]);
  useEffect(() => {
    const abort = new AbortController();
    const params = new URLSearchParams();
    if (scope) params.set('scope', 'all');
    if (query) params.set('q', query);
    if (target !== 'applicant') params.set('qt', target);
    if (stages.length) params.set('stages', stages.join(','));
    if (media) params.set('media', media);
    if (cursor) params.set('cursor', cursor);
    if (refresh !== queueRefreshSeen.current) params.set('fresh', '1');
    queueRefreshSeen.current = refresh;
    const source = scope ? applicantAllQueueFixture : applicantQueueFixture;
    const fixtureItems = source.items.filter(i => (!stages.length || stages.includes(i.stage_id)) && (!media || i.media === media) && (!query || (target === 'deal' ? i.deal : target === 'listing' ? i.job : i.name).includes(query)));
    const work = fixture ? Promise.resolve({ ...source, items: fixtureItems, total: fixtureItems.length }) : read<ApplicantQueue>(`/api/crm/applicants/queue?${params.toString()}`, abort.signal);
    void work.then(data => { if (abort.signal.aborted) return; setQueue(old => cursor && old ? { ...data, items: [...old.items, ...data.items] } : data); setError(''); setLoading(false); }).catch((e: unknown) => { if (!abort.signal.aborted) { setError(e instanceof Error ? e.message : '応募を読み込めません'); setLoading(false); } });
    return () => { abort.abort(); };
  }, [fixture, scope, query, target, stages, media, cursor, refresh]);
  useEffect(() => {
    if (!selectedId) return;
    const abort = new AbortController();
    const fresh = refresh !== detailRefreshSeen.current;
    detailRefreshSeen.current = refresh;
    const work = fixture ? Promise.resolve(applicantDetailFixture(selectedId)) : read<ApplicantDetail>(`/api/crm/applicants/${encodeURIComponent(selectedId)}${fresh ? '?fresh=1' : ''}`, abort.signal);
    void work.then(data => { if (!abort.signal.aborted) { setDetail(data); setDetailError(''); } }).catch((e: unknown) => { if (!abort.signal.aborted) setDetailError(e instanceof Error ? e.message : '詳細を読み込めません'); });
    return () => { abort.abort(); };
  }, [fixture, selectedId, refresh]);
  function applySearch(value: string) { const next = value.trim(); if (next === query) return; dispatch({ type: 'activate', panel: 'queue' }); setQuery(next); setCursor(null); setQueue(null); setLoading(true); }
  function filter(nextStages: string[], nextMedia: string) { dispatch({ type: 'activate', panel: 'queue' }); setStages(nextStages); setMedia(nextMedia); setCursor(null); setQueue(null); setLoading(true); setSelected(null); setSelectedId(null); setDetail(null); }
  function select(item: ApplicantRow) { if (selectedId === item.id) return; setSelected(item); setSelectedId(item.id); setDetail(null); setDetailError(''); setDialNotice(''); }
  const headline = detail?.row ?? selected;
  const viewers = useApplicantPresence(fixture, selectedId, detail?.phone ?? headline?.phone ?? '');
  const presenceHere = selectedId ? presenceText(viewersOf(viewers, selectedId, detail?.phone ?? headline?.phone ?? '')) : '';
  const headlineName = (headline?.name ?? '').trim();
  const firstPipeline = queue?.pipelines[0];
  const busy = zoom.call.phase === 'connected' || zoom.call.phase === 'ringing' || zoom.pending !== null;
  const dialZoom = { ...zoom, dial: (raw: string | null | undefined) => {
    if (!zoom.heard && zoom.embed !== 'timeout' && zoom.embed !== 'disabled' && toE164Jp(raw) !== null && !busy) { setLoginPrompt({ restore: !drawerOpen }); setDrawerOpen(true); setDialNotice(ZOOM_LOGIN_REQUIRED); return 'login_required' as const; }
    return zoom.dial(raw);
  } };
  const [wasStalled, setWasStalled] = useState(zoom.stalled);
  if (wasStalled !== zoom.stalled) {
    setWasStalled(zoom.stalled);
    if (zoom.stalled) { setDrawerOpen(true); setDialNotice(ZOOM_NO_RESPONSE); }
  }
  if (loginPrompt && zoom.heard) {
    if (loginPrompt.restore) setDrawerOpen(false);
    setLoginPrompt(null);
    setDialNotice(ZOOM_LOGIN_DONE);
  }
  const fixed: FixedLink[] = [
    { id: 'applicant-posting', label: '求人票', url: detail?.posting_url ?? null, missing: 'この応募には求人票の URL がありません。' },
    { id: 'applicant-sheet', label: '報告シート', url: detail?.sheet_url ?? null, missing: 'この応募には報告シートの URL がありません' },
    { id: 'applicant-script', label: 'スクリプト', url: SHARED_LINKS.script, missing: 'スクリプトの URL がまだ設定されていません。' },
    { id: 'applicant-customer-sheet', label: '顧客シート', url: SHARED_LINKS.customerSheet, missing: '顧客シートの URL がまだ設定されていません。' },
  ];
  const tabs = useCenterTabs(selectedId, fixed, 'applicant-posting');
  const panels: Record<PanelId, ReactNode> = {
    queue: <section className="cq-col cq-list" aria-label="応募一覧">
      <div className="cq-list-head"><h1>{scope ? 'すべての応募' : '一次対応が必要な応募'}</h1><span>{queue ? `${String(queue.total)}件` : '読み込み中…'}</span></div>
      <div className="cq-list-scroll">
        {loading && <p role="status">応募を読み込み中…</p>}
        <ul className="cq-list" aria-label="応募キュー">{queue?.items.map(item => <li className={`cq-row${selectedId === item.id ? ' is-selected' : ''}`} key={item.id}>
          <button className="cq-row-button" aria-current={selectedId === item.id ? 'true' : undefined} onClick={() => { select(item); }}>
            <span className="cq-row-l1"><strong className="cq-row-company">{item.name || '氏名未登録'}</strong><span className="cq-stage">{item.stage}</span></span>
            <span className="cq-row-l2"><span className="cq-phone">{(formatPhoneForDisplay(item.phone) ?? item.phone) || '電話番号未登録'}</span></span>
            <span className="cq-row-extra">{item.deal} · {item.job}</span>
            <span className="cq-row-l3"><span>応募日 {date(item.date)}</span><span>{item.media}</span></span>
            {item.appointment && <span className="cq-row-extra">約束架電日時 {date(item.appointment)}</span>}
            <PresenceTag text={presenceText(viewersOf(viewers, item.id, item.phone))} />
          </button>
        </li>)}</ul>
        {queue?.items.length === 0 && <><p>条件に一致する応募はありません。</p>{query && !scope && <button onClick={() => { setScope(true); filter([], media); }}>すべての応募で探す</button>}</>}
        {queue?.next_cursor && <button onClick={() => { setCursor(queue.next_cursor); }}>続きを読み込む</button>}
      </div>
    </section>,
    properties: <aside className="dock-scroll applicants-properties" aria-label="応募のプロパティ">
      {!detail && <p>応募を選択し、詳細の読み込みをお待ちください。</p>}
      {detail?.sections.map((section, i) => i === 1 ? null : <section className="crm-card" key={section.title}><h3>{section.title}</h3>
        {i === 2 && <p><strong>{detail.row.deal}</strong><br />{detail.row.job}</p>}
        <Fields section={{ ...section, fields: section.fields.filter(f => !detail.writes_enabled || !detail.editable.some(e => e.label === f.label && (i === 0 ? e.section === '連絡先' : i === 4 && e.section === 'ヒアリング'))) }} />
        {i === 0 && <div ref={setContactTarget} />}{i === 4 && <div ref={setHearingTarget} />}
      </section>)}
      {detail && <>
        {detail.related_notice && <p role="status">{detail.related_notice}</p>}
        {(['listings', 'deals'] as const).map(kind => {
          const cards = detail[kind];
          return <section className="crm-card applicant-related" key={kind} aria-label={kind === 'listings' ? '関連の求人票' : '関連の取引'} tabIndex={-1} ref={el => { relatedRefs.current[kind] = el; }}>
            <h3>{kind === 'listings' ? '求人票' : '取引'}</h3>
            {cards.length === 0 ? <p>関連する{kind === 'listings' ? '求人票' : '取引'}はありません。</p> : <>
              {cards[0] && <RelatedCard card={cards[0]} />}
              {cards.length > 1 && <details key={detail.row.id}><summary>ほかの{kind === 'listings' ? '求人票' : '取引'} {cards.length - 1} 件</summary>{cards.slice(1).map(card => <RelatedCard key={card.id} card={card} />)}</details>}
            </>}
          </section>;
        })}
      </>}
    </aside>,
    overview: <main className="applicants-detail" aria-label="応募の詳細">
      {!selectedId ? <p className="dock-placeholder">一覧から応募を選択してください。</p> : <>
        <article className="wd wd-compact"><div className="wd-top wd-top-compact">
          <div className="wd-line wd-line-1"><h2 className="wd-line-company">{headline ? headlineName || '氏名未登録' : '読み込み中…'}</h2>{detail && <span className="wd-line-end"><a href={detail.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a></span>}</div>
          <p className="applicant-job"><button type="button" disabled={!detail} onClick={() => { showRelated('deals'); }}>{headline?.deal.length ? headline.deal : '取引を確認'}</button> · <button type="button" disabled={!detail} onClick={() => { showRelated('listings'); }}>{headline?.job.length ? headline.job : '求人票を確認'}</button></p>
          <div className="wd-line wd-line-2"><PhoneRow label="応募者" raw={detail?.phone ?? headline?.phone ?? ''} zoom={dialZoom} compact blocked={!!detail?.stop_reason || busy || !detail} blockReason={!detail ? (detailError ? '詳細を確認できません' : '読み込み中…') : detail.stop_reason ?? (busy ? '通話中です' : null)} /></div>
          <p className="applicant-meta">{headline?.stage} · {headline?.media} · 応募日 {date(headline?.date ?? '')}</p>
          {presenceHere && <p className="applicant-presence" role="status">{presenceHere}</p>}
          {detail?.stop_reason && <p role="alert">架電禁止・ブロック: {detail.stop_reason}</p>}{dialNotice && <p role="status">{dialNotice}</p>}
        </div></article>
        {detailError && <p role="alert">{detailError}<button onClick={() => { setRefresh(v => v + 1); }}>再試行</button></p>}
        {!detail && !detailError && <p role="status">詳細を読み込み中…</p>}
        {detail && <ApplicantRules key={detail.row.id} section={detail.sections[1]} />}
      </>}
    </main>,
    result: <ApplicantEditor detail={detail} fixture={fixture} contactTarget={contactTarget} hearingTarget={hearingTarget} onNext={id => {
      if (selectedRef.current !== id) return;
      const index = queue?.items.findIndex(item => item.id === id) ?? -1;
      const next = queue?.items[index + 1];
      if (next) select(next);
      else { setDetail(null); setSelected(null); setSelectedId(null); setCursor(null); setRefresh(v => v + 1); }
    }} />,
    activity: <section className="dock-scroll applicant-notes" aria-label="応募のメモ">
      {!detail ? <p>応募を選択してください。</p> : <>{detail.notes.length === 0 && <p>メモはありません。</p>}{detail.notes.slice(0, 5).map((n, i) => <article key={i}><time>{date(n.date)}</time><div dangerouslySetInnerHTML={{ __html: sanitizeHtml(n.html) }} /></article>)}</>}
    </section>,
    links: <aside className="cq-linkpanel applicants-right" aria-label="求人票・シート・スクリプト">
      <div className="cq-linkpanel-head"><CenterTabBar tabs={tabs} /></div>
      <div className="cq-cpanels">{tabs.fixed.map(item => <CenterPanel key={item.def.id} id={item.def.id} active={tabs.active === item.def.id}>
        {SHARED_TABS.has(item.def.id) ? (item.tab === null ? <div className="dock-scroll"><p>{item.def.missing}</p></div> : item.opened && <LinkView key={item.tab.url} tab={item.tab} />) : !selectedId ? <p className="dock-placeholder">応募を選択してください。</p> : !detail ? <p role="status">詳細を読み込み中…</p> : item.def.id === 'applicant-sheet' ? (item.opened && <ApplicantSheet detail={detail} />) : item.tab === null ? <div className="dock-scroll"><p>{item.def.missing}</p>{detail.jobcopy_url && safeHttpUrl(detail.jobcopy_url) && <a href={detail.jobcopy_url} target="_blank" rel="noopener noreferrer">案件管理で見る</a>}</div> : item.opened && <LinkView key={item.tab.url} tab={item.tab} actions={detail.jobcopy_url && safeHttpUrl(detail.jobcopy_url) ? <a href={detail.jobcopy_url} target="_blank" rel="noopener noreferrer">案件管理で見る</a> : null} />}
      </CenterPanel>)}</div>
    </aside>,
  };
  return <div className="crm-app cq-app applicants-app">
    <header className="applicants-head"><a href="/">HR_HR</a><strong>CRM</strong><CrmModeSwitch applicants /><span>{fixture ? '架空サンプル' : 'HubSpot の応募'}</span>{!fixture && <button onClick={() => { setDrawerOpen(v => !v); }}>Zoom</button>}<button onClick={() => { setCursor(null); setDetail(null); setRefresh(v => v + 1); }}>最新にする</button></header>
    <div className="cq-filter-summary applicants-filter-summary"><span>{scope ? 'すべての応募' : '一次対応が必要な未完了'} · ステージ: {stages.length ? `${String(stages.length)}件選択` : 'すべて'} · 応募媒体: {media || 'すべて'}</span><button type="button" aria-expanded={filtersOpen} onClick={() => { setFiltersOpen(v => !v); }}>条件を変更</button></div>
    <form className="applicants-search" role="search" onSubmit={e => { e.preventDefault(); applySearch(search); }}><label>検索対象<select value={target} onChange={e => { dispatch({ type: 'activate', panel: 'queue' }); setTarget(e.target.value); setCursor(null); setQueue(null); setLoading(true); }}><option value="applicant">応募者</option><option value="deal">取引</option><option value="listing">求人</option></select></label><label>応募を検索<input type="search" value={search} onChange={e => { setSearch(e.target.value); }} placeholder={target === 'deal' ? '取引名' : target === 'listing' ? '求人名' : '応募者名'} /></label></form>
    {filtersOpen && <div className="applicants-toolbar">
      <label className={`cq-field cq-target-label${scope ? ' is-set' : ''}`}>範囲<select aria-label="範囲" value={scope ? 'all' : 'open'} onChange={e => { setScope(e.target.value === 'all'); filter([], media); }}><option value="open">一次対応が必要な未完了</option><option value="all">すべての応募</option></select></label>
      {queue && firstPipeline && <StageFilter all pipeline={{ ...firstPipeline, fallbackName: '', unknownStages: [], stages: firstPipeline.stages.map(s => ({ ...s, rule: 'all' })) }}
        pipelines={queue.pipelines.map(p => ({ ...p, fallbackName: '', unknownStages: [], stages: p.stages.map(s => ({ ...s, rule: 'all' })) }))}
        selected={stages} onApply={s => { filter(s, media); }} />}
      <label>応募媒体<select value={media} onChange={e => { filter(stages, e.target.value); }}><option value="">すべて</option>{queue?.media_options.map(o => <option value={o.id} key={o.id}>{o.label}</option>)}</select></label>
    </div>}
    <ApplicantReminderBand fixture={fixture} refresh={refresh} onOpen={select} viewers={viewers} />
    {error && <p role="alert">{error}<button onClick={() => { setRefresh(v => v + 1); }}>再試行</button></p>}
    <Dock layout={layout} dispatch={dispatch} panels={panels} labels={LABELS} />
    {!fixture && <div className={`cq-zoom-drawer${drawerOpen ? ' is-open' : ''}`} inert={!drawerOpen} aria-hidden={!drawerOpen} onKeyDown={e => { if (e.key === 'Escape') setDrawerOpen(false); }}><ZoomPhonePanel zoom={zoom} iframeRef={iframeRef} onClose={() => { setDrawerOpen(false); }} /></div>}
  </div>;
}
