import { useEffect, useId, useRef, useState } from 'react';
import { apiGet } from '../../api/client';
import type { JobCopyRecord } from './data';
import type { HubSpotListing, HubSpotListingPage } from './hubspotListings';
import { listingRecord, mediaLabel, publicationLabel } from './hubspotListings';
import { loadCompany, loadVersions } from './listingDetailCache';
import { loadApplicationDates } from './listingApplicationDates';
import { PREFECTURE_NAMES } from './applicantArea';
import { buildDistribution } from './applicantCompositionModel';
import { comparisonAttributes, comparisonPeriod, commonRecentPeriod, fourWeekLabel, contentDifferences, jstToday, localCandidates, periodPerformance, variantDistribution } from './abComparisonModel';
import type { ComparisonPeriod } from './abComparisonModel';
import { APPLICATION_TEXT_SOURCES, reasonSourceLabels } from './applicantReasonsModel';
import { formatDateJst, formatPeriodJst, formatYen, orderCategories } from './format';
import { ImageGallery } from './ImageGallery';
import './ab-comparison.css';

const number = (value: number | null | undefined, unit = '') => value == null ? '未取得' : `${value.toLocaleString('ja-JP', { maximumFractionDigits: 2 })}${unit}`;
const dimensions = [{ id: 'gender', label: '性別' }, { id: 'age', label: '年代' }, { id: 'prefecture', label: '都道府県' }, { id: 'municipality', label: '市区町村' }] as const;
interface Selection { id: string; title: string; row?: HubSpotListing; job?: JobCopyRecord; error?: string; loading?: boolean; applicantsFailed?: boolean }
function Composition({ jobs }: { jobs: JobCopyRecord[] }) {
  const distribution = (job: JobCopyRecord, dimension: typeof dimensions[number]['id']) => job.applicationRows ? buildDistribution(job.applicationRows.map(row => ({ ...row, age: row.age && /^\d+$/.test(row.age) ? Number(row.age) : null })), dimension) : variantDistribution(job, undefined, 'record', dimension);
  return <details><summary>応募者構成（性別・年代・地域）</summary><p>取得できた応募の構成です。上の期間だけの構成ではありません。割合は属性不明の応募を含みます。応募が多い求人は取得上限により一部の構成です。</p>{dimensions.map(dimension => {
    let distributions = jobs.map(job => distribution(job, dimension.id));
    if (dimension.id === 'prefecture' || dimension.id === 'municipality') {
      const names = new Set(distributions.flatMap(d => d?.categories.map(c => c.category) ?? []).filter(name => name !== 'その他' && name !== '不明'));
      const hidden = new Set([...names].filter(name => distributions.some(d => d?.categories.some(c => c.category === 'その他') && !d.categories.some(c => c.category === name))));
      distributions = distributions.map(d => {
        if (!d) return null;
        const counts = new Map<string, number>();
        for (const c of d.categories) { const name = hidden.has(c.category) ? 'その他' : c.category; counts.set(name, (counts.get(name) ?? 0) + c.count); }
        return { total: d.total, categories: [...counts].map(([category, count]) => ({ category, count, percentage: d.total ? count / d.total * 100 : null })) };
      });
    }
    const categories = orderCategories(dimension.id, [...new Set(distributions.flatMap(d => d?.categories.map(c => c.category) ?? []))].map(category => ({ category })));
    return <details key={dimension.id}><summary>{dimension.label}を比較</summary><div className="ab-table-scroll"><table><caption>{dimension.label}の応募者構成（件数・割合）</caption><thead><tr><th>区分</th>{jobs.map(job => <th key={job.id}>{job.title}</th>)}</tr></thead><tbody>{categories.map(({ category }) => <tr key={category}><th>{category}</th>{distributions.map((d, i) => {
      const value = d?.categories.find(c => c.category === category);
      const count = d ? value?.count ?? 0 : null;
      const percent = d?.total ? (count ?? 0) / d.total * 100 : null;
      const base = distributions[0]; const baseCount = base?.categories.find(c => c.category === category)?.count ?? 0;
      const delta = percent !== null && base?.total ? percent - baseCount / base.total * 100 : null;
      return <td key={jobs[i]?.id}>{number(count, '件')}・{number(percent, '%')}{i > 0 && <small>今の求人との差：{number(delta, 'ポイント')}</small>}</td>;
    })}</tr>)}</tbody></table>{!categories.length && <p>応募者構成は未取得、または対象の応募が0件です。</p>}</div></details>;
  })}</details>;
}
function Reasons({ job }: { job: JobCopyRecord }) {
  const reasons = job.applicantReasons;
  const texts = reasons?.available ? reasons.items.filter(item => APPLICATION_TEXT_SOURCES.includes(item.sourceProperty)) : [];
  return <><p>{job.title}</p>{!reasons?.available ? <p>応募理由は未取得です。</p> : <><p>表示対象：{texts.length}記述。応募人数・回答率とは異なります。{reasons.truncated ? '取得上限による一部表示です。' : ''}</p>{texts.map(item => <details key={item.id}><summary>原文を開く · {reasonSourceLabels[item.sourceProperty] ?? '記録欄'} · {formatDateJst(item.applicationDate, '応募日不明')}</summary><blockquote>{item.text}</blockquote></details>)}</>}</>;
}

function CandidateNumbers({ entry, today }: { entry: Selection; today: string }) {
  const live = entry.row?.recent_performance;
  const local = entry.job ? periodPerformance(entry.job, comparisonPeriod(entry.job, 'recent', today)).total : null;
  const apps = live?.applications ?? local?.applications;
  const cost = live?.cost_yen ?? local?.cost_yen;
  const cpa = apps != null && apps > 0 && cost != null ? cost / apps : null;
  return <small className="ab-candidate-numbers">直近4週の応募：{number(apps, '件')} · 応募単価：{formatYen(cpa, '未取得')}</small>;
}
function ComparisonImages({ entry }: { entry: Selection | undefined }) {
  return entry?.job ? <ImageGallery title={`${entry.title}の画像`} images={comparisonAttributes(entry.job).images} /> : <>未取得</>;
}

export function AbComparison({ job, records, source = 'fixed' }: { job: JobCopyRecord; records: JobCopyRecord[]; source?: 'fixed' | 'hubspot' }) {
  const id = useId();
  const live = source === 'hubspot';
  const initial = comparisonAttributes(job);
  const [today] = useState(jstToday);
  const [tab, setTab] = useState<'same' | 'other'>('same');
  const [filters, setFilters] = useState({ occupation: initial.occupation, prefecture: initial.prefecture, media: '', q: '' });
  const [offset, setOffset] = useState(0);
  const [mode, setMode] = useState<ComparisonPeriod>('start');
  const [loadedPage, setPage] = useState<HubSpotListingPage | null>(null);
  const [loadedError, setListError] = useState('');
  const [loadedQuery, setLoadedQuery] = useState('');
  const [reload, setReload] = useState(0);
  const [selected, setSelected] = useState<Selection[]>([]);
  const actions = useRef(new Map<string, AbortController>());
  const filterQuery = new URLSearchParams(filters).toString();
  useEffect(() => { const pending = actions.current; return () => { for (const action of pending.values()) action.abort(); }; }, []);
  const requestKey = `${job.id}:${tab}:${String(offset)}:${filterQuery}:${String(reload)}`;
  const page = loadedQuery === requestKey ? loadedPage : null;
  const listError = loadedQuery === requestKey ? loadedError : '';
  useEffect(() => {
    if (!live || !job.accountId) return;
    const controller = new AbortController();
    const query = new URLSearchParams({ offset: String(offset), page_size: '20', exclude_listing: job.hubspotId ?? '', sort: tab === 'same' ? 'same_account' : 'recent_applications' });
    if (tab === 'same') { query.set('account_id', job.accountId); if (job.hubspotId) query.set('comparison_origin', job.hubspotId); }
    else {
      query.set('exclude_account_id', job.accountId);
      const values = new URLSearchParams(filterQuery);
      for (const [key, value] of values) if (value) query.set(key, key === 'media' ? value === 'HRハッカー' ? 'hrh' : 'airwork' : value);
    }
    void apiGet<HubSpotListingPage>(`/api/job-copy/listings?${query.toString()}`, { signal: controller.signal, timeoutMs: 120_000 }).then(result => {
      if (controller.signal.aborted) return;
      setLoadedQuery(requestKey);
      if (result.ok) { setPage(result.data); setListError(''); } else setListError('比較する求人を読み込めませんでした。');
    });
    return () => { controller.abort(); };
  }, [live, job.accountId, job.hubspotId, tab, offset, filterQuery, requestKey]);
  const update = (entryId: string, patch: Partial<Selection>) => { setSelected(items => items.map(item => item.id === entryId ? { ...item, ...patch } : item)); };
  const load = async (entry: Selection) => {
    if (!entry.row) return;
    actions.current.get(entry.id)?.abort();
    const controller = new AbortController(); actions.current.set(entry.id, controller);
    update(entry.id, { loading: true, error: '', applicantsFailed: false });
    const [versions, company, applicants] = await Promise.all([loadVersions(entry.row.id, { signal: controller.signal }), loadCompany(entry.row.id, controller.signal), loadApplicationDates(entry.row.id, controller.signal)]);
    if (controller.signal.aborted) return;
    actions.current.delete(entry.id);
    if (!versions.ok) { update(entry.id, { loading: false, error: '求人の内容を読み込めませんでした。' }); return; }
    const loaded = listingRecord(versions.data, entry.row);
    update(entry.id, { loading: false, applicantsFailed: applicants === null, job: { ...loaded, company: company ?? loaded.company, ...(applicants ? { overallApplications: applicants.overall, applicationRows: applicants.rows, applicantReasons: applicants.reasons } : {}) } });
  };
  const add = (entry: Selection) => {
    if (selected.length >= 4 || selected.some(item => item.id === entry.id)) return;
    setSelected(items => [...items, { ...entry, loading: Boolean(entry.row) }]);
    if (entry.row) void load(entry);
  };
  const remove = (entryId: string) => { actions.current.get(entryId)?.abort(); actions.current.delete(entryId); setSelected(items => items.filter(item => item.id !== entryId)); };
  const local = localCandidates(job, records, tab, filters, today);
  const candidates: Selection[] = live ? (page?.listings ?? []).map(row => ({ id: row.id, title: row.title ?? '求人名未取得', row })) : local.slice(offset, offset + 20).map(other => ({ id: other.id, title: other.title, job: other }));
  const entries: Selection[] = [{ id: job.id, title: job.title, job }, ...selected];
  const jobs = entries.flatMap(item => item.job ? [item.job] : []);
  const commonPeriod = jobs.length === entries.length ? commonRecentPeriod(jobs, today) : null;
  const periods = entries.map(entry => entry.job ? mode === 'recent' ? commonPeriod : comparisonPeriod(entry.job, mode, today) : null);
  const metrics = entries.map((entry, i) => entry.job ? periodPerformance(entry.job, periods[i] ?? null) : null);
  const content = contentDifferences(entries.map(entry => entry.job ?? { id: entry.id, title: '', versions: [], company: '', media: '', mediaJobId: '', location: '' }));
  const next = live ? page?.next_offset ?? null : offset + 20 < local.length ? offset + 20 : null;
  const changeFilter = (key: keyof typeof filters, value: string) => { setFilters(values => ({ ...values, [key]: value })); setOffset(0); };
  const headers = (actions: boolean) => <tr><th scope="col">項目</th>{entries.map((entry, i) => <th scope="col" key={entry.id}>{i === 0 && <small>今の求人</small>}{entry.title}{actions && i > 0 && <button className="jc-button" onClick={() => { remove(entry.id); }}>{entry.title}を外す</button>}{actions && entry.loading && <p role="status">本文・画像・応募・課金を読み込んでいます…</p>}{actions && entry.error && <p role="alert">{entry.error}<button onClick={() => { void load(entry); }}>再取得</button></p>}{actions && entry.applicantsFailed && <p role="alert">応募者構成・理由を読み込めませんでした。<button onClick={() => { void load(entry); }}>再取得</button></p>}</th>)}</tr>;
  const contentTable = (different: boolean) => <div className="ab-table-scroll" tabIndex={0} role="region" aria-label={different ? '違う項目の横並び比較' : '同じ項目・未取得の横並び比較'}><table className="ab-content-table"><thead>{headers(false)}</thead><tbody>{content.filter(row => row.different === different).map(row => <tr key={row.key}><th scope="row">{row.label}</th>{row.values.map((value, i) => <td key={entries[i]?.id} className={row.differences[i] ? 'ab-different' : ''}>{row.key === 'images' ? <ComparisonImages entry={entries[i]} /> : <div className={row.key.startsWith('body') ? 'ab-body' : ''}>{value === '' ? '記載なし' : value ?? '未取得'}</div>}</td>)}</tr>)}</tbody></table></div>;
  return <section className="ab-comparison" aria-label="求人比較"><h2>求人比較</h2><p>同じ顧客の求人で内容と成果を比べたり、ほかの顧客で応募が多い求人を参考にできます。相手は4件まで選べます。</p>
    <details className="ab-picker jc-no-print" open><summary>比較する相手を探す</summary><div className="ab-tabs" role="tablist" aria-label="相手の探し方">{(['same', 'other'] as const).map(value => <button key={value} id={`${id}-${value}`} role="tab" aria-controls={`${id}-candidates`} aria-selected={tab === value} tabIndex={tab === value ? 0 : -1} onKeyDown={event => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) { event.preventDefault(); const next = event.key === 'Home' ? 'same' : event.key === 'End' ? 'other' : value === 'same' ? 'other' : 'same'; setTab(next); setOffset(0); document.getElementById(`${id}-${next}`)?.focus(); } }} onClick={() => { setTab(value); setOffset(0); }}>{value === 'same' ? '同じ顧客' : 'ほかの顧客'}</button>)}</div>
      <div id={`${id}-candidates`} role="tabpanel" aria-labelledby={`${id}-${tab}`}>
        <p>{tab === 'same' ? '同じ案件の求人を先頭に、掲載中、過去の順で表示します。' : '直近4週の媒体応募が多い順です。職種・都道府県を空にすると全国・全職種から探せます。未取得の求人は末尾に表示します。'}</p>
        {tab === 'other' && <div className="ab-controls"><label>本文の職種<input value={filters.occupation} maxLength={200} onChange={e => { changeFilter('occupation', e.target.value); }} /></label><label>都道府県<select value={filters.prefecture} onChange={e => { changeFilter('prefecture', e.target.value); }}><option value="">全国</option>{PREFECTURE_NAMES.map(prefecture => <option key={prefecture}>{prefecture}</option>)}</select></label><label>媒体<select value={filters.media} onChange={e => { changeFilter('media', e.target.value); }}><option value="">すべて</option><option>HRハッカー</option><option>AirWork</option></select></label><label>求人名の検索<input type="search" maxLength={100} value={filters.q} onChange={e => { changeFilter('q', e.target.value); }} /></label><button className="jc-button" onClick={() => { setFilters({ occupation: '', prefecture: '', media: '', q: '' }); setOffset(0); }}>絞り込みを外す</button></div>}
        {live && !job.accountId ? <p>顧客が分からないため、比較する相手を探せません。</p> : <>{listError ? <p role="alert">{listError}<button onClick={() => { setReload(v => v + 1); }}>再取得</button></p> : live && !page ? <p role="status">比較する求人を読み込んでいます…</p> : page?.status === 'preparing' ? <p role="status">求人一覧を準備しています。<button onClick={() => { setReload(v => v + 1); }}>もう一度確認</button></p> : <>
          {page?.refresh_failed && <p>一覧を更新できなかったため、前回取得した候補を表示しています。</p>}
          <ul className="ab-candidates">{candidates.map(entry => <li key={entry.id}><div><strong>{entry.title}</strong><small>{entry.row ? [entry.row.summary?.occupation, entry.row.prefecture, mediaLabel(entry.row.media), publicationLabel(entry.row.publication_status)].filter(Boolean).join(' · ') : [entry.job?.company, entry.job?.location, entry.job?.media].filter(Boolean).join(' · ')}</small><CandidateNumbers entry={entry} today={today} /></div><button className="jc-button" disabled={selected.length >= 4 || selected.some(item => item.id === entry.id)} onClick={() => { add(entry); }}>{selected.some(item => item.id === entry.id) ? '選択済み' : `${entry.title}を比較に追加`}</button></li>)}</ul>
          {!candidates.length && <p>条件に合う求人はありません。</p>}
          <nav className="ab-pages" aria-label="比較候補のページ"><button className="jc-button" disabled={offset === 0} onClick={() => { setOffset(v => Math.max(0, v - 20)); }}>前の20件</button><span>{candidates.length ? `${String(offset + 1)}〜${String(offset + candidates.length)}件` : '0件'}</span><button className="jc-button" disabled={next === null} onClick={() => { if (next !== null) setOffset(next); }}>次の20件</button></nav>
        </>}</>}
      </div>
    </details>
    <div className="ab-period"><label>比べる期間<select value={mode} onChange={e => { setMode(e.target.value === 'recent' ? 'recent' : 'start'); }}><option value="start">それぞれの掲載開始から4週</option><option value="recent">同じ期間（直近4週）</option></select></label><p>応募数は媒体の実績です。掲載開始日を含む週から連続する4週、または全員に共通する最新の4週を使います。日割りはしません。未取得の値は0にしません。</p></div>
    {jobs.slice(1).some(other => other.accountId !== job.accountId || other.company !== job.company || other.location !== job.location || other.media !== job.media || other.listingStartedOn !== job.listingStartedOn) && <p className="jc-notice">顧客・勤務地・媒体・掲載時期に違いがあります。条件も確認してください。</p>}
    <p>本文・画像は最後に取得した内容です。比べる期間の当時の内容とは限りません。</p><p>選択中：{selected.length}件 / 4件。良し悪しは自動で判定しません。</p>
    <div className="ab-table-scroll" tabIndex={0} role="region" aria-label="成果の横並び比較"><table><thead>{headers(true)}</thead><tbody>
      <tr><th scope="row">顧客・勤務地・媒体</th>{entries.map(entry => <td key={entry.id}>{entry.job ? [entry.job.company || '顧客名未取得', entry.job.location, entry.job.media].filter(Boolean).join(' · ') : '未取得'}</td>)}</tr>
      <tr><th scope="row">比べる期間</th>{entries.map((entry, i) => { const period = periods[i]; return <td key={entry.id}>{period ? formatPeriodJst(period.start, period.end) : entry.job ? mode === 'recent' ? '共通の4週分の実績がありません' : '掲載開始日が分かりません' : '未取得'}</td>; })}</tr>
      <tr><th scope="row">含めた週</th>{metrics.map((metric, i) => <td key={entries[i]?.id}>{metric?.covered ? fourWeekLabel(periods[i] ?? null) : '未取得（4週分の実績が不足）'}</td>)}</tr>
      {(['応募数', '表示数', 'クリック数', 'クリック率', '費用', '応募単価'] as const).map(label => <tr key={label}><th scope="row">{label}</th>{metrics.map((m, i) => <td key={entries[i]?.id}>{label === '費用' || label === '応募単価' ? formatYen(label === '費用' ? m?.total.cost_yen : m?.ratios.cpa, '未取得') : number(label === '応募数' ? m?.total.applications : label === '表示数' ? m?.total.impressions : label === 'クリック数' ? m?.total.clicks : m?.ratios.ctr, label === 'クリック率' ? '%' : label === '応募数' ? '件' : '回')}</td>)}</tr>)}
    </tbody></table></div>
    <p>色が付いたところが今の求人と違う項目です。</p><h3>違う項目（{content.filter(row => row.different).length}項目）</h3>{contentTable(true)}<details><summary>同じ項目・未取得（{content.filter(row => !row.different).length}項目）</summary>{contentTable(false)}</details>
    {selected.length > 0 && <><Composition jobs={jobs} /><details className="jc-no-print"><summary>応募理由（内部閲覧）</summary><p>取得済みの応募全体の理由です。上の期間に限定していません。</p><div className="ab-reasons">{jobs.map(item => <section key={item.id}><Reasons job={item} /></section>)}</div></details></>}
  </section>;
}
