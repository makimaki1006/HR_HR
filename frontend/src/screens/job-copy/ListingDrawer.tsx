import { useEffect, useId, useRef } from 'react';
import type { ReactNode } from 'react';
import './listing-overview.css';

/** 右から出る詳細。閉じると元のスクロール位置・押したボタンに戻る（ページは移動しない）。 */
export function ListingDrawer({ title, onClose, wide = false, children }: { title: string; onClose: () => void; wide?: boolean; children: ReactNode }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const headingId = useId();
  useEffect(() => {
    const element = dialog.current;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    if (element && typeof element.showModal === 'function' && !element.open) element.showModal();
    else element?.setAttribute('open', '');
    element?.focus({ preventScroll: true });
    return () => {
      if (element?.open && typeof element.close === 'function') element.close();
      document.body.style.overflow = overflow;
      if (opener?.isConnected) opener.focus({ preventScroll: true });
    };
  }, []);
  return <dialog ref={dialog} className={`lo-drawer${wide ? ' lo-wide' : ''}`} aria-modal="true" aria-labelledby={headingId} tabIndex={-1}
    onCancel={event => { event.preventDefault(); onClose(); }}
    onClick={event => { if (event.target === dialog.current) onClose(); }}>
    <div className="lo-drawer-head"><h2 id={headingId}>{title}</h2><button type="button" className="jc-button" onClick={onClose}>閉じる</button></div>
    <div className="lo-drawer-body">{children}</div>
  </dialog>;
}
