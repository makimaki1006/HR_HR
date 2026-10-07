import { useId, useRef, useState } from 'react';
import type { CopyImage } from './images';

function RetryableImage({ image, onOpen, slot }: { image: CopyImage; onOpen?: () => void; slot?: number }) {
  const [status, setStatus] = useState<'loading' | 'loaded' | 'failed'>('loading');
  const [attempt, setAttempt] = useState(0);
  const failed = status === 'failed';
  const picture = <img key={`${image.url}-${String(attempt)}`} src={image.url} alt={image.caption} loading={onOpen ? 'lazy' : 'eager'} style={failed ? { display: 'none' } : undefined} onLoad={() => { setStatus('loaded'); }} onError={() => { setStatus('failed'); }} />;
  return <>
    {onOpen ? <button className="jc-image-open" disabled={status !== 'loaded'} aria-label={`画像${String(slot)}を拡大: ${image.caption}`} onClick={onOpen}>{picture}{status === 'loaded' && <span>画像を拡大</span>}</button> : picture}
    {status === 'loading' && <p className="jc-muted" role="status" style={{ minHeight: '3rem' }}>画像を取得中です。本文や分析はそのまま操作できます。</p>}
    {failed && <div className="jc-notice"><p role="alert">画像の取得に失敗しました。画像なし・削除とは判定していません。</p><button className="jc-button" aria-label={`画像を再読み込み: ${image.caption}`} onClick={() => { setStatus('loading'); setAttempt(value => value + 1); }}>画像を再読み込み</button></div>}
  </>;
}

export function ImageGallery({ images, title, marks = [] }: { images: CopyImage[] | undefined; title: string; marks?: string[] }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const headingId = useId();
  const [selected, setSelected] = useState<CopyImage | null>(null);
  return <section className="jc-image-section" aria-label={title}><h3>{title}</h3>
    {images === undefined ? <p className="jc-muted">画像は未取得です。画像なし・削除とは判定していません。</p> : images.length === 0 ? <p className="jc-muted">この版の画像は0点です。</p> : <div className="jc-image-grid">{images.map((image, index) => <figure key={`${image.id}-${String(index)}`} className={marks.includes(image.sourceReferenceHash ?? image.url) ? 'jc-image-marked' : ''}>
      <RetryableImage key={image.url} image={image} slot={image.sourceSlot ?? index + 1} onOpen={() => { setSelected(image); dialog.current?.showModal(); }} />
      <figcaption><strong>画像{image.sourceSlot ?? index + 1}</strong> {image.caption}{marks.includes(image.sourceReferenceHash ?? image.url) && <span>変更対象</span>}</figcaption>
    </figure>)}</div>}
    <dialog ref={dialog} className="jc-image-dialog" aria-labelledby={headingId} onClose={() => { setSelected(null); }}><div className="jc-image-dialog-heading"><h3 id={headingId}>{selected?.caption ?? '画像の拡大表示'}</h3><button className="jc-button" onClick={() => { dialog.current?.close(); }}>閉じる</button></div>{selected && <RetryableImage key={selected.url} image={selected} />}<p className="jc-muted">表示用画像の拡大です。Escキーでも閉じられます。</p></dialog>
  </section>;
}
