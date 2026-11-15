import { useCallback, useEffect, useId, useRef, useState } from 'react';
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

/**
 * 左の「求人を探す」。押したときだけ開く（マウスが通るだけでは開かない）。開いたら最初の入力欄（検索）へ移る。
 * 「開いたままにする」を押すと、閉じずに左に置いておける。
 */
export function JobCopySidebar({ pinned, expanded, setRoot, setTrigger, panelId, reveal, dismiss, deferClose, togglePin, title, onOpen, children }: ReturnType<typeof useJobCopySidebar> & { title: string; onOpen?: () => void; children: ReactNode }) {
  const panel = useRef<HTMLElement>(null);
  // 開いたあと、検索欄へフォーカスを移すまで待っている間は true。
  const wantsFocus = useRef(false);
  // 開いた一覧の検索欄（無ければ最初の入力欄）へ移る。見つかって移れたら待つのをやめる。
  // 欄がまだ描かれていなければ次の描画で、開く動き（visibility の切り替え）の途中で移れなければ動きのあとに探し直す。
  const focusSearch = useCallback(() => {
    if (!wantsFocus.current) return;
    const fields = [...(panel.current?.querySelectorAll<HTMLInputElement>('#jc-listing-q, .jc-list-search input, input:not([type="hidden"])') ?? [])];
    const field = fields.find(element => !element.disabled && !element.closest('[hidden]'));
    if (!field) return;
    field.focus({ preventScroll: true });
    if (document.activeElement === field) wantsFocus.current = false;
    else window.setTimeout(() => { if (wantsFocus.current) { field.focus({ preventScroll: true }); wantsFocus.current = false; } }, 200);
  }, []);
  useEffect(() => {
    if (!expanded) { wantsFocus.current = false; return; }
    requestAnimationFrame(focusSearch);
  });
  function open() {
    reveal(); onOpen?.();
    wantsFocus.current = true;
  }
  return <div className="jc-sidebar" ref={setRoot}
    onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) deferClose(); }}>
    <div className="jc-sidebar-rail"><button type="button" ref={setTrigger} aria-label="左の一覧で求人を探す" title={title} aria-expanded={expanded} aria-controls={panelId} onClick={() => { if (expanded && !pinned) dismiss(true); else open(); }}><svg aria-hidden="true" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="8.5" cy="8.5" r="5.5" /><path d="M12.6 12.6 17 17" strokeLinecap="round" /></svg><small>求人を<br />探す</small></button></div>
    <aside className="jc-list" id={panelId} ref={panel} aria-label="求人一覧と絞り込み" inert={!expanded} aria-hidden={!expanded}>
      <div className="jc-sidebar-actions"><strong>求人を探す</strong><button type="button" className="jc-button jc-sidebar-pin" aria-label="開いたままにする" aria-pressed={pinned} onClick={togglePin}>{pinned ? '✓ 開いたままにする' : '開いたままにする'}</button></div>
      {children}
    </aside>
  </div>;
}
