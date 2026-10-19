import { useEffect, useRef, useState } from 'react';
import type { ApplicantQueue } from '../../generated/ApplicantQueue';
import type { ApplicantDetail } from '../../generated/ApplicantDetail';
import type { ApplicantRow } from '../../generated/ApplicantRow';
import type { ApplicantSection } from '../../generated/ApplicantSection';
import { CrmModeSwitch } from './CrmModeSwitch';
import { applicantQueueFixture, applicantDetailFixture } from './applicantFixture';
import { StageFilter } from './StageFilter';
import { LinkView, PropLink } from './CenterTabs';
import { makeLinkTab, safeHttpUrl } from './centerLinks';
import { sanitizeHtml } from './activityHtml';
import { useZoomPhone } from './useZoomPhone';
import { ZOOM_EMBED_SRC, toE164Jp } from './smartEmbed';
import { ApplicantEditor } from './ApplicantEditor';
import './crm.css';
import './queue.css';
import './applicants.css';

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
function Fields({ section }: { section: ApplicantSection }) {
  return <dl className="crm-property-list">{section.fields.map((f, i) => <div key={`${f.label}-${String(i)}`} className={f.label.startsWith('以前の') ? 'applicant-legacy' : undefined}><dt>{f.label}</dt><dd>
    {safeHttpUrl(f.value) ? <PropLink url={f.value} /> : f.value.length > 200 ? <details><summary>内容を表示</summary><p>{f.value}</p></details> : f.value}
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
export function ApplicantsScreen({ fixture = new URLSearchParams(window.location.search).get('mode') === 'fixture' }: { fixture?: boolean }) {
  const detailRef = useRef<HTMLElement>(null);
  const selectedRef = useRef<string | null>(null);
  const [contactTarget, setContactTarget] = useState<HTMLDivElement | null>(null);
  const [hearingTarget, setHearingTarget] = useState<HTMLDivElement | null>(null);
  const queueRefreshSeen = useRef(0);
  const detailRefreshSeen = useRef(0);
  const [queue, setQueue] = useState<ApplicantQueue | null>(null);
  const [stages, setStages] = useState<string[]>([]);
  const [media, setMedia] = useState('');
  const [cursor, setCursor] = useState<string | null>(null);
  const [selected, setSelected] = useState<ApplicantRow | null>(null);
  const [detail, setDetail] = useState<ApplicantDetail | null>(null);
  const [error, setError] = useState('');
  const [detailError, setDetailError] = useState('');
  const [loading, setLoading] = useState(true);
  const [refresh, setRefresh] = useState(0);
  const [right, setRight] = useState<'sheet' | 'zoom'>('sheet');
  const [dialNotice, setDialNotice] = useState('');
  const { zoom, iframeRef } = useZoomPhone(!fixture);
  useEffect(() => { selectedRef.current = selected?.id ?? null; }, [selected]);
  useEffect(() => {
    const abort = new AbortController();
    const params = new URLSearchParams();
    if (stages.length) params.set('stages', stages.join(','));
    if (media) params.set('media', media);
    if (cursor) params.set('cursor', cursor);
    if (refresh !== queueRefreshSeen.current) params.set('fresh', '1');
    queueRefreshSeen.current = refresh;
    const fixtureItems = applicantQueueFixture.items.filter(i => (!stages.length || stages.includes(i.stage_id)) && (!media || i.media === media));
    const work = fixture ? Promise.resolve({ ...applicantQueueFixture, items: fixtureItems, total: fixtureItems.length }) : read<ApplicantQueue>(`/api/crm/applicants/queue?${params.toString()}`, abort.signal);
    void work.then(data => { if (abort.signal.aborted) return; setQueue(old => cursor && old ? { ...data, items: [...old.items, ...data.items] } : data); setError(''); setLoading(false); }).catch((e: unknown) => { if (!abort.signal.aborted) { setError(e instanceof Error ? e.message : '応募を読み込めません'); setLoading(false); } });
    return () => { abort.abort(); };
  }, [fixture, stages, media, cursor, refresh]);
  useEffect(() => {
    if (!selected) return;
    const abort = new AbortController();
    const fresh = refresh !== detailRefreshSeen.current;
    detailRefreshSeen.current = refresh;
    const work = fixture ? Promise.resolve(applicantDetailFixture(selected.id)) : read<ApplicantDetail>(`/api/crm/applicants/${selected.id}${fresh ? '?fresh=1' : ''}`, abort.signal);
    void work.then(data => { if (!abort.signal.aborted) { setDetail(data); setDetailError(''); } }).catch((e: unknown) => { if (!abort.signal.aborted) setDetailError(e instanceof Error ? e.message : '詳細を読み込めません'); });
    return () => { abort.abort(); };
  }, [fixture, selected, refresh]);
  function filter(nextStages: string[], nextMedia: string) { setStages(nextStages); setMedia(nextMedia); setCursor(null); setQueue(null); setLoading(true); setSelected(null); setDetail(null); }
  function select(item: ApplicantRow) { if (detailRef.current) detailRef.current.scrollTop = 0; if (selected?.id === item.id) return; setSelected(item); setDetail(null); setDetailError(''); setDialNotice(''); }
  const headline = detail?.row ?? selected;
  const headlineName = (headline?.name ?? '').trim();
  const firstPipeline = queue?.pipelines[0];
  const canDial = detail && !fixture && !detail.stop_reason && toE164Jp(detail.phone) !== null && zoom.embed === 'loaded' && zoom.call.phase !== 'connected' && zoom.call.phase !== 'ringing';
  return <div className="crm-app applicants-app">
    <header className="applicants-head"><a href="/">HR_HR</a><strong>CRM</strong><CrmModeSwitch applicants /><span>{fixture ? '架空サンプル' : 'HubSpot の応募'}</span><button onClick={() => { setCursor(null); setDetail(null); setRefresh(v => v + 1); }}>最新にする</button></header>
    <div className="applicants-toolbar">
      <h1>一次対応が必要な応募</h1><span>{queue ? `${String(queue.total)}件` : '読み込み中…'}</span>
      {queue && firstPipeline && <StageFilter all pipeline={{ ...firstPipeline, fallbackName: '', unknownStages: [], stages: firstPipeline.stages.map(s => ({ ...s, rule: 'all' })) }}
        pipelines={queue.pipelines.map(p => ({ ...p, fallbackName: '', unknownStages: [], stages: p.stages.map(s => ({ ...s, rule: 'all' })) }))}
        selected={stages} onApply={s => { filter(s, media); }} />}
      <label>応募媒体<select value={media} onChange={e => { filter(stages, e.target.value); }}><option value="">すべて</option>{queue?.media_options.map(o => <option value={o.id} key={o.id}>{o.label}</option>)}</select></label>
    </div>
    {error && <p role="alert">{error}<button onClick={() => { setRefresh(v => v + 1); }}>再試行</button></p>}
    <div className="applicants-columns">
      <aside className="applicants-list" aria-label="応募一覧">
        {loading && <p role="status">応募を読み込み中…</p>}
        {queue?.items.map(item => <button className="applicant-row" key={item.id} aria-current={selected?.id === item.id ? 'true' : undefined} onClick={() => { select(item); }}>
          <strong>{item.name || '氏名未登録'}</strong><span>{item.deal}</span><span>{item.job}</span><small>応募日 {date(item.date)} · {item.stage}</small><small>約束架電日時 {date(item.appointment)}</small><small>{item.media}</small>
        </button>)}
        {queue?.items.length === 0 && <p>条件に一致する応募はありません。</p>}
        {queue?.next_cursor && <button onClick={() => { setCursor(queue.next_cursor); }}>続きを読み込む</button>}
      </aside>
      <main ref={detailRef} className="applicants-detail" aria-label="応募の詳細">
        {!selected ? <p>一覧から応募を選択してください。</p> : <>
          <section className="crm-card applicant-identity"><h2>{headlineName || '氏名未登録'}</h2><p>{headline?.stage} · {headline?.media} · 応募日 {date(headline?.date ?? '')}</p>
            {detail && <><a href={detail.deep_link} target="_blank" rel="noreferrer">HubSpotで開く</a><p>{detail.phone || '電話番号未登録'}</p>
              <button disabled={!canDial} onClick={() => { if (!canDial) return; setRight('zoom'); const result = zoom.dial(detail.phone); setDialNotice(result === 'sent' ? '発信を依頼しました' : 'Zoom の読み込みとログインを確認してください'); }}>Zoom で発信</button>
              {detail.stop_reason && <p role="alert">架電禁止・ブロック: {detail.stop_reason}</p>}{dialNotice && <p role="status">{dialNotice}</p>}
              {detail.sections[0] && <Fields section={{ ...detail.sections[0], fields: detail.sections[0].fields.filter(f => !detail.writes_enabled || !detail.editable.some(e => e.section === '連絡先' && e.label === f.label)) }} />}<div ref={setContactTarget} /></>}
          </section>
          {detailError && <p role="alert">{detailError}<button onClick={() => { setRefresh(v => v + 1); }}>再試行</button></p>}
          {!detail && !detailError && <p role="status">詳細を読み込み中…</p>}
          {detail?.sections.map((section, i) => i === 0 ? null : i < 3 ? <section className="crm-card" key={section.title}><h3>{section.title}</h3>
            {i === 2 && <p><strong>{detail.row.deal}</strong><br />{detail.row.job}</p>}
            <Fields section={section} />
            {i === 2 && detail.jobs.map(job => <p key={job.url}>{job.label} <a href={job.url} target="_blank" rel="noreferrer">HubSpotで開く</a></p>)}
            {i === 1 && section.fields.length === 0 && <p>この求人の対応ルールは登録されていません</p>}
          </section> : <details className="crm-card" key={section.title} open={i === 4}><summary>{section.title}</summary><Fields section={section} />{i === 4 && <div ref={setHearingTarget} />}</details>)}
          {detail && <section className="crm-card"><h3>メモ</h3>{detail.notes.length === 0 && <p>メモはありません。</p>}{detail.notes.map((n, i) => <article key={i}><time>{date(n.date)}</time><div dangerouslySetInnerHTML={{ __html: sanitizeHtml(n.html) }} /></article>)}</section>}
        </>}
      </main>
      <aside className="applicants-right" aria-label="報告シートと電話">
        <div role="tablist" aria-label="応募の補助画面">{(['sheet', 'zoom'] as const).map(t => <button key={t} id={`applicant-tab-${t}`} aria-controls={`applicant-panel-${t}`} role="tab" aria-selected={right === t} tabIndex={right === t ? 0 : -1} onKeyDown={e => {
          if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) {
            e.preventDefault(); const next = e.key === 'Home' ? 'sheet' : e.key === 'End' ? 'zoom' : right === 'sheet' ? 'zoom' : 'sheet';
            setRight(next); document.getElementById(`applicant-tab-${next}`)?.focus();
          }
        }} onClick={() => { setRight(t); }}>{t === 'sheet' ? '報告シート' : 'Zoom'}</button>)}</div>
        <div role="tabpanel" id="applicant-panel-sheet" aria-labelledby="applicant-tab-sheet" hidden={right !== 'sheet'}><ApplicantSheet detail={detail} /></div>
        <div role="tabpanel" id="applicant-panel-zoom" aria-labelledby="applicant-tab-zoom" hidden={right !== 'zoom'}>
          {fixture ? <p>架空サンプルでは発信できません</p> : <><p>{zoom.embed === 'timeout' ? 'Zoom Phone を読み込めません。再読み込みしてください。' : 'Zoom にログインしてから発信してください。'}</p><iframe ref={iframeRef} title="Zoom Phone" src={ZOOM_EMBED_SRC} allow="microphone; clipboard-read; clipboard-write" onLoad={zoom.onLoad} /></>}
        </div>
        <ApplicantEditor detail={detail} fixture={fixture} contactTarget={contactTarget} hearingTarget={hearingTarget} onNext={id => {
          if (selectedRef.current !== id) return;
          const index = queue?.items.findIndex(item => item.id === id) ?? -1;
          const next = queue?.items[index + 1];
          if (next) select(next);
          else { setDetail(null); setSelected(null); setCursor(null); setRefresh(v => v + 1); }
        }} />

      </aside>
    </div>
  </div>;
}
