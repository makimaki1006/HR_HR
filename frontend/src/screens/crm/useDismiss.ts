import { useEffect } from 'react';
import type { RefObject } from 'react';

/** 開いている間だけ、外側のクリック / タッチと Esc で onDismiss を呼ぶ (ref の中のクリックは無視) */
export function useDismiss(ref: RefObject<HTMLElement | null>, active: boolean, onDismiss: () => void, onEscape?: () => void) {
  useEffect(() => {
    if (!active) return undefined;
    const outside = (e: Event) => {
      const t = e.target;
      if (t instanceof Node && ref.current?.contains(t) !== true) onDismiss();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { (onEscape ?? onDismiss)(); }
    };
    document.addEventListener('mousedown', outside);
    document.addEventListener('touchstart', outside);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('mousedown', outside);
      document.removeEventListener('touchstart', outside);
      document.removeEventListener('keydown', key);
    };
  }, [ref, active, onDismiss, onEscape]);
}
