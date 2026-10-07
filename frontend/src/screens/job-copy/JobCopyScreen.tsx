import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { ChangeEvent } from 'react';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import { compareCopy } from './diff';
import { compareImages, compareImageBytes, referenceImages, imagesByVersion } from './images';
import { ImageGallery } from './ImageGallery';
import { MediaCaptureImport } from './MediaCaptureImport';
import { ApplicantComposition } from './ApplicantComposition';
import { HrhPerformance } from './HrhPerformance';
import { MarketContext } from './MarketContext';
import { ApplicationTrend } from './ApplicationTrend';
import { MarketFactors } from './MarketFactors';
import { ApplicantReasonReview } from './ApplicantReasonReview';
import { JobFeatureTabs, JobFeaturePanel, jobFeatureGroups } from './JobFeatureTabs';
import type { JobFeature } from './JobFeatureTabs';
import { ReverseSearch } from './ReverseSearch';
import { BillingImportPanel } from './BillingImportPanel';
import { JobCopyDataImport } from './JobCopyDataImport';
import type { BillingPeriod } from './billingTypes';
import { AbComparison } from './AbComparison';
import { ConsultantReview } from './ConsultantReview';
import { HubSpotReadPanel } from './HubSpotReadPanel';
import { JobTimeline } from './JobTimeline';
import { JobOverview } from './JobOverview';
import { billingEntriesByJob } from './timelineModel';
import type { ConsultantDraft } from './ConsultantReview';
import { apiGet } from '../../api/client';
import { parseRealMoc } from './realMoc';
import { applicationCountLabel, orderJobs } from './jobList';
import type { JobListOrder } from './jobList';
import { snapshotErrorGuidance, SnapshotErrorNotice } from './SnapshotErrorNotice';
import type { SnapshotErrorGuidance } from './SnapshotErrorNotice';
import { jobApplicationTotal, linkedApplicationCount, noLinkedApplicationsMessage, unmatchedApplicationCount } from './applicationCountsModel';
import { formatDateTimeJst, joinPresent, plainWording } from './format';
import { AssumptionsNote } from './AssumptionsNote';
import './job-copy.css';

const statusLabels = { initial: '初回取得', unchanged: '変更なし', format_only: '表記差のみ', changed: '内容変更あり', unavailable: '判定不能' };
const certaintyLabels = { confirmed: '確定', estimated: '推定', unknown: '不明' };
const date = (value: string) => formatDateTimeJst(value, '日時不明');
const published = (job: JobCopyRecord) => job.versions.filter(version => version.kind === 'published');
const latest = (job: JobCopyRecord) => published(job).at(-1);
const versionImages = (version: CopyVersion | undefined) => version ? version.images ?? imagesByVersion[version.id] : undefined;
const changeStatus = (job: JobCopyRecord) => {
  const versions = published(job);
  return compareCopy(versions.at(-2)?.body ?? null, versions.at(-1)?.body ?? null).status;
};

function ApplicationSummary({ job, version, live = false }: { job: JobCopyRecord; version: CopyVersion; live?: boolean }) {
  if (version.kind === 'ai_draft') return <div className="jc-notice">未掲載のAI案です。応募実績には対応させていません。</div>;
  if (version.applications === null) return <p className="jc-notice">応募実績は未取得です。0件とは判定していません。掲載開始日時と過去版も未取得です。</p>;
  const unmatched = unmatchedApplicationCount(job);
  return <section className="jc-applications" aria-label="版別の応募状況">
    <div className="jc-period"><strong>この文面に対応する応募</strong><span>{version.publishedFrom ? date(version.publishedFrom) : '開始不明'} → {version.publishedUntil ? date(version.publishedUntil) : '終了未確認'} · 期間{certaintyLabels[version.certainty]}</span></div>
    {linkedApplicationCount(version) === 0 && <p className="jc-no-linked" role="status">{noLinkedApplicationsMessage(jobApplicationTotal(job))}</p>}
    <div className="jc-counts"><div><span>確定対応</span><strong>{version.applications.confirmed}<small>件</small></strong></div><div><span>推定対応</span><strong>{version.applications.estimated}<small>件</small></strong></div><div><span title="応募日が無い、または取得した版の期間に入らない応募です。求人全体で数えた値で、応募者構成と同じ件数です。">どの版への応募か不明（求人全体）</span><strong>{unmatched ?? '—'}<small>件</small></strong></div></div>
    {live
      ? <AssumptionsNote summary="HubSpotに記録された応募を、応募日とその日に取得した版で突き合わせた件数です。" items={['「推定対応」は、変更に気づいた日の版を基準に数えた件数です。', 'どの版への応募か不明な件数の合計は「応募者構成」で確認できます。']} />
      : <AssumptionsNote summary="架空の件数です。" items={['「どの版への応募か不明」は求人全体で数えた件数で、応募者構成と同じ値です。「確定対応」には含めません。']} />}
  </section>;
}

function CopyDetail({ job, records, onAdd, reviewed, onReview, onBack, billing, demo = false }: { job: JobCopyRecord; records: JobCopyRecord[]; onAdd: (version: CopyVersion) => void; reviewed: string[]; onReview: (id: string) => void; onBack: () => void; billing?: readonly BillingEntry[] | undefined; demo?: boolean }) {
  const current = latest(job) ?? (job.dataSource === 'hubspot' ? job.versions.at(-1) : undefined);
  const [tab, setCurrentTab] = useState<JobFeature>('timeline');
  const [remembered, setRemembered] = useState<Partial<Record<string, JobFeature>>>({});
  const [visited, setVisited] = useState<JobFeature[]>(['timeline']);
  const tabPrefix = useId();
  function setTab(next: JobFeature) {
    setCurrentTab(next);
    const group = jobFeatureGroups.find(item => item.features.some(feature => feature.id === next));
    if (group) setRemembered(previous => ({ ...previous, [group.id]: next }));
    setVisited(previous => previous.includes(next) ? previous : [...previous, next]);
  }
  function openFeatureFromContent(next: JobFeature) {
    setTab(next);
    requestAnimationFrame(() => { document.getElementById(`${tabPrefix}-feature-${next}`)?.focus(); });
  }
  function returnToFeatures() {
    document.getElementById(`${tabPrefix}-feature-${tab}`)?.focus({ preventScroll: true });
    const group = jobFeatureGroups.find(item => item.features.some(feature => feature.id === tab));
    if (group) document.getElementById(`${tabPrefix}-group-${group.id}`)?.closest('.jc-feature-primary')?.scrollIntoView({ block: 'start' });
  }
  const [reportDraft, setReportDraft] = useState<ConsultantDraft>({ stage: 'plan', target: 'both', fields: {}, selection: [published(job)[0]?.id ?? '', published(job)[1]?.id ?? published(job)[0]?.id ?? ''] });
  const [selected, setSelected] = useState(current?.id ?? '');
  const [before, setBefore] = useState(published(job).at(-2)?.id ?? current?.id ?? '');
  const [after, setAfter] = useState(current?.id ?? '');
  const [incoming, setIncoming] = useState('');
  const [source, setSource] = useState('外部文面の貼り付け');
  const [message, setMessage] = useState('');
  const [reading, setReading] = useState(false);
  const [readError, setReadError] = useState('');
  const [compared, setCompared] = useState(false);
  const [diffLimit, setDiffLimit] = useState(300);
  const [changesOnly, setChangesOnly] = useState(false);
  const version = job.versions.find(item => item.id === selected) ?? current;
  const left = job.versions.find(item => item.id === before);
  const right = job.versions.find(item => item.id === after);
  const result = compareCopy(left?.body ?? null, right?.body ?? null);
  const imageResult = compareImages(referenceImages(left), referenceImages(right));
  const imageBytes = compareImageBytes(versionImages(left), versionImages(right), left?.historicalImageBytesAvailable, right?.historicalImageBytesAvailable);
  const incomingResult = compareCopy(current?.body ?? null, incoming);
  const diffLines = changesOnly ? result.lines.filter(line => line.kind !== 'same') : result.lines;
  const changedLines = result.lines.filter(line => line.kind !== 'same').length;
  const imageReferenceLabel = imageResult.status === 'unknown' ? '未取得・判定不能' : imageResult.status === 'same_reference' ? '参照・順番は同じ' : `追加${String(imageResult.added.length)}・削除${String(imageResult.removed.length)}${imageResult.reordered ? '・順番変更' : ''}`;
  const imageBytesLabel = imageBytes === 'unknown' ? '原本不足・未確認' : imageBytes === 'same_files' ? '保存した原本ハッシュは一致' : imageBytes === 'changed_files' ? '保存した原本の内容変更あり（再圧縮等も含む）' : '両版とも画像0点';

  async function readFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    setReadError(''); setCompared(false); setReading(true); setMessage('');
    try {
      if (file.size > 200_000) throw new Error('200KB以下のUTF-8テキストを選んでください。');
      if (!file.name.toLowerCase().endsWith('.txt')) throw new Error('.txtのファイルだけ読み込めます。CSVの読み込みにはまだ対応していません。');
      const body = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
      setIncoming(body); setSource(file.name);
    } catch (error) {
      setIncoming(''); setReadError(error instanceof Error ? error.message : 'テキストを読み込めませんでした。');
    } finally { setReading(false); event.target.value = ''; }
  }

  function addObservation() {
    if (!compared || incomingResult.status === 'unavailable') return;
    if (incomingResult.status === 'unchanged') {
      setMessage('本文に変更がないことをデモで確認しました。新しい本文の版は作りません。この確認は保存されません。');
      return;
    }
    const now = new Date().toISOString();
    const newVersion: CopyVersion = { id: `demo-received-${crypto.randomUUID()}`, label: `受信版 ${String(job.versions.filter(item => item.kind === 'received').length + 1)}`, observedAt: now,
      certainty: 'unknown', kind: 'received', source: source.trim() || '受信元未指定', body: incoming,
      applications: null, note: '外部文面の受領のみ。媒体での更新・掲載は未確認。応募情報未取得。' };
    onAdd(newVersion); setSelected(newVersion.id); setBefore(current?.id ?? ''); setAfter(newVersion.id); openFeatureFromContent('body');
    setMessage('受信版を画面内のデモ履歴に追加しました。媒体での掲載確認・HubSpot保存は行っていません。');
  }

  return <article className="jc-detail" id="job-details" tabIndex={-1}>
    <header className="jc-detail-heading"><div><p className="jc-eyebrow">求人レコード / {job.mediaJobId}</p><h1>{job.title}</h1><p>{job.company} <span>·</span> {job.location} <span>·</span> {job.media}</p></div><span className="jc-badge">本文：{statusLabels[changeStatus(job)]}</span></header>
    <nav className="jc-detail-actions" aria-label="求人の閲覧操作"><span title={job.title}>表示中：{job.title}</span><button type="button" className="jc-button" onClick={onBack}>求人一覧に戻る</button><button type="button" className="jc-button" onClick={returnToFeatures}>機能を切り替える</button></nav>
    <div className="jc-record-meta"><span>{current?.source === 'HubSpot shigotonaiyou' ? '現在のHubSpot値' : '現在の取得した版'}: {current?.label ?? '本文未取得'}</span><span title="ファイルを取得した日時です。掲載が変わった日時ではありません。">取得日時: {current ? date(current.observedAt) : '—'}</span><span>{job.hubspotId ? `HubSpot求人ID: ${job.hubspotId}` : 'HubSpotリンク: 実求人IDの接続待ち'}</span></div>
    {message && <p className="jc-message" role="status">{message}</p>}
    <JobFeatureTabs value={tab} onChange={setTab} remembered={remembered} prefix={tabPrefix}>
    <JobFeaturePanel feature="timeline" active={tab === 'timeline'} prefix={tabPrefix}>{visited.includes('timeline') && <JobTimeline job={job} billing={billing} marketMode={demo ? 'demo' : 'api'} onOpenVersion={id => { setSelected(id); openFeatureFromContent('body'); }} onCompareVersions={(from, to) => { setBefore(from); setAfter(to); openFeatureFromContent('diff'); }} />}</JobFeaturePanel>
    <JobFeaturePanel feature="applications" active={tab === 'applications'} prefix={tabPrefix}>{tab === 'applications' && <ApplicationTrend job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature="applicants" active={tab === 'applicants'} prefix={tabPrefix}>{visited.includes('applicants') && <ApplicantComposition job={job} includeReasons={false} />}</JobFeaturePanel>
    <JobFeaturePanel feature="reasons" active={tab === 'reasons'} prefix={tabPrefix}>{visited.includes('reasons') && <ApplicantReasonReview job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature="ab" active={tab === 'ab'} prefix={tabPrefix}><AbComparison job={job} records={records} /></JobFeaturePanel>
    <JobFeaturePanel feature="performance" active={tab === 'performance'} prefix={tabPrefix}>{tab === 'performance' && <HrhPerformance job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature={tab === 'market-table' ? 'market-table' : 'market'} active={tab === 'market' || tab === 'market-table'} prefix={tabPrefix}>{(visited.includes('market') || visited.includes('market-table')) && <MarketContext job={job} view={tab === 'market' ? 'charts' : tab === 'market-table' ? 'table' : 'inactive'} />}</JobFeaturePanel>
    {tab !== 'market-table' && <JobFeaturePanel feature="market-table" active={false} prefix={tabPrefix}>{null}</JobFeaturePanel>}
    {tab === 'market-table' && <JobFeaturePanel feature="market" active={false} prefix={tabPrefix}>{null}</JobFeaturePanel>}
    <JobFeaturePanel feature="factors" active={tab === 'factors'} prefix={tabPrefix}>{visited.includes('factors') && <MarketFactors job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature="report" active={tab === 'report'} prefix={tabPrefix}>{tab === 'report' && <ConsultantReview job={job} draft={reportDraft} onDraft={setReportDraft} />}</JobFeaturePanel>
    <JobFeaturePanel feature="body" active={tab === 'body'} prefix={tabPrefix}><div className="jc-history-layout">
      <aside className="jc-history"><h2>文面のタイムライン</h2><p className="jc-muted">本文の版を選ぶと内容が開きます</p>
        {[...job.versions].reverse().map(item => <button key={item.id} className="jc-version" aria-pressed={version?.id === item.id} onClick={() => { setSelected(item.id); }}>
          <span className="jc-version-top"><strong>{item.label}</strong><small>{item.kind === 'ai_draft' ? 'AI案・未掲載' : item.publishedFrom ? '掲載を確認' : item.kind === 'published' ? '媒体取得・掲載時刻不明' : '受領・掲載未確認'}</small></span>
          <time>{date(item.observedAt)}</time><span>{item.source}</span><small>{reviewed.includes(item.id) ? '確認済み（デモ）' : '未確認'}</small>
        </button>)}
        {!job.versions.length && <p className="jc-empty">本文はまだ届いていません。</p>}
      </aside>
      <section className="jc-reading">{version ? <><div className="jc-reading-title"><div><h2>{version.label}の文面</h2><p className="jc-muted">{joinPresent([version.source, date(version.observedAt)])}</p></div><button className="jc-button" onClick={() => { onReview(version.id); }}>{reviewed.includes(version.id) ? '未確認に戻す' : '確認済みにする'}</button></div>
        {version.note.trim() && <p className="jc-notice">{plainWording(version.note)}</p>}
        {version.observedPublicationStatus !== undefined && <p>媒体CSVの公開状態: {version.observedPublicationStatus || '未取得'}</p>}
        <ImageGallery title="この版の掲載画像" images={versionImages(version)} />
        <div className="jc-full-copy-heading"><h3>{version.source === 'HubSpot shigotonaiyou' ? '仕事内容（HubSpotの現在値）' : '求人票の本文（全文）'}</h3><span>読み取り専用 · 原文の段落・改行を保持</span></div><pre className="jc-body">{version.body}</pre>
        {job.hubspotUrl && <a href={job.hubspotUrl} target="_blank" rel="noreferrer">HubSpotで求人レコードを開く</a>}
        {version.kind === 'received' ? <p className="jc-notice">応募情報は未取得です。0件とは判定していません。</p> : <ApplicationSummary job={job} version={version} live={job.dataSource === 'hubspot'} />}
        <button className="jc-text-button" onClick={() => { const index = job.versions.findIndex(item => item.id === version.id); setBefore(job.versions[index - 1]?.id ?? version.id); setAfter(version.id); openFeatureFromContent('diff'); }}>この版を前の版と比較する →</button>
      </> : <div className="jc-empty"><h2>本文未取得</h2><p>欠損を「変更なし」や「削除」と判断しません。</p><button className="jc-button" onClick={() => { openFeatureFromContent('receive'); }}>外部文面を確認する</button></div>}</section>
    </div></JobFeaturePanel>
    <JobFeaturePanel feature="diff" active={tab === 'diff'} prefix={tabPrefix}>{tab === 'diff' && <section className="jc-comparison"><div className="jc-compare-controls"><label>比較元<select value={before} onChange={event => { setBefore(event.target.value); }}><option value="">本文なし</option>{job.versions.map(item => <option key={item.id} value={item.id}>{item.label} · {item.kind === 'ai_draft' ? 'AI案' : item.source}</option>)}</select></label><span aria-hidden="true">→</span><label>比較先<select value={after} onChange={event => { setAfter(event.target.value); }}><option value="">本文なし</option>{job.versions.map(item => <option key={item.id} value={item.id}>{item.label} · {item.kind === 'ai_draft' ? 'AI案' : item.source}</option>)}</select></label></div>
      <section className="jc-comparison-overview" aria-label="比較結果の要約"><div><span>本文・募集条件</span><strong>{statusLabels[result.status]}</strong><small>追加{result.lines.filter(line => line.kind === 'added').length}行・削除{result.lines.filter(line => line.kind === 'removed').length}行</small></div><div><span>画像参照・掲載順</span><strong>{imageReferenceLabel}</strong></div><div><span>画像ファイル内容</span><strong>{imageBytesLabel}</strong></div></section>
      <nav className="jc-comparison-jumps" aria-label="差分の確認箇所"><a href="#job-copy-text-diff">本文の差分へ</a><a href="#job-copy-image-diff">画像の比較へ</a></nav>
      <section className="jc-image-comparison" id="job-copy-image-diff" aria-label="画像の差分"><h2>掲載画像の比較</h2><p className="jc-notice">{imageResult.status === 'unknown' ? '画像未取得の版があり、変更の有無は判定できません。' : imageResult.status === 'same_reference' ? '画像参照・並び順は同じです。画像ファイルの中身は未検証です。' : `画像参照の変更：追加${String(imageResult.added.length)}点・削除${String(imageResult.removed.length)}点${imageResult.reordered ? '・並び順変更あり' : ''}`}</p>
        {(left?.historicalImageBytesAvailable === false || right?.historicalImageBytesAvailable === false) && <p className="jc-notice">過去時点の画像原本は未保存です。後日取得した画像がある場合も、当時の画像内容とは確認できません。画像参照の変化と画像内容の変化を区別してください。</p>}
        <div className="jc-image-compare-grid"><ImageGallery title="比較元の画像" images={versionImages(left)} marks={imageResult.removed.map(image => image.url)} /><ImageGallery title="比較先の画像" images={versionImages(right)} marks={imageResult.added.map(image => image.url)} /></div>
        <p className="jc-muted">操作デモは架空のイラスト、媒体取得版は取得した実画像です。初回取得だけでは過去との画像変更を判定できません。</p>
      </section>
      <p className="jc-notice">画像ファイル内容：{imageBytesLabel}</p>
      <h2 className="jc-text-diff-heading" id="job-copy-text-diff">本文・募集条件の比較</h2>
      <label className="jc-diff-toggle"><input type="checkbox" checked={changesOnly} onChange={event => { setChangesOnly(event.target.checked); setDiffLimit(300); }} />変更箇所だけを表示（追加・削除{changedLines}行）</label>
      <div className="jc-diff-summary"><strong>本文：{statusLabels[result.status]}</strong><span><i className="jc-added-key" />追加 <i className="jc-removed-key" />削除 · 原文の行単位で比較</span></div>
      {changesOnly && !diffLines.length && result.status !== 'unavailable' && <p className="jc-notice" role="status">追加・削除された本文の行はありません。チェックを外すとすべての行を確認できます。</p>}
      {result.status === 'unavailable' ? <p className="jc-empty">比較先の本文がありません。変更の有無は判定できません。</p> : <><div className="jc-diff-lines" aria-label="文面の差分">{diffLines.slice(0, diffLimit).map((line, index) => <div key={index} className={`jc-diff-line jc-line-${line.kind}`}><span className="jc-line-mark" aria-label={line.kind === 'added' ? '追加' : line.kind === 'removed' ? '削除' : '変更なし'}>{line.kind === 'added' ? '+' : line.kind === 'removed' ? '−' : ' '}</span><pre>{line.text || ' '}</pre></div>)}</div>{diffLines.length > diffLimit && <button className="jc-button" onClick={() => { setDiffLimit(count => count + 300); }}>さらに300行を表示（全{diffLines.length}行）</button>}</>}
      <details className="jc-originals"><summary>比較する2つの原文を見る</summary><div><section><h3>比較元</h3><pre className="jc-body">{left?.body ?? '本文なし'}</pre></section><section><h3>比較先</h3><pre className="jc-body">{right?.body ?? '本文なし'}</pre></section></div></details>
      <p className="jc-muted">改行コードのみの差は表記差です。給与・数字・否定表現を消して比較しません。AI案との差は、掲載変更を意味しません。</p>
    </section>}</JobFeaturePanel>
    <JobFeaturePanel feature="receive" active={tab === 'receive'} prefix={tabPrefix}>{tab === 'receive' && <section className="jc-receive"><h2>届いた文面を、現在の本文と照合する</h2><p>外部で作成された文面をここへ渡します。比較する本文の入力で、求人本文の直接編集ではありません。</p>
      <div className="jc-import-options"><label className="jc-file">UTF-8テキストを読み込む<input type="file" accept=".txt,text/plain" disabled={reading} onChange={event => { void readFile(event); }} /></label><span>{reading ? '読み込み中…' : '200KBまで / CSV・HubSpot取り込みは未接続'}</span></div>
      {readError && <p className="jc-error" role="alert">{readError}</p>}
      <label>受信元・資料名<input value={source} maxLength={200} onChange={event => { setSource(event.target.value); }} /></label>
      <label>受け取った文面<textarea value={incoming} maxLength={100_000} rows={12} placeholder="求人票の外部文面を貼り付けてください" onChange={event => { setIncoming(event.target.value); setCompared(false); }} /></label>
      <div className="jc-receive-actions"><button className="jc-button jc-primary" disabled={reading} onClick={() => { setCompared(true); }}>現在の本文と比較</button><button className="jc-button" onClick={() => { setIncoming(current?.body ?? ''); setSource('現在版の再取得デモ'); setCompared(false); }}>同じ文面で試す</button></div>
      {compared && <div className="jc-receive-result" role="status"><h3>{statusLabels[incomingResult.status]}</h3><p>掲載更新の確認: 未確認。判定は本文比較の結果です。</p>{incomingResult.status !== 'unavailable' && <button className="jc-button" onClick={addObservation}>{incomingResult.status === 'unchanged' ? '変更なしをデモ確認' : '受信版をデモ履歴に追加'}</button>}</div>}
      <p className="jc-notice">将来の文字起こし → AI文面案の生成は、タイムラインの「AI案」として接続予定です。この画面はまだAIを呼び出しません。</p>
    </section>}</JobFeaturePanel>
    </JobFeatureTabs>
  </article>;
}

export function JobCopyScreen() {
  const query = new URLSearchParams(window.location.search);
  const [initialId] = useState(query.get('job'));
  const [snapshotRequested] = useState(query.get('demo') !== '1' && (query.get('data') === 'actual' || window.location.pathname === '/app/job-copy' || window.location.pathname === '/'));
  const [records, setRecords] = useState<JobCopyRecord[]>(snapshotRequested ? [] : jobs);
  const [snapshotAt, setSnapshotAt] = useState('');
  const [snapshotError, setSnapshotError] = useState<SnapshotErrorGuidance | null>(null);
  const [snapshotLoading, setSnapshotLoading] = useState(snapshotRequested);
  const [snapshotSlow, setSnapshotSlow] = useState(false);
  const [snapshotAttempt, setSnapshotAttempt] = useState(0);
  const snapshotAbort = useRef<AbortController | null>(null);
  const [captured, setCaptured] = useState(false);
  const [live, setLive] = useState(false);
  const [panelEpoch, setPanelEpoch] = useState(0);
  const [reviewed, setReviewed] = useState<string[]>([]);
  const [selectedId, setSelectedId] = useState(snapshotRequested ? '' : jobs.find(job => job.id === initialId)?.id ?? jobs[0]?.id ?? '');
  const [search, setSearch] = useState('');
  const [media, setMedia] = useState('all');
  const [customer, setCustomer] = useState('all');
  const [status, setStatus] = useState('all');
  const [listOrder, setListOrder] = useState<JobListOrder>('source');
  const [view, setView] = useState<'list' | 'overview'>('list');
  // 課金CSVから反映した課金期間。画面のメモリ上だけで持ち、再読み込みで消える (サーバーへ送らない)。
  const [billingPeriods, setBillingPeriods] = useState<BillingPeriod[]>([]);
  const billingByJob = useMemo(() => billingEntriesByJob(billingPeriods), [billingPeriods]);
  const demoMode = !snapshotRequested && !live && !captured;
  useEffect(() => {
    if (!snapshotRequested) return;
    const controller = new AbortController();
    snapshotAbort.current = controller;
    const slowTimer = window.setTimeout(() => {
      if (!controller.signal.aborted) setSnapshotSlow(true);
    }, 5_000);
    void apiGet<{ capturedAt: string }>('/api/job-copy/moc', { signal: controller.signal, timeoutMs: 30_000 }).then(result => {
      if (controller.signal.aborted) return;
      if (result.ok) {
        try {
          const items = parseRealMoc(JSON.stringify(result.data));
          setRecords(items); setSelectedId(items.find(job => job.id === initialId)?.id ?? items[0]?.id ?? '');
          setSnapshotAt(result.data.capturedAt); setLive(true);
        } catch {
          setSnapshotError({ message: '実データの形式・求人と応募の対応を確認できませんでした。架空データへ置き換えず、読み込みを停止しています。管理者にデータの内容を確認してもらってください。' });
        }
      } else setSnapshotError(snapshotErrorGuidance(result.error));
    }).catch(() => {
      if (!controller.signal.aborted) setSnapshotError({ message: '求人データを取得できませんでした。再取得してください。続く場合は管理者に取得状況を確認してください。' });
    }).finally(() => {
      window.clearTimeout(slowTimer);
      if (!controller.signal.aborted) { setSnapshotLoading(false); setSnapshotSlow(false); }
    });
    return () => { window.clearTimeout(slowTimer); controller.abort(); };
  }, [snapshotRequested, initialId, snapshotAttempt]);
  function stopSnapshot() {
    snapshotAbort.current?.abort();
    setSnapshotLoading(false); setSnapshotSlow(false); setSnapshotError(null);
  }
  function retrySnapshot() {
    snapshotAbort.current?.abort();
    setSnapshotLoading(true); setSnapshotSlow(false); setSnapshotError(null);
    setSnapshotAttempt(value => value + 1);
  }
  const normalizedSearch = search.trim().toLocaleLowerCase('ja-JP');
  const visible = orderJobs(records.filter(job => (!normalizedSearch || `${job.title} ${job.company} ${job.mediaJobId} ${job.location}`.toLocaleLowerCase('ja-JP').includes(normalizedSearch)) && (customer === 'all' || (job.id.startsWith('demo-job-') || job.dataSource === 'hubspot' ? job.company : 'unlinked') === customer) && (media === 'all' || job.media === media) && (status === 'all' || changeStatus(job) === status)), listOrder);
  const selected = visible.find(job => job.id === selectedId) ?? visible[0];
  const filtersActive = Boolean(normalizedSearch || customer !== 'all' || media !== 'all' || status !== 'all');
  function resetFilters() { setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); document.getElementById('job-list-search')?.focus(); }
  function returnToList() { const heading = document.getElementById('job-list-heading'); heading?.focus({ preventScroll: true }); heading?.scrollIntoView({ block: 'start' }); }
  function choose(job: JobCopyRecord) { setSelectedId(job.id); const url = new URL(window.location.href); url.searchParams.set('job', job.id); window.history.replaceState(null, '', url); if (window.matchMedia('(max-width: 800px)').matches) window.requestAnimationFrame(() => { const detail = document.getElementById('job-details'); detail?.focus({ preventScroll: true }); detail?.scrollIntoView({ block: 'start' }); }); }
  return <div className="jc-app"><header className="jc-page-heading"><h1>求人文面管理</h1><span className="jc-mode" title="開発中の画面です。表示や操作は今後変わります。">試作版</span></header>
    <div className="jc-demo"><strong>{snapshotLoading ? '実データを読み込み中' : snapshotAt ? '実データ（取得済み）' : snapshotRequested && !records.length ? '実データ未表示' : live ? 'HubSpot読み取り' : captured ? '媒体取得版' : '操作デモ'}</strong><span>{snapshotAt ? `媒体CSVの本文・画像とHubSpotの実求人・応募集計です。応募集計取得：${date(snapshotAt)}。最新値の自動更新ではありません。確認記録は画面内のみ保持します。` : live ? records.some(job => published(job).length > 0) ? '実際の取引先・求人に、媒体から取得した本文・画像と応募をつないでいます。画像の取得時点や、どの版への応募か不明な件数は各表示で確認してください。検証記録はこの画面の中だけに残ります。' : '実レコードの現在値です。媒体全文・画像・日次版との接続は別途必要です。確認状況・受信版は画面内だけに保持します。' : captured ? 'HRハッカーの本文・画像です。過去版の有無と画像の取得時点は各版の注記を確認してください。応募未取得・HubSpot未保存です。' : snapshotRequested && !records.length ? '取得済みの実データを読み取ります。欠損を架空データで補いません。' : '求人・本文・応募数はすべて架空です。HubSpot未接続。追加した履歴・確認状況は再読み込みで消えます。'}</span></div>
    {snapshotLoading && <div className="jc-notice"><p role="status">{snapshotSlow ? '読み込みに時間がかかっています。待機を続けるか、求人データを再取得できます。' : '求人一覧・本文・応募集計を読み込んでいます…画像は表示時に取得します。'}</p>{snapshotSlow && <button type="button" className="jc-button" onClick={retrySnapshot}>求人データを再取得</button>}</div>}
    {snapshotError && <><SnapshotErrorNotice guidance={snapshotError} /><button type="button" className="jc-button" onClick={retrySnapshot}>求人データを再取得</button></>}
    {snapshotAt && <section className="jc-snapshot-summary" aria-label="実データの取得範囲"><span><strong>{new Set(records.map(job => job.company)).size}</strong>取引先</span><span><strong>{records.length}</strong>求人</span><span><strong>{records.reduce((sum, job) => sum + published(job).length, 0)}</strong>取得した本文の版</span><span><strong>{records.reduce((sum, job) => sum + (job.overallApplications?.total ?? 0), 0)}</strong>応募（HubSpot記録分）</span><span title="応募日から、どの版を見て応募したかを決められなかった件数です">どの版への応募か不明 <strong>{records.reduce((sum, job) => sum + (unmatchedApplicationCount(job) ?? 0), 0)}</strong>件</span></section>}
    <HubSpotReadPanel key={panelEpoch} onOpen={job => { stopSnapshot(); setSnapshotAt(''); setRecords([job]); setSelectedId(job.id); setReviewed([]); setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setCaptured(false); setLive(true); }} />
    <MediaCaptureImport onImport={items => { stopSnapshot(); setSnapshotAt(''); setRecords(items); setSelectedId(items[0]?.id ?? ''); setReviewed([]); setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setCaptured(true); setLive(false); setPanelEpoch(value => value + 1); }} />
    <JobCopyDataImport><BillingImportPanel records={records} applied={billingPeriods} onApply={setBillingPeriods} onClear={() => { setBillingPeriods([]); }} /></JobCopyDataImport>
    <ReverseSearch records={records} onChoose={job => { setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); choose(job); }} />
    <div className="jc-workspace"><aside className="jc-list"><div className="jo-view-toggle" role="group" aria-label="表示の切り替え"><button type="button" aria-pressed={view === 'list'} onClick={() => { setView('list'); }}>一覧</button><button type="button" aria-pressed={view === 'overview'} onClick={() => { setView('overview'); }}>横断比較</button></div><div className="jc-list-heading"><h2 id="job-list-heading" tabIndex={-1}>求人レコード</h2><span aria-live="polite" aria-atomic="true">{snapshotLoading ? '取得中' : snapshotError && !records.length ? '未取得' : `${String(visible.length)} / ${String(records.length)}件`}</span></div><label>求人・企業・媒体IDを検索<input id="job-list-search" type="search" value={search} placeholder="求人名、企業名、勤務地" onChange={event => { setSearch(event.target.value); }} /></label>
      <label>取引先<select aria-label="取引先" value={customer} onChange={event => { setCustomer(event.target.value); }}><option value="all">すべての取引先</option>{captured ? <option value="unlinked">取引先未紐付け</option> : [...new Set(records.map(job => job.company))].map(value => <option key={value} value={value}>{value}{live ? '' : '（架空）'}</option>)}</select></label>
      <div className="jc-filters"><label>媒体<select value={media} onChange={event => { setMedia(event.target.value); }}><option value="all">すべて</option>{[...new Set(records.map(job => job.media))].map(value => <option key={value}>{value}</option>)}</select></label><label>変更判定<select value={status} onChange={event => { setStatus(event.target.value); }}><option value="all">すべて</option>{Object.entries(statusLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label></div>
      <label>並び順<select aria-label="並び順" value={listOrder} onChange={event => { setListOrder(event.target.value === 'applications' ? 'applications' : 'source'); }}><option value="source">取得順</option><option value="applications">応募数が多い順</option></select></label>
      {filtersActive && <button type="button" className="jc-button jc-filter-reset" onClick={resetFilters}>検索条件をリセット</button>}
      <div className="jc-list-scroll">{visible.map(job => <button className="jc-job" key={job.id} aria-pressed={selected?.id === job.id} onClick={() => { choose(job); }}><span className="jc-job-company">{job.company}</span><strong>{job.title}</strong><span>{job.location} · {job.media}</span><span className="jc-job-bottom"><small>{published(job).length}版{job.versions.some(version => version.kind === 'ai_draft') ? ' + AI案' : ''}</small><small>{statusLabels[changeStatus(job)]}</small></span><small>{applicationCountLabel(job)}</small></button>)}{!visible.length && <div className="jc-empty"><p>{snapshotLoading ? '求人一覧を取得中です。' : records.length ? '一致する求人はありません。' : '表示できる求人がありません。'}</p></div>}</div>
      <p className="jc-list-footer">本文の取得と掲載の確認を分けて管理<br />求人を選ぶと文面と履歴が開きます</p>
    </aside>{view === 'overview' && records.length > 0 ? <JobOverview records={visible} billing={billingByJob} onChoose={job => { setView('list'); choose(job); }} /> : selected ? <CopyDetail key={`${selected.id}-${selected.versions[0]?.id ?? ''}`} job={selected} records={records} billing={billingByJob[selected.id]} demo={demoMode} reviewed={reviewed} onBack={returnToList} onReview={id => { setReviewed(items => items.includes(id) ? items.filter(item => item !== id) : [...items, id]); }} onAdd={version => { setRecords(items => items.map(job => job.id === selected.id ? { ...job, versions: [...job.versions, version] } : job)); }} /> : <main className="jc-detail jc-empty"><h1>{snapshotLoading ? '求人データを取得しています' : records.length ? '一致する求人はありません' : '表示できる求人がありません'}</h1><p>{snapshotLoading ? '取得完了後に本文と応募集計を表示します。' : records.length ? '検索・取引先・媒体・変更判定の条件を見直すか、一覧の「検索条件をリセット」を押してください。' : 'データの取得状況と、画面上部の案内を確認してください。'}</p></main>}</div>
  </div>;
}
