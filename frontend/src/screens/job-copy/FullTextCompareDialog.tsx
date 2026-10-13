import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { formatDateTimeJst } from './format';
import { ImageGallery } from './ImageGallery';
import { compareImages, imagesByVersion, referenceImages } from './images';
import { comparableRows, compareRows, EMPTY_TEXT, isAirwork, markPair, NOT_RECORDED_TEXT } from './fullTextCompareModel';
import type { CompareRow } from './fullTextCompareModel';
import type { InlineSegment } from './diff';
import './full-text-compare.css';

const versionImages = (version: CopyVersion | undefined) => version ? version.images ?? imagesByVersion[version.id] : undefined;
const day = (value: string | undefined) => value ? formatDateTimeJst(value, '日時不明') : '日時不明';

function Text({ value, segments, side }: { value: string | null; segments: InlineSegment[] | undefined; side: 'left' | 'right' }) {
  if (value === null) return <p className="jc-ftc-muted">{NOT_RECORDED_TEXT}</p>;
  if (!value) return <p className="jc-ftc-muted">{EMPTY_TEXT}</p>;
  if (!segments) return <p>{value}</p>;
  return <p>{segments.map((segment, index) => !segment.changed ? segment.text : side === 'left' ? <del key={index} className="jc-mark-removed">{segment.text}</del> : <ins key={index} className="jc-mark-added">{segment.text}</ins>)}</p>;
}

function Picker({ label, versions, value, onChange }: { label: string; versions: CopyVersion[]; value: string; onChange: (id: string) => void }) {
  const index = versions.findIndex(item => item.id === value);
  const note = index < 0 ? '' : index === 0 ? '掲載開始時の内容' : `${day(versions[index - 1]?.observedAt)}〜${day(versions[index]?.observedAt)} の間に変わった内容`;
  return <div className="jc-ftc-picker"><strong>{label}</strong>
    <div role="group" aria-label={label}>{versions.map(item => <button key={item.id} type="button" className="jc-ftc-pill" aria-pressed={item.id === value} onClick={() => { onChange(item.id); }}>{item.label}<small>{day(item.observedAt)}</small></button>)}</div>
    <p className="jc-ftc-range">{note}</p></div>;
}

export function FullTextCompareDialog({ record, open, onClose, initialLeft, initialRight }: { record: JobCopyRecord; open: boolean; onClose: () => void; initialLeft?: string | undefined; initialRight?: string | undefined }) {
  if (!open) return null;
  return <Inner record={record} onClose={onClose} initialLeft={initialLeft} initialRight={initialRight} />;
}

function Inner({ record, onClose, initialLeft, initialRight }: { record: JobCopyRecord; onClose: () => void; initialLeft: string | undefined; initialRight: string | undefined }) {
  const versions = record.versions;
  const dialog = useRef<HTMLDialogElement>(null);
  const headingId = useId();
  const [mode, setMode] = useState<'diff' | 'all'>('diff');
  const [leftId, setLeftId] = useState(() => initialLeft ?? versions.at(-2)?.id ?? versions.at(-1)?.id ?? '');
  const [rightId, setRightId] = useState(() => initialRight ?? versions.at(-1)?.id ?? '');
  const left = versions.find(item => item.id === leftId); const right = versions.find(item => item.id === rightId);
  useEffect(() => {
    const element = dialog.current;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    element?.showModal();
    element?.focus({ preventScroll: true });
    return () => { element?.close(); document.body.style.overflow = overflow; if (opener?.isConnected) opener.focus({ preventScroll: true }); };
  }, []);
  const rows = useMemo(() => compareRows(record, left, right), [record, left, right]);
  const comparable = comparableRows(rows);
  const imageResult = compareImages(referenceImages(left), referenceImages(right));
  const imageKnown = imageResult.status !== 'unknown';
  const imageChanged = imageKnown && imageResult.status !== 'same_reference';
  const changedCount = rows.filter(row => row.different).length + Number(imageChanged);
  const totalCount = comparable.length + Number(imageKnown);
  const shown = mode === 'diff' ? rows.filter(row => row.different) : rows;
  const pairs = (row: CompareRow) => row.different && row.left !== null && row.right !== null ? markPair(row.left, row.right) : undefined;
  return <dialog ref={dialog} className="jc-ftc" role="dialog" aria-modal="true" aria-labelledby={headingId} tabIndex={-1}
    onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); onClose(); } }}
    onCancel={event => { event.preventDefault(); onClose(); }}>
    <header className="jc-ftc-head"><div><h2 id={headingId}>求人の全文を比べる</h2><p>{record.title}（{record.media}）</p></div>
      <div className="jc-ftc-toggle" role="group" aria-label="表示する項目"><button type="button" className="jc-button" aria-pressed={mode === 'diff'} onClick={() => { setMode('diff'); }}>違う項目だけ</button><button type="button" className="jc-button" aria-pressed={mode === 'all'} onClick={() => { setMode('all'); }}>全文</button></div>
      <button type="button" className="jc-button" onClick={onClose}>閉じる</button></header>
    <div className="jc-ftc-scroll">
      {versions.length === 0 ? <p className="jc-notice">比べられる本文がまだありません。</p> : <>
        <div className="jc-ftc-grid jc-ftc-pickers"><strong>比べる版</strong><Picker label="左（前）" versions={versions} value={leftId} onChange={setLeftId} /><Picker label="右（後）" versions={versions} value={rightId} onChange={setRightId} /></div>
        <p className="jc-ftc-summary" role="status">全 {totalCount} 項目のうち {changedCount} 項目が違います。</p>
        <div className="jc-ftc-rows">
          {shown.map(row => { const marked = pairs(row); return <div key={row.key} className="jc-ftc-grid jc-ftc-row" data-changed={row.different}>
            <div className="jc-ftc-label">{row.label}{row.different && <span className="jc-ftc-tag">変わった</span>}</div>
            <Text value={row.left} segments={marked?.[0]} side="left" /><Text value={row.right} segments={marked?.[1]} side="right" /></div>; })}
          {mode === 'diff' && changedCount === 0 && <p className="jc-ftc-muted">違う項目はありません。</p>}
          {(mode === 'all' || imageChanged) && <div className="jc-ftc-grid jc-ftc-row" data-changed={imageChanged}>
            <div className="jc-ftc-label">画像{imageChanged && <span className="jc-ftc-tag">変わった</span>}</div>
            <div><ImageGallery title="左の画像" images={versionImages(left)} marks={imageResult.removed.map(image => image.url)} /></div>
            <div><ImageGallery title="右の画像" images={versionImages(right)} marks={imageResult.added.map(image => image.url)} /></div></div>}
        </div>
        {isAirwork(record) && <p className="jc-ftc-note">この媒体の履歴には、求人名・仕事内容・勤務時間・給与の形態・職種・画像だけが残ります。ほかの項目は「{NOT_RECORDED_TEXT}」と表示します。</p>}
      </>}
      <p className="jc-ftc-note">変わった日は「前に記録した日」から「この日」までのどこかです。記録していない項目は比べられません。</p>
    </div>
  </dialog>;
}
