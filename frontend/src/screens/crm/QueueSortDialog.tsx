import { useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
export function QueueSortDialog({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => { const d = ref.current; d?.showModal(); return () => { d?.close(); }; }, []);
  return <dialog ref={ref} className="cq-sort-dialog" aria-label="ほかの項目で並べ替え" onCancel={e => { e.preventDefault(); onClose(); }}>
    <div className="cq-drawer-heading"><h2>並べ替え</h2><button type="button" className="cq-btn" onClick={onClose}>閉じる</button></div>{children}
  </dialog>;
}
