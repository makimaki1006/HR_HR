import type { ReactNode } from 'react';

export type NoteKind = 'hw-scope' | 'correlation' | 'source' | 'custom';

/** Fixed wording per kind. Must stay free of FORBIDDEN_PHRASES (see Note.test.tsx). */
export const NOTE_TEXT = {
  'hw-scope': 'ハローワーク掲載求人のみが対象で、全求人市場ではありません。',
  correlation: '相関関係であり、因果関係を示すものではありません。',
  source: '出典: ',
} as const;

export interface NoteProps {
  kind: NoteKind;
  /** Required in practice for 'custom'; for 'source' it is the source name (after "出典: "). */
  children?: ReactNode;
}

/** Scope / interpretation caveat shown next to numbers (HW-only scope, correlation != causation). */
export function Note({ kind, children }: NoteProps) {
  return (
    <p className="hw-note" role="note" data-note-kind={kind}>
      {kind === 'custom' ? null : <span>{NOTE_TEXT[kind]}</span>}
      {children}
    </p>
  );
}
