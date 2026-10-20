// 一覧の小さな部品: 選考状況の印、新着・更新の印、吹き出し、画面下の知らせ。
import { useEffect, useRef, type ReactNode, type RefObject } from 'react';
import { formatTime, cx } from '../../ui/kit';
import { IconX } from '../../ui/icons';
import type { ApplicantRow } from '../../types';
import type { CellState, Toast } from './useCellSaver';

export function StatusPill({ value, dot, className }: { value: string; dot: string; className?: string }) {
  if (value === '') return <span className="text-pt-faint">—</span>;
  return (
    <span className={cx('inline-flex h-[22px] max-w-full items-center gap-1.5 rounded-full border border-pt-line bg-pt-surface pr-2 pl-1.5 text-[12px] text-pt-ink', className)}>
      <span className={cx('h-1.5 w-1.5 shrink-0 rounded-full', dot)} />
      <span className="truncate">{value}</span>
    </span>
  );
}

export function ChangeMark({ kind }: { kind: ApplicantRow['changedSinceLastView'] }) {
  if (kind === 'new') return <span className="shrink-0 text-[11px] leading-4 font-medium text-pt-new">新着</span>;
  if (kind === 'updated') return <span className="shrink-0 text-[11px] leading-4 font-medium text-pt-warn">更新</span>;
  return null;
}

/** 新着・更新の行の左端の 2px の縦線の色。 */
export function changeLine(kind: ApplicantRow['changedSinceLastView']): string | undefined {
  if (kind === 'new') return 'var(--color-pt-new)';
  if (kind === 'updated') return 'var(--color-pt-warn)';
  return undefined;
}

/** マスの右上の小さな印。 */
export function CellMark({ state }: { state: CellState | undefined }) {
  if (!state) return null;
  const base = 'pointer-events-none absolute top-1 right-1';
  if (state.kind === 'saving') return <span className={cx(base, 'h-1.5 w-1.5 animate-pulse rounded-full bg-pt-faint')} title="保存中" />;
  if (state.kind === 'saved') return <span className={cx(base, 'text-pt-ok')} title="保存しました"><svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={3} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M20 6L9 17l-5-5" /></svg></span>;
  if (state.kind === 'conflict') return <span className={cx(base, 'h-1.5 w-1.5 rounded-full bg-pt-warn')} title="シートで先に変更されていました" />;
  return <span className={cx(base, 'h-1.5 w-1.5 rounded-full bg-pt-danger')} title="保存できませんでした" />;
}

export function useOutside(ref: RefObject<HTMLElement | null>, onOutside: () => void, active = true): void {
  const cb = useRef(onOutside);
  useEffect(() => { cb.current = onOutside; }, [onOutside]);
  useEffect(() => {
    if (!active) return undefined;
    const h = (e: MouseEvent) => {
      if (ref.current && e.target instanceof Node && !ref.current.contains(e.target)) cb.current();
    };
    document.addEventListener('mousedown', h);
    return () => { document.removeEventListener('mousedown', h); };
  }, [ref, active]);
}

/** 先に変更されていた・保存できなかったときの吹き出しの中身。 */
export function SaveProblem({ state, display, onTakeSheet, onOverwrite, onRetry, onClose }: {
  state: CellState;
  display: (v: string) => string;
  onTakeSheet: () => void;
  onOverwrite: () => void;
  onRetry: () => void;
  onClose: () => void;
}) {
  if (state.kind === 'conflict') {
    return (
      <div className="text-[12px] leading-relaxed">
        <div className="flex items-start gap-2">
          <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-pt-warn" />
          <p className="min-w-0 flex-1 text-pt-ink">シートで {state.by} さんが「{display(state.latest)}」に変更<span className="pt-num text-pt-muted">（{formatTime(state.at)}）</span></p>
          <CloseX onClick={onClose} />
        </div>
        <p className="mt-0.5 pl-3.5 text-pt-muted">あなたの入力: {display(state.mine)}</p>
        <div className="mt-2 flex flex-wrap gap-1.5 pl-3.5">
          <SmallButton onClick={onTakeSheet}>シートの値にする</SmallButton>
          <SmallButton primary onClick={onOverwrite}>自分の値で上書き保存</SmallButton>
        </div>
      </div>
    );
  }
  if (state.kind === 'failed') {
    return (
      <div className="text-[12px] leading-relaxed">
        <div className="flex items-start gap-2">
          <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-pt-danger" />
          <p className="min-w-0 flex-1 text-pt-ink">保存できませんでした。入力した「{display(state.value)}」はまだシートにありません。</p>
          <CloseX onClick={onClose} />
        </div>
        <div className="mt-2 flex gap-1.5 pl-3.5">
          <SmallButton primary onClick={onRetry}>もう一度</SmallButton>
          <SmallButton onClick={onTakeSheet}>やめる</SmallButton>
        </div>
      </div>
    );
  }
  return null;
}

function CloseX({ onClick }: { onClick: () => void }) {
  return <button type="button" aria-label="閉じる" onClick={onClick} className="-mt-0.5 -mr-1 rounded p-0.5 text-pt-faint hover:bg-pt-sunken hover:text-pt-ink"><IconX size={13} /></button>;
}

export function SmallButton({ primary = false, onClick, children }: { primary?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cx('inline-flex h-11 items-center justify-center rounded-md px-2.5 text-[12px] font-medium whitespace-nowrap sm:h-7', primary ? 'bg-pt-primary text-white hover:bg-pt-primary-hover' : 'border border-pt-line-strong bg-pt-surface text-pt-ink hover:bg-pt-bg')}
    >
      {children}
    </button>
  );
}

/** 画面下の小さな知らせ。PC は右下の角に小さく、スマホは下のメニューの上。 */
export function ToastBar({ toast, onUndo, onClose }: { toast: Toast | null; onUndo: () => void; onClose: () => void }) {
  if (!toast) return null;
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-20 z-40 flex justify-center px-4 lg:inset-x-auto lg:right-5 lg:bottom-5 lg:justify-end lg:px-0">
      <div role="status" className="pointer-events-auto flex max-w-[420px] items-center gap-2 rounded-lg bg-pt-ink py-1.5 pr-1.5 pl-3 text-[12px] text-white shadow-pt-pop">
        <span className="min-w-0 truncate">{toast.text}</span>
        {toast.undo && (
          <button type="button" onClick={onUndo} className="shrink-0 rounded px-1.5 py-1 font-medium text-white underline-offset-2 hover:underline">
            元に戻す<span className="ml-1 hidden text-[11px] text-white/60 lg:inline">Ctrl+Z</span>
          </button>
        )}
        {toast.action && (
          <button type="button" onClick={() => { toast.action?.run(); onClose(); }} className="shrink-0 rounded px-1.5 py-1 font-medium text-white underline-offset-2 hover:underline">{toast.action.label}</button>
        )}
        <button type="button" aria-label="閉じる" onClick={onClose} className="shrink-0 rounded p-1 text-white/60 hover:text-white"><IconX size={13} /></button>
      </div>
    </div>
  );
}
