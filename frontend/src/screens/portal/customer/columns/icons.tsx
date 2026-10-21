// この画面だけで使う線の絵（ui/icons.tsx と同じ書き方）。
import type { ReactNode } from 'react';

function Svg({ size = 16, children, className }: { size?: number; children: ReactNode; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className={className}>
      {children}
    </svg>
  );
}

interface P { size?: number; className?: string }

export const IconText = (p: P) => <Svg {...p}><path d="M4 7V5h16v2" /><path d="M12 5v14" /><path d="M9 19h6" /></Svg>;
export const IconCalendar = (p: P) => <Svg {...p}><rect x="3" y="5" width="18" height="16" rx="2" /><path d="M3 10h18M8 3v4M16 3v4" /></Svg>;
export const IconList = (p: P) => <Svg {...p}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M8 10l4 4 4-4" /></Svg>;
export const IconArrowUp = (p: P) => <Svg {...p}><path d="M12 19V5" /><path d="M5 12l7-7 7 7" /></Svg>;
export const IconArrowDown = (p: P) => <Svg {...p}><path d="M12 5v14" /><path d="M19 12l-7 7-7-7" /></Svg>;
export const IconEyeOff = (p: P) => <Svg {...p}><path d="M3 3l18 18" /><path d="M10.6 6.1A9.8 9.8 0 0 1 12 6c6.5 0 10 6 10 6a17 17 0 0 1-3.1 3.7M6.6 6.6A16.7 16.7 0 0 0 2 12s3.5 6 10 6a9.7 9.7 0 0 0 4-.9" /><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2" /></Svg>;
export const IconTrash = (p: P) => <Svg {...p}><path d="M4 7h16" /><path d="M10 11v6M14 11v6" /><path d="M6 7l1 13h10l1-13" /><path d="M9 7V4h6v3" /></Svg>;
