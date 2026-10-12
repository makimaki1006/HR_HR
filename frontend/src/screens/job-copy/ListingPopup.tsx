import { useId, useLayoutEffect, useRef, useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { formatDateTimeJst } from './format';
import { ImageGallery } from './ImageGallery';
import { listingComparisonRows, listingPopupModel } from './listingPopupModel';
import { markInlineChanges } from './diff';
import './listing-popup.css';

export interface ListingNavigation {
  ids: string[];
  load: (id: string, signal: AbortSignal) => Promise<JobCopyRecord>;
}
function ChangedText({ value, reference, different }: { value: string; reference: string; different: boolean }) {
  const segments = different ? markInlineChanges([{ kind: 'removed', text: reference }, { kind: 'added', text: value }]).find(line => line.kind === 'added')?.segments : undefined;
  return <p className={value === '記載なし' ? 'jc-listing-absent' : undefined}>{segments ? segments.map((segment, index) => segment.changed ? <mark key={index} className="jc-mark-added">{segment.text}</mark> : segment.text) : value}</p>;
}
export function ListingPopup({ job, version, onClose, returnFocus, navigation }: {
  job: JobCopyRecord; version?: CopyVersion | undefined; onClose: () => void; returnFocus?: HTMLElement | null; navigation?: ListingNavigation | undefined;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const request = useRef<AbortController | null>(null);
  const [switched, setSwitched] = useState<JobCopyRecord | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [changesOnly, setChangesOnly] = useState(false);
  const displayed = switched ?? job;
  const selectedVersion = switched ? switched.versions.filter(item => item.kind === 'published').at(-1) : version;
  const versions = displayed.versions.filter(item => item.kind === 'published');
  const previous = versions[versions.findIndex(item => item.id === selectedVersion?.id) - 1];
  const headingId = useId();
  const model = listingPopupModel(displayed, selectedVersion);
  const old = previous ? listingPopupModel(displayed, previous) : undefined;
  const rows = listingComparisonRows(model, old);
  const imagesChanged = Boolean(old && model.imageSignature !== undefined && old.imageSignature !== undefined && model.imageSignature !== old.imageSignature);
  const changedCount = rows.filter(row => row.different).length + Number(imagesChanged);
  const index = navigation?.ids.indexOf(displayed.id) ?? -1;
  const navigate = async (direction: number) => {
    const id = navigation?.ids[index + direction];
    if (!navigation || index < 0 || !id || busy) return;
    request.current?.abort();
    const controller = new AbortController(); request.current = controller;
    setBusy(true); setError('');
    try {
      const next = await navigation.load(id, controller.signal);
      if (controller.signal.aborted) return;
      setSwitched(next); setChangesOnly(false);
      if (scroller.current) scroller.current.scrollTop = 0;
    } catch {
      if (!controller.signal.aborted) setError('切り替え先の求人を取得できませんでした。もう一度切り替えてください。');
    } finally { if (!controller.signal.aborted) setBusy(false); }
  };
  useLayoutEffect(() => {
    const element = dialog.current;
    const opener = returnFocus ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    element?.showModal();
    element?.focus({ preventScroll: true });
    return () => {
      request.current?.abort(); element?.close();
      document.body.style.overflow = overflow;
      if (opener?.isConnected) opener.focus({ preventScroll: true });
    };
  }, [returnFocus]);
  useLayoutEffect(() => {
    dialog.current?.focus({ preventScroll: true });
    if (scroller.current) scroller.current.scrollTop = 0;
  }, [displayed.id]);
  const images = (item: typeof model, recorded?: CopyVersion) => <section className="jc-listing-popup-section jc-listing-image-cell">
    <h3>{imagesChanged && <span className="jc-listing-change-dot" aria-label="変更あり">● </span>}画像</h3>
    {item.images?.length ? <><ImageGallery failureMessage="画像はありますが、取得できていません" title="掲載画像" images={item.images} />{recorded?.historicalImageBytesAvailable === false && <p className="jc-listing-popup-note">画像は当時と異なる可能性があります。</p>}</> : item.observedImages ? <><p className="jc-listing-popup-note">現在取得できる画像（確認日時：{formatDateTimeJst(item.observedImages.observedAt, '未取得')}）。本文の記録時点の画像とは限りません。</p><ImageGallery failureMessage="画像はありますが、取得できていません" title="掲載画像" images={item.observedImages.images} /></> : <p>{item.imageReason}</p>}
  </section>;
  const missing = (item: typeof model) => <details className="jc-listing-popup-missing"><summary>記載のない任意項目（{item.missing.length}件）</summary><ul>{item.missing.map((name, i) => <li key={i}>{name}</li>)}</ul></details>;
  return <dialog ref={dialog} className="jc-listing-popup" role="dialog" aria-modal="true" aria-labelledby={headingId} tabIndex={-1}
    onKeyDown={event => {
      if (!(event.target instanceof HTMLElement) || event.target.closest('textarea, select, [contenteditable]:not([contenteditable="false"])') || event.target.matches('input:not([type="checkbox"]):not([type="radio"]):not([type="button"])') || event.target.closest('dialog') !== dialog.current) return;
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') { event.preventDefault(); void navigate(event.key === 'ArrowLeft' ? -1 : 1); }
      if (event.key === 'ArrowUp' || event.key === 'ArrowDown') { event.preventDefault(); scroller.current?.scrollBy({ top: event.key === 'ArrowUp' ? -100 : 100 }); }
    }}
    onCancel={event => { event.preventDefault(); onClose(); }}
    onClick={event => {
      if (event.target !== event.currentTarget) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) onClose();
    }}>
    <header className="jc-listing-popup-heading"><div><span>求人の内容</span><h2 id={headingId}>{model.title}</h2><nav aria-label="求人の切り替え"><button className="jc-button" disabled={busy || index <= 0} onClick={() => { void navigate(-1); }}>‹ 前の求人</button><span>{index >= 0 ? `${String(index + 1)} / ${String(navigation?.ids.length)}件目` : '一覧の範囲外'}</span><button className="jc-button" disabled={busy || index < 0 || index >= (navigation?.ids.length ?? 0) - 1} onClick={() => { void navigate(1); }}>次の求人 ›</button></nav></div><button type="button" className="jc-button jc-listing-close" aria-label="求人の内容を閉じる" onClick={onClose}>×</button></header>
    <div ref={scroller} className="jc-listing-popup-scroll" aria-busy={busy}>
      {busy && <p role="status">求人の内容を読み込んでいます…</p>}{error && <p role="alert">{error}</p>}
      <dl className="jc-listing-popup-summary">{[['求人名', model.title], ['媒体', displayed.media], ['公開状況', model.publication], ['勤務地', model.location], ['給与', model.salary], ['応募数', model.applications]].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
      <p className="jc-listing-popup-note">本文が記録された日時です。掲載開始日時とは異なります。{selectedVersion?.source === 'HubSpotの現在の本文' && ' 現在の本文は確認した日時です。本文の記録日時は不明です。'}{model.hasCurrentConditions && ' 左の本文以外の補足条件・公開状況は現在の取得値です。過去の本文に補っていません。'} 応募数は求人全体の取得値で、この本文への応募数ではありません。</p>
      {selectedVersion?.kind === 'ai_draft' && <p className="jc-notice">未掲載の案です。</p>}
      {!model.bodyAvailable && <p className="jc-notice">本文を取得できていません。</p>}
      {old && <label className="jc-listing-changes"><input type="checkbox" checked={changesOnly} onChange={event => { setChangesOnly(event.target.checked); }} />変わった項目だけ表示（{changedCount}項目）</label>}
      <div className={`jc-listing-comparison ${old ? 'jc-listing-two-columns' : ''}`}>
        <div className="jc-listing-column-heading"><h3>選んでいる本文</h3><p>記録日時：{formatDateTimeJst(selectedVersion?.observedAt, '未取得')}</p></div>
        {old && <div className="jc-listing-column-heading"><h3>1つ前に記録された本文</h3><p>記録日時：{formatDateTimeJst(previous?.observedAt, '未取得')}</p></div>}
        {rows.filter(row => !changesOnly || row.different).map(row => <div className="jc-listing-comparison-row" key={row.key} data-changed={row.different}><section className="jc-listing-popup-section"><h3>{row.different && <span className="jc-listing-change-dot" aria-label="変更あり">● </span>}{row.label}</h3><ChangedText value={row.value} reference={row.before} different={row.different} /></section>{old && <section className="jc-listing-popup-section"><h3>{row.different && <span className="jc-listing-change-dot" aria-label="変更あり">● </span>}{row.label}</h3><ChangedText value={row.before} reference={row.value} different={row.different} /></section>}</div>)}
        {changesOnly && changedCount === 0 && <p>確認できる変更はありません。</p>}
        {(!changesOnly || imagesChanged) && <>{images(model, selectedVersion)}{old && images(old, previous)}</>}
        {!changesOnly && <><div key={`${displayed.id}-missing`}>{missing(model)}</div>{old && <div key={`${displayed.id}-previous-missing`}>{missing(old)}</div>}</>}
      </div>
    </div>
  </dialog>;
}
