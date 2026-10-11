import { useId, useLayoutEffect, useRef } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { formatDateTimeJst } from './format';
import { ImageGallery } from './ImageGallery';
import { listingPopupModel } from './listingPopupModel';
import './listing-popup.css';

export function ListingPopup({ job, version, onClose, returnFocus }: {
  job: JobCopyRecord; version?: CopyVersion | undefined; onClose: () => void; returnFocus?: HTMLElement | null;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const headingId = useId();
  const model = listingPopupModel(job, version);
  useLayoutEffect(() => {
    const element = dialog.current;
    const opener = returnFocus ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    element?.showModal();
    element?.focus({ preventScroll: true });
    return () => {
      element?.close();
      document.body.style.overflow = overflow;
      if (opener?.isConnected) opener.focus({ preventScroll: true });
    };
  }, [returnFocus]);
  return <dialog ref={dialog} className="jc-listing-popup" role="dialog" aria-modal="true" aria-labelledby={headingId} tabIndex={-1}
    onCancel={event => { event.preventDefault(); onClose(); }}
    onClick={event => {
      if (event.target !== event.currentTarget) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) onClose();
    }}>
    <header className="jc-listing-popup-heading"><div><span>求人の内容</span><h2 id={headingId}>{model.title}</h2><p>記録日時：{formatDateTimeJst(version?.observedAt, '未取得')}</p></div><button type="button" className="jc-button" aria-label="求人の内容を閉じる" onClick={onClose}>×</button></header>
    <div className="jc-listing-popup-scroll">
      <dl className="jc-listing-popup-summary">{[['求人名', model.title], ['媒体', job.media], ['公開状況', model.publication], ['勤務地', model.location], ['給与', model.salary], ['応募数', model.applications]].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
      <p className="jc-listing-popup-note">{version?.source === 'HubSpotの現在の本文' ? '日時は現在の本文を確認した日時です。本文の記録日時は不明です。' : '本文が記録された日時です。掲載開始日時とは異なります。'}{model.hasCurrentConditions && ' 本文以外の補足条件・公開状況は現在の取得値です。'} 応募数は求人全体の取得値で、この本文への応募数ではありません。</p>
      {version?.kind === 'ai_draft' && <p className="jc-notice">未掲載の案です。</p>}
      {!version?.body.trim() && <p className="jc-notice">本文は未取得です。</p>}
      {model.sections.map(section => <section className="jc-listing-popup-section" key={section.heading}><h3>{section.heading}</h3>{section.entries.map(item => <div key={item.key}>{(section.entries.length > 1 || item.label !== section.heading) && <h4>{item.label}</h4>}<p>{item.value}</p></div>)}</section>)}
      {model.images !== undefined && <><ImageGallery title="画像" images={model.images} />{version?.historicalImageBytesAvailable === false && <p className="jc-listing-popup-note">画像は当時と異なる可能性があります。</p>}</>}
      {model.observedImages && <><p className="jc-listing-popup-note">現在取得できる画像（確認日時：{formatDateTimeJst(model.observedImages.observedAt, '未取得')}）。本文の記録時点の画像とは限りません。</p><ImageGallery title="画像" images={model.observedImages.images} /></>}
      <section className="jc-listing-popup-section jc-listing-popup-missing"><h3>未取得の項目（{model.missing.length}件）</h3>{model.missing.length ? <ul>{model.missing.map((name, index) => <li key={index}>{name}</li>)}</ul> : <p>未取得の項目はありません。</p>}</section>
    </div>
  </dialog>;
}
