import type { ReactNode } from 'react';

export type NoteKind = 'hw-scope' | 'correlation' | 'custom';

/** Fixed wording per kind. Must stay free of FORBIDDEN_PHRASES (see Note.test.tsx). */
export const NOTE_TEXT = {
  'hw-scope': 'ハローワーク掲載求人のみが対象で、全求人市場ではありません。',
  correlation: '相関関係であり、因果関係を示すものではありません。',
} as const;

export interface NoteProps {
  kind: NoteKind;
  /** Required in practice for 'custom'; appended after the fixed text for the other kinds. */
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
