import { useId, useLayoutEffect, useRef, useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { formatDateTimeJst } from './format';
import { ImageGallery } from './ImageGallery';
import { listingComparisonRows, listingPopupModel } from './listingPopupModel';
import { FullTextCompareDialog } from './FullTextCompareDialog';
import { InfoTip } from './InfoTip';
import './listing-popup.css';

export interface ListingNavigation {
  ids: string[];
  load: (id: string, signal: AbortSignal) => Promise<JobCopyRecord>;
}
/**
 * 求人の内容の小窓。今の内容を見る専用（2026-11-16）。前の本文と比べたいときは、全文比較を開く。
 */
export function ListingPopup({ job, version, onClose, returnFocus, navigation }: {
  job: JobCopyRecord; version?: CopyVersion | undefined; onClose: () => void; returnFocus?: HTMLElement | null; navigation?: ListingNavigation | undefined;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const request = useRef<AbortController | null>(null);
  const [switched, setSwitched] = useState<JobCopyRecord | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [compareOpen, setCompareOpen] = useState(false);
  const displayed = switched ?? job;
  const selectedVersion = switched ? switched.versions.filter(item => item.kind === 'published').at(-1) : version;
  const versions = displayed.versions.filter(item => item.kind === 'published');
  const previous = versions[versions.findIndex(item => item.id === selectedVersion?.id) - 1];
  const headingId = useId();
  const model = listingPopupModel(displayed, selectedVersion);
  const rows = listingComparisonRows(model, undefined);
  const index = navigation?.ids.indexOf(displayed.id) ?? -1;
  const total = navigation?.ids.length ?? 0;
  const navigate = async (direction: number) => {
    const id = navigation?.ids[index + direction];
    if (!navigation || index < 0 || !id || busy) return;
    request.current?.abort();
    const controller = new AbortController(); request.current = controller;
    setBusy(true); setError('');
    try {
      const next = await navigation.load(id, controller.signal);
      if (controller.signal.aborted) return;
      setSwitched(next); setCompareOpen(false);
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
    <h3>画像</h3>
    {item.images?.length ? <><ImageGallery failureMessage="画像はありますが、取得できていません" title="掲載画像" images={item.images} />{recorded?.historicalImageBytesAvailable === false && <p className="jc-listing-popup-note">画像は当時と異なる可能性があります。</p>}</> : item.observedImages ? <><p className="jc-listing-popup-note">現在取得できる画像（確認日時：{formatDateTimeJst(item.observedImages.observedAt, '未取得')}）。本文の記録時点の画像とは限りません。</p><ImageGallery failureMessage="画像はありますが、取得できていません" title="掲載画像" images={item.observedImages.images} /></> : <p>{item.imageReason}</p>}
  </section>;
  const missing = (item: typeof model) => !item.optionalFieldsKnown ? <p className="jc-listing-popup-note">任意項目の記載状況を取得できていません。</p> : <details className="jc-listing-popup-missing"><summary>記載のない任意項目（{item.missing.length}件）</summary><ul>{item.missing.map((name, i) => <li key={i}>{name}</li>)}</ul></details>;
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
    <header className="jc-listing-popup-heading"><div><span>求人の内容</span><h2 id={headingId}>{model.title}</h2>{index >= 0 && total > 1 && <nav aria-label="求人の切り替え">{index > 0 && <button className="jc-button" disabled={busy} onClick={() => { void navigate(-1); }}>‹ 前の求人</button>}<span>{`${String(index + 1)} / ${String(total)}件目`}</span>{index < total - 1 && <button className="jc-button" disabled={busy} onClick={() => { void navigate(1); }}>次の求人 ›</button>}</nav>}</div><button type="button" className="jc-button jc-listing-close" aria-label="求人の内容を閉じる" onClick={onClose}>閉じる</button></header>
    <div ref={scroller} className="jc-listing-popup-scroll" aria-busy={busy}>
      {busy && <p role="status">求人の内容を読み込んでいます…</p>}{error && <p role="alert">{error}</p>}
      <dl className="jc-listing-popup-summary">{[['求人名', model.title], ['媒体', displayed.media], ['公開状況', model.publication], ['勤務地', model.location], ['給与', model.salary], ['応募数', model.applications]].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
      <div className="jc-listing-popup-bar">
        <p className="jc-listing-popup-note">記録日時：{formatDateTimeJst(selectedVersion?.observedAt, '未取得')}</p>
        <InfoTip className="jc-infotip-left" iconFirst label="日時と応募数について"><p>本文が記録された日時です。掲載開始日時とは異なります。{selectedVersion?.source === 'HubSpotの現在の本文' && '現在の本文は確認した日時です。本文の記録日時は不明です。'}</p>{model.hasCurrentConditions && <p>本文以外の補足条件・公開状況は現在の取得値です。過去の本文に補っていません。</p>}<p>応募数は求人全体の取得値で、この本文への応募数ではありません。</p></InfoTip>
        {previous && selectedVersion && <button type="button" className="jc-button" onClick={() => { setCompareOpen(true); }}>前の本文と全文を比べる</button>}
      </div>
      {selectedVersion?.kind === 'ai_draft' && <p className="jc-notice">未掲載の案です。</p>}
      {!model.bodyAvailable && <p className="jc-notice">本文を取得できていません。</p>}
      <div className="jc-listing-comparison">
        {rows.map(row => <div className="jc-listing-comparison-row" key={row.key}><section className="jc-listing-popup-section"><h3>{row.label}</h3><p className={row.value === '記載なし' ? 'jc-listing-absent' : undefined}>{row.value}</p></section></div>)}
        {images(model, selectedVersion)}
        <div key={`${displayed.id}-missing`}>{missing(model)}</div>
      </div>
    </div>
    <FullTextCompareDialog record={displayed} open={compareOpen} onClose={() => { setCompareOpen(false); }} initialLeft={previous?.id} initialRight={selectedVersion?.id} />
  </dialog>;
}
