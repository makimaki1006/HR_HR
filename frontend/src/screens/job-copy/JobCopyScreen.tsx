import { DraftFacts, DraftReview } from './DraftComparison';
import { draftVersion } from './hubspotListings';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import { JobCopyBody } from './JobCopyBody';
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import type { ChangeEvent } from 'react';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import { compareCopy, markInlineChanges } from './diff';
import { compareImages, compareImageBytes, referenceImages, imagesByVersion } from './images';
import { ImageGallery } from './ImageGallery';
import { MediaCaptureImport } from './MediaCaptureImport';
import { ApplicantComposition } from './ApplicantComposition';
import { HrhPerformance } from './HrhPerformance';
import { MarketContext } from './MarketContext';
import { ApplicationTrend } from './ApplicationTrend';
import { MarketFactors } from './MarketFactors';
import { ApplicantReasonReview } from './ApplicantReasonReview';
import { JobFeatureTabs, JobFeaturePanel, featureFocusId, jobFeatureGroups } from './JobFeatureTabs';
import type { JobFeature } from './JobFeatureTabs';
import { ReverseSearch } from './ReverseSearch';
import { BillingImportPanel } from './BillingImportPanel';
import { JobCopyDataImport } from './JobCopyDataImport';
import type { BillingPeriod } from './billingTypes';
import { AbComparison } from './AbComparison';
import { ConsultantReview } from './ConsultantReview';
import { HubSpotReadPanel } from './HubSpotReadPanel';
import { HubSpotListingsPanel } from './HubSpotListingsPanel';
import { HUBSPOT_BODY_SOURCE } from './liveApplications';
import { JobTimeline } from './JobTimeline';
import { parseListingStatus, withPublicationFor } from './mediaPublication';
import { JobOverview } from './JobOverview';
import { MarketCacheContext } from './marketSource';
import type { MarketCache } from './marketSource';
import { billingEntriesByJob } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import type { ConsultantDraft } from './ConsultantReview';
import { apiGet } from '../../api/client';
import { parseRealMoc } from './realMoc';
import { applicationCountLabel, orderJobs } from './jobList';
import type { JobListOrder } from './jobList';
import { snapshotErrorGuidance, SnapshotErrorNotice } from './SnapshotErrorNotice';
import type { SnapshotErrorGuidance } from './SnapshotErrorNotice';
import { applicationsOutsideTimeline, jobApplicationTotal, linkedApplicationCount, noLinkedApplicationsMessage, unmatchedApplicationCount } from './applicationCountsModel';
import { InfoTip } from './InfoTip';
import { formatDateTimeJst, joinPresent, plainWording } from './format';
import { AssumptionsNote } from './AssumptionsNote';
import { DUMMY_BILLING_ENABLED } from './dummyBilling';
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
  if (version.applications === null) return <p className="jc-notice">この版に対応する応募は未取得です。掲載開始日時は不明です。</p>;
  const unmatched = unmatchedApplicationCount(job);
  return <section className="jc-applications" aria-label="版別の応募状況">
    <div className="jc-period"><strong>この文面に対応する応募</strong><span>{version.publishedFrom ? date(version.publishedFrom) : '開始不明'} → {version.publishedUntil ? date(version.publishedUntil) : '終了未確認'} · 期間{certaintyLabels[version.certainty]}</span></div>
    {linkedApplicationCount(version) === 0 && <p className="jc-no-linked" role="status">{noLinkedApplicationsMessage(jobApplicationTotal(job))}</p>}
    <div className="jc-counts"><div><span>応募日で結びついた応募</span><strong>{version.applications.confirmed}<small>件</small></strong></div><div><span>気づいた日の版で数えた応募</span><strong>{version.applications.estimated}<small>件</small></strong></div><div><span>どの版への応募か不明（求人全体）</span><strong>{unmatched ?? '—'}<small>件</small></strong><InfoTip className="jc-infotip-left" label="どんな応募か"><p>応募日が無い、または取得した版の期間に入らない応募です。求人全体で数えた値で、応募者構成と同じ件数です。</p></InfoTip></div></div>
    {live
      ? <AssumptionsNote summary="HubSpotに記録された応募を、応募日とその日に取得した版で突き合わせた件数です。" items={['「気づいた日の版で数えた応募」は、掲載が変わった日が分からないため、変化に気づいた取得日の版を基準に数えた件数です。', 'どの版への応募か不明な件数の合計は「応募者構成」で確認できます。']} />
      : <AssumptionsNote summary="架空の件数です。" items={['「どの版への応募か不明」は求人全体で数えた件数で、応募者構成と同じ値です。「応募日で結びついた応募」には含めません。']} />}
  </section>;
}

/** 画面の外（データ取込など）から、この求人の機能を開くための依頼。nonce が変わるたびに 1 回だけ開く。 */
export interface FeatureRequest { feature: JobFeature; nonce: number }

function CopyDetail({ job, records, onAdd, onDraftSaved, reviewed, onReview, billing, demo = false, request = null, showDummyBilling = DUMMY_BILLING_ENABLED }: { job: JobCopyRecord; records: JobCopyRecord[]; onAdd: (version: CopyVersion) => void; onDraftSaved: (draft: DraftSnapshot, revision: string) => void; reviewed: string[]; onReview: (id: string) => void; billing?: readonly BillingEntry[] | undefined; demo?: boolean; request?: FeatureRequest | null; showDummyBilling?: boolean }) {
  const current = latest(job);
  const draft = job.latestDraftId ? job.versions.find(item => item.draft?.draft_id === job.latestDraftId) : undefined;
  const [tab, setCurrentTab] = useState<JobFeature>(draft ? 'diff' : job.id.startsWith('hubspot-history-') ? 'body' : 'timeline');
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
    requestAnimationFrame(() => { document.getElementById(featureFocusId(tabPrefix, next))?.focus(); });
  }
  const handledRequest = useRef<number | null>(null);
  useEffect(() => {
    if (!request || handledRequest.current === request.nonce) return;
    handledRequest.current = request.nonce;
    openFeatureFromContent(request.feature);
    // openFeatureFromContent は毎回作り直す関数なので、依頼の nonce だけで動かす
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);
  const [reportDraft, setReportDraft] = useState<ConsultantDraft>({ stage: 'plan', target: 'both', fields: {}, selection: [published(job)[0]?.id ?? '', published(job)[1]?.id ?? published(job)[0]?.id ?? ''] });
  const [selected, setSelected] = useState(current?.id ?? '');
  const [before, setBefore] = useState(draft ? current?.id ?? '' : published(job).at(-2)?.id ?? current?.id ?? '');
  const [after, setAfter] = useState(draft?.id ?? current?.id ?? '');
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
  const comparedDraft = right?.draft ?? left?.draft;
  const result = compareCopy(left?.body ?? null, right?.body ?? null);
  const imageResult = compareImages(referenceImages(left), referenceImages(right));
  const imageBytes = compareImageBytes(versionImages(left), versionImages(right), left?.historicalImageBytesAvailable, right?.historicalImageBytesAvailable);
  const incomingResult = compareCopy(current?.body ?? null, incoming);
  const markedLines = markInlineChanges(result.lines);
  const diffLines = changesOnly ? markedLines.filter(line => line.kind !== 'same') : markedLines;
  const changedLines = result.lines.filter(line => line.kind !== 'same').length;
  const imageReferenceLabel = imageResult.status === 'unknown' ? '画像がない版があり比べられません' : imageResult.status === 'same_reference' ? '同じ画像・同じ並び順' : `追加${String(imageResult.added.length)}点・削除${String(imageResult.removed.length)}点${imageResult.reordered ? '・並び順の変更' : ''}`;
  const imageBytesLabel = imageBytes === 'unknown' ? '画像の中身を確認できません' : imageBytes === 'same_files' ? '画像の中身は同じ' : imageBytes === 'changed_files' ? '画像の中身が変わっています' : '両方とも画像なし';
  const imageBytesHelp = imageBytes === 'unknown' ? '比べる画像の元ファイルが保存されていないため、中身までは比べていません。' : imageBytes === 'changed_files' ? '保存した画像ファイルの中身が違います。同じ絵柄でも、画質や大きさを変えただけで「変わった」になります。' : imageBytes === 'same_files' ? '保存した画像ファイルの中身が同じです。' : '';
  const pastImagesMissing = left?.historicalImageBytesAvailable === false || right?.historicalImageBytesAvailable === false;

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
    const newVersion: CopyVersion = { id: `demo-received-${crypto.randomUUID()}`, label: `確認待ちの文面 ${String(job.versions.filter(item => item.kind === 'received').length + 1)}`, observedAt: now,
      certainty: 'unknown', kind: 'received', source: source.trim() || '受信元未指定', body: incoming,
      applications: null, note: '外部文面の受領のみ。媒体での更新・掲載は未確認。応募情報未取得。' };
    onAdd(newVersion); setSelected(newVersion.id); setBefore(current?.id ?? ''); setAfter(newVersion.id); openFeatureFromContent('body');
    setMessage('確認待ちの文面として画面内のデモ履歴に追加しました。媒体での掲載確認・HubSpot保存は行っていません。');
  }

  return <article className="jc-detail" id="job-details" tabIndex={-1}>
    <header className="jc-detail-heading"><div><h1>{job.title}</h1><p>{job.company} <span>·</span> {job.location} <span>·</span> {job.media}</p></div><span className="jc-badge">本文：{statusLabels[changeStatus(job)]}</span></header>
    {job.historyMayBeIncomplete && <p className="jc-notice" role="status">履歴は項目ごとに20件までの可能性があり、過去の版がすべて含まれているとは限りません。</p>}
    <div className="jc-record-meta"><span>{current?.source === HUBSPOT_BODY_SOURCE ? '表示中の文面（HubSpotの現在値）' : '表示中の文面'}: {current?.label ?? '本文未取得'}</span><InfoTip className="jc-infotip-left" label={`取得日時: ${current ? date(current.observedAt) : '—'}`}><p>{current?.source === 'HubSpotで取得した現在の文面' ? '現在の文面を今回取得した日時です。保存された日時と掲載開始日時は不明です。' : job.id.startsWith('hubspot-history-') ? 'HubSpotに保存された日時です。掲載が変わった日時ではありません。' : 'ファイルを取得した日時です。掲載が変わった日時ではありません。'}</p></InfoTip>{job.hubspotId ? null : <InfoTip className="jc-infotip-left" label="HubSpotの求人と未連携"><p>この求人は、HubSpot の求人レコードとまだつながっていません。つながると、応募の件数と HubSpot へのリンクが表示されます。</p></InfoTip>}</div>
    {message && <p className="jc-message" role="status">{message}</p>}
    <JobFeatureTabs value={tab} onChange={setTab} remembered={remembered} prefix={tabPrefix} onLeaveHidden={() => { openFeatureFromContent('timeline'); }}>
    <JobFeaturePanel feature="timeline" active={tab === 'timeline'} prefix={tabPrefix}>{visited.includes('timeline') && <JobTimeline job={job} billing={billing} showDummyBilling={showDummyBilling} marketMode={demo ? 'demo' : 'api'} onOpenVersion={id => { setSelected(id); openFeatureFromContent('body'); }} onCompareVersions={(from, to) => { setBefore(from); setAfter(to); openFeatureFromContent('diff'); }} />}</JobFeaturePanel>
    <JobFeaturePanel feature="applications" active={tab === 'applications'} prefix={tabPrefix}>{tab === 'applications' && <ApplicationTrend job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature="applicants" active={tab === 'applicants'} prefix={tabPrefix}>{visited.includes('applicants') && <ApplicantComposition job={job} includeReasons={false} billing={billing} />}</JobFeaturePanel>
    <JobFeaturePanel feature="reasons" active={tab === 'reasons'} prefix={tabPrefix}>{visited.includes('reasons') && <ApplicantReasonReview job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature="ab" active={tab === 'ab'} prefix={tabPrefix}><AbComparison job={job} records={records} /></JobFeaturePanel>
    <JobFeaturePanel feature="performance" active={tab === 'performance'} prefix={tabPrefix}>{tab === 'performance' && <HrhPerformance job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature={tab === 'market-table' ? 'market-table' : 'market'} active={tab === 'market' || tab === 'market-table'} prefix={tabPrefix}>{(visited.includes('market') || visited.includes('market-table')) && <MarketContext job={job} mode={demo ? 'demo' : 'api'} view={tab === 'market' ? 'charts' : tab === 'market-table' ? 'table' : 'inactive'} />}</JobFeaturePanel>
    {tab !== 'market-table' && <JobFeaturePanel feature="market-table" active={false} prefix={tabPrefix}>{null}</JobFeaturePanel>}
    {tab === 'market-table' && <JobFeaturePanel feature="market" active={false} prefix={tabPrefix}>{null}</JobFeaturePanel>}
    <JobFeaturePanel feature="factors" active={tab === 'factors'} prefix={tabPrefix}>{visited.includes('factors') && <MarketFactors job={job} />}</JobFeaturePanel>
    <JobFeaturePanel feature="report" active={tab === 'report'} prefix={tabPrefix}>{tab === 'report' && <ConsultantReview job={job} draft={reportDraft} onDraft={setReportDraft} billing={billing} />}</JobFeaturePanel>
    <JobFeaturePanel feature="body" active={tab === 'body'} prefix={tabPrefix}><div className="jc-history-layout">
      <aside className="jc-history"><h2>文面のタイムライン</h2><p className="jc-muted">本文の版を選ぶと内容が開きます</p>
        {[...job.versions].reverse().map(item => <button key={item.id} className="jc-version" aria-pressed={version?.id === item.id} onClick={() => { setSelected(item.id); }}>
          <span className="jc-version-top"><strong>{item.label}</strong><small>{item.kind === 'ai_draft' ? 'AI案・未掲載' : item.publishedFrom ? '掲載を確認' : item.kind === 'published' ? job.id.startsWith('hubspot-history-') ? '保存を確認・掲載時刻不明' : '媒体取得・掲載時刻不明' : '受領・掲載未確認'}</small></span>
          <time>{date(item.observedAt)}</time><span>{plainWording(item.source)}</span><small>{reviewed.includes(item.id) ? '確認済み（デモ）' : '未確認'}</small>
        </button>)}
        {!job.versions.length && <p className="jc-empty">本文はまだ届いていません。</p>}
      </aside>
      <section className="jc-reading">{version ? <><div className="jc-reading-title"><div><h2>{version.label}の文面</h2><p className="jc-muted">{joinPresent([plainWording(version.source), date(version.observedAt)])}</p></div><button className="jc-button" onClick={() => { onReview(version.id); }}>{reviewed.includes(version.id) ? '未確認に戻す' : '確認済みにする'}</button></div>
        {version.note.trim() && (job.id.startsWith('hubspot-history-') ? <details className="jc-originals"><summary>保存日時と画像について</summary><p className="jc-notice">{plainWording(version.note)}</p></details> : <p className="jc-notice">{plainWording(version.note)}</p>)}
        {version.observedPublicationStatus !== undefined && <p>媒体CSVの公開状態: {version.observedPublicationStatus || '未取得'}</p>}
        {job.id.startsWith('hubspot-history-') ? <div className="jc-copy-content"><JobCopyBody body={version.body} sections={version.bodySections} /><div>{versionImages(version) === undefined && job.currentImageObservation ? <><ImageGallery title="現在取得できる掲載画像" images={job.currentImageObservation.images} /><p className="jc-muted">画像を確認した日時：{date(job.currentImageObservation.observedAt)}。この版の保存時点の画像は不明です。</p></> : <ImageGallery title="この版の掲載画像" images={versionImages(version)} />}</div></div> : <><ImageGallery title="この版の掲載画像" images={versionImages(version)} /><div className="jc-full-copy-heading"><h3>{version.source === HUBSPOT_BODY_SOURCE ? '仕事内容（HubSpotの現在値）' : '求人票の本文（全文）'}</h3><span>読み取り専用 · 原文の段落・改行を保持</span></div><pre className="jc-body">{version.body}</pre></>}
        {job.hubspotUrl && <a href={job.hubspotUrl} target="_blank" rel="noreferrer">HubSpotで求人レコードを開く</a>}
        {version.kind === 'received' ? <p className="jc-notice">応募情報は未取得です。0件とは判定していません。</p> : <ApplicationSummary job={job} version={version} live={job.dataSource === 'hubspot'} />}
        <button className="jc-text-button" onClick={() => { const index = job.versions.findIndex(item => item.id === version.id); setBefore(job.versions[index - 1]?.id ?? version.id); setAfter(version.id); openFeatureFromContent('diff'); }}>この版を前の版と比較する →</button>
      </> : <div className="jc-empty"><h2>本文未取得</h2><p>{job.id.startsWith('hubspot-history-') ? 'この求人の文面の履歴はありません。本文は未取得です。' : '欠損を「変更なし」や「削除」と判断しません。'}</p>{job.id.startsWith('hubspot-history-') && <ImageGallery title="現在取得できる掲載画像" images={job.currentImageObservation?.images} />}<button className="jc-button" onClick={() => { openFeatureFromContent('receive'); }}>外部文面を確認する</button></div>}</section>
    </div></JobFeaturePanel>
    <JobFeaturePanel feature="diff" active={tab === 'diff'} prefix={tabPrefix}>{tab === 'diff' && <section className="jc-comparison">{job.draftHistoryMayBeIncomplete && <p className="jc-notice">過去の案はすべて取得できていない可能性があります。</p>}{comparedDraft && <><DraftReview key={`${comparedDraft.draft_id}-${comparedDraft.review_status}`} job={job} draft={comparedDraft} onSaved={onDraftSaved} /><DraftFacts draft={comparedDraft} current={current} location={job.location} /></>}<div className="jc-compare-controls"><label>比較元<select aria-label="比較元" value={before} onChange={event => { setBefore(event.target.value); }}><option value="">本文なし</option>{job.versions.map(item => <option key={item.id} value={item.id}>{item.label} · {item.kind === 'ai_draft' ? 'AI案' : plainWording(item.source)}</option>)}</select></label><span aria-hidden="true">→</span><label>比較先<select aria-label="比較先" value={after} onChange={event => { setAfter(event.target.value); }}><option value="">本文なし</option>{job.versions.map(item => <option key={item.id} value={item.id}>{item.label} · {item.kind === 'ai_draft' ? 'AI案' : plainWording(item.source)}</option>)}</select></label></div>
      <section className="jc-comparison-overview" aria-label="比較結果の要約"><div><span>本文・募集条件</span><strong>{statusLabels[result.status]}</strong><small>追加{result.lines.filter(line => line.kind === 'added').length}行・削除{result.lines.filter(line => line.kind === 'removed').length}行</small></div><div><span>画像の差し替え・並び順</span><strong>{imageReferenceLabel}</strong></div><div><span>画像の中身</span><strong>{imageBytesLabel}</strong>{imageBytesHelp && <small>{imageBytesHelp}</small>}</div></section>
      <nav className="jc-comparison-jumps" aria-label="差分の確認箇所"><a href="#job-copy-text-diff">本文の差分へ</a><a href="#job-copy-image-diff">画像の比較へ</a></nav>
      <section className="jc-image-comparison" id="job-copy-image-diff" aria-label="画像の差分"><h2>掲載画像の比較</h2><p className="jc-notice">{imageResult.status === 'unknown' ? '画像がない版があるため、画像が変わったかどうかは分かりません。' : imageResult.status === 'same_reference' ? '同じ画像が同じ順に並んでいます。' : `画像の差し替え：追加${String(imageResult.added.length)}点・削除${String(imageResult.removed.length)}点${imageResult.reordered ? '・並び順の変更あり' : ''}`}{pastImagesMissing ? '過去の時点の画像は保存されていないため、当時の画像の中身は確認できません。' : ''}</p>
        <div className="jc-image-compare-grid"><ImageGallery title="比較元の画像" images={versionImages(left)} marks={imageResult.removed.map(image => image.url)} /><ImageGallery title="比較先の画像" images={versionImages(right)} marks={imageResult.added.map(image => image.url)} /></div>
        <p className="jc-muted">操作デモの画像は架空のイラスト、媒体から取り込んだ求人の画像は媒体から取得した実画像です。初回取得だけでは過去との画像変更を判定できません。</p>
      </section>
      <h2 className="jc-text-diff-heading" id="job-copy-text-diff">本文・募集条件の比較</h2>
      <label className="jc-diff-toggle"><input type="checkbox" checked={changesOnly} onChange={event => { setChangesOnly(event.target.checked); setDiffLimit(300); }} />変更箇所だけを表示（追加・削除{changedLines}行）</label>
      <div className="jc-diff-summary"><strong>本文：{statusLabels[result.status]}</strong><span><i className="jc-added-key" />追加 <i className="jc-removed-key" />削除 · 行ごとに比べ、行の中で変わった文字・数字は濃い色で示します</span></div>
      {changesOnly && !diffLines.length && result.status !== 'unavailable' && <p className="jc-notice" role="status">追加・削除された本文の行はありません。チェックを外すとすべての行を確認できます。</p>}
      {result.status === 'unavailable' ? <p className="jc-empty">比較先の本文がありません。変更の有無は判定できません。</p> : <><div className="jc-diff-lines" aria-label="文面の差分">{diffLines.slice(0, diffLimit).map((line, index) => <div key={index} className={`jc-diff-line jc-line-${line.kind}`}><span className="jc-line-mark" aria-label={line.kind === 'added' ? '追加' : line.kind === 'removed' ? '削除' : '変更なし'}>{line.kind === 'added' ? '+' : line.kind === 'removed' ? '−' : ' '}</span><pre>{line.segments ? line.segments.map((segment, part) => segment.changed ? line.kind === 'added' ? <ins key={part} className="jc-mark-added">{segment.text}</ins> : <del key={part} className="jc-mark-removed">{segment.text}</del> : segment.text) : line.text || ' '}</pre></div>)}</div>{diffLines.length > diffLimit && <button className="jc-button" onClick={() => { setDiffLimit(count => count + 300); }}>さらに300行を表示（全{diffLines.length}行）</button>}</>}
      <details className="jc-originals"><summary>比較する2つの原文を見る</summary><div><section><h3>比較元</h3><pre className="jc-body">{left?.body ?? '本文なし'}</pre></section><section><h3>比較先</h3><pre className="jc-body">{right?.body ?? '本文なし'}</pre></section></div></details>
      <p className="jc-muted">改行コードのみの差は表記差です。給与・数字・否定表現を消して比較しません。AI案との差は、掲載変更を意味しません。</p>
    </section>}</JobFeaturePanel>
    <JobFeaturePanel feature="receive" active={tab === 'receive'} prefix={tabPrefix}>{tab === 'receive' && <section className="jc-receive"><h2>届いた文面を、現在の本文と照合する</h2><p>外部で作成された文面をここへ渡します。比較する本文の入力で、求人本文の直接編集ではありません。</p>
      <div className="jc-import-options"><label className="jc-file">UTF-8テキストを読み込む<input type="file" accept=".txt,text/plain" disabled={reading} onChange={event => { void readFile(event); }} /></label><span>{reading ? '読み込み中…' : '200KBまで / CSV・HubSpot取り込みは未接続'}</span></div>
      {readError && <p className="jc-error" role="alert">{readError}</p>}
      <label>受信元・資料名<input value={source} maxLength={200} onChange={event => { setSource(event.target.value); }} /></label>
      <label>受け取った文面<textarea value={incoming} maxLength={100_000} rows={12} placeholder="求人票の外部文面を貼り付けてください" onChange={event => { setIncoming(event.target.value); setCompared(false); }} /></label>
      <div className="jc-receive-actions"><button className="jc-button jc-primary" disabled={reading} onClick={() => { setCompared(true); }}>現在の本文と比較</button><button className="jc-button" onClick={() => { setIncoming(current?.body ?? ''); setSource('現在版の再取得デモ'); setCompared(false); }}>同じ文面で試す</button></div>
      {compared && <div className="jc-receive-result" role="status"><h3>{statusLabels[incomingResult.status]}</h3><p>掲載更新の確認: 未確認。判定は本文比較の結果です。</p>{incomingResult.status !== 'unavailable' && <button className="jc-button" onClick={addObservation}>{incomingResult.status === 'unchanged' ? '変更なしをデモ確認' : '確認待ちの文面としてデモ履歴に追加'}</button>}</div>}
      <p className="jc-notice">将来の文字起こし → AI文面案の生成は、タイムラインの「AI案」として接続予定です。この画面はまだAIを呼び出しません。</p>
    </section>}</JobFeaturePanel>
    </JobFeatureTabs>
  </article>;
}

/** 「仮の課金データを表示」の選択をブラウザごとに覚えるキー。読めない・書けないときは既定値（表示する）。 */
export const DUMMY_BILLING_STORAGE_KEY = 'jobCopy.showDummyBilling';
function readDummyBillingChoice(): boolean {
  try {
    const value = window.localStorage.getItem(DUMMY_BILLING_STORAGE_KEY);
    return value === null ? DUMMY_BILLING_ENABLED : value === '1';
  } catch { return DUMMY_BILLING_ENABLED; }
}
function writeDummyBillingChoice(value: boolean) {
  try { window.localStorage.setItem(DUMMY_BILLING_STORAGE_KEY, value ? '1' : '0'); } catch { /* 覚えられなくても表示は切り替える */ }
}

export function JobCopyScreen() {
  const query = new URLSearchParams(window.location.search);
  const [initialId] = useState(query.get('job'));
  const [snapshotRequested] = useState(!query.has('listing') && query.get('demo') !== '1' && (query.get('data') === 'actual' || window.location.pathname === '/app/job-copy' || window.location.pathname === '/'));
  const [records, setRecords] = useState<JobCopyRecord[]>(snapshotRequested || query.has('listing') ? [] : jobs);
  const [snapshotAt, setSnapshotAt] = useState('');
  const [snapshotError, setSnapshotError] = useState<SnapshotErrorGuidance | null>(null);
  const [snapshotLoading, setSnapshotLoading] = useState(snapshotRequested);
  const [snapshotSlow, setSnapshotSlow] = useState(false);
  const [snapshotAttempt, setSnapshotAttempt] = useState(0);
  const snapshotAbort = useRef<AbortController | null>(null);
  const [captured, setCaptured] = useState(false);
  const [live, setLive] = useState(false);
  const originalListing = useRef<{ records: JobCopyRecord[]; snapshotAt: string; live: boolean; captured: boolean; selectedId: string } | null>(null);
  const [listingSource, setListingSource] = useState<'fixed' | 'hubspot'>(() => new URLSearchParams(location.search).has('listing') ? 'hubspot' : 'fixed');
  const [listingDetail, setListingDetail] = useState('idle');
  const [mobileListingView, setMobileListingView] = useState<'list' | 'detail'>('list');
  const [listingFailure, setListingFailure] = useState('');
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
  // One market-data cache for the whole screen: the timeline and the 市場 tabs of every job share it.
  const [marketCache] = useState<MarketCache>(() => new Map());
  const billingByJob = useMemo(() => billingEntriesByJob(billingPeriods), [billingPeriods]);
  const [showDummyBilling, setShowDummyBilling] = useState(readDummyBillingChoice);
  function changeDummyBilling(value: boolean) { setShowDummyBilling(value); writeDummyBillingChoice(value); }
  const demoMode = !snapshotRequested && !live && !captured;
  // データ取込と応募者の条件検索は、主作業（一覧とタイムライン）の外に置き、ボタンで開く。
  const [importOpen, setImportOpen] = useState(false);
  const [reverseOpen, setReverseOpen] = useState(false);
  const [featureRequest, setFeatureRequest] = useState<FeatureRequest | null>(null);
  // 一覧と詳細を、画面の下端まで使う高さにする（上の帯の高さを測る）。
  const workspace = useRef<HTMLDivElement>(null);
  const [workspaceTop, setWorkspaceTop] = useState(0);
  useLayoutEffect(() => {
    const element = workspace.current;
    if (!element) return;
    const measure = () => { setWorkspaceTop(Math.max(0, Math.round(element.getBoundingClientRect().top + window.scrollY))); };
    measure();
    window.addEventListener('resize', measure);
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
    // Navigation arrives asynchronously; it can move the workspace without resizing jc-app.
    for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) observer?.observe(ancestor);
    return () => { window.removeEventListener('resize', measure); observer?.disconnect(); };
  }, []);
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
          // 媒体の公開状況 is read separately: a failure only marks those jobs, the list stays.
          void apiGet<unknown>('/api/job-copy/listing-status', { signal: controller.signal, timeoutMs: 30_000 }).then(status => {
            if (controller.signal.aborted) return;
            const listings = status.ok ? parseListingStatus(status.data) : null;
            setRecords(current => withPublicationFor(current, items, listings));
          }).catch(() => { if (!controller.signal.aborted) setRecords(current => withPublicationFor(current, items, null)); });
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
  // Memoized so 横断比較 (which diffs every version pair of every job) is not rebuilt on unrelated
  // state changes such as opening a panel or a window resize.
  const visible = useMemo(() => orderJobs(records.filter(job => (!normalizedSearch || `${job.title} ${job.company} ${job.mediaJobId} ${job.location}`.toLocaleLowerCase('ja-JP').includes(normalizedSearch)) && (customer === 'all' || (job.id.startsWith('demo-job-') || job.dataSource === 'hubspot' ? job.company : 'unlinked') === customer) && (media === 'all' || job.media === media) && (status === 'all' || changeStatus(job) === status)), listOrder), [records, normalizedSearch, customer, media, status, listOrder]);
  const selected = visible.find(job => job.id === selectedId) ?? visible[0];
  const filtersActive = Boolean(normalizedSearch || customer !== 'all' || media !== 'all' || status !== 'all');
  const filterCount = [customer !== 'all', media !== 'all', status !== 'all', listOrder !== 'source'].filter(Boolean).length;
  function resetFilters() { setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); document.getElementById('job-list-search')?.focus(); }
  function choose(job: JobCopyRecord) { setFeatureRequest(null); setSelectedId(job.id); const url = new URL(window.location.href); url.searchParams.set('job', job.id); window.history.replaceState(null, '', url); if (window.matchMedia('(max-width: 800px)').matches) window.requestAnimationFrame(() => { const detail = document.getElementById('job-details'); detail?.focus({ preventScroll: true }); detail?.scrollIntoView({ block: 'start' }); }); }
  const bannerLabel = snapshotLoading ? '実データを読み込み中' : snapshotAt ? '実データ（取得済み）' : snapshotRequested && !records.length ? '実データ未表示' : live ? 'HubSpot読み取り' : captured ? '媒体から取り込んだ求人' : '操作デモ';
  // The dummy billing is on by default, so the banner of real data says the amounts are made up.
  const dummyNote = showDummyBilling ? ' 課金額は、実際の課金データがまだ無いため仮の金額（ダミー）を表示しています。実際の請求額ではありません（「仮の課金データを表示」を外すと消えます）。' : '';
  const bannerText = snapshotAt ? `媒体CSVの本文・画像とHubSpotの実求人・応募集計です。応募集計取得：${date(snapshotAt)}。最新値の自動更新ではありません。確認記録は画面内のみ保持します。${dummyNote}` : live ? `${records.some(job => job.id.startsWith('hubspot-history-')) ? 'HubSpotに保存された求人の変更履歴です。応募件数は一覧で確認できます。応募日と版の対応は未取得です。日時は保存日時で、掲載開始日時は不明です。' : records.some(job => published(job).length > 0) ? '実際の取引先・求人に、媒体から取得した本文・画像と応募をつないでいます。画像の取得時点や、どの版への応募か不明な件数は各表示で確認してください。検証記録はこの画面の中だけに残ります。' : 'HubSpotに保存された求人を表示しています。版の日時や取得範囲は各版の注記を確認してください。確認状況は画面内だけに保持します。'}${dummyNote}` : captured ? `HRハッカーの本文・画像です。過去版の有無と画像の取得時点は各版の注記を確認してください。応募未取得・HubSpot未保存です。${dummyNote}` : snapshotRequested && !records.length ? '取得済みの実データを読み取ります。欠損を架空データで補いません。' : '求人・本文・応募数・課金額はすべて架空です。HubSpot未接続。追加した履歴・確認状況は再読み込みで消えます。';
  // 一覧の求人を入れ替えたら、新しい一覧に無い求人の課金CSVの反映を外す（表示と反映中の表示を合わせる）。
  function keepBillingFor(next: readonly JobCopyRecord[]) {
    setBillingPeriods(previous => previous.filter(period => next.some(job => job.id === period.jobId)));
  }
  function openReceive() {
    if (!selected) return;
    setImportOpen(false); setView('list'); setFeatureRequest(previous => ({ feature: 'receive', nonce: (previous?.nonce ?? 0) + 1 }));
  }
  function toggleImport() {
    setImportOpen(open => {
      if (!open) window.requestAnimationFrame(() => { document.getElementById('job-copy-data-import-heading')?.focus(); });
      return !open;
    });
  }
  function openListingHistory(job: JobCopyRecord) {
    if (!records.some(item => item.id.startsWith('hubspot-history-'))) originalListing.current = { records, snapshotAt, live, captured, selectedId };
    stopSnapshot(); setSnapshotAt(''); setListingDetail('ready');
    setRecords(items => [...items.filter(item => item.id.startsWith('hubspot-history-') && item.id !== job.id), job]);
    setFeatureRequest({ feature: job.latestDraftId ? 'diff' : 'body', nonce: Date.now() }); setSelectedId(job.id); setReviewed([]); setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setView('list'); setCaptured(false); setLive(true);
  }
  function restoreOriginalListing() {
    setListingSource('fixed');
    const original = originalListing.current;
    if (!original) return;
    stopSnapshot(); setSnapshotAt(original.snapshotAt); setRecords(original.records); setSelectedId(original.selectedId);
    setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setLive(original.live); setCaptured(original.captured); setView('list'); setFeatureRequest(null);
    originalListing.current = null;
    if (!original.records.length && snapshotRequested) retrySnapshot();
  }
  return <MarketCacheContext.Provider value={marketCache}><div className="jc-app"><div className="jc-topline"><header className="jc-page-heading"><h1>求人文面管理</h1><InfoTip className="jc-mode jc-infotip-left" label="試作版"><p>開発中の画面です。表示や操作は今後変わります。</p></InfoTip></header>
    <div className="jc-demo" title={bannerText}><strong>{bannerLabel}</strong><span>{bannerText}</span></div>
    {snapshotAt && <InfoTip className="jc-snapshot-tip" label="取得した範囲"><section className="jc-snapshot-summary" aria-label="実データの取得範囲"><span><strong>{new Set(records.map(job => job.company)).size}</strong>取引先</span><span><strong>{records.length}</strong>求人</span><span><strong>{records.reduce((sum, job) => sum + published(job).length, 0)}</strong>取得した本文の版</span><span><strong>{records.reduce((sum, job) => sum + (job.overallApplications?.total ?? 0), 0)}</strong>応募（HubSpot記録分・求人ごとの件数の合計（重複あり））</span><span>どの版への応募か分からない応募 <strong>{records.reduce((sum, job) => sum + (unmatchedApplicationCount(job) ?? 0), 0)}</strong>件（求人ごとの件数の合計（重複あり））</span><span>期間比較表の版の行に入らない応募 <strong>{records.reduce((sum, job) => sum + (applicationsOutsideTimeline(job) ?? 0), 0)}</strong>件（求人ごとの件数の合計（重複あり））</span></section><p className="jc-snapshot-note">1件の応募が複数の求人に関連することがあるため、求人ごとの件数を足した値は応募の実数より多いことがあります。「期間比較表の版の行に入らない応募」は、応募日が無い応募、最初の取得より前・取得日の間・最後の取得より後の日付の応募、複数の求人に関連する応募です。</p></InfoTip>}
    <button type="button" className="jc-button jc-import-toggle" aria-expanded={importOpen} aria-controls="job-copy-data-import" onClick={toggleImport}>データ取込</button></div>
    <div className="jc-scope-line"><p>{listingSource === 'hubspot' ? '媒体ごとに別の求人として表示します。応募は求人に関連する件数です。' : <>この画面は、選んで取り込んだ一部の求人{records.length ? `（${String(records.length)}件）` : ''}だけを表示しています。管理しているすべての求人ではありません。</>}</p>
      <label className="jc-dummy-toggle"><input type="checkbox" checked={showDummyBilling} onChange={event => { changeDummyBilling(event.target.checked); }} />仮の課金データを表示</label></div>
    {snapshotLoading && <div className="jc-notice"><p role="status">{snapshotSlow ? '読み込みに時間がかかっています。待機を続けるか、求人データを再取得できます。' : '求人一覧・本文・応募集計を読み込んでいます…画像は表示時に取得します。'}</p>{snapshotSlow && <button type="button" className="jc-button" onClick={retrySnapshot}>求人データを再取得</button>}</div>}
    {snapshotError && <><SnapshotErrorNotice guidance={snapshotError} /><button type="button" className="jc-button" onClick={retrySnapshot}>求人データを再取得</button></>}
    <JobCopyDataImport open={importOpen} onClose={() => { setImportOpen(false); document.querySelector<HTMLButtonElement>('.jc-import-toggle')?.focus(); }}>
      <HubSpotReadPanel key={panelEpoch} onOpen={job => { stopSnapshot(); setSnapshotAt(''); keepBillingFor([job]); setRecords([job]); setSelectedId(job.id); setReviewed([]); setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setCaptured(false); setLive(true); }} />
    <MediaCaptureImport onImport={items => { stopSnapshot(); setSnapshotAt(''); keepBillingFor(items); setRecords(items); setSelectedId(items[0]?.id ?? ''); setReviewed([]); setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setCaptured(true); setLive(false); setPanelEpoch(value => value + 1); }} />
      <BillingImportPanel records={records} applied={billingPeriods} onApply={setBillingPeriods} onClear={() => { setBillingPeriods([]); }} />
      <section className="jc-receive-entry" aria-labelledby="job-copy-receive-entry-heading"><h3 id="job-copy-receive-entry-heading">外部文面を確認</h3><p>外部から届いた本文を、選んでいる求人{selected ? `「${selected.title}」` : ''}の現在の本文と照合します。求人本文は書き換えません。</p><button type="button" className="jc-button" disabled={!selected} onClick={openReceive}>外部文面を照合する</button></section>
    </JobCopyDataImport>
    <div className={`jc-workspace jc-mobile-${mobileListingView}`} ref={workspace} style={{ '--jc-workspace-top': `${String(workspaceTop)}px` } as CSSProperties}><aside className="jc-list"><nav className="jc-list-source" aria-label="求人一覧の選択"><button type="button" aria-pressed={listingSource === 'hubspot'} onClick={() => { if (listingSource === 'hubspot') return; originalListing.current = { records, snapshotAt, live, captured, selectedId }; stopSnapshot(); setListingSource('hubspot'); setListingDetail('idle'); setMobileListingView('list'); }}>HubSpot の求人</button><button type="button" aria-pressed={listingSource === 'fixed'} onClick={restoreOriginalListing}>固定一覧を表示</button></nav>
      <HubSpotListingsPanel initialListingId={new URLSearchParams(location.search).get('listing') ?? undefined} active={listingSource === 'hubspot'} onOpen={openListingHistory} onLoading={() => { setListingDetail('loading'); setMobileListingView('detail'); }} onFailure={text => { setListingDetail('error'); setListingFailure(text); }} />
      <div className="jc-fixed-list" hidden={listingSource !== 'fixed'}><div className="jo-view-toggle" role="group" aria-label="表示の切り替え"><button type="button" aria-pressed={view === 'list'} onClick={() => { setView('list'); }}>一覧</button><button type="button" aria-pressed={view === 'overview'} onClick={() => { setView('overview'); }}>横断比較</button></div><div className="jc-list-heading"><h2 id="job-list-heading" tabIndex={-1}>求人レコード</h2><span aria-live="polite" aria-atomic="true">{snapshotLoading ? '取得中' : snapshotError && !records.length ? '未取得' : `${String(visible.length)} / ${String(records.length)}件`}</span><button type="button" className="jc-button jc-reverse-toggle" aria-label="応募者の条件で探す" aria-expanded={reverseOpen} aria-controls="job-copy-reverse-search" title="応募者の性別・年代・地域の組み合わせから、該当する応募が多い求人を探します" onClick={() => { setReverseOpen(open => !open); if (!reverseOpen) window.requestAnimationFrame(() => { document.getElementById('job-copy-reverse-search-heading')?.focus(); }); }}>応募者で探す</button></div><label className="jc-list-search"><span className="jc-visually-hidden">求人名・企業名・勤務地で検索</span><input id="job-list-search" type="search" value={search} placeholder="求人名・企業名・勤務地で検索" onChange={event => { setSearch(event.target.value); }} /></label>
      <details className="jc-filter-more"><summary>絞り込み{filterCount > 0 ? `（${String(filterCount)}）` : ''}・並び順</summary><div className="jc-filter-body"><label>取引先<select aria-label="取引先" value={customer} onChange={event => { setCustomer(event.target.value); }}><option value="all">すべての取引先</option>{captured ? <option value="unlinked">取引先未紐付け</option> : [...new Set(records.map(job => job.company))].map(value => <option key={value} value={value}>{value}{live ? '' : '（架空）'}</option>)}</select></label>
      <div className="jc-filters"><label>媒体<select value={media} onChange={event => { setMedia(event.target.value); }}><option value="all">すべて</option>{[...new Set(records.map(job => job.media))].map(value => <option key={value}>{value}</option>)}</select></label><label>変更判定<select value={status} onChange={event => { setStatus(event.target.value); }}><option value="all">すべて</option>{Object.entries(statusLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label></div>
      <label>並び順<select aria-label="並び順" value={listOrder} onChange={event => { setListOrder(event.target.value === 'applications' ? 'applications' : 'source'); }}><option value="source">取得順</option><option value="applications">応募数が多い順</option></select></label></div></details>
      {filtersActive && <button type="button" className="jc-button jc-filter-reset" onClick={resetFilters}>検索条件をリセット</button>}
      <div className="jc-list-scroll">{visible.map(job => <button className="jc-job" key={job.id} aria-pressed={selected?.id === job.id} onClick={() => { choose(job); }}><span className="jc-job-company">{job.company}</span><strong>{job.title}</strong><span>{job.location} · {job.media}</span><span className="jc-job-bottom"><small>{published(job).length}版{job.versions.some(version => version.kind === 'ai_draft') ? ' + AI案' : ''}</small><small>{statusLabels[changeStatus(job)]}</small></span><small>{applicationCountLabel(job)}</small></button>)}{!visible.length && <div className="jc-empty"><p>{snapshotLoading ? '求人一覧を取得中です。' : records.length ? '一致する求人はありません。' : '表示できる求人がありません。'}</p></div>}</div>
    </div></aside><div className="jc-main">{listingSource === 'hubspot' && <button type="button" className="jc-button jc-mobile-back" onClick={() => { setMobileListingView('list'); }}>一覧に戻る</button>}{listingSource === 'hubspot' && listingDetail !== 'ready' ? <main className="jc-detail jc-detail-placeholder" id="job-details"><span className="jc-placeholder-icon" aria-hidden="true">▤</span><h1>{listingDetail === 'loading' ? '求人票を取得しています' : listingDetail === 'error' ? '求人票を取得できませんでした' : '求人を選んで内容を確認'}</h1><p role={listingDetail === 'error' ? 'alert' : 'status'}>{listingDetail === 'loading' ? '本文・画像・文面の履歴を確認しています。' : listingDetail === 'error' ? listingFailure : '一覧から求人を選ぶと、本文と取得できた画像をここに表示します。'}</p></main> : <>{reverseOpen && <ReverseSearch records={records} onClose={() => { setReverseOpen(false); document.querySelector<HTMLButtonElement>('.jc-reverse-toggle')?.focus(); }} onChoose={job => { setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setReverseOpen(false); setView('list'); choose(job); }} />}{view === 'overview' && records.length > 0 ? <JobOverview records={visible} billing={billingByJob} showDummyBilling={showDummyBilling} onChoose={job => { setView('list'); choose(job); }} /> : selected ? <CopyDetail key={`${selected.id}-${selected.versions[0]?.id ?? ''}`} job={selected} records={records} billing={billingByJob[selected.id]} showDummyBilling={showDummyBilling} demo={demoMode} request={featureRequest} onDraftSaved={(draft, revision) => { setRecords(items => items.map(item => item.id === selected.id ? { ...item, draftRevision: revision, versions: item.versions.map((version, index) => version.draft?.draft_id === draft.draft_id ? draftVersion(draft, item.versions.slice(0, index).filter(v => v.draft).length) : version) } : item)); }} reviewed={reviewed} onReview={id => { setReviewed(items => items.includes(id) ? items.filter(item => item !== id) : [...items, id]); }} onAdd={version => { setRecords(items => items.map(job => job.id === selected.id ? { ...job, versions: [...job.versions, version] } : job)); }} /> : <main className="jc-detail jc-empty"><h1>{snapshotLoading ? '求人データを取得しています' : records.length ? '一致する求人はありません' : '表示できる求人がありません'}</h1><p>{snapshotLoading ? '取得完了後に本文と応募集計を表示します。' : records.length ? '検索・取引先・媒体・変更判定の条件を見直すか、一覧の「検索条件をリセット」を押してください。' : 'データの取得状況と、画面上部の案内を確認してください。'}</p></main>}</>}</div></div>
  </div></MarketCacheContext.Provider>;
}
