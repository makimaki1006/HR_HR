// 詳細だけで使う線の絵（ui/icons.tsx と同じ書き方）。
import type { ReactNode } from 'react';

function Svg({ size = 16, children, className }: { size?: number; children: ReactNode; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className={className}>
      {children}
    </svg>
  );
}

interface P { size?: number; className?: string }

export const IconPencil = (p: P) => <Svg {...p}><path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" /></Svg>;
