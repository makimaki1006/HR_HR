import { useEffect, useState } from 'react';
import type { ChangeEvent } from 'react';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import { compareCopy } from './diff';
import { compareImages, compareImageBytes, referenceImages, imagesByVersion } from './images';
import { ImageGallery } from './ImageGallery';
import { MediaCaptureImport } from './MediaCaptureImport';
import { ApplicantComposition } from './ApplicantComposition';
import { ConsultantReview } from './ConsultantReview';
import { HubSpotReadPanel } from './HubSpotReadPanel';
import type { ConsultantDraft } from './ConsultantReview';
import { apiGet } from '../../api/client';
import { parseRealMoc } from './realMoc';
import { applicationCountLabel, orderJobs } from './jobList';
import type { JobListOrder } from './jobList';
import { snapshotErrorGuidance, SnapshotErrorNotice } from './SnapshotErrorNotice';
import type { SnapshotErrorGuidance } from './SnapshotErrorNotice';
import './job-copy.css';

const statusLabels = { initial: '初回取得', unchanged: '変更なし', format_only: '表記差のみ', changed: '内容変更あり', unavailable: '判定不能' };
const certaintyLabels = { confirmed: '確定', estimated: '推定', unknown: '不明' };
const date = (value: string) => new Date(value).toLocaleString('ja-JP', { timeZone: 'Asia/Tokyo', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
const published = (job: JobCopyRecord) => job.versions.filter(version => version.kind === 'published');
const latest = (job: JobCopyRecord) => published(job).at(-1);
const versionImages = (version: CopyVersion | undefined) => version ? version.images ?? imagesByVersion[version.id] : undefined;
const changeStatus = (job: JobCopyRecord) => {
  const versions = published(job);
  return compareCopy(versions.at(-2)?.body ?? null, versions.at(-1)?.body ?? null).status;
};

function ApplicationSummary({ version, live = false }: { version: CopyVersion; live?: boolean }) {
  if (version.kind === 'ai_draft') return <div className="jc-notice">未掲載のAI案です。応募実績には対応させていません。</div>;
  if (version.applications === null) return <p className="jc-notice">応募実績は未取得です。0件とは判定していません。掲載開始日時と過去版も未取得です。</p>;
  return <section className="jc-applications" aria-label="版別の応募状況">
    <div className="jc-period"><strong>この文面に対応する応募</strong><span>{version.publishedFrom ? date(version.publishedFrom) : '開始不明'} → {version.publishedUntil ? date(version.publishedUntil) : '終了未確認'} · 期間{certaintyLabels[version.certainty]}</span></div>
    <div className="jc-counts"><div><span>確定対応</span><strong>{version.applications.confirmed}<small>件</small></strong></div><div><span>推定対応</span><strong>{version.applications.estimated}<small>件</small></strong></div><div><span>版の対応不明</span><strong>{version.applications.unknown}<small>件</small></strong></div></div>
    <p>{live ? 'HubSpot応募レコードの日付対応による集計です。推定対応は変更検知日の代表版を基準にしています。版対応不明の応募総数は応募者構成で確認できます。' : '架空の集計です。不明件数はこの観測区間に残る未配賦応募で、確定件数に含めません。'}本文変更による効果を示す値ではありません。</p>
  </section>;
}

function CopyDetail({ job, onAdd, reviewed, onReview }: { job: JobCopyRecord; onAdd: (version: CopyVersion) => void; reviewed: string[]; onReview: (id: string) => void }) {
  const current = latest(job) ?? (job.dataSource === 'hubspot' ? job.versions.at(-1) : undefined);
  const [tab, setTab] = useState<'body' | 'diff' | 'receive' | 'applicants' | 'report'>('body');
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
      if (!file.name.toLowerCase().endsWith('.txt')) throw new Error('このMOCでは.txtのみ対応しています。CSVの列対応は未接続です。');
      const body = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
      setIncoming(body); setSource(file.name);
    } catch (error) {
      setIncoming(''); setReadError(error instanceof Error ? error.message : 'テキストを読み込めませんでした。');
    } finally { setReading(false); event.target.value = ''; }
  }

  function addObservation() {
    if (!compared || incomingResult.status === 'unavailable') return;
    if (incomingResult.status === 'unchanged') {
      setMessage('変更なしの観測をデモ確認しました。新しい本文版は作りません。観測の永続保存は未接続です。');
      return;
    }
    const now = new Date().toISOString();
    const newVersion: CopyVersion = { id: `demo-received-${crypto.randomUUID()}`, label: `受信版 ${String(job.versions.filter(item => item.kind === 'received').length + 1)}`, observedAt: now,
      certainty: 'unknown', kind: 'received', source: source.trim() || '受信元未指定', body: incoming,
      applications: null, note: '外部文面の受領のみ。媒体での更新・掲載は未確認。応募情報未取得。' };
    onAdd(newVersion); setSelected(newVersion.id); setBefore(current?.id ?? ''); setAfter(newVersion.id); setTab('body');
    setMessage('受信版を画面内のデモ履歴に追加しました。媒体での掲載確認・HubSpot保存は行っていません。');
  }

  return <article className="jc-detail" id="job-details" tabIndex={-1}>
    <header className="jc-detail-heading"><div><p className="jc-eyebrow">求人レコード / {job.mediaJobId}</p><h1>{job.title}</h1><p>{job.company} <span>·</span> {job.location} <span>·</span> {job.media}</p></div><span className="jc-badge">本文：{statusLabels[changeStatus(job)]}</span></header>
    <div className="jc-record-meta"><span>{current?.source === 'HubSpot shigotonaiyou' ? '現在のHubSpot値' : '現在の掲載観測版'}: {current?.label ?? '本文未取得'}</span><span>観測ラベル: {current ? date(current.observedAt) : '—'} JST</span><span>{job.hubspotId ? `HubSpot求人ID: ${job.hubspotId}` : 'HubSpotリンク: 実求人IDの接続待ち'}</span></div>
    {message && <p className="jc-message" role="status">{message}</p>}
    <nav className="jc-tabs" aria-label="求人文面の表示"><button aria-pressed={tab === 'body'} onClick={() => { setTab('body'); }}>本文・履歴</button><button aria-pressed={tab === 'diff'} onClick={() => { setTab('diff'); }}>差分比較</button><button aria-pressed={tab === 'applicants'} onClick={() => { setTab('applicants'); }}>応募者構成</button><button aria-pressed={tab === 'report'} onClick={() => { setTab('report'); }}>顧客報告・検証</button><button aria-pressed={tab === 'receive'} onClick={() => { setTab('receive'); }}>外部文面を確認</button></nav>
    {tab === 'applicants' && <ApplicantComposition job={job} />}
    {tab === 'report' && <ConsultantReview job={job} draft={reportDraft} onDraft={setReportDraft} />}
    {tab === 'body' && <div className="jc-history-layout">
      <aside className="jc-history"><h2>文面のタイムライン</h2><p className="jc-muted">本文の版を選ぶと内容が開きます</p>
        {[...job.versions].reverse().map(item => <button key={item.id} className="jc-version" aria-pressed={version?.id === item.id} onClick={() => { setSelected(item.id); }}>
          <span className="jc-version-top"><strong>{item.label}</strong><small>{item.kind === 'ai_draft' ? 'AI案・未掲載' : item.publishedFrom ? '掲載観測' : item.kind === 'published' ? '媒体取得・掲載時刻不明' : '受領・掲載未確認'}</small></span>
          <time>{date(item.observedAt)}</time><span>{item.source}</span><small>{reviewed.includes(item.id) ? '確認済み（デモ）' : '未確認'}</small>
        </button>)}
        {!job.versions.length && <p className="jc-empty">本文はまだ届いていません。</p>}
      </aside>
      <section className="jc-reading">{version ? <><div className="jc-reading-title"><div><h2>{version.label}の文面</h2><p className="jc-muted">{version.source} · {date(version.observedAt)} JST</p></div><button className="jc-button" onClick={() => { onReview(version.id); }}>{reviewed.includes(version.id) ? '未確認に戻す' : '確認済みにする'}</button></div>
        <p className="jc-notice">{version.note}</p>
        {version.observedPublicationStatus !== undefined && <p>媒体CSVの公開状態: {version.observedPublicationStatus || '未取得'}</p>}
        <ImageGallery title="この版の掲載画像" images={versionImages(version)} />
        <div className="jc-full-copy-heading"><h3>{version.source === 'HubSpot shigotonaiyou' ? '仕事内容（HubSpotの現在値）' : '求人票の本文（全文）'}</h3><span>読み取り専用 · 原文の段落・改行を保持</span></div><pre className="jc-body">{version.body}</pre>
        {job.hubspotUrl && <a href={job.hubspotUrl} target="_blank" rel="noreferrer">HubSpotで求人レコードを開く</a>}
        {version.kind === 'received' ? <p className="jc-notice">応募情報は未取得です。0件とは判定していません。</p> : <ApplicationSummary version={version} live={job.dataSource === 'hubspot'} />}
        <button className="jc-text-button" onClick={() => { const index = job.versions.findIndex(item => item.id === version.id); setBefore(job.versions[index - 1]?.id ?? version.id); setAfter(version.id); setTab('diff'); }}>この版を前の版と比較する →</button>
      </> : <div className="jc-empty"><h2>本文未取得</h2><p>欠損を「変更なし」や「削除」と判断しません。</p><button className="jc-button" onClick={() => { setTab('receive'); }}>外部文面を確認する</button></div>}</section>
    </div>}
    {tab === 'diff' && <section className="jc-comparison"><div className="jc-compare-controls"><label>比較元<select value={before} onChange={event => { setBefore(event.target.value); }}><option value="">本文なし</option>{job.versions.map(item => <option key={item.id} value={item.id}>{item.label} · {item.kind === 'ai_draft' ? 'AI案' : item.source}</option>)}</select></label><span aria-hidden="true">→</span><label>比較先<select value={after} onChange={event => { setAfter(event.target.value); }}><option value="">本文なし</option>{job.versions.map(item => <option key={item.id} value={item.id}>{item.label} · {item.kind === 'ai_draft' ? 'AI案' : item.source}</option>)}</select></label></div>
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
    </section>}
    {tab === 'receive' && <section className="jc-receive"><h2>届いた文面を、現在の本文と照合する</h2><p>外部で作成された文面をここへ渡します。比較する本文の入力で、求人本文の直接編集ではありません。</p>
      <div className="jc-import-options"><label className="jc-file">UTF-8テキストを読み込む<input type="file" accept=".txt,text/plain" disabled={reading} onChange={event => { void readFile(event); }} /></label><span>{reading ? '読み込み中…' : '200KBまで / CSV・HubSpot取り込みは未接続'}</span></div>
      {readError && <p className="jc-error" role="alert">{readError}</p>}
      <label>受信元・資料名<input value={source} maxLength={200} onChange={event => { setSource(event.target.value); }} /></label>
      <label>受け取った文面<textarea value={incoming} maxLength={100_000} rows={12} placeholder="求人票の外部文面を貼り付けてください" onChange={event => { setIncoming(event.target.value); setCompared(false); }} /></label>
      <div className="jc-receive-actions"><button className="jc-button jc-primary" disabled={reading} onClick={() => { setCompared(true); }}>現在の本文と比較</button><button className="jc-button" onClick={() => { setIncoming(current?.body ?? ''); setSource('現在版の再取得デモ'); setCompared(false); }}>同じ文面で試す</button></div>
      {compared && <div className="jc-receive-result" role="status"><h3>{statusLabels[incomingResult.status]}</h3><p>掲載更新の確認: 未確認。判定は本文比較の結果です。</p>{incomingResult.status !== 'unavailable' && <button className="jc-button" onClick={addObservation}>{incomingResult.status === 'unchanged' ? '変更なしをデモ確認' : '受信版をデモ履歴に追加'}</button>}</div>}
      <p className="jc-notice">将来の文字起こし → AI文面案の生成は、タイムラインの「AI案」として接続予定です。このMOCはLLMを呼び出しません。</p>
    </section>}
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
  useEffect(() => {
    if (!snapshotRequested) return;
    const controller = new AbortController();
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
      setSnapshotLoading(false);
    });
    return () => { controller.abort(); };
  }, [snapshotRequested, initialId]);
  const normalizedSearch = search.trim().toLocaleLowerCase('ja-JP');
  const visible = orderJobs(records.filter(job => (!normalizedSearch || `${job.title} ${job.company} ${job.mediaJobId} ${job.location}`.toLocaleLowerCase('ja-JP').includes(normalizedSearch)) && (customer === 'all' || (job.id.startsWith('demo-job-') || job.dataSource === 'hubspot' ? job.company : 'unlinked') === customer) && (media === 'all' || job.media === media) && (status === 'all' || changeStatus(job) === status)), listOrder);
  const selected = visible.find(job => job.id === selectedId) ?? visible[0];
  function choose(job: JobCopyRecord) { setSelectedId(job.id); const url = new URL(window.location.href); url.searchParams.set('job', job.id); window.history.replaceState(null, '', url); if (window.matchMedia('(max-width: 800px)').matches) window.requestAnimationFrame(() => { const detail = document.getElementById('job-details'); detail?.focus({ preventScroll: true }); detail?.scrollIntoView({ block: 'start' }); }); }
  return <div className="jc-app"><header className="jc-page-heading"><h1>求人文面管理</h1><span className="jc-mode">MOC</span></header>
    <div className="jc-demo"><strong>{snapshotLoading ? '実データを読み込み中' : snapshotAt ? '実データMOC（取得済み）' : snapshotRequested && !records.length ? '実データ未表示' : live ? 'HubSpot読み取り' : captured ? '媒体取得版' : '操作デモ'}</strong><span>{snapshotAt ? `媒体CSVの本文・画像とHubSpotの実求人・応募集計です。応募集計取得：${date(snapshotAt)}。最新値の自動更新ではありません。確認記録は画面内のみ保持します。` : live ? records.some(job => published(job).length > 0) ? '実取引先・求人に媒体の本文・画像観測と応募を接続しています。画像の取得時点・欠測・版対応不明は各表示を確認してください。検証記録は画面内だけに保持します。' : '実レコードの現在値です。媒体全文・画像・日次版との接続は別途必要です。確認状況・受信版は画面内だけに保持します。' : captured ? 'HRハッカーの本文・画像です。過去版の有無と画像の取得時点は各版の注記を確認してください。応募未取得・HubSpot未保存です。' : snapshotRequested && !records.length ? '取得済みの実データを読み取ります。欠損を架空データで補いません。' : '求人・本文・応募数はすべて架空です。HubSpot未接続。追加した履歴・確認状況は再読み込みで消えます。'}</span></div>
    {snapshotLoading && <p className="jc-notice" role="status">求人本文・画像・応募集計を読み込んでいます…</p>}
    {snapshotError && <SnapshotErrorNotice guidance={snapshotError} />}
    {snapshotAt && <section className="jc-snapshot-summary" aria-label="実データの取得範囲"><span><strong>{new Set(records.map(job => job.company)).size}</strong>取引先</span><span><strong>{records.length}</strong>求人</span><span><strong>{records.reduce((sum, job) => sum + published(job).length, 0)}</strong>本文観測</span><span><strong>{records.reduce((sum, job) => sum + (job.overallApplications?.total ?? 0), 0)}</strong>応募レコード</span><span>版対応不明 <strong>{records.reduce((sum, job) => sum + (job.attributionUnknown ?? 0), 0)}</strong>件</span></section>}
    <HubSpotReadPanel key={panelEpoch} onOpen={job => { setSnapshotAt(''); setSnapshotError(null); setRecords([job]); setSelectedId(job.id); setReviewed([]); setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setCaptured(false); setLive(true); }} />
    <MediaCaptureImport onImport={items => { setSnapshotAt(''); setSnapshotError(null); setRecords(items); setSelectedId(items[0]?.id ?? ''); setReviewed([]); setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); setCaptured(true); setLive(false); setPanelEpoch(value => value + 1); }} />
    <div className="jc-workspace"><aside className="jc-list"><div className="jc-list-heading"><h2>求人レコード</h2><span>{visible.length} / {records.length}件</span></div><label>求人・企業・媒体IDを検索<input type="search" value={search} placeholder="求人名、企業名、勤務地" onChange={event => { setSearch(event.target.value); }} /></label>
      <label>取引先<select aria-label="取引先" value={customer} onChange={event => { setCustomer(event.target.value); }}><option value="all">すべての取引先</option>{captured ? <option value="unlinked">取引先未紐付け</option> : [...new Set(records.map(job => job.company))].map(value => <option key={value} value={value}>{value}{live ? '' : '（架空）'}</option>)}</select></label>
      <div className="jc-filters"><label>媒体<select value={media} onChange={event => { setMedia(event.target.value); }}><option value="all">すべて</option>{[...new Set(records.map(job => job.media))].map(value => <option key={value}>{value}</option>)}</select></label><label>変更判定<select value={status} onChange={event => { setStatus(event.target.value); }}><option value="all">すべて</option>{Object.entries(statusLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label></div>
      <label>並び順<select aria-label="並び順" value={listOrder} onChange={event => { setListOrder(event.target.value === 'applications' ? 'applications' : 'source'); }}><option value="source">取得順</option><option value="applications">応募数が多い順</option></select></label>
      <div className="jc-list-scroll">{visible.map(job => <button className="jc-job" key={job.id} aria-pressed={selected?.id === job.id} onClick={() => { choose(job); }}><span className="jc-job-company">{job.company}</span><strong>{job.title}</strong><span>{job.location} · {job.media}</span><span className="jc-job-bottom"><small>{published(job).length}版{job.versions.some(version => version.kind === 'ai_draft') ? ' + AI案' : ''}</small><small>{statusLabels[changeStatus(job)]}</small></span><small>{applicationCountLabel(job)}</small></button>)}{!visible.length && <div className="jc-empty"><p>一致する求人はありません。</p><button className="jc-button" onClick={() => { setSearch(''); setMedia('all'); setCustomer('all'); setStatus('all'); }}>絞り込みを解除</button></div>}</div>
      <p className="jc-list-footer">本文の観測と掲載確認を分けて管理<br />求人を選ぶと文面と履歴が開きます</p>
    </aside>{selected ? <CopyDetail key={`${selected.id}-${selected.versions[0]?.id ?? ''}`} job={selected} reviewed={reviewed} onReview={id => { setReviewed(items => items.includes(id) ? items.filter(item => item !== id) : [...items, id]); }} onAdd={version => { setRecords(items => items.map(job => job.id === selected.id ? { ...job, versions: [...job.versions, version] } : job)); }} /> : <main className="jc-detail jc-empty"><h1>求人を選択してください</h1><p>検索条件を変更すると候補が表示されます。</p></main>}</div>
  </div>;
}
