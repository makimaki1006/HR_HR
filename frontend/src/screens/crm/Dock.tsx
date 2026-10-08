import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { CSSProperties, DragEvent, KeyboardEvent, PointerEvent, ReactNode } from 'react';
import { createPortal } from 'react-dom';
import {
  COLUMN_INDEXES, COLUMN_LABELS, KEY_RESIZE_STEP, PANEL_IDS, PANEL_LABELS, PINNED_PANEL, columnOf, isEmptyColumn, leftPercent, tabPanels,
} from './dockModel';
import type { ColumnIndex, DockAction, DockLayout, PanelId } from './dockModel';
import './dock.css';

/**
 * 架電画面のパネルの置き場 (3 列)。列ごとにパネルをタブで切り替える。
 *
 * - パネルの中身は 1 回だけ描き、パネル用の箱 (DOM) に入れて、置いた列の枠へ箱ごと差し込む
 *   (React の部品は作り直さないので、入力途中の内容・選んだタブなどの状態は移しても残る。
 *   枠の中のページ (iframe) は箱を移すとブラウザが読み直す)
 * - タブはドラッグで別の列へ移せる。キーボードでは各タブの「移動」から「左へ移動 / 中央へ移動 / 右へ移動」
 * - 列の間の区切りはドラッグか矢印キーで幅を変える
 * - 配置を変えても HubSpot は呼ばない (画面の中だけの状態)
 */

const DRAG_TYPE = 'application/x-hrhr-crm-panel';
/** 列の最小幅 (px)。比の最小 (dockModel.MIN_WIDTH_RATIO) と大きい方が効く */
export const MIN_COLUMN_PX = 220;

export const dockTabId = (p: PanelId) => `dock-tab-${p}`;
export const dockPanelId = (p: PanelId) => `dock-panel-${p}`;
const menuButtonId = (p: PanelId) => `dock-move-${p}`;

/** パネルの箱を、置いた列の枠へ差し込む */
function Slot({ host }: { host: HTMLElement }) {
  const ref = useRef<HTMLDivElement | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el === null) return;
    el.appendChild(host);
    return () => { if (host.parentNode === el) el.removeChild(host); };
  }, [host]);
  return <div className="dock-slot" ref={ref} />;
}

/** 描き直しの後に、移したパネルのタブ (固定のパネルは「移動」のボタン) へフォーカスを移す */
function focusPanelControl(panel: PanelId) {
  requestAnimationFrame(() => {
    const id = panel === PINNED_PANEL ? menuButtonId(panel) : dockTabId(panel);
    document.getElementById(id)?.focus();
  });
}

/** パネルの「移動」メニュー (左へ移動 / 中央へ移動 / 右へ移動 / この列で前へ・後ろへ) */
function MoveMenu({ panel, layout, dispatch }: { panel: PanelId; layout: DockLayout; dispatch: (a: DockAction) => void }) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLSpanElement | null>(null);
  const col = columnOf(layout, panel);
  const order = layout.columns[col].panels.filter(p => p !== PINNED_PANEL);
  const pos = order.indexOf(panel);
  interface MenuItem { key: string; label: string; act: DockAction }
  const items: MenuItem[] = COLUMN_INDEXES.filter(i => i !== col)
    .map((i): MenuItem => ({ key: `to-${String(i)}`, label: `${COLUMN_LABELS[i]}へ移動`, act: { type: 'move', panel, to: i } }));
  if (panel !== PINNED_PANEL && pos > 0) items.push({ key: 'prev', label: 'この列で前へ', act: { type: 'reorder', panel, delta: -1 } });
  if (panel !== PINNED_PANEL && pos !== -1 && pos < order.length - 1) items.push({ key: 'next', label: 'この列で後ろへ', act: { type: 'reorder', panel, delta: 1 } });
  useEffect(() => {
    if (!open) return;
    const onDown = (e: Event) => { if (!wrapRef.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('pointerdown', onDown);
    wrapRef.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    return () => { document.removeEventListener('pointerdown', onDown); };
  }, [open]);
  function close(refocus: boolean) {
    setOpen(false);
    if (refocus) document.getElementById(menuButtonId(panel))?.focus();
  }
  function onMenuKey(e: KeyboardEvent<HTMLDivElement>) {
    const all = Array.from(wrapRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
    const i = all.findIndex(el => el === document.activeElement);
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(true); return; }
    if (e.key === 'Tab') { setOpen(false); return; }
    let to: number | null = null;
    if (e.key === 'ArrowDown') to = (i + 1) % all.length;
    else if (e.key === 'ArrowUp') to = (i - 1 + all.length) % all.length;
    else if (e.key === 'Home') to = 0;
    else if (e.key === 'End') to = all.length - 1;
    if (to === null) return;
    e.preventDefault();
    all[to]?.focus();
  }
  const label = PANEL_LABELS[panel];
  return <span className="dock-move" ref={wrapRef}>
    <button type="button" id={menuButtonId(panel)} className="dock-move-btn" aria-haspopup="menu" aria-expanded={open}
      aria-label={`「${label}」の移動`} title={`「${label}」を別の列へ移す`}
      onClick={() => { setOpen(o => !o); }}
      onKeyDown={e => { if (e.key === 'ArrowDown' && !open) { e.preventDefault(); setOpen(true); } }}>⋮</button>
    {open && <div className="dock-menu" role="menu" aria-label={`「${label}」の移動`} onKeyDown={onMenuKey}>
      {items.map(it => <button key={it.key} type="button" role="menuitem" tabIndex={-1}
        onClick={() => { setOpen(false); dispatch(it.act); focusPanelControl(panel); }}>{it.label}</button>)}
    </div>}
  </span>;
}

function ColumnView({ index, layout, dispatch, hosts, dragging, setDragging }: {
  index: ColumnIndex; layout: DockLayout; dispatch: (a: DockAction) => void; hosts: Record<PanelId, HTMLElement>;
  dragging: PanelId | null; setDragging: (p: PanelId | null) => void;
}) {
  const col = layout.columns[index];
  const tabs = tabPanels(col);
  const pinned = col.panels.includes(PINNED_PANEL);
  const [over, setOver] = useState(false);
  const listRef = useRef<HTMLDivElement | null>(null);

  function onTabKey(e: KeyboardEvent<HTMLDivElement>) {
    const i = tabs.findIndex(p => p === col.active);
    let to: number | null = null;
    if (e.key === 'ArrowRight') to = (i + 1) % tabs.length;
    else if (e.key === 'ArrowLeft') to = (i - 1 + tabs.length) % tabs.length;
    else if (e.key === 'Home') to = 0;
    else if (e.key === 'End') to = tabs.length - 1;
    const target = to === null ? undefined : tabs[to];
    if (target === undefined || !(e.target instanceof HTMLElement) || e.target.getAttribute('role') !== 'tab') return;
    e.preventDefault();
    dispatch({ type: 'activate', panel: target });
    document.getElementById(dockTabId(target))?.focus();
  }
  const accepts = (e: DragEvent) => dragging !== null || e.dataTransfer.types.includes(DRAG_TYPE);
  function onDragOver(e: DragEvent<HTMLElement>) {
    if (!accepts(e)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (!over) setOver(true);
  }
  function onDrop(e: DragEvent<HTMLElement>) {
    const raw = e.dataTransfer.getData(DRAG_TYPE) || dragging;
    setOver(false);
    setDragging(null);
    const panel = PANEL_IDS.find(p => p === raw);
    if (panel === undefined) return;
    e.preventDefault();
    // タブの上に落としたらそのタブの前、それ以外は列の末尾
    const before = (e.target as HTMLElement).closest<HTMLElement>('[data-dock-tab]')?.dataset.dockTab;
    const idx = before !== undefined ? col.panels.indexOf(before as PanelId) : -1;
    dispatch({ type: 'move', panel, to: index, ...(idx >= 0 ? { index: idx } : {}) });
    focusPanelControl(panel);
  }
  const dragProps = (p: PanelId) => ({
    draggable: true,
    onDragStart: (e: DragEvent<HTMLElement>) => { e.dataTransfer.setData(DRAG_TYPE, p); e.dataTransfer.effectAllowed = 'move'; setDragging(p); },
    onDragEnd: () => { setDragging(null); },
  });
  const dropProps = { onDragOver, onDragLeave: (e: DragEvent<HTMLElement>) => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setOver(false); }, onDrop };

  if (isEmptyColumn(col)) {
    return <section className={`dock-col is-empty${dragging !== null ? ' is-dragging' : ''}${over ? ' is-over' : ''}`} aria-label={`${COLUMN_LABELS[index]}の列(空き)`}
      data-testid={`dock-col-${String(index)}`} title={`パネルのタブをここへドラッグするか、タブの「⋮」から「${COLUMN_LABELS[index]}へ移動」を選ぶと、この列に置けます`} {...dropProps}>
      <span className="dock-empty-label" aria-hidden="true">{dragging !== null ? 'ここに置く' : '＋'}</span>
    </section>;
  }
  return <section className={`dock-col${over ? ' is-over' : ''}`} aria-label={`${COLUMN_LABELS[index]}の列`} data-testid={`dock-col-${String(index)}`} {...dropProps}>
    {pinned && <div className="dock-pinned" data-testid="dock-pinned"><Slot host={hosts[PINNED_PANEL]} /></div>}
    <div className="dock-tabrow">
      {tabs.length > 0 && <div className="dock-tabs" role="tablist" aria-label={`${COLUMN_LABELS[index]}の列のパネル`} ref={listRef} onKeyDown={onTabKey}>
        {tabs.map(p => {
          const selected = p === col.active;
          return <span key={p} className={`dock-tab${selected ? ' is-active' : ''}${dragging === p ? ' is-dragging' : ''}`} role="presentation" data-dock-tab={p} {...dragProps(p)}>
            <button type="button" role="tab" id={dockTabId(p)} aria-selected={selected} aria-controls={dockPanelId(p)} tabIndex={selected ? 0 : -1}
              title="クリックで表示。ドラッグで別の列へ移せます" onClick={() => { dispatch({ type: 'activate', panel: p }); }}>{PANEL_LABELS[p]}</button>
          </span>;
        })}
      </div>}
      {/* 前に出ているタブの「移動」(キーボードではタブから Tab で届く。ほかのタブは矢印キーで前に出してから) */}
      {col.active !== null && <MoveMenu key={col.active} panel={col.active} layout={layout} dispatch={dispatch} />}
      {pinned && <span className="dock-pinned-chip" data-dock-tab={PINNED_PANEL} {...dragProps(PINNED_PANEL)} title="「案件の概要」はこの列の上端に固定で出ます。ドラッグで別の列へ移せます">
        <span>{PANEL_LABELS[PINNED_PANEL]}</span><MoveMenu panel={PINNED_PANEL} layout={layout} dispatch={dispatch} />
      </span>}
    </div>
    {tabs.length > 0 && <div className="dock-panels">
      {/* 枠は決まった順に並べる (タブの並びを変えても枠は動かさない = 枠の中のページを読み直さない) */}
      {PANEL_IDS.filter(p => tabs.includes(p)).map(p => <div key={p} role="tabpanel" id={dockPanelId(p)} aria-labelledby={dockTabId(p)}
        className={`dock-panel${p === col.active ? ' is-active' : ''}`} inert={p !== col.active} data-testid={`dock-panel-${p}`}>
        <Slot host={hosts[p]} />
      </div>)}
    </div>}
  </section>;
}

/** 列の間の区切り (ドラッグ・矢印キーで幅を変える) */
function Divider({ left, right, layout, dispatch, containerRef }: {
  left: ColumnIndex; right: ColumnIndex; layout: DockLayout; dispatch: (a: DockAction) => void; containerRef: React.RefObject<HTMLDivElement | null>;
}) {
  const last = useRef<number | null>(null);
  const pct = leftPercent(layout, left, right);
  const minRatio = () => {
    const w = containerRef.current?.getBoundingClientRect().width ?? 0;
    return w > 0 ? Math.max(0.12, MIN_COLUMN_PX / w) : undefined;
  };
  function onPointerDown(e: PointerEvent<HTMLDivElement>) {
    if (e.button !== 0) return;
    e.preventDefault();
    last.current = e.clientX;
    try { e.currentTarget.setPointerCapture(e.pointerId); } catch { /* 取れない環境ではそのまま */ }
  }
  function onPointerMove(e: PointerEvent<HTMLDivElement>) {
    if (last.current === null) return;
    const w = containerRef.current?.getBoundingClientRect().width ?? 0;
    if (w <= 0) return;
    const dx = e.clientX - last.current;
    if (dx === 0) return;
    last.current = e.clientX;
    const m = minRatio();
    dispatch({ type: 'resize', left, right, delta: dx / w, ...(m !== undefined ? { minRatio: m } : {}) });
  }
  function end() { last.current = null; }
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    let delta: number | null = null;
    if (e.key === 'ArrowLeft') delta = -KEY_RESIZE_STEP;
    else if (e.key === 'ArrowRight') delta = KEY_RESIZE_STEP;
    else if (e.key === 'Home') delta = -1;
    else if (e.key === 'End') delta = 1;
    if (delta === null) return;
    e.preventDefault();
    const m = minRatio();
    dispatch({ type: 'resize', left, right, delta, ...(m !== undefined ? { minRatio: m } : {}) });
  }
  const name = `${COLUMN_LABELS[left]}の列と${COLUMN_LABELS[right]}の列の幅`;
  return <div className="dock-divider" role="separator" aria-orientation="vertical" tabIndex={0} aria-label={name}
    aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct} aria-valuetext={`${COLUMN_LABELS[left]}の列 ${String(pct)}%`}
    title="ドラッグか左右の矢印キーで幅を変えます" data-testid={`dock-divider-${String(left)}-${String(right)}`}
    onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={end} onPointerCancel={end} onLostPointerCapture={end} onKeyDown={onKeyDown} />;
}

export function Dock({ layout, dispatch, panels }: {
  layout: DockLayout; dispatch: (a: DockAction) => void;
  /** パネルの中身 (常に描く。見えていないパネルも外さない) */
  panels: Record<PanelId, ReactNode>;
}) {
  // パネルごとの箱 (画面を開いている間ずっと同じ箱を使う)
  const [hosts] = useState<Record<PanelId, HTMLElement>>(() => {
    const out = {} as Record<PanelId, HTMLElement>;
    for (const p of PANEL_IDS) {
      const el = document.createElement('div');
      el.className = `dock-host dock-host-${p}`;
      out[p] = el;
    }
    return out;
  });
  const [dragging, setDragging] = useState<PanelId | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const setDrag = useCallback((p: PanelId | null) => { setDragging(p); }, []);

  // 見えなくなったパネルの中にフォーカスが残ったら、その列の前に出たタブへ移す (キーボードで迷子にならない)
  useEffect(() => {
    const ae = document.activeElement;
    if (!(ae instanceof HTMLElement)) return;
    const hidden = ae.closest<HTMLElement>('.dock-panel[inert]');
    if (hidden === null) return;
    const col = hidden.closest('.dock-col');
    col?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
  }, [layout]);

  const items: ReactNode[] = [];
  const template: string[] = [];
  let prevVisible: ColumnIndex | null = null;
  for (const i of COLUMN_INDEXES) {
    const empty = isEmptyColumn(layout.columns[i]);
    if (!empty && prevVisible !== null) {
      items.push(<Divider key={`d-${String(prevVisible)}-${String(i)}`} left={prevVisible} right={i} layout={layout} dispatch={dispatch} containerRef={containerRef} />);
      template.push('6px');
    }
    items.push(<ColumnView key={`c-${String(i)}`} index={i} layout={layout} dispatch={dispatch} hosts={hosts} dragging={dragging} setDragging={setDrag} />);
    template.push(empty ? (dragging !== null ? '120px' : '28px') : `minmax(${String(MIN_COLUMN_PX)}px, ${String(layout.widths[i])}fr)`);
    if (!empty) prevVisible = i;
  }
  const style: CSSProperties = { gridTemplateColumns: template.join(' ') };
  return <>
    <div className="dock" ref={containerRef} style={style} data-testid="dock">{items}</div>
    {/* 中身は列の後に描く (列の枠に箱を差し込んでから、中身の effect が動く) */}
    {PANEL_IDS.map(p => createPortal(panels[p], hosts[p], p))}
  </>;
}
