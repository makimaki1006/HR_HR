import { useEffect, useId, useRef, useState } from 'react';
import type { ReactNode } from 'react';

const storageKey = 'hrhr-job-copy-sidebar-pinned';
function readPinned() {
  try { return localStorage.getItem(storageKey) === 'true'; } catch { return false; }
}

export function useJobCopySidebar() {
  const [pinned, setPinned] = useState(readPinned);
  const [open, setOpen] = useState(false);
  const [desktop, setDesktop] = useState(() => window.matchMedia('(min-width: 801px)').matches);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const returningFocus = useRef(false);
  const panelId = useId();
  const expanded = !desktop || pinned || open;
  function cancelClose() {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }
  function reveal() { cancelClose(); setOpen(true); }
  function dismiss(returnToTrigger = false) {
    cancelClose();
    if (pinned || !desktop) return;
    setOpen(false);
    if (returnToTrigger) {
      returningFocus.current = true;
      trigger.current?.focus({ preventScroll: true });
      returningFocus.current = false;
    }
  }
  function selected() {
    dismiss();
    if (!pinned && desktop) requestAnimationFrame(() => {
      document.getElementById('job-details')?.focus({ preventScroll: true });
    });
  }
  function deferClose() {
    cancelClose();
    timer.current = setTimeout(() => {
      if (!root.current?.contains(document.activeElement)) setOpen(false);
    }, 250);
  }
  function togglePin() {
    cancelClose();
    const next = !pinned;
    setPinned(next); setOpen(next);
    try { localStorage.setItem(storageKey, String(next)); } catch { /* 保存できなくても画面内の操作は続けられる */ }
    if (!next) requestAnimationFrame(() => {
      returningFocus.current = true;
      trigger.current?.focus({ preventScroll: true });
      returningFocus.current = false;
    });
  }
  useEffect(() => {
    const query = window.matchMedia('(min-width: 801px)');
    const change = () => { setDesktop(query.matches); };
    query.addEventListener('change', change);
    return () => { query.removeEventListener('change', change); };
  }, []);
  useEffect(() => () => { if (timer.current !== null) clearTimeout(timer.current); }, []);
  useEffect(() => {
    if (!desktop || pinned || !open) return;
    function outside(event: PointerEvent) {
      if (event.target instanceof Node && !root.current?.contains(event.target)) {
        cancelClose(); setOpen(false);
        // 外側がフォーカスを受け取らない場所でも、隠れる一覧にフォーカスを残さない。
        if (root.current?.contains(document.activeElement)) {
          returningFocus.current = true;
          trigger.current?.focus({ preventScroll: true });
          returningFocus.current = false;
        }
      }
    }
    function escape(event: KeyboardEvent) {
      if (event.key !== 'Escape') return;
      event.preventDefault(); cancelClose(); setOpen(false);
      returningFocus.current = true;
      trigger.current?.focus({ preventScroll: true });
      returningFocus.current = false;
    }
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', outside); document.removeEventListener('keydown', escape); };
  }, [desktop, pinned, open]);
  return { pinned, expanded, setRoot: (element: HTMLDivElement | null) => { root.current = element; }, setTrigger: (element: HTMLButtonElement | null) => { trigger.current = element; }, panelId, revealOnFocus: () => { if (!returningFocus.current) reveal(); }, reveal, dismiss, selected, deferClose, togglePin };
}

export function JobCopySidebar({ pinned, expanded, setRoot, setTrigger, panelId, reveal, revealOnFocus, deferClose, togglePin, title, children }: ReturnType<typeof useJobCopySidebar> & { title: string; children: ReactNode }) {
  return <div className="jc-sidebar" ref={setRoot} onMouseEnter={reveal} onMouseLeave={deferClose}
    onFocus={revealOnFocus}
    onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) deferClose(); }}>
    <div className="jc-sidebar-rail"><button type="button" ref={setTrigger} aria-label="求人一覧を開く" aria-expanded={expanded} aria-controls={panelId} onClick={reveal}><span aria-hidden="true">☰</span><small>一覧</small></button><span className="jc-sidebar-current" title={title}>{title}</span></div>
    <aside className="jc-list" id={panelId} aria-label="求人一覧と絞り込み" inert={!expanded} aria-hidden={!expanded}>
      <div className="jc-sidebar-actions"><strong>求人を選ぶ</strong><button type="button" className="jc-button" aria-pressed={pinned} onClick={togglePin}>{pinned ? '固定を解除' : '固定'}</button></div>
      {children}
    </aside>
  </div>;
}
