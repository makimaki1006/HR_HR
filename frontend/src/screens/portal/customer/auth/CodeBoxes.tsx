// 6 桁のコードの入力。試作 (Code.tsx) と本番の入口 (live/PortalApp.tsx) で使う。サンプルのデータを読まない。
import { useState } from 'react';
import { cx } from '../../ui/kit';

/** 全角の数字（日本語入力がオンのとき）も半角にしてから数字だけ残す。 */
export const toDigits = (raw: string) => raw.normalize('NFKC').replace(/\D/g, '');

/** 6 つの枠の入力。貼り付けで全部入り、戻しキーで前の枠へ。 */
export function CodeBoxes({ value, invalid, onChange }: { value: string; invalid: boolean; onChange: (v: string) => void }) {
  // 日本語入力の変換中は、打った文字をそのまま見せておき、確定してから数字にする。
  const [draft, setDraft] = useState<{ i: number; text: string } | null>(null);
  const focusAt = (i: number) => { document.getElementById(`code-${String(Math.min(Math.max(i, 0), 5))}`)?.focus(); };
  const setAt = (i: number, raw: string) => {
    // 1 マスには最後の 1 文字だけ入れる。複数桁は貼り付けでだけ受ける。
    const digits = toDigits(raw).slice(-1);
    if (digits === '') {
      onChange(value.slice(0, i) + value.slice(i + 1));
      return;
    }
    const next = (value.slice(0, i) + digits + value.slice(i + 1)).slice(0, 6);
    onChange(next);
    focusAt(i + digits.length);
  };
  return (
    <div className="flex justify-between gap-2" role="group" aria-label="6 桁のコード">
      {Array.from({ length: 6 }, (_, i) => (
        <input
          key={i}
          id={`code-${String(i)}`}
          value={draft?.i === i ? draft.text : (value[i] ?? '')}
          inputMode="numeric"
          pattern="[0-9]*"
          autoComplete={i === 0 ? 'one-time-code' : 'off'}
          aria-label={`${String(i + 1)} 桁目`}
          aria-invalid={invalid}
          className={cx('pt-num h-12 w-full min-w-0 rounded-md border bg-pt-surface text-center text-xl font-medium text-pt-ink focus:border-pt-primary focus:outline-none focus:ring-2 focus:ring-pt-primary-soft', invalid ? 'border-pt-danger' : 'border-pt-line-strong')}
          onChange={(e) => {
            if ((e.nativeEvent as InputEvent).isComposing) { setDraft({ i, text: e.target.value }); return; }
            setAt(i, e.target.value);
          }}
          onCompositionEnd={(e) => { setDraft(null); setAt(i, e.currentTarget.value); }}
          onFocus={(e) => { e.target.select(); }}
          onKeyDown={(e) => {
            if (e.key === 'Backspace' && !value[i]) focusAt(i - 1);
            if (e.key === 'ArrowLeft') focusAt(i - 1);
            if (e.key === 'ArrowRight') focusAt(i + 1);
          }}
          onPaste={(e) => {
            e.preventDefault();
            onChange(toDigits(e.clipboardData.getData('text')).slice(0, 6));
            focusAt(5);
          }}
        />
      ))}
    </div>
  );
}
