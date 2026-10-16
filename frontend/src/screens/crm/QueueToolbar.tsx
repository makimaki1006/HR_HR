import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';

/** 条件を変える間だけ、一覧の上に操作欄を重ねる */
export function QueueToolbar({ summary, keepOpen, onClose, children }: {
  summary: string; keepOpen: boolean; onClose: () => void; children: ReactNode;
}) {
  const [hoverOpen, setHoverOpen] = useState(false);
  const [pinned, setPinned] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hovering = useRef(false);
  const open = hoverOpen || pinned || keepOpen;
  function cancel() { if (timer.current !== null) clearTimeout(timer.current); timer.current = null; }
  function scheduleClose() {
    cancel();
    timer.current = setTimeout(() => {
      timer.current = null;
      // select の選択中も、入力欄と同じくフォーカスが残る
      if (!root.current?.querySelector('.cq-toolbar-controls')?.contains(document.activeElement)
        && !root.current?.querySelector('.cq-stage-panel')) setHoverOpen(false);
    }, 300);
  }
  function close() {
    cancel(); setHoverOpen(false); setPinned(false); onClose(); toggle.current?.focus();
  }
  useEffect(() => () => { if (timer.current !== null) clearTimeout(timer.current); }, []);
  // body 直下のダイアログにも Esc が届く。子の選択欄が使った Esc は優先する
  useEffect(() => {
    if (!open) return;
    const escape = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      if (document.activeElement !== document.body && !root.current?.contains(document.activeElement)) return;
      if (document.activeElement !== toggle.current && (keepOpen || root.current?.querySelector('.cq-toolbar-controls [aria-expanded="true"]') || document.querySelector('.dp-pop, .sv-backdrop, .cq-sort-dialog'))) return;
      e.preventDefault();
      if (timer.current !== null) clearTimeout(timer.current);
      setHoverOpen(false); setPinned(false); onClose(); toggle.current?.focus();
    };
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('keydown', escape); };
  }, [open, keepOpen, onClose]);
  return <div className="cq-toolbar" ref={root}
    onPointerEnter={e => {
      if (e.pointerType === 'touch') return;
      hovering.current = true; cancel();
      timer.current = setTimeout(() => { timer.current = null; setHoverOpen(true); }, 300);
    }}
    onPointerLeave={() => { hovering.current = false; scheduleClose(); }}
    onFocus={e => {
      if (e.target.closest('.cq-toolbar-controls')) { cancel(); setHoverOpen(true); }
    }}
    onBlur={() => { if (!hovering.current) scheduleClose(); }}>
    <button type="button" ref={toggle} className="cq-toolbar-summary" aria-label={`条件を変更: ${summary}`}
      aria-expanded={open} aria-controls="cq-toolbar-controls" aria-pressed={pinned} title={summary}
      onClick={() => { if (pinned) close(); else { cancel(); setPinned(true); } }}>
      <span>{summary}</span><span className="cq-toolbar-action">{pinned ? '閉じる' : '条件を変更'} <span aria-hidden="true">{open ? '▴' : '▾'}</span></span>
    </button>
    <div id="cq-toolbar-controls" className="cq-toolbar-controls" hidden={!open}>{children}</div>
  </div>;
}
