import { useEffect, useState } from 'react';

/** 拡張機能 (extensions/hrhr-crm-frames) の content script が <html> に付ける属性 (data-hrhr-frames="<version>") */
const ATTR = 'data-hrhr-frames';

export interface FrameExtensionState { installed: boolean; version: string | null }

function read(): FrameExtensionState {
  const v = document.documentElement.getAttribute(ATTR);
  return v === null ? { installed: false, version: null } : { installed: true, version: v };
}

/** 枠の拡張機能が入っているか。最初の描画より少し遅れて属性が付くことがあるので、属性の変化を監視する */
export function useFrameExtension(): FrameExtensionState {
  const [state, setState] = useState<FrameExtensionState>(read);
  useEffect(() => {
    const sync = () => {
      const next = read();
      setState(prev => (prev.installed === next.installed && prev.version === next.version ? prev : next));
    };
    sync();
    const mo = new MutationObserver(sync);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: [ATTR] });
    return () => { mo.disconnect(); };
  }, []);
  return state;
}

/** 「枠の中では開けないことがあります」の案内を出すか (拡張機能が入っていれば出さない) */
export const shouldShowFrameHint = (installed: boolean): boolean => !installed;
