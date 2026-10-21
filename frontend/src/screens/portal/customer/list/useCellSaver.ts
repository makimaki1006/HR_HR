// 1 マスの保存と、その状態（保存中・保存済み・先に変更されていた・保存できなかった）、元に戻す。
// 表とスマホのカードで同じものを使う。書き込みは必ず actions.saveCell を通す（記録が残る）。
import { useCallback, useEffect, useRef, useState } from 'react';
import { actions } from '../../data';
import type { CellSaveResult } from '../../types';
import { omit } from './model';

export type CellState =
  | { kind: 'saving'; value: string }
  | { kind: 'saved' }
  | { kind: 'conflict'; mine: string; latest: string; by: string; at: string }
  | { kind: 'failed'; base: string; value: string };

export interface UndoEntry {
  rowNumber: number;
  column: string;
  before: string;
  after: string;
  /** 知らせに出す「山田 花子さんの「面接日」」の部分。 */
  label: string;
}

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  text: string;
  undo?: boolean;
  action?: ToastAction;
}

export const cellKey = (rowNumber: number, column: string): string => `${String(rowNumber)}|${column}`;

export interface CellSaver {
  cells: Record<string, CellState>;
  toast: Toast | null;
  canUndo: boolean;
  /** 吹き出しを出しているマス。 */
  bubble: string | null;
  setBubble: (key: string | null) => void;
  save: (rowNumber: number, column: string, base: string, next: string, label: string) => void;
  undo: () => void;
  takeSheet: (key: string) => void;
  overwrite: (key: string, label: string) => void;
  retry: (key: string, label: string) => void;
  notify: (text: string, action?: ToastAction) => void;
  closeToast: () => void;
}

let toastSeq = 0;

export function useCellSaver(sheetId: string): CellSaver {
  const [cells, setCells] = useState<Record<string, CellState>>({});
  const [undoStack, setUndoStack] = useState<UndoEntry[]>([]);
  const [toast, setToast] = useState<Toast | null>(null);
  const [bubble, setBubble] = useState<string | null>(null);
  const cellsRef = useRef(cells);
  useEffect(() => { cellsRef.current = cells; }, [cells]);
  const undoRef = useRef(undoStack);
  useEffect(() => { undoRef.current = undoStack; }, [undoStack]);

  const setCell = useCallback((key: string, s: CellState | null) => {
    setCells((prev) => {
      return s ? { ...prev, [key]: s } : omit(prev, key);
    });
  }, []);

  const notify = useCallback((text: string, undo = false, action?: ToastAction) => {
    toastSeq += 1;
    setToast(action ? { id: toastSeq, text, undo, action } : { id: toastSeq, text, undo });
  }, []);

  useEffect(() => {
    if (!toast) return undefined;
    const t = setTimeout(() => { setToast((cur) => (cur?.id === toast.id ? null : cur)); }, toast.undo || toast.action ? 7000 : 4000);
    return () => { clearTimeout(t); };
  }, [toast]);

  const run = useCallback(async (rowNumber: number, column: string, base: string, next: string, label: string, mode: 'edit' | 'undo') => {
    const key = cellKey(rowNumber, column);
    setCell(key, { kind: 'saving', value: next });
    const r: CellSaveResult = await actions.saveCell(sheetId, rowNumber, column, base, next);
    if (r.kind === 'saved') {
      setCell(key, { kind: 'saved' });
      setTimeout(() => { setCells((prev) => (prev[key]?.kind === 'saved' ? omit(prev, key) : prev)); }, 1800);
      if (mode === 'edit') {
        setUndoStack((s) => [...s.slice(-19), { rowNumber, column, before: base, after: next, label }]);
        notify(`${label}を変更しました`, true);
      } else {
        notify(`${label}を元に戻しました`);
      }
    } else if (r.kind === 'conflict') {
      setCell(key, { kind: 'conflict', mine: next, latest: r.latest, by: r.by, at: r.at });
      setBubble(key);
    } else {
      setCell(key, { kind: 'failed', base, value: next });
      setBubble(key);
    }
  }, [sheetId, setCell, notify]);

  const save = useCallback((rowNumber: number, column: string, base: string, next: string, label: string) => {
    if (base === next) return;
    void run(rowNumber, column, base, next, label, 'edit');
  }, [run]);

  const undo = useCallback(() => {
    const s = undoRef.current;
    const last = s[s.length - 1];
    if (!last) return;
    undoRef.current = s.slice(0, -1);
    setUndoStack(undoRef.current);
    void run(last.rowNumber, last.column, last.after, last.before, last.label, 'undo');
  }, [run]);

  const takeSheet = useCallback((key: string) => {
    setCell(key, null);
    setBubble(null);
  }, [setCell]);

  const parse = (key: string): [number, string] => {
    const i = key.indexOf('|');
    return [Number(key.slice(0, i)), key.slice(i + 1)];
  };

  const overwrite = useCallback((key: string, label: string) => {
    const s = cellsRef.current[key];
    if (s?.kind !== 'conflict') return;
    const [rowNumber, column] = parse(key);
    setBubble(null);
    void run(rowNumber, column, s.latest, s.mine, label, 'edit');
  }, [run]);

  const retry = useCallback((key: string, label: string) => {
    const s = cellsRef.current[key];
    if (s?.kind !== 'failed') return;
    const [rowNumber, column] = parse(key);
    setBubble(null);
    void run(rowNumber, column, s.base, s.value, label, 'edit');
  }, [run]);

  return {
    cells,
    toast,
    canUndo: undoStack.length > 0,
    bubble,
    setBubble,
    save,
    undo,
    takeSheet,
    overwrite,
    retry,
    notify: (text: string, action?: ToastAction) => { notify(text, false, action); },
    closeToast: () => { setToast(null); },
  };
}
