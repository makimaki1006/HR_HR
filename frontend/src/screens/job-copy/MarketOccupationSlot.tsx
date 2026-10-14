import { createPortal } from 'react-dom';
import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { apiGet, apiPut, ApiHttpError, AuthRequiredError } from '../../api/client';
import type { JobCopyRecord } from './data';
import { designatedTitle, indeedTitleState } from './marketMatch';
import type { MarketData } from './marketChartModel';
import './market-occupation.css';

export interface IndeedTitleChange { indeedTitle: string | null; indeedTitleState: 'set' | 'none' }
interface Props { record: JobCopyRecord; prefecture: string | null; onChanged: (next: IndeedTitleChange) => void }

const NONE = '__none__';
const normalize = (value: string) => value.normalize('NFKC').replace(/\s+/gu, '').toLowerCase();

export function errorText(error: unknown): string {
  if (error instanceof AuthRequiredError) return 'Googleでログインしてから保存してください。';
  if (error instanceof ApiHttpError) {
    const code = typeof error.body === 'object' && error.body !== null && 'code' in error.body ? (error.body as { code?: unknown }).code : undefined;
    if (code === 'indeed_title_property_missing') return '保存先の項目がまだ用意されていません';
    if (code === 'indeed_title_unknown') return '選んだ職種が市場データの一覧にありません。選び直してください。';
    if (code === 'hubspot_busy' || error.status === 429) return 'HubSpotが混み合っています。少し待ってからもう一度保存してください。';
  }
  return '保存できませんでした。何も変更されていません。もう一度お試しください。';
}

/** 職種 + 状態の表示と、市場と比べる Indeed 職種を人が指定するための右側の入力欄。自動では決めない。 */
export function MarketOccupationSlot({ record, prefecture, onChanged }: Props) {
  const state = indeedTitleState(record);
  const [open, setOpen] = useState(false);
  const label = state === 'set' ? designatedTitle(record) ?? '' : state === 'none' ? '市場と比べない' : '未指定';
  return <div className="mo-slot" data-state={state}>
    <span className="mo-label">職種</span>
    {state !== 'unset' && <strong className="mo-value">{label}</strong>}
    <span className={`mo-chip mo-chip-${state}`}>{state === 'set' ? '指定済み' : state === 'none' ? '市場と比べない' : '未指定'}</span>
    {prefecture && <span className="mo-pref">都道府県：{prefecture}</span>}
    <button type="button" className="jc-button mo-open" onClick={() => { setOpen(true); }}>{state === 'unset' ? '職種を指定する' : '職種を変える'}</button>
    {state === 'unset' && <a className="mo-bulk-link" href="/app/job-copy?view=occupations">職種が未指定の求人をまとめて指定する</a>}
    {open && <OccupationDrawer record={record} onClose={() => { setOpen(false); }} onSaved={next => { onChanged(next); setOpen(false); }} />}
  </div>;
}

function OccupationDrawer({ record, onClose, onSaved }: { record: JobCopyRecord; onClose: () => void; onSaved: (next: IndeedTitleChange) => void }) {
  const headingId = useId();
  const search = useRef<HTMLInputElement>(null);
  const [titles, setTitles] = useState<string[] | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [query, setQuery] = useState('');
  // No pre-selection: a person has to choose.
  const [choice, setChoice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    let cancelled = false;
    void apiGet<MarketData>('/api/job-copy/market').then(result => {
      if (cancelled) return;
      if (result.ok) setTitles(result.data.titles); else setLoadError(true);
    });
    return () => { cancelled = true; };
  }, []);
  useEffect(() => { search.current?.focus(); }, []);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape' && !saving) onClose(); };
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('keydown', onKey); };
  }, [onClose, saving]);
  const shown = useMemo(() => {
    const needle = normalize(query);
    return (titles ?? []).filter(title => !needle || normalize(title).includes(needle));
  }, [titles, query]);
  const fields = record.currentListingFields ?? {};
  const hints: [string, string][] = [['求人票の職種', fields['職種'] ?? ''], ['Indeed表示職種名', fields['Indeed表示職種名'] ?? '']];
  const shownHints = hints.filter(([, value]) => value.trim());

  async function save() {
    if (choice === null || !record.hubspotId) return;
    setSaving(true); setError('');
    const body = choice === NONE ? { title: null, compare: false } : { title: choice, compare: true };
    const result = await apiPut<{ indeed_title: string | null; indeed_title_state: 'set' | 'none' }>(`/api/job-copy/listings/${encodeURIComponent(record.hubspotId)}/indeed-title`, body);
    setSaving(false);
    if (result.ok) onSaved({ indeedTitle: result.data.indeed_title, indeedTitleState: result.data.indeed_title_state });
    else setError(errorText(result.error));
  }

  return createPortal(<div className="mo-backdrop" onMouseDown={event => { if (event.target === event.currentTarget && !saving) onClose(); }}>
    <aside className="mo-drawer" role="dialog" aria-modal="true" aria-labelledby={headingId}>
      <header><h2 id={headingId}>市場と比べる職種を指定する</h2><button type="button" className="jc-button" aria-label="閉じる" onClick={onClose} disabled={saving}>×</button></header>
      <div className="mo-body">
        <section className="mo-hints" aria-label="選ぶときの手がかり">
          <h3>選ぶときの手がかり（自動では決めません）</h3>
          {shownHints.length ? <dl>{shownHints.map(([name, value]) => <div key={name}><dt>{name}</dt><dd>{value}</dd></div>)}</dl> : <p>この求人から読み取れる手がかりはありません。</p>}
        </section>
        <label className="mo-search">職種を探す<input ref={search} type="search" value={query} onChange={event => { setQuery(event.target.value); }} placeholder="例：ドライバー" /></label>
        {loadError && <p role="alert">職種の一覧を取得できませんでした。</p>}
        {!titles && !loadError && <p role="status">職種の一覧を読み込み中…</p>}
        {titles && <fieldset className="mo-list">
          <legend className="jc-visually-hidden">Indeed の職種</legend>
          <label className="mo-option mo-option-none"><input type="radio" name="mo-title" checked={choice === NONE} onChange={() => { setChoice(NONE); }} />市場と比べない</label>
          {shown.map(title => <label className="mo-option" key={title}><input type="radio" name="mo-title" checked={choice === title} onChange={() => { setChoice(title); }} />{title}</label>)}
          {!shown.length && <p>一致する職種はありません。</p>}
        </fieldset>}
        <p className="mo-note">この求人だけに指定します。HubSpot の求人の項目「比べる Indeed 職種」に入り、HubSpot の画面からも直せます。</p>
        {error && <p role="alert" className="mo-error">{error}</p>}
      </div>
      <footer><button type="button" className="jc-button" onClick={onClose} disabled={saving}>やめる</button><button type="button" className="jc-button" disabled={choice === null || saving || !record.hubspotId} onClick={() => { void save(); }}>この職種を指定する</button></footer>
    </aside>
  </div>, document.body);
}
